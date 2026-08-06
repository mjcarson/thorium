//! The internal traits and output buffer both `CaRT` codecs are built on
//!
//! Everything in here is crate private. It exists so that the streaming, manual, and version
//! detecting layers above it are written once instead of once per version.

use bytes::{Bytes, BytesMut};

use crate::error::Error;
use crate::header::Header;
use crate::key::CartKey;
use crate::version::{CartOptions, CartVersion};

/// The largest slice of input a codec is handed in a single call
///
/// [`Encode::push`] and [`Decode::push`] always consume everything they are given, so the only
/// thing bounding how much a codec appends to its output in one call is how much input it is
/// handed. Every caller in this crate chops its input at this size.
pub(crate) const MAX_INPUT_CHUNK: usize = 128 * 1024;

/// The largest slice of carted input a *decoder* is handed in a single call
///
/// The decode direction needs a tighter cap than the encode direction because compression runs
/// backwards through it. DEFLATE's worst case expansion ratio is 1032:1, so this bounds how much
/// plaintext a single [`Decode::push`] can be made to produce at roughly 16 MB however
/// pathological the input is. That matters because the input is a file somebody uploaded.
pub(crate) const MAX_DECODE_CHUNK: usize = 16 * 1024;

/// How much plaintext a decoder will pile up before it stops taking input
///
/// Both compressors amplify: a few dozen bytes of Zstd can expand to a whole block and DEFLATE
/// can expand 1032:1. Since [`Decode::push`] appends to an [`OutBuf`] that grows on demand, a
/// hostile file could otherwise turn one small read into gigabytes of resident memory.
///
/// So a decoder stops accepting input once `out` holds this much, and tells the caller how much
/// of the chunk it actually took. The caller drains `out` and hands the rest back. It is a
/// *soft* cap because a decoder always makes progress on a non empty chunk, so the real ceiling
/// is this plus whatever one step of the codec can produce.
pub(crate) const SOFT_OUTPUT_CAP: usize = 1024 * 1024;

/// The smallest writable region [`OutBuf::writable`] will hand out
const MIN_WRITABLE: usize = 32 * 1024;

/// The output buffer a codec writes into
///
/// This is a plain `BytesMut` plus two cursors, and the cursors are the whole point:
///
/// * `buf[..start]` has already been handed to the caller.
/// * `buf[start..filled]` is finished output waiting to be read.
/// * `buf[filled..]` is slack the codec may write into.
///
/// Reading drains with [`OutBuf::advance`], which moves a cursor rather than memmoving the
/// remainder, so a caller reading a large buffer through a small [`tokio::io::ReadBuf`] is
/// linear rather than quadratic. Writing grows through [`OutBuf::writable`], which only ever
/// zeroes the *new* part of the buffer, so a steady state stream stops paying for the memset
/// entirely once the buffer reaches its high water mark.
///
/// The zeroing is only there because the crate is `#![forbid(unsafe_code)]` and so cannot hand
/// out a view of a `Vec`'s uninitialized spare capacity. Stale bytes left over from a previous
/// round are fine; nothing reads slack that was not committed.
#[derive(Debug, Default)]
pub(crate) struct OutBuf {
    /// The buffer itself
    buf: BytesMut,
    /// Where the unread output starts
    start: usize,
    /// Where the unread output ends and the writable slack begins
    filled: usize,
}

impl OutBuf {
    /// Build an empty output buffer
    pub(crate) fn new() -> Self {
        OutBuf::default()
    }

    /// Get a writable region of at least `want` bytes
    ///
    /// The returned slice is frequently longer than `want`; codecs are expected to use all of
    /// it and report back how much they actually wrote through [`OutBuf::commit`].
    ///
    /// # Arguments
    ///
    /// * `want` - The smallest number of writable bytes the caller can make progress with
    pub(crate) fn writable(&mut self, want: usize) -> &mut [u8] {
        // work out how long the buffer has to be to give the caller what it asked for
        let needed = self.filled + want.max(MIN_WRITABLE);
        // only grow, so that the zeroing is paid once rather than once per call
        if self.buf.len() < needed {
            self.buf.resize(needed, 0);
        }
        &mut self.buf[self.filled..]
    }

    /// Mark bytes written into the region from [`OutBuf::writable`] as finished output
    ///
    /// # Arguments
    ///
    /// * `written` - The number of bytes the codec wrote
    pub(crate) fn commit(&mut self, written: usize) {
        self.filled += written;
    }

    /// Append a slice to the finished output
    ///
    /// # Arguments
    ///
    /// * `data` - The bytes to append
    pub(crate) fn extend(&mut self, data: &[u8]) {
        let room = self.writable(data.len());
        room[..data.len()].copy_from_slice(data);
        self.commit(data.len());
    }

    /// The finished output that has not been read yet
    pub(crate) fn filled(&self) -> &[u8] {
        &self.buf[self.start..self.filled]
    }

    /// How many bytes of finished output are waiting to be read
    pub(crate) fn len(&self) -> usize {
        self.filled - self.start
    }

    /// Whether there is any finished output waiting to be read
    pub(crate) fn is_empty(&self) -> bool {
        self.start == self.filled
    }

    /// Drop the first `n` bytes of finished output
    ///
    /// # Arguments
    ///
    /// * `n` - The number of bytes that have been read
    pub(crate) fn advance(&mut self, n: usize) {
        self.start = (self.start + n).min(self.filled);
        // once the buffer is drained rewind both cursors so the allocation gets reused
        if self.start == self.filled {
            self.start = 0;
            self.filled = 0;
        }
    }

    /// Take up to `want` bytes of finished output as an owned [`Bytes`]
    ///
    /// This is O(1): it hands over a slice of the existing allocation rather than copying it.
    /// The manual `CaRT` API uses it so that the S3 upload path can own an exactly part sized
    /// chunk without the per part `Bytes::copy_from_slice` it used to do, and without having to
    /// take more than one part's worth to get any at all.
    ///
    /// # Arguments
    ///
    /// * `want` - The most bytes to take, clamped to what is actually ready
    pub(crate) fn take_up_to(&mut self, want: usize) -> Bytes {
        let take = want.min(self.len());
        // drop the part the caller has already read so the split lands on unread output
        if self.start > 0 {
            drop(self.buf.split_to(self.start));
            self.filled -= self.start;
            self.start = 0;
        }
        // split the requested bytes off the front, leaving the writable slack behind for reuse
        let taken = self.buf.split_to(take).freeze();
        self.filled -= take;
        taken
    }

    /// Take all of the finished output as an owned [`Bytes`]
    pub(crate) fn take(&mut self) -> Bytes {
        self.take_up_to(self.len())
    }
}

/// The write half of a `CaRT` codec
///
/// [`Encode::push`] always consumes all of its input, growing `out` as far as it needs to. That
/// is what makes it impossible to express the "report more work to do without making progress"
/// livelock the old manual API had: there is no "output full" state to get stuck in. Callers
/// bound memory by bounding how much input they hand over, not by sizing an output buffer.
pub(crate) trait Encode: Send + Sync + Unpin {
    /// The `CaRT` version this codec writes
    fn version(&self) -> CartVersion;

    /// Feed plaintext in, appending carted bytes to `out`
    ///
    /// # Arguments
    ///
    /// * `input` - The plaintext to cart
    /// * `out` - The buffer to append carted bytes to
    ///
    /// # Errors
    ///
    /// Returns an error if compression fails or if the cart was already finished.
    fn push(&mut self, input: &[u8], out: &mut OutBuf) -> Result<(), Error>;

    /// Flush everything still buffered and write the `CaRT` footer
    ///
    /// # Arguments
    ///
    /// * `out` - The buffer to append the remaining carted bytes and the footer to
    ///
    /// # Errors
    ///
    /// Returns an error if compression fails or if the cart was already finished.
    fn finish(&mut self, out: &mut OutBuf) -> Result<(), Error>;
}

/// The read half of a `CaRT` codec
///
/// Unlike [`Encode`], a decoder is allowed to take only part of a chunk. Decompression
/// amplifies, so "consume everything you are given" would hand an attacker control of how much
/// memory one read turns into. Instead a decoder stops once it has produced
/// [`SOFT_OUTPUT_CAP`] bytes and reports how far it got, which is exactly the shape
/// [`tokio::io::AsyncBufRead::poll_fill_buf`] plus `consume` wants anyway.
pub(crate) trait Decode: Send + Sync + Unpin {
    /// The `CaRT` version this codec reads
    fn version(&self) -> CartVersion;

    /// Feed carted bytes in, appending plaintext to `out`, and report how many were taken
    ///
    /// A decoder always takes at least one byte of a non empty chunk, so a caller that drains
    /// `out` and re-pushes the remainder cannot livelock. Bytes that were taken are gone: the
    /// caller must not hand them over again.
    ///
    /// # Arguments
    ///
    /// * `input` - The carted bytes to uncart
    /// * `out` - The buffer to append plaintext to
    ///
    /// # Errors
    ///
    /// Returns an error if the data is corrupt, malformed, or fails authentication.
    fn push(&mut self, input: &[u8], out: &mut OutBuf) -> Result<usize, Error>;

    /// Tell the codec the input stream has ended
    ///
    /// This is where truncation is caught: a codec that has not seen the end of its body or a
    /// valid footer by the time this is called must return an error rather than pretending the
    /// file was complete.
    ///
    /// # Arguments
    ///
    /// * `out` - The buffer to append any remaining plaintext to
    ///
    /// # Errors
    ///
    /// Returns an error if the file ended before the codec reached the end of the `CaRT` body.
    fn finish(&mut self, out: &mut OutBuf) -> Result<(), Error>;
}

/// Build the encoder for a `CaRT` version
///
/// This is the crate's single version fork on the write side. It costs one virtual call per
/// chunk of up to [`MAX_INPUT_CHUNK`] bytes, which is unmeasurable next to compressing that
/// chunk, and in exchange the streaming and manual APIs are plain non generic types.
///
/// # Arguments
///
/// * `version` - The `CaRT` version to write
/// * `key` - The key to cart with
/// * `options` - The options for every version, of which only the matching one is read
///
/// # Errors
///
/// Returns an error if the requested version was compiled out or its options are invalid.
pub(crate) fn new_encoder(
    version: CartVersion,
    key: CartKey,
    options: CartOptions,
) -> Result<Box<dyn Encode>, Error> {
    match version {
        #[cfg(feature = "v1")]
        CartVersion::V1 => Ok(Box::new(crate::v1::V1Encoder::new(key, options.v1)?)),
        #[cfg(feature = "v2")]
        CartVersion::V2 => Ok(Box::new(crate::v2::V2Encoder::new(key, options.v2)?)),
        // only reachable when a version was compiled out
        #[allow(unreachable_patterns)]
        disabled => Err(Error::VersionDisabled(disabled)),
    }
}

/// Build the decoder for an already parsed `CaRT` header
///
/// The read path never takes a version from the caller. It comes from the file, because the
/// alternative is a deployment whose config decides how to read objects it did not write.
///
/// # Arguments
///
/// * `header` - The parsed mandatory header of the file being read
///
/// # Errors
///
/// Returns an error if the file's version was compiled out or its header is malformed.
pub(crate) fn new_decoder(header: &Header) -> Result<Box<dyn Decode>, Error> {
    match header.version {
        #[cfg(feature = "v1")]
        CartVersion::V1 => Ok(Box::new(crate::v1::V1Decoder::new(header))),
        #[cfg(feature = "v2")]
        CartVersion::V2 => Ok(Box::new(crate::v2::V2Decoder::new(header)?)),
        // only reachable when a version was compiled out
        #[allow(unreachable_patterns)]
        disabled => Err(Error::VersionDisabled(disabled)),
    }
}

/// Holds back the last `N` bytes of a stream
///
/// The `CaRT` footer is a fixed 28 bytes glued onto the end of the body with nothing marking
/// where the body stopped. The old reader handed those 28 bytes straight to the decompressor
/// and got away with it only because `miniz_oxide` happens to tolerate trailing bytes after
/// the end of a zlib stream — a property that evaporated the moment a second DEFLATE backend
/// entered the build, and cost about a day of debugging.
///
/// This makes it structural instead. Bytes go in, and the last `N` of everything seen so far
/// never come out until [`TailWithholder::finish`] is called at end of stream. The decoder
/// physically cannot be handed the footer, whatever the decompressor would have done with it.
///
/// Only V1 needs this. V2 length prefixes every block, so its decompressor is fed exclusively
/// from the plaintext of a block whose length was known before a byte of it was read.
#[cfg(feature = "v1")]
#[derive(Debug)]
pub(crate) struct TailWithholder<const N: usize> {
    /// The bytes being held back
    tail: [u8; N],
    /// How many of `tail`'s bytes are real
    len: usize,
}

#[cfg(feature = "v1")]
impl<const N: usize> Default for TailWithholder<N> {
    /// Build an empty tail withholder
    fn default() -> Self {
        TailWithholder {
            tail: [0; N],
            len: 0,
        }
    }
}

#[cfg(feature = "v1")]
impl<const N: usize> TailWithholder<N> {
    /// Build an empty tail withholder
    pub(crate) fn new() -> Self {
        TailWithholder::default()
    }

    /// Feed a chunk in and find out what is now safe to release
    ///
    /// Returns `(carry, carry_len, release_len)`. `carry[..carry_len]` are bytes evicted from
    /// the held tail and must be released *first*; `input[..release_len]` are bytes released
    /// straight out of the caller's slice, which keeps the common case copy free.
    ///
    /// # Arguments
    ///
    /// * `input` - The chunk to feed in
    pub(crate) fn push(&mut self, input: &[u8]) -> ([u8; N], usize, usize) {
        // work out how much of everything we have seen is now safe to let go of
        let release = (self.len + input.len()).saturating_sub(N);
        // release out of the held tail first, since those bytes came in first
        let from_tail = release.min(self.len);
        // and take whatever is left out of the front of the new chunk
        let from_input = release - from_tail;
        // copy out the bytes leaving the tail before we overwrite them
        let mut carry = [0; N];
        carry[..from_tail].copy_from_slice(&self.tail[..from_tail]);
        // shuffle whatever is still held down to the front
        self.tail.copy_within(from_tail..self.len, 0);
        self.len -= from_tail;
        // and hold on to the part of the new chunk we did not release
        let keep = &input[from_input..];
        self.tail[self.len..self.len + keep.len()].copy_from_slice(keep);
        self.len += keep.len();
        (carry, from_tail, from_input)
    }

    /// The bytes still being held back at end of stream
    pub(crate) fn finish(&self) -> &[u8] {
        &self.tail[..self.len]
    }
}

#[cfg(test)]
mod tests {
    use super::OutBuf;
    #[cfg(feature = "v1")]
    use super::TailWithholder;

    #[test]
    fn out_buf_appends_and_drains() {
        let mut out = OutBuf::new();
        assert!(out.is_empty());
        out.extend(b"corn");
        assert_eq!(out.filled(), b"corn");
        assert_eq!(out.len(), 4);
        out.advance(2);
        assert_eq!(out.filled(), b"rn");
        out.advance(2);
        assert!(out.is_empty());
    }

    #[test]
    fn out_buf_writable_grows_without_losing_committed_output() {
        let mut out = OutBuf::new();
        out.extend(b"header");
        let room = out.writable(1024);
        assert!(room.len() >= 1024);
        room[..3].copy_from_slice(b"abc");
        out.commit(3);
        assert_eq!(out.filled(), b"headerabc");
    }

    /// Draining through a small window must not memmove the remainder every time
    #[test]
    fn out_buf_drains_through_a_tiny_window() {
        let mut out = OutBuf::new();
        let data: Vec<u8> = (0..=255_u8).cycle().take(10_000).collect();
        out.extend(&data);
        let mut drained = Vec::new();
        while !out.is_empty() {
            let take = out.len().min(7);
            drained.extend_from_slice(&out.filled()[..take]);
            out.advance(take);
        }
        assert_eq!(drained, data);
    }

    #[test]
    fn out_buf_take_hands_over_exactly_the_unread_output() {
        let mut out = OutBuf::new();
        out.extend(b"0123456789");
        out.advance(4);
        assert_eq!(&out.take()[..], b"456789");
        assert!(out.is_empty());
        // and the buffer keeps working afterwards
        out.extend(b"more");
        assert_eq!(&out.take()[..], b"more");
    }

    /// The whole point of the withholder: the last N bytes never come out early
    #[cfg(feature = "v1")]
    #[test]
    fn the_tail_is_never_released_early() {
        for chunk_size in [1_usize, 2, 3, 5, 27, 28, 29, 64, 1000] {
            let data: Vec<u8> = (0..200_u8).collect();
            let mut withholder = TailWithholder::<28>::new();
            let mut released = Vec::new();
            for chunk in data.chunks(chunk_size) {
                let (carry, carry_len, release_len) = withholder.push(chunk);
                released.extend_from_slice(&carry[..carry_len]);
                released.extend_from_slice(&chunk[..release_len]);
                // at no point may the released bytes reach into the last 28
                assert!(
                    released.len() <= data.len() - 28,
                    "released {} of {} bytes with chunks of {chunk_size}",
                    released.len(),
                    data.len()
                );
            }
            assert_eq!(released, &data[..data.len() - 28]);
            assert_eq!(withholder.finish(), &data[data.len() - 28..]);
        }
    }

    #[cfg(feature = "v1")]
    #[test]
    fn a_stream_shorter_than_the_tail_releases_nothing() {
        let mut withholder = TailWithholder::<28>::new();
        for byte in 0..27_u8 {
            let (_, carry_len, release_len) = withholder.push(&[byte]);
            assert_eq!(carry_len, 0);
            assert_eq!(release_len, 0);
        }
        assert_eq!(withholder.finish().len(), 27);
    }

    #[cfg(feature = "v1")]
    #[test]
    fn an_empty_stream_holds_nothing() {
        let mut withholder = TailWithholder::<28>::new();
        let (_, carry_len, release_len) = withholder.push(&[]);
        assert_eq!((carry_len, release_len), (0, 0));
        assert!(withholder.finish().is_empty());
    }

    /// The withholder over ragged chunk schedules
    ///
    /// The tests above feed it uniformly sized chunks, which is the one shape a real reader
    /// never produces. This is the property that would otherwise want its own fuzz target, but
    /// `TailWithholder` is `pub(crate)` and so unreachable from the external fuzz crate — a
    /// proptest in here reaches it and shrinks better besides.
    #[cfg(feature = "v1")]
    mod properties {
        use proptest::prelude::*;

        use super::TailWithholder;

        /// How many bytes the withholder holds back, matching the `CaRT` footer
        const N: usize = 28;

        proptest! {
            /// Whatever the chunk schedule, everything comes out exactly once and in order, and
            /// the last N bytes only ever come out of `finish`
            #[test]
            fn the_tail_is_withheld_under_any_chunk_schedule(
                data in prop::collection::vec(any::<u8>(), 0..512),
                // sizes start at one so the schedule always makes progress; a schedule of all
                // zeros would spin the test rather than test anything
                chunks in prop::collection::vec(1_usize..40, 1..64),
            ) {
                let mut withholder = TailWithholder::<N>::new();
                let mut released = Vec::new();
                let mut fed = 0;
                // walk the data using the generated chunk sizes, cycling them if we run short
                for size in chunks.iter().cycle() {
                    // stop once everything has been fed in
                    if fed >= data.len() {
                        break;
                    }
                    // push an empty chunk before every real one. A reader handing over an empty
                    // slice is exactly the shape that used to reach the compressor and abort an
                    // upload, so the withholder has to be a no-op on it rather than a special
                    // case, and doing it here covers that on every generated schedule.
                    prop_assert_eq!(
                        {
                            let (_, carry_len, release_len) = withholder.push(&[]);
                            (carry_len, release_len)
                        },
                        (0, 0),
                        "an empty push released bytes",
                    );
                    let chunk = &data[fed..(fed + size).min(data.len())];
                    fed += chunk.len();
                    let (carry, carry_len, release_len) = withholder.push(chunk);
                    // evicted bytes come out before bytes from the new chunk, always
                    released.extend_from_slice(&carry[..carry_len]);
                    released.extend_from_slice(&chunk[..release_len]);
                    // and at no point may what has been released reach into the last N bytes
                    prop_assert!(
                        released.len() <= data.len().saturating_sub(N),
                        "released {} bytes of a {} byte stream",
                        released.len(),
                        data.len(),
                    );
                }
                // a chunk list of all zeros never feeds anything, so only assert completeness
                // once the whole stream actually went in
                prop_assert_eq!(fed, data.len(), "the chunk schedule did not feed everything");
                // the released bytes are exactly the stream minus its tail
                let split = data.len().saturating_sub(N);
                prop_assert_eq!(&released[..], &data[..split]);
                prop_assert_eq!(withholder.finish(), &data[split..]);
            }
        }
    }
}

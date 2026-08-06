//! Reading `CaRT` files
//!
//! [`UncartStream`] wraps an [`AsyncRead`] over a `CaRT` file and is itself an [`AsyncRead`]
//! over the original bytes.
//!
//! Unlike the write side it has **no version type parameter and takes no version from the
//! caller**. The version comes out of the file's own header, because the alternative is a
//! deployment whose config decides how to read objects it did not write — thread
//! `conf.cart_version` into the read path once and every previously stored file becomes
//! unreadable the day that value changes.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::{AsyncRead, ReadBuf};

use crate::codec::{Decode, MAX_DECODE_CHUNK, MAX_INPUT_CHUNK, OutBuf, new_decoder};
use crate::error::Error;
use crate::header::{HEADER_LEN, Header};
use crate::version::CartVersion;

/// An [`AsyncRead`] that uncarts another [`AsyncRead`] as it is read
///
/// This holds no pinned state of its own, so it is [`Unpin`] whatever the reader is, and it is
/// `Send + Sync` whenever the reader is. Four call sites hand it to `tokio_tar::Archive` or
/// `indicatif`, both of which need all three, so the test module pins that down with a
/// compile-time assertion — a regression should break this crate rather than four consumers.
pub struct UncartStream<R> {
    /// The reader holding the `CaRT` file
    reader: R,
    /// The codec for the version in the file's header, installed once that header is complete
    decoder: Option<Box<dyn Decode>>,
    /// The mandatory header, accumulated across however many reads it takes
    header: [u8; HEADER_LEN],
    /// How much of the mandatory header has been seen
    header_len: usize,
    /// Carted bytes read but not yet handed to the decoder, allocated on first poll
    input: Vec<u8>,
    /// Where the unfed carted bytes start
    start: usize,
    /// Where the unfed carted bytes end
    end: usize,
    /// Plaintext waiting to be read
    out: OutBuf,
    /// Whether the reader has hit end of file
    eof: bool,
    /// Whether the decoder has been told the stream ended
    done: bool,
}

impl<R> UncartStream<R> {
    /// Wrap a reader holding a `CaRT` file
    ///
    /// This is infallible on purpose. A malformed header is discovered on the first poll, not
    /// at construction, so wrapping a reader never needs a `?`.
    ///
    /// # Arguments
    ///
    /// * `reader` - The reader holding the `CaRT` file
    ///
    /// # Examples
    ///
    /// ```
    /// use cart_rs::UncartStream;
    ///
    /// let uncart = UncartStream::new(&b"not a cart file"[..]);
    /// # let _ = uncart;
    /// ```
    pub fn new(reader: R) -> Self {
        UncartStream {
            reader,
            decoder: None,
            header: [0; HEADER_LEN],
            header_len: 0,
            input: Vec::new(),
            start: 0,
            end: 0,
            out: OutBuf::new(),
            eof: false,
            done: false,
        }
    }

    /// The `CaRT` version this file turned out to be, once its header has been read
    ///
    /// This is `None` until enough of the file has been polled to parse the header.
    pub fn version(&self) -> Option<CartVersion> {
        self.decoder.as_ref().map(|decoder| decoder.version())
    }

    /// Get back the reader holding the `CaRT` file
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Hand the buffered carted bytes to the header parser or to the decoder
    ///
    /// This always consumes at least one buffered byte, which is what keeps the poll loop from
    /// spinning.
    ///
    /// # Errors
    ///
    /// Returns an error if the file is not a `CaRT`, is a version this build cannot read, or
    /// is corrupt.
    fn step(&mut self) -> Result<(), Error> {
        // there is no decoder to feed until the mandatory header is complete
        if self.decoder.is_none() {
            // top up the header, which may take several reads if the reader is unhelpful
            let take = (HEADER_LEN - self.header_len).min(self.end - self.start);
            self.header[self.header_len..self.header_len + take]
                .copy_from_slice(&self.input[self.start..self.start + take]);
            self.header_len += take;
            self.start += take;
            // wait for the rest of it before deciding what this file is
            if self.header_len < HEADER_LEN {
                return Ok(());
            }
            // the version comes from the file, never from the caller
            let header = Header::parse(&self.header)?;
            self.decoder = Some(new_decoder(&header)?);
            return Ok(());
        }
        // cap how much goes in at once so that one call can not be made to inflate into
        // hundreds of megabytes, however pathological the compression ratio is
        let take = (self.end - self.start).min(MAX_DECODE_CHUNK);
        let decoder = self
            .decoder
            .as_mut()
            .ok_or_else(|| Error::Generic("the CaRT decoder went missing".to_string()))?;
        let taken = decoder.push(&self.input[self.start..self.start + take], &mut self.out)?;
        // a decoder that took nothing from a non empty chunk would spin this loop forever, so
        // catch a codec bug here rather than pinning a worker thread
        if taken == 0 {
            return Err(Error::Generic(
                "the CaRT decoder stopped making progress".to_string(),
            ));
        }
        self.start += taken;
        Ok(())
    }
}

impl<R> std::fmt::Debug for UncartStream<R> {
    /// Write out a human readable version of this stream
    ///
    /// # Arguments
    ///
    /// * `f` - The formatter to write to
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UncartStream")
            .field("version", &self.version())
            .field("ready", &self.out.len())
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for UncartStream<R> {
    /// Read the original bytes, uncarting more of the underlying reader as needed
    ///
    /// # Arguments
    ///
    /// * `cx` - The context to poll the underlying reader with
    /// * `buf` - The buffer to write the original bytes into
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // nothing here is pinned, so the whole state machine runs off a plain &mut
        let this = self.get_mut();
        // a caller with no room can not be handed anything, and returning from the bottom of
        // this function having written nothing is indistinguishable from end of file
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        loop {
            // hand back whatever is already uncarted. this is the only place bytes leave the
            // buffer and it returns immediately, so no later branch can drop them on the floor
            if !this.out.is_empty() {
                let take = this.out.len().min(buf.remaining());
                buf.put_slice(&this.out.filled()[..take]);
                this.out.advance(take);
                return Poll::Ready(Ok(()));
            }
            // with the buffer drained and the codec closed out, this is the one real end of file
            if this.done {
                return Poll::Ready(Ok(()));
            }
            // work through the carted bytes already in hand before asking for more
            if this.start < this.end {
                this.step()?;
                continue;
            }
            // the buffer is empty, so either close the file out or go get more of it
            if this.eof {
                match this.decoder.as_mut() {
                    // this is where truncation is caught, since a codec that never saw the end
                    // of its body has to say so rather than pretend the file was whole
                    Some(decoder) => decoder.finish(&mut this.out)?,
                    // the file ended before even the mandatory header was complete
                    None => Err(Error::Truncated { section: "header" })?,
                }
                this.done = true;
                continue;
            }
            // give the reader somewhere to land the first time we need it
            if this.input.is_empty() {
                this.input.resize(MAX_INPUT_CHUNK, 0);
            }
            // pull in more carted bytes, leaving the waker to the reader if it is not ready
            let mut input = ReadBuf::new(this.input.as_mut_slice());
            ready!(Pin::new(&mut this.reader).poll_read(cx, &mut input))?;
            let filled = input.filled().len();
            this.start = 0;
            this.end = filled;
            this.eof = filled == 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncReadExt;

    use super::UncartStream;
    use crate::cart::CartStream;
    use crate::key::CartKey;
    use crate::version::CartVersion;

    /// Every `CaRT` version this build can actually write
    const VERSIONS: &[CartVersion] = &[
        #[cfg(feature = "v1")]
        CartVersion::V1,
        #[cfg(feature = "v2")]
        CartVersion::V2,
    ];

    /// Build the key every test carts with
    fn key() -> CartKey {
        CartKey::from_password("SecretCornIsBest").unwrap()
    }

    /// Build a plaintext of the given length that does not compress to nothing
    fn plaintext(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    /// Cart a plaintext through the streaming API
    ///
    /// # Arguments
    ///
    /// * `plain` - The plaintext to cart
    /// * `version` - The version to cart with
    async fn cart(plain: &[u8], version: CartVersion) -> Vec<u8> {
        let mut stream = CartStream::builder(key(), plain)
            .version(version)
            .build()
            .unwrap();
        let mut carted = Vec::new();
        stream.read_to_end(&mut carted).await.unwrap();
        carted
    }

    #[tokio::test]
    /// Both versions round trip at sizes that straddle every internal boundary
    async fn both_versions_round_trip() {
        for &version in VERSIONS {
            for len in [0, 1, 37, 38, 39, 1023, 1024, 1025, 300_000, 3_000_000] {
                let plain = plaintext(len);
                let carted = cart(&plain, version).await;
                let mut uncart = UncartStream::new(carted.as_slice());
                let mut plain_again = Vec::new();
                uncart.read_to_end(&mut plain_again).await.unwrap();
                assert_eq!(plain, plain_again, "{version} at {len} bytes");
                assert_eq!(uncart.version(), Some(version));
            }
        }
    }

    #[tokio::test]
    /// A header split across reads is not mistaken for a corrupt file
    async fn a_dribbled_header_still_parses() {
        for &version in VERSIONS {
            let plain = plaintext(10_000);
            let carted = cart(&plain, version).await;
            // hand the whole file over one byte at a time, which is the shape that used to
            // report a perfectly good file as malformed
            let mut uncart = UncartStream::new(Dribble {
                data: carted,
                pos: 0,
            });
            let mut plain_again = Vec::new();
            uncart.read_to_end(&mut plain_again).await.unwrap();
            assert_eq!(plain, plain_again, "{version}");
        }
    }

    #[tokio::test]
    /// A truncated file is an error rather than a short read
    async fn a_truncated_file_is_rejected() {
        for &version in VERSIONS {
            let plain = plaintext(10_000);
            let carted = cart(&plain, version).await;
            // every prefix of a CaRT file is either an error or, at full length, the file
            for cut in [0, 1, 20, 37, 38, 39, 100, carted.len() - 1] {
                let mut uncart = UncartStream::new(&carted[..cut]);
                let mut plain_again = Vec::new();
                assert!(
                    uncart.read_to_end(&mut plain_again).await.is_err(),
                    "{version} truncated to {cut} bytes was accepted"
                );
            }
        }
    }

    #[tokio::test]
    /// Something that is not a `CaRT` file at all is rejected
    async fn junk_is_rejected() {
        let mut uncart = UncartStream::new(&b"this is not a CaRT file, not even a little"[..]);
        let mut plain = Vec::new();
        assert!(uncart.read_to_end(&mut plain).await.is_err());
    }

    #[cfg(feature = "v1")]
    #[tokio::test]
    /// A caller with no room is not told the stream ended
    async fn a_full_buffer_is_not_end_of_file() {
        let carted = cart(b"EvilCorn\n", CartVersion::V1).await;
        let mut uncart = UncartStream::new(carted.as_slice());
        assert_eq!(uncart.read(&mut []).await.unwrap(), 0);
        let mut plain = Vec::new();
        uncart.read_to_end(&mut plain).await.unwrap();
        assert_eq!(plain, b"EvilCorn\n");
    }

    #[test]
    /// The stream stays usable by `tokio_tar` and `indicatif`
    fn the_stream_is_send_sync_and_unpin() {
        /// Fail to compile if `T` lost one of the auto traits our consumers need
        const fn assert<T: Send + Sync + Unpin>() {}
        assert::<UncartStream<tokio::io::Empty>>();
    }

    /// A reader that hands over exactly one byte per poll
    struct Dribble {
        /// The bytes to dribble out
        data: Vec<u8>,
        /// How many have been handed over
        pos: usize,
    }

    impl tokio::io::AsyncRead for Dribble {
        /// Hand over a single byte
        ///
        /// # Arguments
        ///
        /// * `_cx` - The context to poll with, unused since this reader is never pending
        /// * `buf` - The buffer to write the byte into
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            // stop at the end of the data, and never fill a buffer with no room
            if self.pos < self.data.len() && buf.remaining() > 0 {
                buf.put_slice(&[self.data[self.pos]]);
                self.pos += 1;
            }
            std::task::Poll::Ready(Ok(()))
        }
    }
}

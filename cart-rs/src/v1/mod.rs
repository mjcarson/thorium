//! The V1 `CaRT` codec: RC4 + zlib wrapped DEFLATE
//!
//! V1 is the official `CaRT` format, so this half of the crate exists to stay byte compatible
//! with every other `CaRT` tool rather than to be fast or clever. A V1 file looks like this:
//!
//! ```text
//! [0..38)                 mandatory header, plaintext
//! [38..38+opt_len)        optional header,  RC4       JSON metadata
//! [38+opt_len..pos)       body,             RC4       a zlib stream
//! [pos..pos+len)          optional footer,  RC4       JSON hashes
//! last 28 bytes           mandatory footer, plaintext "TRAC" + pos + len
//! ```
//!
//! Two properties of that layout drive everything in here:
//!
//! * **Every section restarts the RC4 keystream.** A single continuous RC4 from offset 38
//!   decrypts the optional header and then yields garbage for the body. This is why cart-rs
//!   could not read a file the reference tool produced; see `tests/data/corn.v1.cart`.
//! * **Nothing marks where the zlib stream ends.** The optional footer and the mandatory
//!   footer are simply glued on after it. The old reader handed those trailing bytes straight
//!   to `inflate` and got away with it only because `miniz_oxide` tolerates them — a property
//!   that evaporated the moment a second DEFLATE backend entered the build. Here a
//!   [`TailWithholder`] makes it structurally impossible for the mandatory footer to reach the
//!   decompressor, and the decoder stops feeding it the instant it reports `StreamEnd`.
//!
//! `miniz_oxide` is used directly, in both directions, rather than through `flate2`. Cargo
//! features are additive, so a crate anywhere else in the workspace turning on a different
//! `flate2` backend would silently change the bytes this codec writes. A direct dependency
//! cannot be swapped out from under us.

use miniz_oxide::deflate::core::{CompressorOxide, create_comp_flags_from_zip_params};
use miniz_oxide::deflate::stream::deflate;
use miniz_oxide::inflate::stream::{InflateState, inflate};
use miniz_oxide::{DataFormat, MZError, MZFlush, MZStatus};
use rc4::consts::U16;
use rc4::{KeyInit, Rc4, StreamCipher};

use crate::codec::{Decode, Encode, MAX_DECODE_CHUNK, OutBuf, SOFT_OUTPUT_CAP, TailWithholder};
use crate::error::Error;
use crate::footer::{FOOTER_LEN, Footer};
use crate::header::{HEADER_LEN, Header};
use crate::key::CartKey;
use crate::version::{CartVersion, V1Options};

/// The window size DEFLATE uses, as a base 2 log
///
/// A positive value tells `miniz_oxide` to wrap the DEFLATE stream in a zlib header and Adler-32
/// trailer, which is what `CaRT` stores.
const WINDOW_BITS: i32 = 15;

/// The DEFLATE strategy to compress with
///
/// 0 is the default strategy, which is what every other `CaRT` implementation uses.
const STRATEGY: i32 = 0;

/// The smallest writable region the decoder will ask for before calling `inflate`
const MIN_INFLATE_ROOM: usize = 64 * 1024;

/// Build the RC4 cipher for one `CaRT` section
///
/// Each section restarts the keystream, so this is called once per section rather than once per
/// file.
///
/// # Arguments
///
/// * `key` - The key from the `CaRT` header
fn section_cipher(key: &CartKey) -> Rc4<U16> {
    // CartKey is always exactly KEY_LEN bytes long, so this cannot fail
    Rc4::<U16>::new(key.as_bytes().into())
}

/// The V1 encoder: zlib compress, then RC4 encrypt
pub(crate) struct V1Encoder {
    /// The RC4 cipher encrypting the body
    rc4: Rc4<U16>,
    /// The DEFLATE compressor
    ///
    /// This is boxed because `CompressorOxide` holds a 32 KiB dictionary plus its hash tables
    /// and has no business living inline in a future.
    deflate: Box<CompressorOxide>,
    /// The header we have not written out yet
    ///
    /// The header is written lazily so that a caller who builds a cart and then drops it
    /// without ever pushing anything produces nothing rather than a headerless stub.
    header: Option<Header>,
    /// Whether the footer has already been written
    finished: bool,
}

impl std::fmt::Debug for V1Encoder {
    /// Format this encoder without trying to format a cipher or a compressor
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("V1Encoder")
            .field("header_written", &self.header.is_none())
            .field("finished", &self.finished)
            .finish()
    }
}

impl V1Encoder {
    /// Build a V1 encoder
    ///
    /// # Arguments
    ///
    /// * `key` - The key to encrypt this cart with
    /// * `options` - The V1 specific options to compress with
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidOption`] if the DEFLATE level is out of range.
    pub(crate) fn new(key: CartKey, options: V1Options) -> Result<Self, Error> {
        // refuse a level DEFLATE does not have rather than silently clamping it
        options.validate()?;
        // build the compressor, asking for a zlib wrapper around the DEFLATE stream
        let flags = create_comp_flags_from_zip_params(
            i32::from(options.deflate_level),
            WINDOW_BITS,
            STRATEGY,
        );
        Ok(V1Encoder {
            rc4: section_cipher(&key),
            deflate: Box::new(CompressorOxide::new(flags)),
            // cart-rs writes no optional header, so the body starts at HEADER_LEN
            header: Some(Header::new(CartVersion::V1, key)),
            finished: false,
        })
    }

    /// Write the mandatory header out if it has not been written yet
    ///
    /// # Arguments
    ///
    /// * `out` - The buffer to append the header to
    fn write_header(&mut self, out: &mut OutBuf) {
        if let Some(header) = self.header.take() {
            out.extend(&header.to_bytes());
        }
    }

    /// Compress into `out` and RC4 encrypt exactly what was produced
    ///
    /// Returns `(bytes_consumed, bytes_written, stream_ended)`.
    ///
    /// # Arguments
    ///
    /// * `input` - The plaintext to compress
    /// * `flush` - The DEFLATE flush mode to use
    /// * `out` - The buffer to append encrypted compressed bytes to
    ///
    /// # Errors
    ///
    /// Returns [`Error::Generic`] if the compressor reports a fatal error.
    fn deflate_into(
        &mut self,
        input: &[u8],
        flush: MZFlush,
        out: &mut OutBuf,
    ) -> Result<(usize, usize, bool), Error> {
        // DEFLATE can expand incompressible data slightly, so ask for more room than the input
        let want = input.len() + (input.len() / 8) + 1024;
        let room = out.writable(want);
        // compress into that room
        let result = deflate(&mut self.deflate, input, room, flush);
        // and encrypt exactly the bytes it produced, in order, exactly once
        self.rc4.apply_keystream(&mut room[..result.bytes_written]);
        out.commit(result.bytes_written);
        match result.status {
            Ok(MZStatus::Ok) => Ok((result.bytes_consumed, result.bytes_written, false)),
            Ok(MZStatus::StreamEnd) => Ok((result.bytes_consumed, result.bytes_written, true)),
            // NeedDict is a decompression status and cannot come back from deflate
            Ok(MZStatus::NeedDict) => Err(Error::Generic(
                "the DEFLATE compressor asked for a preset dictionary".to_string(),
            )),
            // Buf only means no progress was possible on this call, which is recoverable
            Err(MZError::Buf) => Ok((result.bytes_consumed, result.bytes_written, false)),
            Err(err) => Err(Error::Generic(format!("DEFLATE failed: {err:?}"))),
        }
    }
}

impl Encode for V1Encoder {
    /// The `CaRT` version this codec writes
    fn version(&self) -> CartVersion {
        CartVersion::V1
    }

    /// Feed plaintext in, appending carted bytes to `out`
    ///
    /// # Arguments
    ///
    /// * `input` - The plaintext to cart
    /// * `out` - The buffer to append carted bytes to
    ///
    /// # Errors
    ///
    /// Returns [`Error::AlreadyFinished`] if the footer has already been written, or
    /// [`Error::Generic`] if the compressor fails.
    fn push(&mut self, input: &[u8], out: &mut OutBuf) -> Result<(), Error> {
        // nothing may follow the footer
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.write_header(out);
        // an empty chunk is a no-op; handing one to the compressor is what used to turn a
        // zero length chunk from a multipart body into a fatal Buf error
        if input.is_empty() {
            return Ok(());
        }
        // feed the compressor until it has taken everything, growing `out` as needed
        let mut remaining = input;
        while !remaining.is_empty() {
            let (consumed, written, ended) = self.deflate_into(remaining, MZFlush::None, out)?;
            // the stream cannot end before we ask it to
            if ended {
                return Err(Error::Generic(
                    "the DEFLATE stream ended before it was flushed".to_string(),
                ));
            }
            // `out` grew this round, so making no progress at all means the compressor is stuck
            if consumed == 0 && written == 0 {
                return Err(Error::Generic(
                    "the DEFLATE compressor stopped making progress".to_string(),
                ));
            }
            remaining = &remaining[consumed..];
        }
        Ok(())
    }

    /// Flush the compressor and write the mandatory footer
    ///
    /// # Arguments
    ///
    /// * `out` - The buffer to append the rest of the cart to
    ///
    /// # Errors
    ///
    /// Returns [`Error::AlreadyFinished`] if this was already called, or [`Error::Generic`] if
    /// the compressor fails or stops making progress before the stream is complete.
    fn finish(&mut self, out: &mut OutBuf) -> Result<(), Error> {
        // finishing twice would write two footers
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        // a cart with no body at all still needs its header
        self.write_header(out);
        // flush until the compressor says the DEFLATE stream is complete. This loops rather
        // than calling finish once and hoping, because the old writer did exactly that and
        // could append a valid looking footer over a truncated body.
        loop {
            let (_, written, ended) = self.deflate_into(&[], MZFlush::Finish, out)?;
            if ended {
                break;
            }
            // `out` grows every round, so a round that produced nothing is not "short of room"
            if written == 0 {
                return Err(Error::Generic(
                    "the DEFLATE compressor stopped making progress while finishing".to_string(),
                ));
            }
        }
        // the mandatory footer is plaintext and cart-rs writes no optional footer
        out.extend(&Footer::new().to_bytes());
        self.finished = true;
        Ok(())
    }
}

/// Which section of a V1 file the decoder is reading
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum V1State {
    /// Skipping the optional header
    ///
    /// cart-rs does not need the JSON metadata in here, but its length still moves the body.
    OptHeader {
        /// How many bytes of the optional header are left to skip
        remaining: u64,
    },
    /// Decrypting and inflating the body
    Body,
    /// The zlib stream is complete, so everything left is the optional footer
    Trailing,
}

/// The V1 decoder: RC4 decrypt, then zlib decompress
pub(crate) struct V1Decoder {
    /// The RC4 cipher decrypting the body
    ///
    /// Sections do not share a keystream, so this is a cipher of its own rather than a
    /// continuation of whatever decrypted the optional header.
    rc4: Rc4<U16>,
    /// The DEFLATE decompressor
    inflate: Box<InflateState>,
    /// Scratch space holding decrypted bytes on their way to the decompressor
    ///
    /// RC4 works in place, so the caller's slice cannot be decrypted directly. This is capped
    /// at [`MAX_DECODE_CHUNK`] by the loop in [`V1Decoder::consume`].
    scratch: Vec<u8>,
    /// Holds the mandatory footer back so it can never reach RC4 or the decompressor
    withholder: TailWithholder<FOOTER_LEN>,
    /// Which section of the file is being read
    state: V1State,
    /// The absolute file offset of the next byte to be consumed
    offset: u64,
    /// The absolute file offset the zlib stream ended at, once it has
    body_end: Option<u64>,
}

impl std::fmt::Debug for V1Decoder {
    /// Format this decoder without trying to format a cipher or a decompressor
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("V1Decoder")
            .field("state", &self.state)
            .field("offset", &self.offset)
            .field("body_end", &self.body_end)
            .finish()
    }
}

impl V1Decoder {
    /// Build a V1 decoder for a file whose header has already been parsed
    ///
    /// The caller is expected to have consumed exactly [`HEADER_LEN`] bytes; everything after
    /// that goes to [`Decode::push`].
    ///
    /// # Arguments
    ///
    /// * `header` - The parsed mandatory header
    pub(crate) fn new(header: &Header) -> Self {
        V1Decoder {
            rc4: section_cipher(&header.key),
            inflate: InflateState::new_boxed(DataFormat::Zlib),
            scratch: Vec::new(),
            withholder: TailWithholder::new(),
            // a file with no optional header starts its body immediately
            state: if header.opt_len == 0 {
                V1State::Body
            } else {
                V1State::OptHeader {
                    remaining: header.opt_len,
                }
            },
            offset: HEADER_LEN as u64,
            body_end: None,
        }
    }

    /// Route a run of bytes that are safe to release into whichever section they belong to
    ///
    /// # Arguments
    ///
    /// * `data` - The bytes to route
    /// * `out` - The buffer to append plaintext to
    ///
    /// # Errors
    ///
    /// Returns [`Error::Corrupt`] if the zlib stream is invalid.
    fn consume(&mut self, mut data: &[u8], out: &mut OutBuf) -> Result<(), Error> {
        while !data.is_empty() {
            match self.state {
                V1State::OptHeader { remaining } => {
                    // skip the metadata, but count it so the body lands at the right offset
                    let take = usize::try_from(remaining)
                        .unwrap_or(usize::MAX)
                        .min(data.len());
                    self.offset += take as u64;
                    data = &data[take..];
                    // once it has all gone by the body starts, with a fresh keystream
                    self.state = match remaining - take as u64 {
                        0 => V1State::Body,
                        left => V1State::OptHeader { remaining: left },
                    };
                }
                V1State::Body => {
                    // work in bounded slices so the scratch buffer stays small however much
                    // the caller handed us at once
                    let take = data.len().min(MAX_DECODE_CHUNK);
                    self.inflate_body(&data[..take], out)?;
                    data = &data[take..];
                }
                V1State::Trailing => {
                    // everything after the zlib stream is the optional footer, which holds
                    // only hashes of the file we are already handing back byte for byte
                    self.offset += data.len() as u64;
                    data = &[];
                }
            }
        }
        Ok(())
    }

    /// Decrypt a slice of the body and inflate as much of it as belongs to the zlib stream
    ///
    /// # Arguments
    ///
    /// * `data` - The encrypted body bytes to decrypt and inflate
    /// * `out` - The buffer to append plaintext to
    ///
    /// # Errors
    ///
    /// Returns [`Error::Corrupt`] if the zlib stream is invalid or stops making progress.
    fn inflate_body(&mut self, data: &[u8], out: &mut OutBuf) -> Result<(), Error> {
        // RC4 works in place, so decrypt into scratch rather than into the caller's slice
        self.scratch.clear();
        self.scratch.extend_from_slice(data);
        self.rc4.apply_keystream(&mut self.scratch);
        // and inflate whatever of it belongs to the body
        let mut pos = 0;
        while pos < self.scratch.len() {
            // DEFLATE can expand a great deal, so ask for several times the input and loop
            let want = (self.scratch.len() - pos)
                .saturating_mul(4)
                .max(MIN_INFLATE_ROOM);
            let room = out.writable(want);
            let result = inflate(&mut self.inflate, &self.scratch[pos..], room, MZFlush::None);
            out.commit(result.bytes_written);
            pos += result.bytes_consumed;
            match result.status {
                Ok(MZStatus::Ok) => (),
                Ok(MZStatus::StreamEnd) => {
                    // stop here. Nothing past the end of the zlib stream is ever handed to the
                    // decompressor, whatever a given backend would have done with it.
                    self.body_end = Some(self.offset + pos as u64);
                    self.state = V1State::Trailing;
                    return Ok(());
                }
                Ok(MZStatus::NeedDict) => {
                    return Err(Error::Corrupt(
                        "the zlib stream needs a preset dictionary".to_string(),
                    ));
                }
                // Buf only means no progress was possible on this call
                Err(MZError::Buf) => (),
                Err(err) => {
                    return Err(Error::Corrupt(format!(
                        "the zlib stream is invalid: {err:?}"
                    )));
                }
            }
            // `out` grew this round, so making no progress at all means the stream is stuck
            if result.bytes_consumed == 0 && result.bytes_written == 0 {
                return Err(Error::Corrupt(
                    "the zlib stream stopped making progress".to_string(),
                ));
            }
        }
        self.offset += data.len() as u64;
        Ok(())
    }
}

impl Decode for V1Decoder {
    /// The `CaRT` version this codec reads
    fn version(&self) -> CartVersion {
        CartVersion::V1
    }

    /// Feed carted bytes in, appending plaintext to `out`, and report how many were taken
    ///
    /// # Arguments
    ///
    /// * `input` - The carted bytes to uncart
    /// * `out` - The buffer to append plaintext to
    ///
    /// # Errors
    ///
    /// Returns [`Error::Corrupt`] if the zlib stream is invalid.
    fn push(&mut self, input: &[u8], out: &mut OutBuf) -> Result<usize, Error> {
        let mut taken = 0;
        while taken < input.len() {
            // stop taking input once the caller has plenty to read, but only after making at
            // least one step of progress so that re-pushing the remainder cannot livelock
            if taken > 0 && out.len() >= SOFT_OUTPUT_CAP {
                break;
            }
            // work in bounded steps so one call cannot be made to inflate an unbounded amount
            let step = (input.len() - taken).min(MAX_DECODE_CHUNK);
            let chunk = &input[taken..taken + step];
            // hold the last 28 bytes back so the mandatory footer cannot reach the decompressor
            let (carry, carry_len, release_len) = self.withholder.push(chunk);
            // the bytes evicted from the held tail came in first, so they go out first
            self.consume(&carry[..carry_len], out)?;
            self.consume(&chunk[..release_len], out)?;
            taken += step;
        }
        Ok(taken)
    }

    /// Tell the decoder the file has ended and check that it really was a whole file
    ///
    /// # Arguments
    ///
    /// * `out` - Unused; V1 buffers no plaintext across calls
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] if the file ended before the zlib stream did,
    /// [`Error::MissingFooter`] if the last 28 bytes are not a `CaRT` footer, and
    /// [`Error::Malformed`] if the footer disagrees with where the body actually ended.
    fn finish(&mut self, _out: &mut OutBuf) -> Result<(), Error> {
        // whatever is still held back has to be the mandatory footer
        let footer = Footer::parse(self.withholder.finish())?;
        // and the zlib stream has to have finished, or this file was cut short
        let body_end = match self.state {
            V1State::OptHeader { .. } => {
                return Err(Error::Truncated {
                    section: "optional header",
                });
            }
            _ => self.body_end.ok_or(Error::Truncated { section: "body" })?,
        };
        // a file with an optional footer says where it starts, which is where the body stopped
        if footer.opt_footer_pos != 0 && footer.opt_footer_pos != body_end {
            return Err(Error::Malformed(format!(
                "the zlib stream ended at byte {body_end} but the footer puts the end of the \
                 body at byte {}",
                footer.opt_footer_pos
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{V1Decoder, V1Encoder};
    use crate::codec::{Decode, Encode, OutBuf};
    use crate::error::Error;
    use crate::header::Header;
    use crate::key::CartKey;
    use crate::version::V1Options;

    /// The `cart corn` file the reference Python tool produced
    ///
    /// This is a compat vector: it pins the *format*, not our encoder, so it is never
    /// regenerated. A failure here means V1 compatibility is broken.
    const REFERENCE_CART: &[u8] = include_bytes!("../../tests/data/corn.v1.cart");

    /// Our own V1 output, pinned byte for byte
    ///
    /// This is a *different kind* of fixture to [`REFERENCE_CART`] and conflating the two is
    /// how a team ends up regenerating a vector to make CI green. V1 compatibility is about
    /// files being *decodable*, so our encoder is free to emit different bytes; this canary
    /// exists precisely to make that visible. If it fails, the DEFLATE backend changed —
    /// confirm the change was deliberate, check `an_independent_zlib_accepts_our_output` still
    /// passes, and only then regenerate. This is the test that would have caught the macOS
    /// backend split the day it was introduced.
    const CANARY_CART: &[u8] = include_bytes!("../../tests/data/canary.v1.cart");

    /// The plaintext [`CANARY_CART`] was built from
    ///
    /// Generated rather than committed so the fixture stays small. Knuth's multiplicative hash
    /// gives something that compresses a bit but not trivially.
    fn canary_plaintext() -> Vec<u8> {
        (0..70_000_u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
            .collect()
    }

    /// The key everything in the tree uses
    fn key() -> CartKey {
        CartKey::from_password("SecretCornIsBest").unwrap()
    }

    /// Cart a slice, handing it over in chunks of `chunk` bytes
    fn cart(plain: &[u8], chunk: usize, options: V1Options) -> Vec<u8> {
        let mut encoder = V1Encoder::new(key(), options).unwrap();
        let mut out = OutBuf::new();
        for slice in plain.chunks(chunk.max(1)) {
            encoder.push(slice, &mut out).unwrap();
        }
        encoder.finish(&mut out).unwrap();
        out.filled().to_vec()
    }

    /// Uncart a whole `CaRT` file, handing it over in chunks of `chunk` bytes
    fn uncart(carted: &[u8], chunk: usize) -> Result<Vec<u8>, Error> {
        let header = Header::parse(carted)?;
        let mut decoder = V1Decoder::new(&header);
        let mut out = OutBuf::new();
        let mut plain = Vec::new();
        for slice in carted[38..].chunks(chunk.max(1)) {
            // the decoder is allowed to take only part of a chunk, so keep offering the rest
            let mut rest = slice;
            while !rest.is_empty() {
                let taken = decoder.push(rest, &mut out)?;
                assert_ne!(taken, 0, "the decoder took nothing from a non empty chunk");
                rest = &rest[taken..];
                // drain as we go so the reader side's incremental behaviour is exercised
                plain.extend_from_slice(out.filled());
                out.advance(out.len());
            }
        }
        decoder.finish(&mut out)?;
        plain.extend_from_slice(out.filled());
        Ok(plain)
    }

    /// The compat vector: a file with an optional header, an optional footer, and a per section
    /// RC4 reset, which is everything cart-rs used to get wrong
    #[test]
    fn the_reference_tools_cart_decodes() {
        for chunk in [1_usize, 7, 27, 28, 29, 64, 241, 4096] {
            assert_eq!(
                uncart(REFERENCE_CART, chunk).unwrap(),
                b"EvilCorn\n",
                "failed with chunks of {chunk} bytes"
            );
        }
    }

    /// The body of the reference cart is 17 bytes and the optional footer is 181, so a decoder
    /// that let either the optional footer or the mandatory footer reach `inflate` would notice
    #[test]
    fn the_optional_and_mandatory_footers_never_reach_the_decompressor() {
        let header = Header::parse(REFERENCE_CART).unwrap();
        let mut decoder = V1Decoder::new(&header);
        let mut out = OutBuf::new();
        decoder.push(&REFERENCE_CART[38..], &mut out).unwrap();
        decoder.finish(&mut out).unwrap();
        assert_eq!(out.filled(), b"EvilCorn\n");
        // and the decoder worked out exactly where the body stopped, from the zlib stream alone
        assert_eq!(decoder.body_end, Some(70));
    }

    /// The backend drift canary
    ///
    /// See [`CANARY_CART`] for what a failure here means and does not mean.
    #[test]
    fn our_output_is_byte_for_byte_what_it_was() {
        let carted = cart(&canary_plaintext(), 8192, V1Options::default());
        assert_eq!(
            carted, CANARY_CART,
            "the V1 encoder emitted different bytes than it used to. This is not necessarily a \
             compatibility break, but it does mean the DEFLATE backend or its parameters \
             changed. Confirm that was intentional, then regenerate tests/data/canary.v1.cart."
        );
        // and it is still our own output, so it must still decode
        assert_eq!(uncart(CANARY_CART, 8192).unwrap(), canary_plaintext());
    }

    #[test]
    fn round_trips_at_every_interesting_size() {
        for len in [0_usize, 1, 27, 28, 29, 37, 38, 39, 1024, 65_536, 300_000] {
            let plain: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            let carted = cart(&plain, 4096, V1Options::default());
            assert_eq!(
                uncart(&carted, 4096).unwrap(),
                plain,
                "failed at {len} bytes"
            );
        }
    }

    /// The chunk schedule must not change a single byte of the output, in either direction
    #[test]
    fn the_chunk_schedule_does_not_change_the_bytes() {
        let plain: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
        let reference = cart(&plain, 100_000, V1Options::default());
        for chunk in [1_usize, 3, 1024, 8192, 65_536] {
            assert_eq!(
                cart(&plain, chunk, V1Options::default()),
                reference,
                "carting in {chunk} byte chunks changed the output"
            );
            assert_eq!(
                uncart(&reference, chunk).unwrap(),
                plain,
                "uncarting in {chunk} byte chunks changed the output"
            );
        }
    }

    /// Uploading an empty file used to be a guaranteed 500
    #[test]
    fn a_zero_byte_file_carts_and_uncarts() {
        let carted = cart(&[], 4096, V1Options::default());
        assert_eq!(&carted[..4], b"CART");
        assert_eq!(&carted[carted.len() - 28..carted.len() - 24], b"TRAC");
        assert!(uncart(&carted, 4096).unwrap().is_empty());
    }

    /// A zero length chunk in the middle of a body used to abort the whole upload
    #[test]
    fn empty_chunks_are_a_no_op() {
        let mut encoder = V1Encoder::new(key(), V1Options::default()).unwrap();
        let mut out = OutBuf::new();
        encoder.push(&[], &mut out).unwrap();
        encoder.push(b"corn", &mut out).unwrap();
        encoder.push(&[], &mut out).unwrap();
        encoder.push(b" is best", &mut out).unwrap();
        encoder.push(&[], &mut out).unwrap();
        encoder.finish(&mut out).unwrap();
        assert_eq!(uncart(out.filled(), 4096).unwrap(), b"corn is best");
    }

    #[test]
    fn every_deflate_level_round_trips() {
        let plain: Vec<u8> = (0..50_000).map(|i| (i % 97) as u8).collect();
        for deflate_level in 0..=9 {
            let carted = cart(&plain, 8192, V1Options { deflate_level });
            assert_eq!(
                uncart(&carted, 8192).unwrap(),
                plain,
                "failed at DEFLATE level {deflate_level}"
            );
        }
        assert!(V1Encoder::new(key(), V1Options { deflate_level: 10 }).is_err());
    }

    #[test]
    fn finishing_twice_is_an_error() {
        let mut encoder = V1Encoder::new(key(), V1Options::default()).unwrap();
        let mut out = OutBuf::new();
        encoder.finish(&mut out).unwrap();
        assert!(matches!(
            encoder.finish(&mut out),
            Err(Error::AlreadyFinished)
        ));
        assert!(matches!(
            encoder.push(b"corn", &mut out),
            Err(Error::AlreadyFinished)
        ));
    }

    /// A failed or interrupted upload leaves a truncated object in S3, and reading one back
    /// must be an error rather than a short file
    #[test]
    fn truncated_files_are_rejected() {
        let plain: Vec<u8> = (0..20_000).map(|i| (i % 251) as u8).collect();
        let carted = cart(&plain, 4096, V1Options::default());
        // every prefix of a valid cart is an invalid cart, so just check all of them
        for cut in 0..carted.len() {
            assert!(
                uncart(&carted[..cut], 4096).is_err(),
                "a file cut to {cut} of {} bytes should not decode",
                carted.len()
            );
        }
        // and the whole thing still decodes
        assert_eq!(uncart(&carted, 4096).unwrap(), plain);
    }

    /// Corrupting the body must fail rather than hand back the wrong bytes
    #[test]
    fn a_corrupted_body_is_rejected() {
        let plain: Vec<u8> = (0..20_000).map(|i| (i % 251) as u8).collect();
        let carted = cart(&plain, 4096, V1Options::default());
        let mut damaged = carted.clone();
        // flip a bit in the middle of the body
        damaged[carted.len() / 2] ^= 0x40;
        // V1 has no authentication tag, so the honest claim is only that we do not hand back
        // the original bytes while pretending everything was fine
        if let Ok(recovered) = uncart(&damaged, 4096) {
            assert_ne!(recovered, plain);
        }
    }

    /// A file whose footer says the body ended somewhere else has been tampered with
    #[test]
    fn a_footer_that_disagrees_with_the_body_is_rejected() {
        let plain = b"EvilCorn\n";
        let mut carted = cart(plain, 4096, V1Options::default());
        let len = carted.len();
        // point opt_footer_pos at a byte the zlib stream did not end on
        carted[len - 16..len - 8].copy_from_slice(&7_u64.to_le_bytes());
        assert!(matches!(uncart(&carted, 4096), Err(Error::Malformed(_))));
    }

    /// Our own output is what the rest of the crate pins its golden vector against, so make
    /// sure the header and footer are exactly where they should be
    #[test]
    fn our_output_has_the_layout_we_claim() {
        let carted = cart(b"EvilCorn\n", 4096, V1Options::default());
        let header = Header::parse(&carted).unwrap();
        assert_eq!(header.version, crate::version::CartVersion::V1);
        assert_eq!(header.opt_len, 0);
        assert_eq!(header.reserved, [0; 8]);
        assert_eq!(header.key, key());
        let footer = crate::footer::Footer::parse_tail(&carted).unwrap();
        assert_eq!(footer, crate::footer::Footer::new());
    }
}

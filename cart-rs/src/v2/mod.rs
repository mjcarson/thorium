//! The V2 `CaRT` codec: AES-128-GCM + Zstd
//!
//! V2 is a Thorium specific format. Nothing else reads it, which is exactly why it is allowed to
//! be framed rather than a single opaque stream:
//!
//! ```text
//! [0..38)   mandatory header, plaintext
//!           [6..14) is the per file random nonce salt, which V1 leaves as 8 reserved zeros
//!           opt_len is always 16
//! [38..54)  optional header, plaintext
//!           [0]     block_size_log2  u8
//!           [1]     flags            u8   bit0 = independent frames, bit1 = index present
//!           [2..4)  reserved         u16 LE
//!           [4..8)  reserved         u32 LE
//!           [8..16) index_offset     u64 LE, 0 = none
//! then repeated blocks, each:
//!           [0..4)  sealed_len       u32 LE = zstd frame length + 16, never 0xFFFF_FFFF
//!           [4..)   AES-128-GCM ciphertext of one complete, independent Zstd frame
//!           [..)    16 byte GCM tag
//! then      0xFFFF_FFFF                   the end marker
//! then      a sealed trailer block, laid out like any other block, whose 16 byte plaintext is
//!           block_count u64 LE || total_plain_len u64 LE, and which is not compressed
//! last 28   mandatory footer, plaintext, so the file is still identifiable as a `CaRT`
//! ```
//!
//! Every design choice in there is paying off a specific defect in the abandoned first attempt:
//!
//! * **The salt.** The nonce used to be `"FliffyIs" || counter`, so with one deployment wide key
//!   every file reused the same (key, nonce) pairs. That does not leak file contents — `CaRT`
//!   stores its key in the clear on purpose — but GCM nonce reuse with known plaintext hands
//!   over the GHASH key, so the tag authenticated nothing. `nonce = salt || block_index_be`
//!   fixes it for free, in bytes the format was already wasting.
//! * **The AAD.** It used to be empty, so blocks could be reordered, duplicated, or dropped and
//!   every tag still verified. It is now `header || optional header || block index`.
//! * **The end marker.** End of stream used to be detected by comparing a length prefix against
//!   `b"TRAC"`, which was safe only because that happens to be 1,128,878,676 as a `u32`. A
//!   reserved length value cannot collide with a real one.
//! * **Length delimited blocks.** The decompressor is only ever handed the plaintext of an
//!   authenticated block whose length was known before a byte of it was read, so the footer
//!   cannot structurally reach it. That is the same hardening V1 gets from `TailWithholder`,
//!   except here the format provides it rather than an adapter.
//! * **The sealed trailer.** Nonce and AAD make a *modified* file detectable; only a trailer
//!   makes a *truncated* one detectable, and truncated objects are what a failed upload leaves
//!   behind in S3.
//! * **One independent Zstd frame per block.** This is the prerequisite for compressing and
//!   decompressing blocks in parallel. The writer here is deliberately sequential, but the
//!   format permits parallelism later with no version bump, and `index_offset` reserves the slot
//!   for the block boundary index a streaming parallel reader would need.

use aes_gcm::{AeadInPlace, Aes128Gcm, KeyInit, Nonce, Tag};

use crate::codec::{Decode, Encode, OutBuf, SOFT_OUTPUT_CAP};
use crate::error::Error;
use crate::footer::{FOOTER_LEN, Footer};
use crate::header::{HEADER_LEN, Header, RESERVED_LEN};
use crate::key::CartKey;
use crate::version::{CartVersion, MAX_BLOCK_SIZE_LOG2, MIN_BLOCK_SIZE_LOG2, V2Options};

/// The length of the V2 optional header
pub(crate) const OPT_HEADER_LEN: usize = 16;

/// The length of the `u32` length prefix on the front of every block
const LEN_PREFIX_LEN: usize = 4;

/// The length of an AES-GCM authentication tag
const TAG_LEN: usize = 16;

/// The length of an AES-GCM nonce
const NONCE_LEN: usize = 12;

/// The length of the per file nonce salt, which lives in the header's reserved bytes
const SALT_LEN: usize = RESERVED_LEN;

/// The length prefix value that means "there are no more blocks"
///
/// A real block is at most [`crate::version::MAX_BLOCK_SIZE_LOG2`] worth of Zstd frame plus a
/// tag, which is nowhere near this, so the two can never be confused.
const END_MARKER: u32 = u32::MAX;

/// The block index the trailer is sealed under
///
/// Data blocks are numbered from zero and are rejected once they reach this, so the trailer's
/// nonce and AAD cannot collide with any data block's.
const TRAILER_INDEX: u32 = u32::MAX;

/// The length of the trailer's plaintext: a block count and a total length, both `u64` LE
const TRAILER_PLAIN_LEN: usize = 16;

/// The length of the associated data every block is authenticated under
const AAD_LEN: usize = HEADER_LEN + OPT_HEADER_LEN + 4;

/// Where the block index sits inside the associated data
const AAD_INDEX_AT: usize = HEADER_LEN + OPT_HEADER_LEN;

/// The flag bit meaning every block is its own independent Zstd frame
const FLAG_INDEPENDENT_FRAMES: u8 = 0b0000_0001;

/// The flag bit meaning a block boundary index is present at `index_offset`
const FLAG_INDEX_PRESENT: u8 = 0b0000_0010;

/// Every flag bit this build of cart-rs knows the meaning of
const KNOWN_FLAGS: u8 = FLAG_INDEPENDENT_FRAMES | FLAG_INDEX_PRESENT;

/// Derive the nonce for one block
///
/// The salt is random per file and the index is unique per block, so no two blocks anywhere in a
/// deployment ever share a nonce under the same key.
///
/// # Arguments
///
/// * `salt` - The per file salt from the header's reserved bytes
/// * `index` - The index of the block being sealed or opened
fn nonce_for(salt: &[u8; SALT_LEN], index: u32) -> [u8; NONCE_LEN] {
    let mut nonce = [0; NONCE_LEN];
    nonce[..SALT_LEN].copy_from_slice(salt);
    nonce[SALT_LEN..].copy_from_slice(&index.to_be_bytes());
    nonce
}

/// The V2 optional header
///
/// This is plaintext, like the mandatory header, but it is covered by every block's associated
/// data so it cannot be edited without invalidating the whole file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OptHeader {
    /// The base 2 log of the plaintext block size
    pub(crate) block_size_log2: u8,
    /// The format flags
    pub(crate) flags: u8,
    /// Where a block boundary index lives, or 0 if there is not one
    pub(crate) index_offset: u64,
}

impl OptHeader {
    /// Serialize this optional header into its 16 byte on disk form
    fn to_bytes(self) -> [u8; OPT_HEADER_LEN] {
        let mut raw = [0; OPT_HEADER_LEN];
        raw[0] = self.block_size_log2;
        raw[1] = self.flags;
        // [2..4) and [4..8) are reserved, and leaving them zero also 8 byte aligns index_offset
        raw[8..16].copy_from_slice(&self.index_offset.to_le_bytes());
        raw
    }

    /// Parse an optional header out of the 16 bytes following the mandatory header
    ///
    /// # Arguments
    ///
    /// * `raw` - The 16 bytes to parse
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] if fewer than 16 bytes were given and [`Error::Malformed`]
    /// if the block size or the flags are ones this build does not understand.
    fn parse(raw: &[u8]) -> Result<Self, Error> {
        // we cannot parse an optional header we do not have all of
        let raw: &[u8; OPT_HEADER_LEN] = raw
            .get(..OPT_HEADER_LEN)
            .and_then(|slice| slice.try_into().ok())
            .ok_or(Error::Truncated {
                section: "optional header",
            })?;
        let block_size_log2 = raw[0];
        // the block size decides how much we are willing to allocate per block, so bound it
        // before anything downstream gets to use it
        if !(MIN_BLOCK_SIZE_LOG2..=MAX_BLOCK_SIZE_LOG2).contains(&block_size_log2) {
            return Err(Error::Malformed(format!(
                "a V2 CaRT block size must be between 2^{MIN_BLOCK_SIZE_LOG2} and \
                 2^{MAX_BLOCK_SIZE_LOG2} bytes but this file declares 2^{block_size_log2}"
            )));
        }
        let flags = raw[1];
        // refuse a flag we do not know the meaning of rather than guessing at the layout
        if flags & !KNOWN_FLAGS != 0 {
            return Err(Error::Malformed(format!(
                "this V2 CaRT sets flags {flags:#010b} which this build does not understand"
            )));
        }
        // every V2 file cart-rs has ever written uses independent frames, and a file that does
        // not is a format this codec cannot decode block by block
        if flags & FLAG_INDEPENDENT_FRAMES == 0 {
            return Err(Error::Malformed(
                "this V2 CaRT does not use independent Zstd frames".to_string(),
            ));
        }
        let index_offset = u64::from_le_bytes([
            raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
        ]);
        Ok(OptHeader {
            block_size_log2,
            flags,
            index_offset,
        })
    }
}

/// The V2 encoder: Zstd compress each block, then seal it with AES-128-GCM
pub(crate) struct V2Encoder {
    /// The cipher every block is sealed with
    cipher: Aes128Gcm,
    /// The per file nonce salt
    salt: [u8; SALT_LEN],
    /// The associated data, whose last 4 bytes are rewritten per block
    aad: [u8; AAD_LEN],
    /// How much plaintext goes into one block
    block_size: usize,
    /// The Zstd compressor, reused across blocks so its context is not reallocated per block
    ///
    /// `ZSTD_compress2` resets the session on every call, so reusing this still produces one
    /// complete, independent frame per block.
    compressor: zstd::bulk::Compressor<'static>,
    /// Plaintext waiting for its block to fill up
    pending: Vec<u8>,
    /// The index of the next block to be sealed
    block_index: u32,
    /// How much plaintext has been handed to this encoder
    total_plain_len: u64,
    /// The mandatory and optional headers, until they have been written
    ///
    /// These are written lazily so that a cart nobody ever pushed to produces nothing rather
    /// than a headerless stub.
    preamble: Option<[u8; HEADER_LEN + OPT_HEADER_LEN]>,
    /// Whether the footer has already been written
    finished: bool,
}

impl std::fmt::Debug for V2Encoder {
    /// Format this encoder without trying to format a cipher or a compressor
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("V2Encoder")
            .field("block_size", &self.block_size)
            .field("block_index", &self.block_index)
            .field("total_plain_len", &self.total_plain_len)
            .field("header_written", &self.preamble.is_none())
            .field("finished", &self.finished)
            .finish()
    }
}

impl V2Encoder {
    /// Build a V2 encoder
    ///
    /// # Arguments
    ///
    /// * `key` - The key to seal this cart with
    /// * `options` - The V2 specific options to compress with
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidOption`] if the block size or Zstd level is out of range, or
    /// [`Error::Generic`] if the system random number generator or Zstd itself fails.
    pub(crate) fn new(key: CartKey, options: V2Options) -> Result<Self, Error> {
        // refuse options Zstd does not have rather than silently clamping them
        options.validate()?;
        // every file gets its own salt, which is what stops two files sharing a keystream
        let mut salt = [0; SALT_LEN];
        getrandom::fill(&mut salt)
            .map_err(|err| Error::Generic(format!("could not read a random nonce salt: {err}")))?;
        // build the two headers, which are plaintext but authenticated by every block
        let header = Header::new(CartVersion::V2, key)
            .reserved(salt)
            .opt_len(OPT_HEADER_LEN as u64);
        let opt_header = OptHeader {
            block_size_log2: options.block_size_log2,
            flags: FLAG_INDEPENDENT_FRAMES,
            index_offset: 0,
        };
        // lay them out once, since they are both the file preamble and the front of the AAD
        let mut preamble = [0; HEADER_LEN + OPT_HEADER_LEN];
        preamble[..HEADER_LEN].copy_from_slice(&header.to_bytes());
        preamble[HEADER_LEN..].copy_from_slice(&opt_header.to_bytes());
        let mut aad = [0; AAD_LEN];
        aad[..AAD_INDEX_AT].copy_from_slice(&preamble);
        Ok(V2Encoder {
            cipher: Aes128Gcm::new(key.as_bytes().into()),
            salt,
            aad,
            block_size: options.block_size(),
            compressor: zstd::bulk::Compressor::new(options.zstd_level)
                .map_err(|err| Error::Generic(format!("could not start Zstd: {err}")))?,
            pending: Vec::with_capacity(options.block_size()),
            block_index: 0,
            total_plain_len: 0,
            preamble: Some(preamble),
            finished: false,
        })
    }

    /// Write the mandatory and optional headers out if they have not been written yet
    ///
    /// # Arguments
    ///
    /// * `out` - The buffer to append the headers to
    fn write_preamble(&mut self, out: &mut OutBuf) {
        if let Some(preamble) = self.preamble.take() {
            out.extend(&preamble);
        }
    }

    /// Seal one already laid out block, returning how many bytes of `room` it now occupies
    ///
    /// The caller has written the block's plaintext into `room` at [`LEN_PREFIX_LEN`], `len`
    /// bytes long, leaving the length prefix in front of it and room for a tag behind it. This
    /// encrypts it where it sits and fills in both. The caller commits the returned length,
    /// which cannot happen in here because `room` is a live borrow of the output buffer.
    ///
    /// # Arguments
    ///
    /// * `room` - The writable region holding the laid out block
    /// * `len` - How many bytes of plaintext there are
    /// * `index` - The block index to seal under
    ///
    /// # Errors
    ///
    /// Returns [`Error::Generic`] if AES-GCM fails or the sealed block is somehow too long to
    /// describe in the `u32` length prefix.
    fn seal_in_place(&mut self, room: &mut [u8], len: usize, index: u32) -> Result<usize, Error> {
        // authenticate the headers and this block's position in the file
        self.aad[AAD_INDEX_AT..].copy_from_slice(&index.to_be_bytes());
        let nonce = nonce_for(&self.salt, index);
        // encrypt the block where it already sits
        let tag = self
            .cipher
            .encrypt_in_place_detached(
                Nonce::from_slice(&nonce),
                &self.aad,
                &mut room[LEN_PREFIX_LEN..LEN_PREFIX_LEN + len],
            )
            .map_err(|_| Error::Generic("AES-GCM encryption failed".to_string()))?;
        // the length prefix covers the ciphertext and its tag together
        let sealed_len = len + TAG_LEN;
        let prefix = u32::try_from(sealed_len)
            .ok()
            .filter(|prefix| *prefix != END_MARKER)
            .ok_or_else(|| Error::Generic(format!("a {sealed_len} byte CaRT block is too big")))?;
        room[..LEN_PREFIX_LEN].copy_from_slice(&prefix.to_le_bytes());
        room[LEN_PREFIX_LEN + len..LEN_PREFIX_LEN + sealed_len].copy_from_slice(&tag);
        Ok(LEN_PREFIX_LEN + sealed_len)
    }

    /// Compress, seal, and append one block of plaintext
    ///
    /// # Arguments
    ///
    /// * `plain` - The block's plaintext, which is never empty
    /// * `out` - The buffer to append the sealed block to
    ///
    /// # Errors
    ///
    /// Returns [`Error::Generic`] if Zstd or AES-GCM fails, or if the file has run out of block
    /// indices.
    fn seal_block(&mut self, plain: &[u8], out: &mut OutBuf) -> Result<(), Error> {
        // the trailer owns the last index, so a data block may never reach it
        let index = self.block_index;
        if index == TRAILER_INDEX {
            return Err(Error::Generic(
                "this CaRT has more blocks than the format can number".to_string(),
            ));
        }
        // ask for room for the prefix, the worst case Zstd frame, and the tag, all at once
        let bound = zstd::zstd_safe::compress_bound(plain.len());
        let room = out.writable(LEN_PREFIX_LEN + bound + TAG_LEN);
        // compress the block into its own complete, independent Zstd frame
        let len = self
            .compressor
            .compress_to_buffer(plain, &mut room[LEN_PREFIX_LEN..LEN_PREFIX_LEN + bound])
            .map_err(|err| Error::Generic(format!("Zstd compression failed: {err}")))?;
        // then seal it where it landed and hand the whole thing to the caller
        let sealed = self.seal_in_place(room, len, index)?;
        out.commit(sealed);
        self.block_index += 1;
        Ok(())
    }

    /// Seal and append the trailer block
    ///
    /// The trailer is a fixed 16 byte struct, so unlike a data block it is not compressed.
    ///
    /// # Arguments
    ///
    /// * `out` - The buffer to append the sealed trailer to
    ///
    /// # Errors
    ///
    /// Returns [`Error::Generic`] if AES-GCM fails.
    fn seal_trailer(&mut self, out: &mut OutBuf) -> Result<(), Error> {
        let room = out.writable(LEN_PREFIX_LEN + TRAILER_PLAIN_LEN + TAG_LEN);
        // how many blocks there were and how long the original file was
        room[LEN_PREFIX_LEN..LEN_PREFIX_LEN + 8]
            .copy_from_slice(&u64::from(self.block_index).to_le_bytes());
        room[LEN_PREFIX_LEN + 8..LEN_PREFIX_LEN + TRAILER_PLAIN_LEN]
            .copy_from_slice(&self.total_plain_len.to_le_bytes());
        let sealed = self.seal_in_place(room, TRAILER_PLAIN_LEN, TRAILER_INDEX)?;
        out.commit(sealed);
        Ok(())
    }

    /// Seal whatever plaintext is buffered as a block, without upsetting the borrow checker
    ///
    /// # Arguments
    ///
    /// * `out` - The buffer to append the sealed block to
    ///
    /// # Errors
    ///
    /// Returns whatever [`V2Encoder::seal_block`] does.
    fn seal_pending(&mut self, out: &mut OutBuf) -> Result<(), Error> {
        // take the buffer so the compressor is not reading out of `self` while writing into it
        let mut block = std::mem::take(&mut self.pending);
        let sealed = self.seal_block(&block, out);
        // and hand the allocation straight back however that went
        block.clear();
        self.pending = block;
        sealed
    }
}

impl Encode for V2Encoder {
    /// The `CaRT` version this codec writes
    fn version(&self) -> CartVersion {
        CartVersion::V2
    }

    /// Feed plaintext in, appending sealed blocks to `out`
    ///
    /// # Arguments
    ///
    /// * `input` - The plaintext to cart
    /// * `out` - The buffer to append carted bytes to
    ///
    /// # Errors
    ///
    /// Returns [`Error::AlreadyFinished`] if the footer has already been written, or
    /// [`Error::Generic`] if Zstd or AES-GCM fails.
    fn push(&mut self, mut input: &[u8], out: &mut OutBuf) -> Result<(), Error> {
        // nothing may follow the footer
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.write_preamble(out);
        // an empty chunk is a no-op rather than something that reaches the compressor
        if input.is_empty() {
            return Ok(());
        }
        self.total_plain_len += input.len() as u64;
        while !input.is_empty() {
            // with nothing buffered, whole blocks can be sealed straight out of the caller's
            // slice instead of being copied through `pending` first
            if self.pending.is_empty() && input.len() >= self.block_size {
                let (block, rest) = input.split_at(self.block_size);
                self.seal_block(block, out)?;
                input = rest;
                continue;
            }
            // otherwise top the pending block up. `pending` is never full here, because it is
            // sealed and cleared the moment it fills, so this always takes at least one byte.
            let room = self.block_size - self.pending.len();
            let take = room.min(input.len());
            self.pending.extend_from_slice(&input[..take]);
            input = &input[take..];
            // and seal it as soon as it is full
            if self.pending.len() == self.block_size {
                self.seal_pending(out)?;
            }
        }
        Ok(())
    }

    /// Seal the last partial block, the trailer, and the mandatory footer
    ///
    /// # Arguments
    ///
    /// * `out` - The buffer to append the rest of the cart to
    ///
    /// # Errors
    ///
    /// Returns [`Error::AlreadyFinished`] if this was already called, or [`Error::Generic`] if
    /// Zstd or AES-GCM fails.
    fn finish(&mut self, out: &mut OutBuf) -> Result<(), Error> {
        // finishing twice would write two footers
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        // a cart with no body at all still needs its headers
        self.write_preamble(out);
        // whatever is left over is a short final block
        if !self.pending.is_empty() {
            self.seal_pending(out)?;
        }
        // a reserved length value ends the block stream, so nothing has to be pattern matched
        out.extend(&END_MARKER.to_le_bytes());
        // the sealed trailer is what makes truncation and dropped blocks detectable
        self.seal_trailer(out)?;
        // and the mandatory footer, so any CaRT tool can still identify the file
        out.extend(&Footer::new().to_bytes());
        self.finished = true;
        Ok(())
    }
}

/// Which part of a V2 file the decoder is reading
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum V2State {
    /// Reading the 16 byte optional header
    OptHeader,
    /// Reading a 4 byte block length prefix, which may turn out to be the end marker
    Length,
    /// Reading a sealed block whose length is already known
    Block {
        /// How many bytes of the sealed block are still to come
        remaining: usize,
    },
    /// Reading the 4 byte length prefix of the trailer block
    TrailerLength,
    /// Reading the sealed trailer block
    Trailer {
        /// How many bytes of the sealed trailer are still to come
        remaining: usize,
    },
    /// Reading the 28 byte mandatory footer
    Footer,
    /// The whole file has been read and nothing more may follow
    Done,
}

/// The V2 decoder: open each sealed block, then Zstd decompress it
pub(crate) struct V2Decoder {
    /// The cipher every block is opened with
    cipher: Aes128Gcm,
    /// The per file nonce salt, from the header's reserved bytes
    salt: [u8; SALT_LEN],
    /// The associated data, whose last 4 bytes are rewritten per block
    aad: [u8; AAD_LEN],
    /// The Zstd decompressor, reused across blocks
    decompressor: zstd::bulk::Decompressor<'static>,
    /// How much plaintext one block holds, from the optional header
    block_size: usize,
    /// The largest sealed block this file's block size could possibly produce
    ///
    /// This is what bounds allocation: the `u32` length prefix is attacker controlled, so it is
    /// checked against a limit derived from the declared block size before a byte is buffered.
    max_sealed_len: usize,
    /// The sealed block currently being collected
    scratch: Vec<u8>,
    /// Accumulator for the fixed size fields, so they survive byte at a time delivery
    partial: [u8; FOOTER_LEN],
    /// How much of `partial` is filled
    partial_len: usize,
    /// Which part of the file is being read
    state: V2State,
    /// The index of the next block to be opened
    block_index: u32,
    /// How much plaintext has been recovered so far
    total_plain_len: u64,
}

impl std::fmt::Debug for V2Decoder {
    /// Format this decoder without trying to format a cipher or a decompressor
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("V2Decoder")
            .field("state", &self.state)
            .field("block_size", &self.block_size)
            .field("block_index", &self.block_index)
            .field("total_plain_len", &self.total_plain_len)
            .finish()
    }
}

impl V2Decoder {
    /// Build a V2 decoder for a file whose mandatory header has already been parsed
    ///
    /// The caller is expected to have consumed exactly [`HEADER_LEN`] bytes; the optional
    /// header and everything after it goes to [`Decode::push`].
    ///
    /// # Arguments
    ///
    /// * `header` - The parsed mandatory header
    ///
    /// # Errors
    ///
    /// Returns [`Error::Malformed`] if the header does not declare a V2 optional header, or
    /// [`Error::Generic`] if Zstd fails to start.
    pub(crate) fn new(header: &Header) -> Result<Self, Error> {
        // every V2 file stores exactly one 16 byte optional header
        if header.opt_len != OPT_HEADER_LEN as u64 {
            return Err(Error::Malformed(format!(
                "a V2 CaRT has a {OPT_HEADER_LEN} byte optional header but this one declares {}",
                header.opt_len
            )));
        }
        // the AAD starts with the header exactly as it was written
        let mut aad = [0; AAD_LEN];
        aad[..HEADER_LEN].copy_from_slice(&header.to_bytes());
        Ok(V2Decoder {
            cipher: Aes128Gcm::new(header.key.as_bytes().into()),
            salt: header.reserved,
            aad,
            decompressor: zstd::bulk::Decompressor::new()
                .map_err(|err| Error::Generic(format!("could not start Zstd: {err}")))?,
            // these two are filled in from the optional header, which is the very next thing
            // this decoder reads
            block_size: 0,
            max_sealed_len: 0,
            scratch: Vec::new(),
            partial: [0; FOOTER_LEN],
            partial_len: 0,
            state: V2State::OptHeader,
            block_index: 0,
            total_plain_len: 0,
        })
    }

    /// Copy the front of `data` into the fixed size field accumulator
    ///
    /// Returns what is left of `data`. This is what makes the decoder survive a reader that
    /// hands over one byte at a time, which is how the old header parser reported a perfectly
    /// good file as corrupt.
    ///
    /// # Arguments
    ///
    /// * `data` - The bytes to take from
    /// * `want` - How many bytes the field being accumulated is
    fn fill<'a>(&mut self, data: &'a [u8], want: usize) -> &'a [u8] {
        let take = (want - self.partial_len).min(data.len());
        self.partial[self.partial_len..self.partial_len + take].copy_from_slice(&data[..take]);
        self.partial_len += take;
        &data[take..]
    }

    /// Read the `u32` sitting in the front of the field accumulator
    fn partial_u32(&self) -> u32 {
        u32::from_le_bytes([
            self.partial[0],
            self.partial[1],
            self.partial[2],
            self.partial[3],
        ])
    }

    /// Take the optional header out of the accumulator and set the block geometry from it
    ///
    /// # Errors
    ///
    /// Returns [`Error::Malformed`] if the block size or flags are ones this build cannot use.
    fn open_opt_header(&mut self) -> Result<(), Error> {
        let opt_header = OptHeader::parse(&self.partial[..OPT_HEADER_LEN])?;
        // the optional header is plaintext but is authenticated by every block
        self.aad[HEADER_LEN..AAD_INDEX_AT].copy_from_slice(&self.partial[..OPT_HEADER_LEN]);
        self.block_size = 1_usize << opt_header.block_size_log2;
        // a block can never be longer than the worst case for the declared block size, which is
        // what keeps a hostile length prefix from turning 4 bytes into a 4 GiB allocation
        self.max_sealed_len = zstd::zstd_safe::compress_bound(self.block_size) + TAG_LEN;
        self.partial_len = 0;
        self.state = V2State::Length;
        Ok(())
    }

    /// Check a block length prefix against what this file's block size allows
    ///
    /// # Arguments
    ///
    /// * `raw` - The length prefix that was read
    ///
    /// # Errors
    ///
    /// Returns [`Error::Malformed`] if the block is too short to hold a tag and
    /// [`Error::LimitExceeded`] if it is longer than the declared block size permits.
    fn check_sealed_len(&self, raw: u32) -> Result<usize, Error> {
        let len = raw as usize;
        // a block is at least a tag plus a byte of Zstd frame
        if len <= TAG_LEN {
            return Err(Error::Malformed(format!(
                "a {len} byte CaRT block is too short to hold an authentication tag"
            )));
        }
        // and at most the worst case for the block size the file declared
        if len > self.max_sealed_len {
            return Err(Error::LimitExceeded {
                section: "block",
                declared: len as u64,
                max: self.max_sealed_len as u64,
            });
        }
        Ok(len)
    }

    /// Authenticate, decrypt, and decompress the sealed block sitting in the scratch buffer
    ///
    /// # Arguments
    ///
    /// * `out` - The buffer to append plaintext to
    ///
    /// # Errors
    ///
    /// Returns [`Error::Corrupt`] if the block fails authentication or does not decompress, and
    /// [`Error::Malformed`] if the file has run out of block indices.
    fn open_block(&mut self, out: &mut OutBuf) -> Result<(), Error> {
        // the trailer owns the last index, so a data block may never reach it
        let index = self.block_index;
        if index == TRAILER_INDEX {
            return Err(Error::Malformed(
                "this CaRT has more blocks than the format can number".to_string(),
            ));
        }
        // check_sealed_len already proved the block is longer than a tag
        let frame_len = self.scratch.len() - TAG_LEN;
        self.aad[AAD_INDEX_AT..].copy_from_slice(&index.to_be_bytes());
        let nonce = nonce_for(&self.salt, index);
        // open the block in place, which also proves the headers and this block's position in
        // the file were not tampered with
        let (frame, tag) = self.scratch.split_at_mut(frame_len);
        self.cipher
            .decrypt_in_place_detached(
                Nonce::from_slice(&nonce),
                &self.aad,
                frame,
                Tag::from_slice(tag),
            )
            .map_err(|_| {
                Error::Corrupt(format!("block {index} failed its authentication check"))
            })?;
        // only now does anything reach the decompressor, and it is bounded on both sides: the
        // frame's length was known before a byte of it was buffered, and the destination is
        // exactly one block, so an over long frame is an error rather than an allocation
        let room = &mut out.writable(self.block_size)[..self.block_size];
        let written = self
            .decompressor
            .decompress_to_buffer(&self.scratch[..frame_len], room)
            .map_err(|err| {
                Error::Corrupt(format!("block {index} could not be decompressed: {err}"))
            })?;
        out.commit(written);
        self.total_plain_len += written as u64;
        self.block_index += 1;
        Ok(())
    }

    /// Authenticate the trailer and check it against what was actually read
    ///
    /// # Errors
    ///
    /// Returns [`Error::Corrupt`] if the trailer fails authentication or disagrees with the
    /// blocks that were read, which is how a truncated or tampered with file is caught.
    fn open_trailer(&mut self) -> Result<(), Error> {
        self.aad[AAD_INDEX_AT..].copy_from_slice(&TRAILER_INDEX.to_be_bytes());
        let nonce = nonce_for(&self.salt, TRAILER_INDEX);
        let (plain, tag) = self.scratch.split_at_mut(TRAILER_PLAIN_LEN);
        self.cipher
            .decrypt_in_place_detached(
                Nonce::from_slice(&nonce),
                &self.aad,
                plain,
                Tag::from_slice(tag),
            )
            .map_err(|_| {
                Error::Corrupt("the CaRT trailer failed its authentication check".to_string())
            })?;
        // pull the two counts out of the trailer
        let mut raw = [0; 8];
        raw.copy_from_slice(&plain[..8]);
        let blocks = u64::from_le_bytes(raw);
        raw.copy_from_slice(&plain[8..TRAILER_PLAIN_LEN]);
        let total = u64::from_le_bytes(raw);
        // a block that went missing between the writer and here shows up as a count mismatch
        if blocks != u64::from(self.block_index) {
            return Err(Error::Corrupt(format!(
                "the CaRT trailer says this file has {blocks} blocks but {} were read",
                self.block_index
            )));
        }
        // and so does a block whose plaintext is not the length it was written at
        if total != self.total_plain_len {
            return Err(Error::Corrupt(format!(
                "the CaRT trailer says this file was {total} bytes but {} were recovered",
                self.total_plain_len
            )));
        }
        self.partial_len = 0;
        self.state = V2State::Footer;
        Ok(())
    }

    /// Advance the state machine by one step, consuming at least one byte
    ///
    /// Returns what is left of `data`.
    ///
    /// # Arguments
    ///
    /// * `data` - The bytes to read from, which are never empty
    /// * `out` - The buffer to append plaintext to
    ///
    /// # Errors
    ///
    /// Returns an error if the file is malformed, corrupt, or has bytes after its footer.
    fn step<'a>(&mut self, data: &'a [u8], out: &mut OutBuf) -> Result<&'a [u8], Error> {
        match self.state {
            V2State::OptHeader => {
                let rest = self.fill(data, OPT_HEADER_LEN);
                if self.partial_len == OPT_HEADER_LEN {
                    self.open_opt_header()?;
                }
                Ok(rest)
            }
            V2State::Length => {
                let rest = self.fill(data, LEN_PREFIX_LEN);
                if self.partial_len == LEN_PREFIX_LEN {
                    let raw = self.partial_u32();
                    self.partial_len = 0;
                    self.state = if raw == END_MARKER {
                        // the block stream is over, and this cannot be a real block length
                        V2State::TrailerLength
                    } else {
                        // bound the allocation before reserving a single byte for it
                        let remaining = self.check_sealed_len(raw)?;
                        self.scratch.clear();
                        self.scratch.reserve(remaining);
                        V2State::Block { remaining }
                    };
                }
                Ok(rest)
            }
            V2State::Block { remaining } => {
                // collect the sealed block, which cannot be opened until all of it is here
                let take = remaining.min(data.len());
                self.scratch.extend_from_slice(&data[..take]);
                if take == remaining {
                    self.open_block(out)?;
                    self.state = V2State::Length;
                } else {
                    self.state = V2State::Block {
                        remaining: remaining - take,
                    };
                }
                Ok(&data[take..])
            }
            V2State::TrailerLength => {
                let rest = self.fill(data, LEN_PREFIX_LEN);
                if self.partial_len == LEN_PREFIX_LEN {
                    let raw = self.partial_u32();
                    self.partial_len = 0;
                    // the trailer is a fixed size struct, so its length is not negotiable
                    let remaining = TRAILER_PLAIN_LEN + TAG_LEN;
                    if raw as usize != remaining {
                        return Err(Error::Malformed(format!(
                            "the CaRT trailer must be {remaining} bytes but this one declares \
                             {raw}"
                        )));
                    }
                    self.scratch.clear();
                    self.scratch.reserve(remaining);
                    self.state = V2State::Trailer { remaining };
                }
                Ok(rest)
            }
            V2State::Trailer { remaining } => {
                let take = remaining.min(data.len());
                self.scratch.extend_from_slice(&data[..take]);
                if take == remaining {
                    // opening the trailer is what moves us on to the footer
                    self.open_trailer()?;
                } else {
                    self.state = V2State::Trailer {
                        remaining: remaining - take,
                    };
                }
                Ok(&data[take..])
            }
            V2State::Footer => {
                let rest = self.fill(data, FOOTER_LEN);
                if self.partial_len == FOOTER_LEN {
                    let footer = Footer::parse(&self.partial)?;
                    // V2 never writes an optional footer or uses the footer's reserved bytes,
                    // and the trailer already covers everything one would have told us. This is
                    // also the only thing standing between the last 24 bytes of the file and
                    // being freely editable, since they are outside every block's AAD.
                    if footer != Footer::new() {
                        return Err(Error::Malformed(
                            "a V2 CaRT's footer must be a bare TRAC marker".to_string(),
                        ));
                    }
                    self.partial_len = 0;
                    self.state = V2State::Done;
                }
                Ok(rest)
            }
            V2State::Done => Err(Error::Malformed(
                "there are bytes after the end of this CaRT file".to_string(),
            )),
        }
    }
}

impl Decode for V2Decoder {
    /// The `CaRT` version this codec reads
    fn version(&self) -> CartVersion {
        CartVersion::V2
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
    /// Returns [`Error::Corrupt`] if a block fails authentication or does not decompress and
    /// [`Error::Malformed`] or [`Error::LimitExceeded`] if the file is structurally invalid.
    fn push(&mut self, input: &[u8], out: &mut OutBuf) -> Result<usize, Error> {
        let mut data = input;
        while !data.is_empty() {
            // stop taking input once the caller has plenty to read, but only after opening at
            // least one block, so that re-pushing the remainder cannot livelock
            if data.len() < input.len() && out.len() >= SOFT_OUTPUT_CAP {
                break;
            }
            data = self.step(data, out)?;
        }
        Ok(input.len() - data.len())
    }

    /// Tell the decoder the file has ended and check that it really was a whole file
    ///
    /// # Arguments
    ///
    /// * `_out` - Unused; a V2 block is decompressed the moment it is complete
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] if the file ended part way through a structure and
    /// [`Error::MissingFooter`] if it ended before its footer.
    fn finish(&mut self, _out: &mut OutBuf) -> Result<(), Error> {
        match self.state {
            V2State::Done => Ok(()),
            V2State::OptHeader => Err(Error::Truncated {
                section: "optional header",
            }),
            V2State::Length | V2State::Block { .. } => Err(Error::Truncated { section: "body" }),
            V2State::TrailerLength | V2State::Trailer { .. } => {
                Err(Error::Truncated { section: "trailer" })
            }
            V2State::Footer => Err(Error::MissingFooter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AAD_INDEX_AT, END_MARKER, FLAG_INDEPENDENT_FRAMES, LEN_PREFIX_LEN, OPT_HEADER_LEN,
        OptHeader, TAG_LEN, TRAILER_PLAIN_LEN, V2Decoder, V2Encoder,
    };
    use crate::codec::{Decode, Encode, OutBuf};
    use crate::error::Error;
    use crate::header::{HEADER_LEN, Header};
    use crate::key::CartKey;
    use crate::version::{CartVersion, V2Options};

    /// A small block size so the multi block paths are reachable without megabytes of test data
    const SMALL: V2Options = V2Options {
        zstd_level: 3,
        block_size_log2: 10,
    };

    /// The key everything in the tree uses
    fn key() -> CartKey {
        CartKey::from_password("SecretCornIsBest").unwrap()
    }

    /// Cart a slice, handing it over in chunks of `chunk` bytes
    fn cart(plain: &[u8], chunk: usize, options: V2Options) -> Vec<u8> {
        let mut encoder = V2Encoder::new(key(), options).unwrap();
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
        let mut decoder = V2Decoder::new(&header)?;
        let mut out = OutBuf::new();
        let mut plain = Vec::new();
        for slice in carted[HEADER_LEN..].chunks(chunk.max(1)) {
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

    /// Something that compresses a bit but not trivially
    fn plaintext(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| ((i as u32).wrapping_mul(2_654_435_761) >> 24) as u8)
            .collect()
    }

    #[test]
    fn round_trips_at_every_interesting_size() {
        let block = SMALL.block_size();
        for len in [
            0,
            1,
            block - 1,
            block,
            block + 1,
            2 * block,
            2 * block + 7,
            100_000,
        ] {
            let plain = plaintext(len);
            let carted = cart(&plain, 4096, SMALL);
            assert_eq!(
                uncart(&carted, 4096).unwrap(),
                plain,
                "failed at {len} bytes"
            );
        }
    }

    /// Uploading an empty file used to be a guaranteed 500
    #[test]
    fn a_zero_byte_file_carts_and_uncarts() {
        let carted = cart(&[], 4096, SMALL);
        assert_eq!(&carted[..4], b"CART");
        assert_eq!(&carted[carted.len() - 28..carted.len() - 24], b"TRAC");
        assert!(uncart(&carted, 4096).unwrap().is_empty());
    }

    /// A zero length chunk in the middle of a body used to abort the whole upload
    #[test]
    fn empty_chunks_are_a_no_op() {
        let mut encoder = V2Encoder::new(key(), SMALL).unwrap();
        let mut out = OutBuf::new();
        encoder.push(&[], &mut out).unwrap();
        encoder.push(b"corn", &mut out).unwrap();
        encoder.push(&[], &mut out).unwrap();
        encoder.push(b" is best", &mut out).unwrap();
        encoder.push(&[], &mut out).unwrap();
        encoder.finish(&mut out).unwrap();
        assert_eq!(uncart(out.filled(), 4096).unwrap(), b"corn is best");
    }

    /// Neither direction may care how the bytes were handed over, right down to one at a time
    #[test]
    fn the_chunk_schedule_does_not_change_the_bytes() {
        let plain = plaintext(9_000);
        let reference = cart(&plain, 9_000, SMALL);
        for chunk in [1_usize, 3, 1023, 1024, 1025, 4096, 9_000] {
            // the salt is random per file, so compare what comes back rather than the bytes
            assert_eq!(
                uncart(&cart(&plain, chunk, SMALL), 4096).unwrap(),
                plain,
                "carting in {chunk} byte chunks lost data"
            );
            assert_eq!(
                uncart(&reference, chunk).unwrap(),
                plain,
                "uncarting in {chunk} byte chunks lost data"
            );
        }
    }

    #[test]
    fn every_block_size_and_level_round_trips() {
        let plain = plaintext(40_000);
        for block_size_log2 in [10_u8, 11, 14, 16] {
            for zstd_level in [-3_i32, 0, 1, 3, 9] {
                let options = V2Options {
                    zstd_level,
                    block_size_log2,
                };
                let carted = cart(&plain, 4096, options);
                assert_eq!(
                    uncart(&carted, 4096).unwrap(),
                    plain,
                    "failed at 2^{block_size_log2} byte blocks, level {zstd_level}"
                );
            }
        }
    }

    #[test]
    fn out_of_range_options_are_rejected() {
        assert!(
            V2Encoder::new(
                key(),
                V2Options {
                    block_size_log2: 9,
                    ..V2Options::default()
                }
            )
            .is_err()
        );
        assert!(
            V2Encoder::new(
                key(),
                V2Options {
                    zstd_level: 23,
                    ..V2Options::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn finishing_twice_is_an_error() {
        let mut encoder = V2Encoder::new(key(), SMALL).unwrap();
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

    /// Pin the on disk layout, since nothing outside this crate can tell us we changed it
    #[test]
    fn our_output_has_the_layout_we_claim() {
        let plain = plaintext(2_500);
        let carted = cart(&plain, 4096, SMALL);
        // the mandatory header says V2, carries a salt, and points at a 16 byte optional header
        let header = Header::parse(&carted).unwrap();
        assert_eq!(header.version, CartVersion::V2);
        assert_eq!(header.opt_len, OPT_HEADER_LEN as u64);
        assert_eq!(header.key, key());
        assert_ne!(header.reserved, [0; 8], "the nonce salt was never set");
        // the optional header describes the blocks
        let opt_header =
            OptHeader::parse(&carted[HEADER_LEN..HEADER_LEN + OPT_HEADER_LEN]).unwrap();
        assert_eq!(opt_header.block_size_log2, SMALL.block_size_log2);
        assert_eq!(opt_header.flags, FLAG_INDEPENDENT_FRAMES);
        assert_eq!(opt_header.index_offset, 0);
        // walk the blocks by their length prefixes alone, which is what a seekable parallel
        // reader would one day do
        let mut at = HEADER_LEN + OPT_HEADER_LEN;
        let mut blocks = 0;
        loop {
            let mut raw = [0; LEN_PREFIX_LEN];
            raw.copy_from_slice(&carted[at..at + LEN_PREFIX_LEN]);
            let len = u32::from_le_bytes(raw);
            at += LEN_PREFIX_LEN;
            if len == END_MARKER {
                break;
            }
            assert!(len as usize > TAG_LEN);
            at += len as usize;
            blocks += 1;
        }
        // 2500 bytes at 1 KiB a block is three blocks
        assert_eq!(blocks, 3);
        // then the sealed trailer, then the footer, and nothing else
        let mut raw = [0; LEN_PREFIX_LEN];
        raw.copy_from_slice(&carted[at..at + LEN_PREFIX_LEN]);
        assert_eq!(
            u32::from_le_bytes(raw),
            (TRAILER_PLAIN_LEN + TAG_LEN) as u32
        );
        at += LEN_PREFIX_LEN + TRAILER_PLAIN_LEN + TAG_LEN;
        assert_eq!(&carted[at..at + 4], b"TRAC");
        assert_eq!(at + 28, carted.len());
    }

    /// Two carts of the same bytes must not share a keystream
    #[test]
    fn every_file_gets_its_own_salt() {
        let plain = plaintext(3_000);
        let first = cart(&plain, 4096, SMALL);
        let second = cart(&plain, 4096, SMALL);
        assert_ne!(first[6..14], second[6..14], "the salt was not random");
        // and with a different salt every sealed byte differs too
        assert_ne!(
            first[HEADER_LEN + OPT_HEADER_LEN..],
            second[HEADER_LEN + OPT_HEADER_LEN..]
        );
    }

    /// This is the test that proves the AEAD is doing something. Every single bit of the file
    /// is either authenticated or structural, so flipping any of them must be an error rather
    /// than a different answer.
    #[test]
    fn flipping_any_bit_is_caught() {
        let plain = plaintext(2_500);
        let carted = cart(&plain, 4096, SMALL);
        for byte in 0..carted.len() {
            for bit in [0_u8, 3, 7] {
                let mut damaged = carted.clone();
                damaged[byte] ^= 1 << bit;
                assert!(
                    uncart(&damaged, 4096).is_err(),
                    "flipping bit {bit} of byte {byte} of {} went unnoticed",
                    carted.len()
                );
            }
        }
    }

    /// A failed or interrupted upload leaves a truncated object in S3
    #[test]
    fn truncated_files_are_rejected() {
        let plain = plaintext(2_500);
        let carted = cart(&plain, 4096, SMALL);
        // every prefix of a valid cart is an invalid cart, so just check all of them
        for cut in 0..carted.len() {
            assert!(
                uncart(&carted[..cut], 4096).is_err(),
                "a file cut to {cut} of {} bytes should not decode",
                carted.len()
            );
        }
        assert_eq!(uncart(&carted, 4096).unwrap(), plain);
    }

    /// Anything glued onto the end of a complete file has to be rejected too
    #[test]
    fn trailing_junk_is_rejected() {
        let carted = cart(&plaintext(2_500), 4096, SMALL);
        let mut extended = carted.clone();
        extended.extend_from_slice(b"and one more thing");
        assert!(matches!(uncart(&extended, 4096), Err(Error::Malformed(_))));
    }

    /// Dropping a whole block has to be caught, which nonce and AAD alone would not do
    #[test]
    fn a_dropped_block_is_caught() {
        let plain = plaintext(2_500);
        let carted = cart(&plain, 4096, SMALL);
        // find the first block and cut it out, prefix and all
        let at = HEADER_LEN + OPT_HEADER_LEN;
        let mut raw = [0; LEN_PREFIX_LEN];
        raw.copy_from_slice(&carted[at..at + LEN_PREFIX_LEN]);
        let len = u32::from_le_bytes(raw) as usize;
        let mut short = carted[..at].to_vec();
        short.extend_from_slice(&carted[at + LEN_PREFIX_LEN + len..]);
        // the second block is now block 0, so it fails on its AAD before the trailer is reached
        assert!(matches!(uncart(&short, 4096), Err(Error::Corrupt(_))));
    }

    /// Swapping two blocks is what an empty AAD used to allow
    #[test]
    fn reordered_blocks_are_caught() {
        let plain = plaintext(2_500);
        let carted = cart(&plain, 4096, SMALL);
        // the first two blocks are both full, so they are laid out identically
        let at = HEADER_LEN + OPT_HEADER_LEN;
        let mut raw = [0; LEN_PREFIX_LEN];
        raw.copy_from_slice(&carted[at..at + LEN_PREFIX_LEN]);
        let first = LEN_PREFIX_LEN + u32::from_le_bytes(raw) as usize;
        raw.copy_from_slice(&carted[at + first..at + first + LEN_PREFIX_LEN]);
        let second = LEN_PREFIX_LEN + u32::from_le_bytes(raw) as usize;
        let mut swapped = carted[..at].to_vec();
        swapped.extend_from_slice(&carted[at + first..at + first + second]);
        swapped.extend_from_slice(&carted[at..at + first]);
        swapped.extend_from_slice(&carted[at + first + second..]);
        assert!(matches!(uncart(&swapped, 4096), Err(Error::Corrupt(_))));
    }

    /// A `u32` length prefix must never be believed on its own
    #[test]
    fn an_oversized_block_length_is_rejected_before_it_is_allocated_for() {
        let mut carted = cart(&plaintext(2_500), 4096, SMALL);
        let at = HEADER_LEN + OPT_HEADER_LEN;
        // claim a block just short of 4 GiB
        carted[at..at + LEN_PREFIX_LEN].copy_from_slice(&(u32::MAX - 1).to_le_bytes());
        assert!(matches!(
            uncart(&carted, 4096),
            Err(Error::LimitExceeded { .. })
        ));
    }

    /// The optional header decides how much memory a block may take, so it is bounded too
    #[test]
    fn an_absurd_block_size_is_rejected() {
        let mut carted = cart(&plaintext(2_500), 4096, SMALL);
        carted[HEADER_LEN] = 63;
        assert!(matches!(uncart(&carted, 4096), Err(Error::Malformed(_))));
    }

    /// A flag we do not know the meaning of means a layout we cannot safely guess at
    #[test]
    fn unknown_flags_are_rejected() {
        let mut carted = cart(&plaintext(2_500), 4096, SMALL);
        carted[HEADER_LEN + 1] |= 0b1000_0000;
        assert!(matches!(uncart(&carted, 4096), Err(Error::Malformed(_))));
    }

    /// A V1 header must not build a V2 decoder, whatever else is in the file
    #[test]
    fn a_header_without_a_v2_optional_header_is_rejected() {
        let header = Header::new(CartVersion::V2, key());
        assert!(matches!(V2Decoder::new(&header), Err(Error::Malformed(_))));
    }

    #[test]
    fn the_optional_header_round_trips() {
        let opt_header = OptHeader {
            block_size_log2: 20,
            flags: FLAG_INDEPENDENT_FRAMES,
            index_offset: 0x0102_0304_0506_0708,
        };
        let raw = opt_header.to_bytes();
        assert_eq!(raw.len(), OPT_HEADER_LEN);
        assert_eq!(raw[0], 20);
        assert_eq!(raw[1], FLAG_INDEPENDENT_FRAMES);
        assert_eq!(&raw[2..8], &[0; 6]);
        assert_eq!(OptHeader::parse(&raw).unwrap(), opt_header);
        // and it never reads past what it was given
        for len in 0..OPT_HEADER_LEN {
            assert!(OptHeader::parse(&raw[..len]).is_err());
        }
    }

    /// The AAD has to cover the headers and the block index, or blocks can be moved about
    #[test]
    fn the_aad_covers_the_headers_and_the_block_index() {
        assert_eq!(AAD_INDEX_AT, HEADER_LEN + OPT_HEADER_LEN);
        let encoder = V2Encoder::new(key(), SMALL).unwrap();
        let preamble = encoder.preamble.unwrap();
        assert_eq!(
            &encoder.aad[..AAD_INDEX_AT],
            &preamble[..],
            "the AAD does not start with the file's own headers"
        );
    }

    /// `tokio_tar::Archive` holds its reader in an `Arc` and needs `Send + Sync + Unpin`, so a
    /// codec that is not all three breaks four consumers rather than this line
    #[test]
    fn the_codecs_are_send_sync_and_unpin() {
        const fn assert_send_sync_unpin<T: Send + Sync + Unpin>() {}
        assert_send_sync_unpin::<V2Encoder>();
        assert_send_sync_unpin::<V2Decoder>();
    }
}

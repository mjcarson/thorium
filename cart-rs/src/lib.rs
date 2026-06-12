//! Cart is a file format for storing and transferring malware in a safe way.
//!
//! The primary way the file is made safe is by encrypting it with rc4 to prevent accidental
//! execution. Files are also zipped with zlib to minimize space usage.
//!
//! Cart-rs does not currently support any of the optional fields in the header or footer that the
//! [Cart Spec](https://bitbucket.org/cse-assemblyline/cart/src/master/) allows. We do not plan
//! to ever support the optional footer fields as this would come at the cost of supporting streaming
//! downloads. This library is largely intended to allow the Thorium API to stream
//! `CaRTed` files to and from s3.
//!
//! # Examples
//!
//! ## `CaRTing` a File
//!
//! ```
//! use tokio::fs::{File, OpenOptions};
//! use tokio::io::{BufReader, AsyncWriteExt};
//! use cart_rs::CartStream;
//! use generic_array::{typenum::U16, GenericArray};
//!
//! # async fn exec() -> Result<(), cart_rs::Error> {
//! # let mut source = File::create("/tmp/EvilCorn").await?;
//! # source.write_all("ImMalware".as_bytes()).await?;
//! # // open our file in read mode
//! # let source = File::open("/tmp/EvilCorn").await?;
//! // have a cart password to use
//! let password: GenericArray<u8, U16> =
//!     GenericArray::clone_from_slice(&"SecretCornIsBest".as_bytes()[..16]);
//! // build our cart stream
//! let mut cart_stream = CartStream::builder(&password, BufReader::new(source)).build_v2()?;
//! // open a file to cart to
//! let mut output = OpenOptions::new()
//!     .read(true)
//!     .write(true)
//!     .create(true)
//!     .truncate(true)
//!     .open("/tmp/CartedCorn")
//!     .await?;
//! // write our carted file to disk
//! tokio::io::copy(&mut cart_stream, &mut output).await?;
//! # // validate our file was carted correctly
//! # // open our carted file
//! # let carted = File::open("/tmp/CartedCorn").await?;
//! # // start uncarting this stream
//! # let mut uncart_stream = cart_rs::UncartStream::new(BufReader::new(carted));
//! # // open a file to uncart to
//! # let mut uncarted = tokio::fs::OpenOptions::new()
//! #     .read(true)
//! #     .write(true)
//! #     .create(true)
//! #     .truncate(true)
//! #     .open("/tmp/UnCartedCorn")
//! #     .await?;
//! # // write our uncarted file to disk
//! # tokio::io::copy(&mut uncart_stream, &mut uncarted).await?;
//! # // make sure the files match
//! # let uncarted = tokio::fs::read_to_string("/tmp/UnCartedCorn").await?;
//! # if uncarted != "ImMalware" {
//! #      panic!("{} is not 'ImMalware'", uncarted);
//! # }
//! # std::fs::remove_file("/tmp/EvilCorn")?;
//! # std::fs::remove_file("/tmp/CartedCorn")?;
//! # std::fs::remove_file("/tmp/UnCartedCorn")?;
//! # Ok(())
//! # }
//!
//! # tokio_test::block_on(async {
//! #    exec().await.unwrap()
//! # })
//! ```
//!
//! ## `UnCaRTing` a File
//!
//! ```
//! use tokio::fs::{File, OpenOptions};
//! use cart_rs::UncartStream;
//! # use tokio::io::{BufReader, AsyncWriteExt, AsyncReadExt};
//! # use cart_rs::CartStreamManual;
//! # use generic_array::{typenum::U16, GenericArray};
//! use bytes::BytesMut;
//!
//! # async fn exec() -> Result<(), cart_rs::Error> {
//! # let mut source = File::create("/tmp/MaliciousPotato").await?;
//! # source.write_all("ImMalware".as_bytes()).await?;
//! # // open our file in read mode
//! # let source = File::open("/tmp/MaliciousPotato").await?;
//! # // have a cart password to use
//! # let password: GenericArray<u8, U16> =
//! #     GenericArray::clone_from_slice(&"SecretCornIsBest".as_bytes()[..16]);
//! # // build our cart streamer
//! # let mut cart = CartStreamManual::builder(&password, 16384).build_v1()?;
//! # // Have a file to write our carted data to
//! # let mut dest = File::create("/tmp/CartedPotato").await?;
//! # // wrap our open carted file handle in a BufReader
//! # let mut reader = BufReader::new(source);
//! # // Allocate a bytesmut to write too
//! # let mut temp = BytesMut::zeroed(32_768);
//! # // read our carted file in chunks
//! # loop {
//! #   // read in some bytes
//! #   let bytes_read = reader.read(&mut temp).await.unwrap();
//! #   // check how many bytes were read
//! #   if bytes_read == 0 {
//! #       break;
//! #   }
//! #   // freeze the section of bytes we have written data too
//! #   let frozen = temp.split_to(bytes_read).freeze();
//! #   // add these bytes to our cart stream
//! #   if cart.next_bytes(frozen).unwrap() {
//! #     // keep processing these bytes until they are finished
//! #     while cart.process().unwrap() {
//! #       // if we have more then 5 MiB worth of data then write to disk
//! #       if cart.ready() >= 5_242_800 {
//! #           // get the bytes we have ready to write
//! #           let writable = cart.carted_bytes();
//! #           // write our packed bytes
//! #           dest.write_all(writable).await.unwrap();
//! #           // consume these written bytes
//! #           cart.consume();
//! #       }
//! #     }
//! #   }
//! # }
//! # // finish packing our cart file
//! # let buff = cart.finish().unwrap();
//! # // write our final bytes
//! # dest.write_all(buff).await.unwrap();
//!  // open our carted file
//!  let carted = File::open("/tmp/CartedPotato").await?;
//!  // start uncarting this tream
//!  let mut uncart = cart_rs::UncartStream::new(BufReader::new(carted));
//!  // open a file to uncart to
//!  let mut uncarted = OpenOptions::new()
//!     .read(true)
//!     .write(true)
//!     .create(true)
//!     .truncate(true)
//!     .open("/tmp/UnCartedPotato")
//!     .await?;
//!  // write our uncarted file to disk
//!  tokio::io::copy(&mut uncart, &mut uncarted).await?;
//! # // make sure the files match
//! # let uncarted = tokio::fs::read_to_string("/tmp/UnCartedPotato").await?;
//! # if uncarted != "ImMalware" {
//! #      panic!("{} is not 'ImMalware'", uncarted);
//! # }
//! # std::fs::remove_file("/tmp/MaliciousPotato")?;
//! # std::fs::remove_file("/tmp/CartedPotato")?;
//! # std::fs::remove_file("/tmp/UnCartedPotato")?;
//! # Ok(())
//! # }
//!
//! # tokio_test::block_on(async {
//! #    exec().await.unwrap()
//! # })
//!```
//!
//! ## `CaRTing` a file manually
//!
//! ```
//! use tokio::fs::File;
//! use tokio::io::{BufReader, AsyncWriteExt, AsyncReadExt};
//! use cart_rs::CartStreamManual;
//! use generic_array::{typenum::U16, GenericArray};
//! use bytes::BytesMut;
//!
//! # async fn exec() -> Result<(), cart_rs::Error> {
//! # let mut source = File::create("/tmp/WickedSquash").await?;
//! # source.write_all("ImMalware".as_bytes()).await?;
//! # // open our file in read mode
//! # let source = File::open("/tmp/WickedSquash").await?;
//! // have a cart password to use
//! let password: GenericArray<u8, U16> =
//!     GenericArray::clone_from_slice(&"SecretCornIsBest".as_bytes()[..16]);
//! // build our cart streamer
//! let mut cart = CartStreamManual::builder(&password, 16384).build_v1()?;
//! // Have a file to write our carted data to
//! let mut dest = File::create("/tmp/CartedSquash").await?;
//! // wrap our open carted file handle in a BufReader
//! let mut reader = BufReader::new(source);
//! // Allocate a bytesmut to write too
//! let mut temp = BytesMut::zeroed(32_768);
//! // read our carted file in chunks
//! loop {
//!   // read in some bytes
//!   let bytes_read = reader.read(&mut temp).await.unwrap();
//!   // check how many bytes were read
//!   if bytes_read == 0 {
//!       break;
//!   }
//!   // freeze the section of bytes we have written data too
//!   let frozen = temp.split_to(bytes_read).freeze();
//!   // add these bytes to our cart stream
//!   if cart.next_bytes(frozen).unwrap() {
//!     // keep processing these bytes until they are finished
//!     while cart.process().unwrap() {
//!       // if we have more then 5 MiB worth of data then write to disk
//!       if cart.ready() >= 5_242_800 {
//!           // get the bytes we have ready to write
//!           let writable = cart.carted_bytes();
//!           // write our packed bytes
//!           dest.write_all(writable).await.unwrap();
//!           // consume these written bytes
//!           cart.consume();
//!       }
//!     }
//!   }
//! }
//! // finish packing our cart file
//! let buff = cart.finish().unwrap();
//! // write our final bytes
//! dest.write_all(buff).await.unwrap();
//! # // validate our file was carted correctly
//! # // open our carted file
//! # let carted = File::open("/tmp/CartedSquash").await?;
//! # // start uncarting this tream
//! # let mut uncart = cart_rs::UncartStream::new(BufReader::new(carted));
//! # // open a file to uncart to
//! # let mut uncarted = tokio::fs::OpenOptions::new()
//! #    .read(true).write(true).create(true).truncate(true)
//! #    .open("/tmp/UnCartedSquash").await?;
//! # // write our uncarted file to disk
//! # tokio::io::copy(&mut uncart, &mut uncarted).await?;
//! # // make sure the files match
//! # let uncarted = tokio::fs::read_to_string("/tmp/UnCartedSquash").await?;
//! # if uncarted != "ImMalware" {
//! #      panic!("{} is not 'ImMalware'", uncarted);
//! # }
//! # std::fs::remove_file("/tmp/WickedSquash")?;
//! # std::fs::remove_file("/tmp/CartedSquash")?;
//! # std::fs::remove_file("/tmp/UnCartedSquash")?;
//! # Ok(())
//! # }
//!
//! # tokio_test::block_on(async {
//! #    exec().await.unwrap()
//! # })
//! ```
use aes_gcm::{AeadInPlace, Aes128Gcm};
use bytes::{Buf, Bytes};
use flate2::{Compress, Decompress, FlushCompress, FlushDecompress, Status};
use futures_core::ready;
use generic_array::{ArrayLength, GenericArray};
use rc4::{KeyInit, StreamCipher};
use std::convert::TryFrom;
use std::io::ErrorKind;
use std::io::prelude::*;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufRead, AsyncRead, ReadBuf};
use zstd::stream::raw::Operation;

mod errors;
mod libs;

pub use errors::Error;
pub use flate2::Compression;
pub use libs::{footer, footer::Footer, header, header::Header};

use libs::diag::UncartDiag;

/// Recommended buffer capacity for `BufReader`/`BufWriter` wrapping cart I/O.
///
/// Matches the V2 segment size and internal decryption buffer, so each
/// `poll_fill_buf` / `poll_write` can transfer a full segment in one call
/// instead of many 8 KiB default-sized calls.
pub const CART_IO_BUF_SIZE: usize = 262_144;
/// The default segment size for V2 (256 KiB)
pub const DEFAULT_SEGMENT_SIZE: u32 = 262_144;
/// The static GCM nonce used by V2. CaRT stores the key in plaintext in the
/// header, so there is no security benefit to a random nonce and so a fun easter egg string
/// about mcarson's dog named Fliffy is used.
pub static STATIC_NONCE: [u8; 12] = [70, 108, 105, 102, 102, 121, 73, 115, 71, 111, 111, 100];

/// The CaRT format version
///
/// Version 1 is the official format while version 2 is an unofficial version that
/// only Thorium supports right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CartVersion {
    /// RC4 encryption + zlib compression
    V1,
    /// AES-128-GCM encryption + zstd compression
    V2,
}

impl std::str::FromStr for CartVersion {
    type Err = &'static str;
    /// Cast a str to an `CartVersion`
    ///
    /// # Arguments
    ///
    /// * `s` - The sting to convert to a version
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // match either casing for each supported version
        match s {
            "V1" | "v1" => Ok(CartVersion::V1),
            "V2" | "v2" => Ok(CartVersion::V2),
            _ => Err("expected `V1` or `V2`"),
        }
    }
}

/// Version-specific cart behaviour. Implemented by `CartStreamV1` and `CartStreamV2`.
///
/// Each implementor owns all compressor + cipher state for its version and
/// provides the three operations that `CartStream` dispatches to:
///
/// * `write_header` – serialize the version-specific header bytes
/// * `do_poll_read` – compress and encrypt one round of input data
/// * `is_finished`  – whether the compressor has flushed all output
pub trait CartVersionSupport {
    /// Write the version-specific `CaRT` header into `buf`, returning the
    /// number of bytes written.
    ///
    /// # Arguments
    ///
    /// * `key` - The encryption key (stored in the header)
    /// * `buf` - Destination buffer (must be large enough for the header)
    fn write_header(&self, key: &[u8], buf: &mut [u8]) -> Result<usize, Error>;

    /// Compress and encrypt one round of input, writing the result into `buf`.
    ///
    /// # Arguments
    ///
    /// * `input` - The pinned input reader
    /// * `cx`    - The async task context
    /// * `buf`   - The output read buffer
    fn do_poll_read<R: AsyncBufRead>(
        &mut self,
        input: &mut Pin<&mut R>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>>;

    /// Returns `true` once the compressor has been fully flushed and all
    /// encrypted output has been delivered.
    fn is_finished(&self) -> bool;
}

/// Version 1 cart implementation: RC4 stream cipher with zlib compression.
///
/// Data flows through: input → zlib compress → RC4 encrypt → output.
/// The entire compressed+encrypted stream is written contiguously between the
/// header and footer with no framing.
pub struct CartStreamV1<T: ArrayLength<u8>> {
    /// The zlib compressor
    zlib: Compress,
    /// The RC4 stream cipher
    rc4: rc4::Rc4<T>,
    /// Whether the zlib compressor has reached `StreamEnd`
    finished: bool,
}

impl<T: ArrayLength<u8>> CartStreamV1<T> {
    /// Create a new V1 cart stream with the given key and compression level.
    ///
    /// # Arguments
    ///
    /// * `key`         - The 16-byte RC4 key
    /// * `compression` - The zlib compression level to use
    fn new(key: &GenericArray<u8, T>, compression: Compression) -> Self {
        CartStreamV1 {
            // setup our zlib compressor at the requested level
            zlib: Compress::new(compression, true),
            // setup our rc4 encryptor from the key
            rc4: rc4::Rc4::new(key),
            finished: false,
        }
    }
}

impl<T: ArrayLength<u8>> CartVersionSupport for CartStreamV1<T> {
    /// Write a cart version 1 header
    ///
    /// # Arguments
    ///
    /// * `key` - The encryption key used
    /// * `buf` - The buffer to write our header into
    fn write_header(&self, key: &[u8], buf: &mut [u8]) -> Result<usize, Error> {
        Header::write(CartVersion::V1, key, buf)?;
        Ok(header::HEADER_LEN)
    }

    /// Read and compress/encrypt data
    ///
    /// # Arguments
    ///
    /// * `input` - The reader to read from
    /// * `cx` - The context for this reader
    /// * `buf` - The buffer to write compressed/encrypted data into
    fn do_poll_read<R: AsyncBufRead>(
        &mut self,
        input: &mut Pin<&mut R>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // get the total number of bytes output by compressor before this round
        let old_total_out = self.zlib.total_out();
        // get a reference to the next unfilled chunk of the output buffer
        let output = buf.initialize_unfilled();
        // ingest and compress data until the compressor flushes output
        'compress: loop {
            // read in the next chunk of bytes
            let chunk = ready!(input.as_mut().poll_fill_buf(cx))?;
            let flush = if chunk.is_empty() {
                // input exhausted — flush the compressor's internal buffer
                FlushCompress::Finish
            } else {
                FlushCompress::None
            };
            let old_total_in = self.zlib.total_in();
            // compress the data and write it to the output buffer
            match self.zlib.compress(chunk, output, flush) {
                Ok(status) => match status {
                    Status::Ok => (),
                    Status::BufError => {
                        return Poll::Ready(Err(std::io::Error::new(
                            ErrorKind::Other,
                            "Zip Compression Buffer Error",
                        )));
                    }
                    Status::StreamEnd => {
                        self.finished = true;
                        break 'compress;
                    }
                },
                Err(err) => return Poll::Ready(Err(std::io::Error::new(ErrorKind::Other, err))),
            };
            // calculate and consume the number of input bytes eaten
            let bytes_in = (self.zlib.total_in() - old_total_in) as usize;
            input.as_mut().consume(bytes_in);
            if self.zlib.total_out() != old_total_out {
                // compressor produced output — exit the loop
                break 'compress;
            }
        }
        // calculate the number of compressed bytes produced
        let compressed_out = (self.zlib.total_out() - old_total_out) as usize;
        // encrypt the compressed data in-place
        self.rc4.apply_keystream(&mut output[..compressed_out]);
        // advance the output buffer by the number of bytes written
        buf.advance(compressed_out);
        Poll::Ready(Ok(()))
    }

    /// Returns `true` once the zlib compressor has reached `StreamEnd` and
    /// all encrypted output has been delivered.
    fn is_finished(&self) -> bool {
        self.finished
    }
}

/// Version 2 cart implementation: AES-128-GCM authenticated encryption with
/// zstd compression.
///
/// Unlike V1's contiguous stream, V2 uses **segment-based encryption**: input
/// is compressed into fixed-size segments, each encrypted as a single AES-GCM
/// operation with a 16-byte authentication tag. The output between header and
/// footer is a sequence of `[u32 LE length][ciphertext+tag]` segments.
///
/// Per-segment nonces are derived from a random base nonce stored in the header:
/// `nonce[0..8] = random`, `nonce[8..12] = segment_counter.to_be_bytes()`.
pub struct CartStreamV2 {
    /// The zstd streaming compressor
    zstd: zstd::stream::raw::Encoder<'static>,
    /// The AES-128-GCM cipher (immutable — GCM uses &self for encrypt)
    cipher: Aes128Gcm,
    /// Counter incremented for each encrypted segment (used in nonce derivation)
    segment_counter: u32,
    /// Fixed-size scratch buffer holding the current segment's compressed bytes.
    ///
    /// Allocated once and zero-initialized so zstd always writes into already
    /// initialized memory (no `unsafe` and no per-chunk re-zeroing). Only the
    /// first `segment_len` bytes are valid compressed data.
    segment_buf: Vec<u8>,
    /// Number of valid compressed bytes currently held in `segment_buf`
    segment_len: usize,
    /// Holds the length-prefixed encrypted segment ready to be drained to the reader
    output_buf: Vec<u8>,
    /// Current drain position within `output_buf`
    output_pos: usize,
    /// Maximum compressed plaintext bytes per segment before encryption
    segment_size: usize,
    /// Whether the zstd compressor is in finish mode (input exhausted)
    zstd_finishing: bool,
    /// Whether the zstd compressor has fully flushed all output
    zstd_done: bool,
    /// Whether the final segment has been encrypted
    all_encrypted: bool,
    /// Whether all encrypted output has been drained
    finished: bool,
}

impl CartStreamV2 {
    /// Create a new V2 cart stream.
    ///
    /// # Arguments
    ///
    /// * `key`          - The 16-byte AES key (as a raw slice)
    /// * `level`        - The zstd compression level
    /// * `segment_size` - Maximum compressed bytes per segment
    fn new(key: &[u8], level: i32, segment_size: usize) -> Result<Self, Error> {
        // build the zstd streaming compressor at the requested level
        let zstd = zstd::stream::raw::Encoder::new(level)
            .map_err(|e| Error::new(format!("zstd encoder init failed: {e}")))?;
        // build the AES-128-GCM cipher from the key
        let key_bytes = aes_gcm::Key::<Aes128Gcm>::from_slice(key);
        let cipher = <Aes128Gcm as aes_gcm::KeyInit>::new(key_bytes);
        // zero-initialize the segment scratch buffer once up front so zstd
        // always writes into initialized memory; a 256-byte floor keeps a tiny
        // configured segment size from starving the encoder of output space
        let scratch = segment_size.max(256);
        Ok(CartStreamV2 {
            zstd,
            cipher,
            segment_counter: 0,
            segment_buf: vec![0u8; scratch],
            segment_len: 0,
            output_buf: Vec::with_capacity(segment_size + 4 + 16),
            output_pos: 0,
            segment_size,
            zstd_finishing: false,
            zstd_done: false,
            all_encrypted: false,
            finished: false,
        })
    }

    /// Derive the GCM nonce for the current segment counter.
    ///
    /// Uses the static nonce as a base and overwrites the last 4 bytes with
    /// the big-endian segment counter.
    fn current_nonce(&self) -> aes_gcm::Nonce<generic_array::typenum::U12> {
        let mut n = STATIC_NONCE;
        n[8..12].copy_from_slice(&self.segment_counter.to_be_bytes());
        GenericArray::clone_from_slice(&n)
    }
}

impl CartVersionSupport for CartStreamV2 {
    /// Write a cart version 2 header
    ///
    /// # Arguments
    ///
    /// * `key` - The encryption key used
    /// * `buf` - The buffer to write our header into
    fn write_header(&self, key: &[u8], buf: &mut [u8]) -> Result<usize, Error> {
        Header::write(CartVersion::V2, key, buf)?;
        Ok(header::HEADER_LEN)
    }

    /// Read, compress (zstd), and encrypt (AES-GCM) data in segments.
    ///
    /// # Output format
    ///
    /// The V2 body produced between the header and footer is a sequence of
    /// length-prefixed, authenticated segments:
    ///
    /// ```text
    /// [u32 LE len][ciphertext ... ][16-byte GCM tag]   (repeated per segment)
    /// ```
    ///
    /// Input is first compressed with zstd, then each completed segment of
    /// compressed data is encrypted with a single AES-GCM operation.
    ///
    /// # General flow
    ///
    /// Each call makes at most one phase of forward progress and then returns.
    /// The work is split across four private helpers:
    ///
    /// 1. [`drain_output_buf`](Self::drain_output_buf) - flush any encrypted
    ///    bytes left over from a previous call into the caller's buffer.
    /// 2. [`poll_compress_input`](Self::poll_compress_input) - read input and
    ///    compress it into the current segment until the segment fills or the
    ///    input is exhausted.
    /// 3. [`flush_compressor`](Self::flush_compressor) - once the input is
    ///    exhausted, drain the zstd encoder's internal buffers.
    /// 4. [`encrypt_and_emit`](Self::encrypt_and_emit) - encrypt the finished
    ///    segment and write it to the caller (or stage it for later draining).
    ///
    /// # Re-entrancy / async contract
    ///
    /// This method is polled repeatedly by [`AsyncRead::poll_read`]. It either
    /// returns `Ready(Ok(()))` having written some bytes (or having made
    /// internal progress), or propagates `Poll::Pending` from
    /// `poll_compress_input` via `ready!` when the input reader has no data
    /// available yet. All progress state (`segment_counter`, `zstd_finishing`,
    /// `zstd_done`, `all_encrypted`, `segment_buf`, `output_buf`/`output_pos`)
    /// lives on `self` and persists between calls so the next poll resumes
    /// exactly where this one left off. Completion is signalled separately via
    /// [`is_finished`](Self::is_finished).
    ///
    /// # Fast vs slow emit path
    ///
    /// When the caller's `ReadBuf` has room for a whole segment, it is written
    /// straight into it. Otherwise the segment is staged in `output_buf` and
    /// drained across subsequent polls by `drain_output_buf`.
    ///
    /// # Buffering
    ///
    /// `segment_buf` is a fixed-size, zero-initialized scratch buffer; the write
    /// cursor `segment_len` tracks how many compressed bytes are valid. zstd
    /// always writes into already-initialized memory, so this method contains no
    /// `unsafe` code and never re-zeroes the buffer between chunks.
    ///
    /// # Arguments
    ///
    /// * `input` - The pinned input reader
    /// * `cx`    - The async task context
    /// * `buf`   - The output read buffer
    fn do_poll_read<R: AsyncBufRead>(
        &mut self,
        input: &mut Pin<&mut R>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // Phase 1: drain any pending encrypted output from a previous round
        if self.output_pos < self.output_buf.len() {
            self.drain_output_buf(buf);
            return Poll::Ready(Ok(()));
        }
        // Phase 2: compress input into the current segment until it fills or
        // the input is exhausted (propagating Pending if the reader stalls)
        if !self.zstd_finishing {
            ready!(self.poll_compress_input(input, cx))?;
        }
        // Phase 3: once the input is exhausted, flush the zstd encoder
        if self.zstd_finishing && !self.zstd_done {
            self.flush_compressor()?;
        }
        // Phase 4: encrypt the finished segment and emit it to the caller
        let segment_ready =
            self.segment_len > 0 && (self.segment_len >= self.segment_size || self.zstd_done);
        if segment_ready {
            self.encrypt_and_emit(buf)?;
        }
        Poll::Ready(Ok(()))
    }

    /// Returns `true` once all segments have been encrypted and their output
    /// has been fully drained.
    fn is_finished(&self) -> bool {
        self.finished
    }
}

impl CartStreamV2 {
    /// Drain staged encrypted output into the caller's read buffer.
    ///
    /// Copies as much of `output_buf[output_pos..]` as fits into `buf`,
    /// advancing `output_pos`. Once the staged output is fully drained the
    /// buffer is reset and, if the final segment has already been encrypted,
    /// the stream is marked finished.
    ///
    /// # Arguments
    ///
    /// * `buf` - The output read buffer to copy staged bytes into
    fn drain_output_buf(&mut self, buf: &mut ReadBuf<'_>) {
        // get the undrained portion of our staged output
        let available = &self.output_buf[self.output_pos..];
        // get a reference to the caller's output buffer
        let dest = buf.initialize_unfilled();
        // copy as much as fits into the caller's buffer
        let to_copy = std::cmp::min(available.len(), dest.len());
        dest[..to_copy].copy_from_slice(&available[..to_copy]);
        buf.advance(to_copy);
        // advance our drain position
        self.output_pos += to_copy;
        // if we've drained everything, reset and check if we're done
        if self.output_pos == self.output_buf.len() {
            self.output_buf.clear();
            self.output_pos = 0;
            if self.all_encrypted {
                self.finished = true;
            }
        }
    }

    /// Read input and compress it into the current segment buffer.
    ///
    /// Loops reading chunks from `input` and compressing them into
    /// `segment_buf` until the segment reaches `segment_size` or the input is
    /// exhausted (in which case `zstd_finishing` is set so the caller moves on
    /// to flushing). Returns `Poll::Pending` (via `ready!`) when the reader has
    /// no data available yet.
    ///
    /// # Arguments
    ///
    /// * `input` - The pinned input reader
    /// * `cx`    - The async task context
    fn poll_compress_input<R: AsyncBufRead>(
        &mut self,
        input: &mut Pin<&mut R>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        // loop until we have enough for one segment or there is no more data to read
        loop {
            // read in the next chunk of input bytes
            let chunk = ready!(input.as_mut().poll_fill_buf(cx))?;
            if chunk.is_empty() {
                // input exhausted — switch to flushing mode
                self.zstd_finishing = true;
                break;
            }
            // compress this chunk into the already-initialized spare region of
            // the segment buffer, starting at the current write cursor
            let status = self
                .zstd
                .run_on_buffers(chunk, &mut self.segment_buf[self.segment_len..])
                .map_err(|e| std::io::Error::new(ErrorKind::Other, e))?;
            // advance the write cursor by the compressed bytes produced
            self.segment_len += status.bytes_written;
            // mark the consumed input bytes
            input.as_mut().consume(status.bytes_read);
            if self.segment_len >= self.segment_size {
                // segment buffer is full — ready to encrypt
                break;
            }
        }
        Poll::Ready(Ok(()))
    }

    /// Flush the zstd encoder's internal buffers after input is exhausted.
    ///
    /// Repeatedly drives `zstd.finish` into `segment_buf` until the encoder
    /// reports no remaining data (setting `zstd_done`) or the segment fills (in
    /// which case the remaining output is flushed on a later call, after the
    /// full segment has been encrypted and emitted).
    fn flush_compressor(&mut self) -> std::io::Result<()> {
        // loop until there is no more data or we have a full segment
        loop {
            // flush compressed data into the already-initialized spare region of
            // the segment buffer, starting at the current write cursor
            let mut out_buf =
                zstd::stream::raw::OutBuffer::around(&mut self.segment_buf[self.segment_len..]);
            let remaining = self
                .zstd
                .finish(&mut out_buf, true)
                .map_err(|e| std::io::Error::new(ErrorKind::Other, e))?;
            // advance the write cursor by the bytes flushed
            self.segment_len += out_buf.pos();
            if remaining == 0 {
                // encoder fully flushed
                self.zstd_done = true;
                break;
            }
            if self.segment_len >= self.segment_size {
                // segment is full — encrypt it now, flush the rest next round
                break;
            }
        }
        Ok(())
    }

    /// Encrypt the current segment and emit it to the caller's buffer.
    ///
    /// Encrypts `segment_buf` in place with a per-segment AES-GCM nonce (using
    /// a detached tag to avoid growing the Vec by the 16-byte tag), then writes
    /// `[u32 LE len][ciphertext][tag]` either straight into `buf` (when it has
    /// room) or staged into `output_buf` for draining across later polls.
    ///
    /// # Arguments
    ///
    /// * `buf` - The output read buffer to emit the encrypted segment into
    fn encrypt_and_emit(&mut self, buf: &mut ReadBuf<'_>) -> std::io::Result<()> {
        // number of valid compressed bytes accumulated in this segment
        let len = self.segment_len;
        // derive the per-segment nonce
        let nonce = self.current_nonce();
        // encrypt only the valid region in-place (not the zeroed scratch tail),
        // getting the 16-byte tag separately
        let tag = self
            .cipher
            .encrypt_in_place_detached(&nonce, b"", &mut self.segment_buf[..len])
            .map_err(|_| std::io::Error::new(ErrorKind::Other, "AES-GCM encryption failed"))?;
        // calculate the total ciphertext length (encrypted data + tag)
        let ciphertext_len = (len + 16) as u32;
        // advance the segment counter for the next nonce
        self.segment_counter += 1;
        // mark all segments as encrypted if the compressor is done
        if self.zstd_done {
            self.all_encrypted = true;
        }
        // try to write directly to ReadBuf when it's large enough,
        // skipping the intermediate output_buf entirely
        let dest = buf.initialize_unfilled();
        let total_needed = 4 + len + 16;
        if dest.len() >= total_needed {
            // fast path: write [len][ciphertext][tag] straight to the caller
            dest[..4].copy_from_slice(&ciphertext_len.to_le_bytes());
            dest[4..4 + len].copy_from_slice(&self.segment_buf[..len]);
            dest[4 + len..total_needed].copy_from_slice(tag.as_slice());
            buf.advance(total_needed);
            // reset the write cursor; the scratch buffer stays allocated/initialized
            self.segment_len = 0;
            if self.all_encrypted {
                self.finished = true;
            }
        } else {
            // slow path: stage in output_buf for multi-poll draining
            self.output_buf.clear();
            // write the 4-byte length prefix
            self.output_buf
                .extend_from_slice(&ciphertext_len.to_le_bytes());
            // write the encrypted data (the valid region only)
            self.output_buf.extend_from_slice(&self.segment_buf[..len]);
            // write the 16-byte GCM authentication tag
            self.output_buf.extend_from_slice(tag.as_slice());
            self.output_pos = 0;
            // reset the write cursor; the scratch buffer stays allocated/initialized
            self.segment_len = 0;
            // drain what fits into the caller's buffer (and finish if fully drained)
            self.drain_output_buf(buf);
        }
        Ok(())
    }
}

/// Builder for constructing a `CartStream` with configurable compression/encryption.
///
/// Use `build()` or `build_v2()` for a V2 stream (AES-GCM + zstd, the default),
/// or `build_v1()` for a V1 stream (RC4 + zlib).
///
/// Currently version 2 is **not** official and as such is only recognized by Thorium
/// and other tools that use cart-rs.
///
/// # Examples
///
/// ```
/// // use CartStream directly instead of the builder type
/// use cart_rs::CartStream;
/// use generic_array::{typenum::U16, GenericArray};
///
/// # tokio_test::block_on(async {
/// # let input = std::io::Cursor::new([0u8; 32]);
/// // have a cart password to use
/// let key: GenericArray<u8, U16> =
///     GenericArray::clone_from_slice(&"SecretCornIsBest".as_bytes()[..16]);
/// // build a version 1 cart stream
/// let stream = CartStream::builder(&key, input)
///     .compression_level(6)
///     .build_v1()?;
/// # let input = std::io::Cursor::new([0u8; 32]);
/// // build a version 2 cart stream
/// let stream = CartStream::builder(&key, input)
///     .segment_size(262_144)
///     .build_v2()?;
/// # Ok::<(), cart_rs::Error>(())
/// # }).unwrap()
/// ```
pub struct CartStreamBuilder<'a, T: ArrayLength<u8>, R: AsyncBufRead> {
    /// The key used for encryption (stored in the header)
    key: &'a GenericArray<u8, T>,
    /// The input file stream to cart
    pub input: R,
    /// The zlib compression level for V1 (0–9)
    compression_level: Option<i32>,
    /// The zstd compression level for V2 (typically 1–22, default 3)
    zstd_level: Option<i32>,
    /// The segment size to set for version 2
    segment_size: Option<u32>,
}

impl<'a, T: ArrayLength<u8>, R: AsyncBufRead> CartStreamBuilder<'a, T, R> {
    /// Create a new builder with the given encryption key.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to encrypt this data with
    /// * `input` - The input stream to cart
    pub fn new(key: &'a GenericArray<u8, T>, input: R) -> Self {
        // start with no overrides; defaults are applied at build time
        Self {
            key,
            input,
            compression_level: None,
            zstd_level: None,
            segment_size: None,
        }
    }

    /// Set the zlib compression level for V1 (0–9, ignored for V2).
    ///
    /// # Arguments
    ///
    /// * `level` - The zlib compression level (0 = no compression, 9 = max)
    pub fn compression_level(mut self, level: i32) -> Self {
        self.compression_level = Some(level);
        self
    }

    /// Set the zstd compression level for V2 (typically 1–22, default 3).
    ///
    /// Lower levels are faster with slightly larger output. Level 1 is a
    /// good choice when throughput matters more than compression ratio.
    /// Ignored for V1.
    ///
    /// # Arguments
    ///
    /// * `level` - The zstd compression level (1 = fastest, 22 = max compression)
    pub fn zstd_level(mut self, level: i32) -> Self {
        self.zstd_level = Some(level);
        self
    }

    /// Set the segment size for V2 encryption (ignored for V1).
    ///
    /// Each segment of compressed data is encrypted as a single AES-GCM
    /// operation. Defaults to 256 KiB.
    ///
    /// # Arguments
    ///
    /// * `size` - The maximum number of compressed bytes per segment
    pub fn segment_size(mut self, size: u32) -> Self {
        self.segment_size = Some(size);
        self
    }

    /// Build a V1 cart stream (RC4 + zlib).
    pub fn build_v1(self) -> Result<CartStream<T, R, CartStreamV1<T>>, Error> {
        // make sure the user is using a valid key
        Header::validate_key(self.key)?;
        // use the configured zlib level or fall back to the default
        let compression = match self.compression_level {
            Some(level) => Compression::new(level as u32),
            None => Compression::default(),
        };
        // build the stream with V1 internal state
        Ok(CartStream {
            key: self.key.clone(),
            input: self.input,
            internal: CartStreamV1::new(self.key, compression),
            header_written: false,
            footer_written: false,
        })
    }

    /// Build a V2 cart stream (AES-GCM + zstd). This is the default.
    pub fn build_v2(self) -> Result<CartStream<T, R, CartStreamV2>, Error> {
        // make sure the user is using a valid key
        Header::validate_key(self.key)?;
        // use the configured zstd level or fall back to the default
        let level = self.zstd_level.unwrap_or(3);
        // use the configured segment size or fall back to the default
        let segment_size = self.segment_size.unwrap_or(DEFAULT_SEGMENT_SIZE) as usize;
        // build the V2 internal state (compressor + cipher)
        let internal = CartStreamV2::new(self.key.as_slice(), level, segment_size)?;
        // build the stream with V2 internal state
        Ok(CartStream {
            key: self.key.clone(),
            input: self.input,
            internal,
            header_written: false,
            footer_written: false,
        })
    }

    /// Build a cart stream using the default version (V2).
    pub fn build(self) -> Result<CartStream<T, R, CartStreamV2>, Error> {
        // V2 is the default version
        self.build_v2()
    }
}

/// Packs a Cart file using streaming.
///
/// Generic over:
/// * `R` – the async input reader
/// * `T` – the encryption key length (always `U16` in practice)
/// * `V` – the version implementation (`CartStreamV1` or `CartStreamV2`)
///
/// The version parameter `V` is monomorphized at compile time for zero-cost
/// dispatch. Use `CartStream::new()` for V1 or `CartStreamBuilder` for either.
#[pin_project::pin_project]
pub struct CartStream<T: ArrayLength<u8>, R: AsyncBufRead, V: CartVersionSupport> {
    /// The input file stream to cart
    #[pin]
    pub input: R,
    /// The key used for encryption (stored in the header)
    key: GenericArray<u8, T>,
    /// Version-specific compressor + cipher state
    internal: V,
    /// Signals that the header has been written
    header_written: bool,
    /// Signals that the footer has been written (stream complete)
    footer_written: bool,
}

/// The builder creation doesn't actually use the cart version generic so
/// just default to CartStreamV2
impl<T: ArrayLength<u8>, R: AsyncBufRead> CartStream<T, R, CartStreamV2> {
    /// Create a new builder for configuring and constructing a `CartStream`.
    ///
    /// # Arguments
    ///
    /// * `key`   - The 16-byte key to use when encrypting data
    /// * `input` - A reader for the file to cart
    pub fn builder(key: &GenericArray<u8, T>, input: R) -> CartStreamBuilder<'_, T, R> {
        CartStreamBuilder::new(key, input)
    }
}

impl<T: ArrayLength<u8>, R: AsyncBufRead, V: CartVersionSupport> CartStream<T, R, V> {
    /// Write the `CaRT` header to the output buffer.
    ///
    /// Delegates to the version-specific implementation to write the correct
    /// header format.
    ///
    /// # Arguments
    ///
    /// * `buf` - The output read buffer to write the header into
    fn write_header(self: Pin<&mut Self>, buf: &mut ReadBuf<'_>) -> Result<usize, std::io::Error> {
        let this = self.project();
        let output = buf.initialize_unfilled();
        let header_len = this
            .internal
            .write_header(this.key, output)
            .map_err(|err| std::io::Error::new(ErrorKind::InvalidData, err))?;
        *this.header_written = true;
        Ok(header_len)
    }

    /// Write the `CaRT` footer to the output buffer.
    ///
    /// The footer format is the same for all versions.
    ///
    /// # Arguments
    ///
    /// * `buf` - The output read buffer to write the footer into
    fn write_footer(self: Pin<&mut Self>, buf: &mut ReadBuf<'_>) -> Result<usize, std::io::Error> {
        let this = self.project();
        let output = buf.initialize_unfilled();
        Footer::write(output).map_err(|err| std::io::Error::new(ErrorKind::InvalidData, err))?;
        *this.footer_written = true;
        Ok(footer::FOOTER_LEN)
    }
}

impl<T: ArrayLength<u8>, R: AsyncBufRead, V: CartVersionSupport> AsyncRead for CartStream<T, R, V> {
    /// Poll for data, compressing and encrypting any available input.
    ///
    /// The output sequence is: header → compressed+encrypted data → footer.
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if !self.header_written {
            // write the header if it hasn't been written
            match self.write_header(buf) {
                Ok(bytes_written) => {
                    buf.advance(bytes_written);
                    Poll::Ready(Ok(()))
                }
                Err(err) => Poll::Ready(Err(err)),
            }
        } else if self.internal.is_finished() && !self.footer_written {
            // write the footer once the version impl has finished
            match self.write_footer(buf) {
                Ok(bytes_written) => {
                    buf.advance(bytes_written);
                    Poll::Ready(Ok(()))
                }
                Err(err) => Poll::Ready(Err(err)),
            }
        } else if self.footer_written {
            // signal completion
            Poll::Ready(Ok(()))
        } else {
            // delegate to the version-specific compress+encrypt logic
            let mut this = self.project();
            this.internal.do_poll_read(&mut this.input, cx, buf)
        }
    }
}

/// Version 1 uncart state: RC4 stream cipher with zlib decompression.
///
/// Owns its own decryption buffer since V1 uses a contiguous encrypted stream
/// that is decrypted in chunks.
struct UncartStreamV1 {
    /// The RC4 decryptor built from the key in the header
    rc4: rc4::Rc4<generic_array::typenum::U16>,
    /// The zlib decompressor
    zlib: Decompress,
    /// Buffer holding decrypted-but-still-compressed data
    decrypted: Vec<u8>,
    /// Start offset within the decrypted buffer for decompression
    decrypt_start: usize,
    /// End offset within the decrypted buffer for decompression
    decrypt_end: usize,
    /// Whether the zlib stream has reached `StreamEnd`
    ///
    /// Once the deflate stream ends, the only remaining bytes are the CaRT
    /// footer. Those must never be fed back into the decompressor: lenient zlib
    /// builds (zlib-ng, `miniz_oxide`) ignore the trailing bytes, but strict
    /// builds (such as the system zlib on macOS) reject them as corrupt data.
    finished: bool,
    /// The newest decrypted bytes withheld from the decompressor
    ///
    /// The final [`footer::FOOTER_LEN`] bytes of a V1 cart are the plaintext
    /// CaRT footer rather than deflate data, but a streaming reader only learns
    /// which bytes those are at EOF. We therefore always withhold the newest
    /// footer-sized suffix until later data proves it is stream data. This
    /// guarantees the decompressor never sees the footer even when a backend
    /// (such as the hardware accelerated system zlib on macOS) consumes the
    /// deflate trailer but defers reporting `StreamEnd` until the next call,
    /// leaving `finished` unset at the moment the footer would have been fed.
    carry: [u8; footer::FOOTER_LEN],
    /// How many bytes of `carry` are currently withheld
    carry_len: usize,
    /// Streaming diagnostics enabled via the `CART_DIAG` environment variable
    diag: UncartDiag,
}

impl UncartStreamV1 {
    /// The size of the internal buffer storing decrypted data
    const DECRYPTED_BUF_SIZE: usize = 262_144;

    /// Build a V1 uncart state from a parsed header.
    ///
    /// # Arguments
    ///
    /// * `hdr` - The parsed CaRT header containing the RC4 key
    fn from_header(hdr: &Header) -> Self {
        let key = GenericArray::from_slice(&hdr.key);
        UncartStreamV1 {
            rc4: rc4::Rc4::new(key),
            zlib: Decompress::new(true),
            decrypted: vec![0; Self::DECRYPTED_BUF_SIZE],
            decrypt_start: 0,
            decrypt_end: 0,
            finished: false,
            // nothing is withheld until the first chunk is decrypted
            carry: [0; footer::FOOTER_LEN],
            carry_len: 0,
            // build the opt-in diagnostics state (logs an init line when enabled)
            diag: UncartDiag::new(Self::DECRYPTED_BUF_SIZE),
        }
    }

    /// Give the decompressor one final empty flush once the input is exhausted.
    ///
    /// Some zlib builds (notably the hardware accelerated system zlib on macOS)
    /// consume the deflate trailer but defer reporting `StreamEnd` until the
    /// next inflate call. Without this final flush those builds never get to
    /// run their deferred data check, and feeding them anything else (like the
    /// trailing CaRT footer) makes that check fail with `incorrect data check`.
    /// Flushing with empty input lets the deferred check run against the
    /// trailer the backend already consumed.
    ///
    /// Returns the number of bytes flushed into `decompressed` (expected to be
    /// 0 for every known backend).
    ///
    /// # Arguments
    ///
    /// * `decompressed` - Shared decompression output buffer
    fn finalize_deferred(&mut self, decompressed: &mut [u8]) -> Result<usize, std::io::Error> {
        // snapshot the totals so we can report any final output
        let old_total_out = self.zlib.total_out();
        // hand the decompressor one final empty finish flush
        let status = self.zlib.decompress(&[], decompressed, FlushDecompress::Finish);
        // calculate any output the final flush produced
        let flushed = (self.zlib.total_out() - old_total_out) as usize;
        // track the flushed bytes in the diagnostics when enabled
        self.diag.update_out(&decompressed[..flushed]);
        // inspect the final status
        match status {
            // the deferred data check passed and the stream is complete
            Ok(Status::StreamEnd) => {
                self.finished = true;
                // report the end-of-stream diagnostic summary when enabled
                self.diag.finish(self.zlib.total_in(), self.zlib.total_out());
                Ok(flushed)
            }
            // the stream is genuinely incomplete; fall through to the EOF path
            Ok(_) => Ok(flushed),
            // the deferred data check failed
            Err(err) => {
                // dump the full diagnostic state for this failure when enabled
                self.diag.fail(
                    &err,
                    self.zlib.total_in(),
                    self.zlib.total_out(),
                    &[],
                    self.decrypt_start,
                    self.decrypt_end,
                );
                // surface the backend error and stream position so failures
                // are actionable even without diagnostics enabled
                Err(std::io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "CaRT file cannot be decompressed because data is missing/corrupted: {err:?} (zlib total_in={}, total_out={})",
                        self.zlib.total_in(),
                        self.zlib.total_out()
                    ),
                ))
            }
        }
    }

    /// Read, decrypt, decompress, and output one round of V1 data.
    ///
    /// # Arguments
    ///
    /// * `cart`                    - The pinned cart input reader
    /// * `decompressed`           - Shared decompression output buffer
    /// * `decompressed_remaining` - Bytes remaining in the decompressed buffer
    /// * `decompressed_consumed`  - Bytes already copied from the decompressed buffer
    /// * `cx`                     - The async task context
    /// * `buf`                    - The output read buffer
    fn do_poll_read<R: AsyncBufRead>(
        &mut self,
        cart: Pin<&mut R>,
        decompressed: &mut [u8],
        decompressed_remaining: &mut usize,
        decompressed_consumed: &mut usize,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let output = buf.initialize_unfilled();
        let mut cart = cart;
        let mut returned = 0;
        let mut local_remaining = *decompressed_remaining;
        let mut local_consumed = *decompressed_consumed;
        // track whether further polling could cause data loss
        let mut write_hole = false;
        'decrypt_and_decompress: loop {
            if *decompressed_remaining == 0 || local_remaining == 0 {
                // the deflate stream has ended; stop before touching the trailing
                // footer bytes so they are never fed back into the decompressor
                if self.finished {
                    break 'decrypt_and_decompress;
                }
                // need more decrypted data to decompress
                if self.decrypt_start == self.decrypt_end {
                    // read and decrypt the next chunk from the input
                    let raw = ready!(cart.as_mut().poll_fill_buf(cx))?;
                    if raw.is_empty() {
                        // the withheld bytes are the CaRT footer and are dropped;
                        // give backends that defer their end-of-stream report one
                        // final empty flush so the data check is still validated
                        if !self.finished {
                            let flushed = self.finalize_deferred(decompressed)?;
                            if flushed > 0 {
                                // stage any final flushed bytes; the next poll's
                                // drain path will deliver them to the caller
                                local_remaining = flushed;
                                local_consumed = 0;
                            }
                        }
                        // report end-of-input diagnostics once when enabled
                        self.diag
                            .eof(self.zlib.total_in(), self.zlib.total_out(), self.finished);
                        // input exhausted: break so the trailing buf.advance(returned)
                        // still delivers any bytes copied during this poll. A later
                        // poll with returned == 0 then signals true EOF to the caller.
                        // (returning here directly could end the stream early).
                        break 'decrypt_and_decompress;
                    }
                    // leave room in the decrypted buffer for the withheld prefix
                    let decompressable =
                        std::cmp::min(raw.len(), Self::DECRYPTED_BUF_SIZE - footer::FOOTER_LEN);
                    // lay any withheld bytes down ahead of the new chunk
                    let carried = self.carry_len;
                    self.decrypted[..carried].copy_from_slice(&self.carry[..carried]);
                    // decrypt the new chunk into place after the withheld bytes
                    let decrypt_output = &mut self.decrypted[carried..carried + decompressable];
                    decrypt_output.copy_from_slice(&raw[..decompressable]);
                    self.rc4.apply_keystream(decrypt_output);
                    // track this decrypted chunk in the diagnostics when enabled
                    self.diag.note_fill(decompressable);
                    self.diag.update_in(decrypt_output);
                    cart.as_mut().consume(decompressable);
                    // withhold the newest footer-sized suffix from the decompressor
                    // since it may be the CaRT footer rather than deflate data
                    let total = carried + decompressable;
                    if total <= footer::FOOTER_LEN {
                        // everything decrypted so far could still be the footer
                        self.carry[..total].copy_from_slice(&self.decrypted[..total]);
                        self.carry_len = total;
                        // nothing is feedable yet - read more input
                        self.decrypt_end = 0;
                        self.decrypt_start = 0;
                        continue 'decrypt_and_decompress;
                    }
                    // everything except the newest footer-sized suffix is feedable
                    let feed_end = total - footer::FOOTER_LEN;
                    self.carry.copy_from_slice(&self.decrypted[feed_end..total]);
                    self.carry_len = footer::FOOTER_LEN;
                    self.decrypt_end = feed_end;
                    self.decrypt_start = 0;
                    *decompressed_consumed = 0;
                }
                // decompress the decrypted data
                let dec_slice = &self.decrypted[self.decrypt_start..self.decrypt_end];
                let old_total_in = self.zlib.total_in();
                let old_total_out = self.zlib.total_out();
                let status = self
                    .zlib
                    .decompress(dec_slice, decompressed, FlushDecompress::None);
                let bytes_consumed = (self.zlib.total_in() - old_total_in) as usize;
                let bytes_written = (self.zlib.total_out() - old_total_out) as usize;
                self.decrypt_start += bytes_consumed;
                // track the inflated bytes in the diagnostics when enabled
                self.diag.update_out(&decompressed[..bytes_written]);
                // inspect the decompressor status, recording when the stream ends
                match status {
                    // the deflate stream is complete; mark it so we never feed the
                    // trailing footer bytes back into the decompressor
                    Ok(Status::StreamEnd) => {
                        self.finished = true;
                        // report the end-of-stream diagnostic summary when enabled
                        self.diag.finish(self.zlib.total_in(), self.zlib.total_out());
                    }
                    // more data is still expected, keep going
                    Ok(_) => {}
                    // zlib rejected the data as corrupt/incomplete
                    Err(err) => {
                        // dump the full diagnostic state for this failure when enabled
                        self.diag.fail(
                            &err,
                            self.zlib.total_in(),
                            self.zlib.total_out(),
                            &dec_slice[bytes_consumed.min(dec_slice.len())..],
                            self.decrypt_start,
                            self.decrypt_end,
                        );
                        // surface the backend error and stream position so failures
                        // are actionable even without diagnostics enabled
                        return Poll::Ready(Err(std::io::Error::new(
                            ErrorKind::InvalidData,
                            format!(
                                "CaRT file cannot be decompressed because data is missing/corrupted: {err:?} (zlib total_in={}, total_out={})",
                                self.zlib.total_in(),
                                self.zlib.total_out()
                            ),
                        )));
                    }
                }
                // copy as much decompressed data as fits into the output buffer
                let decompress_end = std::cmp::min(output.len() - returned, bytes_written);
                output[returned..returned + decompress_end]
                    .copy_from_slice(&decompressed[..decompress_end]);
                local_remaining = bytes_written - decompress_end;
                local_consumed = decompress_end;
                returned += decompress_end;
                if returned == output.len() || write_hole || bytes_written < output.len() {
                    break 'decrypt_and_decompress;
                }
            } else {
                // drain remaining decompressed data from a previous round
                let decompress_end = std::cmp::min(output.len() - returned, local_remaining);
                output[returned..returned + decompress_end].copy_from_slice(
                    &decompressed[local_consumed..local_consumed + decompress_end],
                );
                local_remaining -= decompress_end;
                local_consumed += decompress_end;
                returned += decompress_end;
                if returned == output.len() {
                    break 'decrypt_and_decompress;
                }
                write_hole = true;
            }
        }
        *decompressed_remaining = local_remaining;
        *decompressed_consumed = local_consumed;
        buf.advance(returned);
        Poll::Ready(Ok(()))
    }
}

/// Version 2 uncart state: AES-128-GCM authenticated decryption with zstd
/// decompression.
///
/// Reads length-prefixed encrypted segments from the input, decrypts each with
/// AES-GCM (verifying the authentication tag), then decompresses with zstd.
struct UncartStreamV2 {
    /// The AES-128-GCM cipher
    cipher: Aes128Gcm,
    /// The zstd streaming decompressor
    zstd: zstd::stream::raw::Decoder<'static>,
    /// The 12-byte base nonce (static — CaRT doesn't require nonce secrecy)
    base_nonce: [u8; 12],
    /// Counter tracking which segment we're decrypting
    segment_counter: u32,
    /// Buffer accumulating the ciphertext+tag for the current segment
    segment_buf: Vec<u8>,
    /// Expected total ciphertext+tag length for the current segment, or None
    /// if we haven't yet read the 4-byte length prefix
    expected_len: Option<u32>,
    /// Buffer for reading the 4-byte length prefix across poll boundaries
    length_buf: [u8; 4],
    /// How many bytes of the length prefix have been read so far
    length_pos: usize,
    /// Holds decrypted compressed data after AES-GCM decryption, before
    /// decompression
    decrypted_buf: Vec<u8>,
    /// Current read position within `decrypted_buf`
    decrypted_pos: usize,
}

impl UncartStreamV2 {
    /// Build a V2 uncart state from a parsed header.
    ///
    /// # Arguments
    ///
    /// * `hdr` - The parsed CaRT V2 header containing key, nonce, and segment size
    fn from_header(hdr: &Header) -> Result<Self, std::io::Error> {
        // build the AES-128-GCM cipher from the key in the header
        let key_bytes = aes_gcm::Key::<Aes128Gcm>::from_slice(&hdr.key);
        let cipher = <Aes128Gcm as aes_gcm::KeyInit>::new(key_bytes);
        // use the default segment size for pre-allocating buffers
        let segment_size = DEFAULT_SEGMENT_SIZE as usize;
        // build the zstd streaming decompressor
        let zstd = zstd::stream::raw::Decoder::new()
            .map_err(|e| std::io::Error::new(ErrorKind::Other, e))?;
        Ok(UncartStreamV2 {
            cipher,
            zstd,
            base_nonce: STATIC_NONCE,
            segment_counter: 0,
            segment_buf: Vec::with_capacity(segment_size + 16),
            expected_len: None,
            length_buf: [0u8; 4],
            length_pos: 0,
            decrypted_buf: Vec::new(),
            decrypted_pos: 0,
        })
    }

    /// Derive the GCM nonce for the current segment counter
    fn current_nonce(&self) -> aes_gcm::Nonce<generic_array::typenum::U12> {
        let mut n = self.base_nonce;
        n[8..12].copy_from_slice(&self.segment_counter.to_be_bytes());
        GenericArray::clone_from_slice(&n)
    }

    /// Read, decrypt, decompress, and output one round of V2 data.
    ///
    /// # Arguments
    ///
    /// * `cart`                    - The pinned cart input reader
    /// * `decompressed`           - Shared decompression output buffer
    /// * `decompressed_remaining` - Bytes remaining in the decompressed buffer
    /// * `decompressed_consumed`  - Bytes already copied from the decompressed buffer
    /// * `cx`                     - The async task context
    /// * `buf`                    - The output read buffer
    fn do_poll_read<R: AsyncBufRead>(
        &mut self,
        cart: Pin<&mut R>,
        decompressed: &mut [u8],
        decompressed_remaining: &mut usize,
        decompressed_consumed: &mut usize,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let output = buf.initialize_unfilled();
        let mut cart = cart;
        // Phase 1: output any remaining decompressed data from a previous round
        if *decompressed_remaining > 0 {
            // copy as much leftover decompressed data as fits into the output
            let avail = std::cmp::min(*decompressed_remaining, output.len());
            output[..avail].copy_from_slice(
                &decompressed[*decompressed_consumed..*decompressed_consumed + avail],
            );
            // update the remaining/consumed tracking
            *decompressed_remaining -= avail;
            *decompressed_consumed += avail;
            buf.advance(avail);
            return Poll::Ready(Ok(()));
        }
        // Phase 2: decompress more from the decrypted segment buffer
        if self.decrypted_pos < self.decrypted_buf.len() {
            // get the undecompressed portion of the decrypted segment
            let input_slice = &self.decrypted_buf[self.decrypted_pos..];
            // decompress into the shared decompression buffer
            let status = self
                .zstd
                .run_on_buffers(input_slice, decompressed)
                .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))?;
            // advance our read position in the decrypted buffer
            self.decrypted_pos += status.bytes_read;
            // if we've consumed all decrypted data, free the buffer
            if self.decrypted_pos == self.decrypted_buf.len() {
                self.decrypted_buf.clear();
                self.decrypted_pos = 0;
            }
            // copy as much decompressed output as fits into the caller's buffer
            let avail = std::cmp::min(status.bytes_written, output.len());
            output[..avail].copy_from_slice(&decompressed[..avail]);
            // track any leftover for the next poll
            *decompressed_remaining = status.bytes_written - avail;
            *decompressed_consumed = avail;
            buf.advance(avail);
            return Poll::Ready(Ok(()));
        }
        // Phase 3: read the 4-byte segment length prefix
        if self.expected_len.is_none() {
            // accumulate the 4-byte prefix across poll boundaries
            while self.length_pos < 4 {
                let raw = ready!(cart.as_mut().poll_fill_buf(cx))?;
                if raw.is_empty() {
                    if self.length_pos == 0 {
                        // no more segments — stream is done
                        return Poll::Ready(Ok(()));
                    }
                    return Poll::Ready(Err(std::io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "Truncated segment length prefix",
                    )));
                }
                // read as many prefix bytes as available
                let need = 4 - self.length_pos;
                let take = std::cmp::min(need, raw.len());
                self.length_buf[self.length_pos..self.length_pos + take]
                    .copy_from_slice(&raw[..take]);
                self.length_pos += take;
                cart.as_mut().consume(take);
            }
            // check if this is the footer magic ("TRAC") rather than a segment
            if &self.length_buf[..] == footer::MAGIC_NUM {
                return Poll::Ready(Ok(()));
            }
            // parse the length and prepare to accumulate the segment
            let len = u32::from_le_bytes(self.length_buf);
            self.expected_len = Some(len);
            self.length_pos = 0;
            self.segment_buf.clear();
            self.segment_buf.reserve(len as usize);
        }
        // Phase 4 + 5: read, accumulate, and decrypt the segment.
        // When the BufReader already has the full segment buffered we
        // decrypt directly from its buffer into decrypted_buf, avoiding
        // an intermediate copy through segment_buf.
        // get the expected segment length (ciphertext + 16-byte tag)
        let need = self.expected_len.unwrap() as usize;
        if need < 16 {
            return Poll::Ready(Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Encrypted segment too short for GCM tag",
            )));
        }
        // derive the nonce for this segment
        let nonce = self.current_nonce();
        // try the fast path only when we haven't accumulated anything yet: if the
        // whole segment is already sitting in the BufReader we can decrypt straight
        // out of its buffer without copying through segment_buf
        if self.segment_buf.is_empty() {
            // try the fast path: peek to see if the full segment is already buffered
            let raw = ready!(cart.as_mut().poll_fill_buf(cx))?;
            if raw.is_empty() {
                return Poll::Ready(Err(std::io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "Truncated encrypted segment",
                )));
            }
            if raw.len() >= need {
                // fast path: the entire segment is in the BufReader's buffer
                // split the ciphertext from the trailing 16-byte GCM tag
                let ciphertext_len = need - 16;
                let tag = aes_gcm::Tag::from_slice(&raw[ciphertext_len..need]);
                // copy the ciphertext into decrypted_buf and decrypt in-place
                self.decrypted_buf.clear();
                self.decrypted_buf.extend_from_slice(&raw[..ciphertext_len]);
                self.cipher
                    .decrypt_in_place_detached(&nonce, b"", &mut self.decrypted_buf, tag)
                    .map_err(|_| {
                        std::io::Error::new(
                            ErrorKind::InvalidData,
                            "AES-GCM decryption/authentication failed",
                        )
                    })?;
                // consume the segment from the input reader
                cart.as_mut().consume(need);
                // advance to the next segment
                self.segment_counter += 1;
                self.expected_len = None;
                self.decrypted_pos = 0;
            } else {
                // not enough data in one buffer — start accumulating into segment_buf
                // partial: copy what's available to start accumulating, then fall
                // through to the accumulation loop below to finish this segment in
                // this same poll (returning Ok with no output here would look like
                // EOF to the caller)
                let take = std::cmp::min(need, raw.len());
                self.segment_buf.extend_from_slice(&raw[..take]);
                cart.as_mut().consume(take);
            }
        }
        // if the fast path didn't fully decrypt the segment, accumulate the rest
        // (possibly across multiple polls) and then decrypt from segment_buf
        if self.expected_len.is_some() {
            // keep reading until we have the full ciphertext + tag
            while self.segment_buf.len() < need {
                let raw = ready!(cart.as_mut().poll_fill_buf(cx))?;
                if raw.is_empty() {
                    return Poll::Ready(Err(std::io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "Truncated encrypted segment",
                    )));
                }
                // copy as much of the segment as is available
                let remaining = need - self.segment_buf.len();
                let take = std::cmp::min(remaining, raw.len());
                self.segment_buf.extend_from_slice(&raw[..take]);
                cart.as_mut().consume(take);
            }
            // extract the 16-byte GCM tag from the end of the accumulated buffer
            let ciphertext_len = need - 16;
            let tag = aes_gcm::Tag::clone_from_slice(&self.segment_buf[ciphertext_len..need]);
            // truncate to just the ciphertext (without the tag)
            self.segment_buf.truncate(ciphertext_len);
            // decrypt the ciphertext in-place, verifying the tag
            self.cipher
                .decrypt_in_place_detached(&nonce, b"", &mut self.segment_buf, &tag)
                .map_err(|_| {
                    std::io::Error::new(
                        ErrorKind::InvalidData,
                        "AES-GCM decryption/authentication failed",
                    )
                })?;
            // move decrypted data to the decompression input buffer
            std::mem::swap(&mut self.decrypted_buf, &mut self.segment_buf);
            self.segment_buf.clear();
            // advance to the next segment
            self.segment_counter += 1;
            self.expected_len = None;
            self.decrypted_pos = 0;
        }
        // decompress the first chunk of the newly decrypted segment
        let input_slice = &self.decrypted_buf[..];
        let status = self
            .zstd
            .run_on_buffers(input_slice, decompressed)
            .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))?;
        // advance the read position in the decrypted buffer
        self.decrypted_pos = status.bytes_read;
        // if all decrypted data was consumed, free the buffer
        if self.decrypted_pos == self.decrypted_buf.len() {
            self.decrypted_buf.clear();
            self.decrypted_pos = 0;
        }
        // copy as much decompressed output as fits into the caller's buffer
        let avail = std::cmp::min(status.bytes_written, output.len());
        output[..avail].copy_from_slice(&decompressed[..avail]);
        // track any leftover for the next poll
        *decompressed_remaining = status.bytes_written - avail;
        *decompressed_consumed = avail;
        buf.advance(avail);
        Poll::Ready(Ok(()))
    }
}

/// Internal enum for version dispatch in `UncartStream`.
///
/// Unlike `CartStream` which is monomorphized at compile time, `UncartStream`
/// must detect the version at runtime from the file header. The enum holds the
/// version-specific struct after header parsing.
enum UncartInternal {
    /// Header not yet parsed
    Pending,
    /// V1: RC4 + zlib
    Version1(UncartStreamV1),
    /// V2: AES-GCM + zstd
    Version2(UncartStreamV2),
}

/// Unpacks a Cart file using streaming.
///
/// Automatically detects V1 (RC4+zlib) and V2 (AES-GCM+zstd) formats from the
/// header. This Cart file cannot have any comments for this to work.
#[pin_project::pin_project]
pub struct UncartStream<R: AsyncBufRead> {
    /// The carted stream to read from
    #[pin]
    cart: R,
    /// Shared buffer for decompressed output staging
    decompressed: Vec<u8>,
    /// Version-specific decryptor + decompressor state
    internal: UncartInternal,
    /// Bytes remaining in the decompressed buffer to be copied to the caller
    decompressed_remaining: usize,
    /// Bytes already consumed from the decompressed buffer
    decompressed_consumed: usize,
}

impl<R: AsyncBufRead> UncartStream<R> {
    /// The size of the shared decompression output buffer
    const DECOMPRESSED_BUF_SIZE: usize = 524_288;

    /// Create a new uncart stream to uncart a file with no comments.
    ///
    /// Using this to uncart a file with comments will likely fail and should
    /// not be done.
    ///
    /// # Arguments
    ///
    /// * `cart` - A reader for the file to uncart
    pub fn new(cart: R) -> Self {
        UncartStream {
            cart,
            // pre-allocate the shared decompression output buffer
            decompressed: vec![0; Self::DECOMPRESSED_BUF_SIZE],
            // the version is unknown until we parse the header on the first read
            internal: UncartInternal::Pending,
            decompressed_remaining: 0,
            decompressed_consumed: 0,
        }
    }

    /// Parse the header from the first chunk and build the version-specific
    /// internal state.
    ///
    /// # Arguments
    ///
    /// * `first_chunk` - The first bytes of the cart file (must contain the full header)
    fn build_internal(first_chunk: &[u8]) -> Result<(UncartInternal, usize), std::io::Error> {
        // make sure we have enough bytes for the header
        if first_chunk.len() < header::HEADER_LEN {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Invalid CaRT file! CaRT header is malformed or missing.",
            ));
        }
        // parse the header to determine the version and extract the key
        let hdr = Header::get(&first_chunk[..header::HEADER_LEN])
            .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))?;
        // build the version-specific internal state
        match hdr.version {
            CartVersion::V1 => {
                let v1 = UncartStreamV1::from_header(&hdr);
                Ok((UncartInternal::Version1(v1), header::HEADER_LEN))
            }
            CartVersion::V2 => {
                let v2 = UncartStreamV2::from_header(&hdr)?;
                Ok((UncartInternal::Version2(v2), header::HEADER_LEN))
            }
        }
    }
}

impl<R: AsyncBufRead> AsyncRead for UncartStream<R> {
    /// Poll to see if there is any uncarted data available.
    ///
    /// On the first call, the header is parsed and the version-specific
    /// decryptor is initialized. Subsequent calls delegate to the version
    /// struct's `do_poll_read`.
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        // parse the header on the first read, then fall through to data processing
        {
            let mut this = self.as_mut().project();
            if matches!(this.internal, UncartInternal::Pending) {
                let raw = ready!(this.cart.as_mut().poll_fill_buf(cx))?;
                if raw.is_empty() {
                    return Poll::Ready(Err(std::io::Error::new(
                        ErrorKind::InvalidData,
                        "Input file is empty!",
                    )));
                }
                let (built, hlen) = Self::build_internal(raw)?;
                *this.internal = built;
                this.cart.as_mut().consume(hlen);
            }
        }
        // delegate to the version-specific handler
        let this = self.project();
        match this.internal {
            UncartInternal::Pending => unreachable!(),
            UncartInternal::Version1(v1) => v1.do_poll_read(
                this.cart,
                this.decompressed,
                this.decompressed_remaining,
                this.decompressed_consumed,
                cx,
                buf,
            ),
            UncartInternal::Version2(v2) => v2.do_poll_read(
                this.cart,
                this.decompressed,
                this.decompressed_remaining,
                this.decompressed_consumed,
                cx,
                buf,
            ),
        }
    }
}

/// Version-specific manual cart behaviour. Implemented by `CartStreamManualV1`
/// and `CartStreamManualV2`.
///
/// Each implementor owns all compressor + cipher state, the output buffer, and
/// the on-deck/current input buffer management. `CartStreamManual<V>`
/// delegates every public method to these.
pub trait CartManualVersionSupport {
    /// Queue the next buffer of raw bytes and process the previous one.
    ///
    /// Always keeps one buffer in reserve to ensure the final write is handled
    /// correctly by `finish`.
    ///
    /// # Arguments
    ///
    /// * `raw` - The raw bytes to add to the cart stream
    fn next_bytes(&mut self, raw: Bytes) -> Result<bool, Error>;

    /// Continue processing the current input buffer.
    ///
    /// Returns `true` if there is more data to process from this buffer.
    fn process(&mut self) -> Result<bool, Error>;

    /// Get the number of carted bytes ready to be read.
    fn ready(&self) -> usize;

    /// Get a slice of the currently available carted bytes.
    fn carted_bytes(&self) -> &[u8];

    /// Mark all currently available carted bytes as consumed.
    ///
    /// After this call, `ready()` returns 0 and the internal write position
    /// is reset so new carted data will be written from the start of the
    /// output buffer.
    fn consume(&mut self);

    /// Flush all remaining data and append the CaRT footer.
    ///
    /// Processes the last buffered input, flushes the compressor, encrypts
    /// any remaining data, and writes the footer. Returns a slice containing
    /// the final carted bytes (including footer).
    fn finish(&mut self) -> Result<&[u8], Error>;
}

/// V1 manual cart: RC4 stream cipher with zlib compression.
///
/// Compresses and encrypts data directly into a pre-allocated output buffer.
/// The header is written at construction time; data is appended after it.
pub struct CartStreamManualV1<T: ArrayLength<u8>> {
    /// Write position in the output buffer
    skip: usize,
    /// The next buffer to process after the current one is consumed
    on_deck: Option<Bytes>,
    /// The current buffer being processed
    current: Option<Bytes>,
    /// The zlib compressor
    zlib: Compress,
    /// The RC4 stream cipher
    rc4: rc4::Rc4<T>,
    /// Pre-allocated output buffer (header already written at the front)
    output: Vec<u8>,
}

impl<T: ArrayLength<u8>> CartStreamManualV1<T> {
    /// Create a new V1 manual cart stream.
    ///
    /// # Arguments
    ///
    /// * `key`         - The 16-byte RC4 key
    /// * `len`         - The size of the data buffer to allocate (on top of the header)
    /// * `compression` - The zlib compression level to use
    fn new(key: &GenericArray<u8, T>, len: usize, compression: Compression) -> Result<Self, Error> {
        // pre-allocate the output buffer with the V1 header at the front
        let mut output = vec![0u8; header::HEADER_LEN + len + 768_432];
        // write the CaRT V1 header to the start of the output buffer
        Header::write(CartVersion::V1, key, &mut output[..header::HEADER_LEN])?;
        // setup our zlib compressor
        let zlib = Compress::new(compression, true);
        // setup our rc4 encrypto
        let rc4 = rc4::Rc4::new(key);
        // build a cart streamer
        Ok(CartStreamManualV1 {
            skip: header::HEADER_LEN,
            on_deck: None,
            current: None,
            zlib,
            rc4,
            output,
        })
    }

    /// Compress and encrypt the current input buffer into the output buffer.
    ///
    /// Returns `true` if the input buffer still has data remaining.
    fn cart_bytes(&mut self, flush: FlushCompress) -> Result<bool, Error> {
        let Some(buff) = self.current.as_mut() else {
            return Ok(false);
        };
        if self.skip == self.output.len() {
            return Ok(true);
        }
        let old_total = self.zlib.total_out();
        let old_in = self.zlib.total_in();
        let status = self
            .zlib
            .compress(&buff[..], &mut self.output[self.skip..], flush)
            .unwrap();
        if status == Status::BufError {
            return Err(Error::new("Zip Compression Buffer Error".to_owned()));
        }
        let zipped = usize::try_from(self.zlib.total_out() - old_total)?;
        let zip_end = self.skip + zipped;
        // encrypt the compressed data in-place
        self.rc4
            .apply_keystream(&mut self.output[self.skip..zip_end]);
        self.skip += zipped;
        let consumed = self.zlib.total_in() - old_in;
        buff.advance(consumed as usize);
        Ok(buff.has_remaining())
    }
}

impl<T: ArrayLength<u8>> CartManualVersionSupport for CartStreamManualV1<T> {
    /// Queue the next buffer of raw bytes and process the previously queued one
    ///
    /// Always keeps one buffer in reserve so the final write can be handled
    /// correctly by `finish`.
    ///
    /// # Arguments
    ///
    /// * `raw` - The raw bytes to add to the cart stream
    fn next_bytes(&mut self, raw: Bytes) -> Result<bool, Error> {
        // swap the new buffer in, keeping the previous one to process
        if let Some(old) = self.on_deck.replace(raw) {
            // set the previously queued buffer as current and cart it
            self.current = Some(old);
            self.cart_bytes(FlushCompress::Partial)
        } else {
            // this is the first buffer so there is nothing to process yet
            Ok(false)
        }
    }

    /// Continue processing the current input buffer
    fn process(&mut self) -> Result<bool, Error> {
        // compress/encrypt more of the current buffer with a partial flush
        self.cart_bytes(FlushCompress::Partial)
    }

    /// Get the number of carted bytes that are ready to be read
    fn ready(&self) -> usize {
        self.skip
    }

    /// Get a slice to the currently carted bytes
    fn carted_bytes(&self) -> &[u8] {
        &self.output[..self.skip]
    }

    /// Mark all currently available carted bytes as consumed
    fn consume(&mut self) {
        // reset our write position so new carted data overwrites the consumed output
        self.skip = 0;
    }

    /// Flush all remaining data and append the `CaRT` footer
    fn finish(&mut self) -> Result<&[u8], Error> {
        // get our last buffered input, erroring if no data was ever supplied
        let Some(buff) = self.on_deck.take() else {
            return Err(Error::FinishBeforeData);
        };
        // set it as current and cart it with a final flush
        self.current = Some(buff);
        self.cart_bytes(FlushCompress::Finish)?;
        // ensure enough room for the footer
        if self.skip + footer::FOOTER_LEN > self.output.capacity() {
            self.output.try_reserve_exact(footer::FOOTER_LEN)?;
            self.output.extend((0..footer::FOOTER_LEN).map(|_| 0));
        }
        // get a reference to the footer region of the output buffer
        let mut footer_buf = &mut self.output[self.skip..self.skip + footer::FOOTER_LEN];
        // zero out the footer region (may overlap previously carted data)
        footer_buf.fill(0);
        // write the CaRT footer magic number
        footer_buf.write_all(footer::MAGIC_NUM)?;
        Ok(&self.output[..self.skip + footer::FOOTER_LEN])
    }
}

/// V2 manual cart: AES-128-GCM authenticated encryption with zstd compression.
///
/// Compresses input into fixed-size segments, encrypts each with AES-GCM, and
/// writes `[u32 LE ciphertext+tag len][ciphertext+tag]` segments to the output
/// buffer. The header is written at construction time.
pub struct CartStreamManualV2 {
    /// Write position in the output buffer
    skip: usize,
    /// The next buffer to process after the current one is consumed
    on_deck: Option<Bytes>,
    /// The current buffer being processed
    current: Option<Bytes>,
    /// The zstd streaming compressor
    zstd: zstd::stream::raw::Encoder<'static>,
    /// The AES-128-GCM cipher
    cipher: Aes128Gcm,
    /// Counter for per-segment nonce derivation
    segment_counter: u32,
    /// Fixed-size scratch buffer holding the current segment's compressed bytes.
    ///
    /// Allocated once and zero-initialized so zstd always writes into already
    /// initialized memory (no `unsafe` and no per-chunk re-zeroing). Only the
    /// first `segment_len` bytes are valid compressed data.
    segment_buf: Vec<u8>,
    /// Number of valid compressed bytes currently held in `segment_buf`
    segment_len: usize,
    /// Maximum compressed plaintext bytes per segment before encryption
    segment_size: usize,
    /// Pre-allocated output buffer (header already written at the front)
    output: Vec<u8>,
}

impl CartStreamManualV2 {
    /// Create a new V2 manual cart stream.
    ///
    /// # Arguments
    ///
    /// * `key`          - The 16-byte AES key (as a raw slice)
    /// * `len`          - The size of the data buffer to allocate
    /// * `level`        - The zstd compression level
    /// * `segment_size` - Maximum compressed bytes per segment
    fn new(key: &[u8], len: usize, level: i32, segment_size: usize) -> Result<Self, Error> {
        // validate the key before allocating anything
        Header::validate_key(key)?;
        // pre-allocate the output buffer with the V2 header at the front
        let mut output = vec![0u8; header::HEADER_LEN + len + 768_432];
        // write the CaRT V2 header to the start of the output buffer
        Header::write(CartVersion::V2, key, &mut output[..header::HEADER_LEN])?;
        // build the zstd streaming compressor at the requested level
        let zstd = zstd::stream::raw::Encoder::new(level)
            .map_err(|e| Error::new(format!("zstd encoder init failed: {e}")))?;
        // build the AES-128-GCM cipher from the key
        let key_bytes = aes_gcm::Key::<Aes128Gcm>::from_slice(key);
        let cipher = <Aes128Gcm as aes_gcm::KeyInit>::new(key_bytes);
        // zero-initialize the segment scratch buffer once up front so zstd
        // always writes into initialized memory; a 256-byte floor keeps a tiny
        // configured segment size from starving the encoder of output space
        let scratch = segment_size.max(256);
        Ok(CartStreamManualV2 {
            skip: header::HEADER_LEN,
            on_deck: None,
            current: None,
            zstd,
            cipher,
            segment_counter: 0,
            segment_buf: vec![0u8; scratch],
            segment_len: 0,
            segment_size,
            output,
        })
    }

    /// Derive the GCM nonce for the current segment counter.
    fn current_nonce(&self) -> aes_gcm::Nonce<generic_array::typenum::U12> {
        let mut n = STATIC_NONCE;
        n[8..12].copy_from_slice(&self.segment_counter.to_be_bytes());
        GenericArray::clone_from_slice(&n)
    }

    /// Encrypt the current segment buffer and append it to the output,
    /// using detached tag to avoid extending segment_buf.
    ///
    /// Writes `[u32 LE ciphertext+tag len][ciphertext][tag]` to `output[skip..]`.
    fn flush_segment(&mut self) -> Result<(), Error> {
        // nothing to encrypt if no compressed bytes have accumulated
        if self.segment_len == 0 {
            return Ok(());
        }
        // number of valid compressed bytes in this segment
        let len = self.segment_len;
        // derive the per-segment nonce
        let nonce = self.current_nonce();
        // encrypt only the valid region in-place (not the zeroed scratch tail),
        // getting the 16-byte tag separately
        let tag = self
            .cipher
            .encrypt_in_place_detached(&nonce, b"", &mut self.segment_buf[..len])
            .map_err(|_| Error::new("AES-GCM encryption failed"))?;
        // ensure the output buffer has room for [len][ciphertext][tag]
        let needed = 4 + len + 16;
        if self.skip + needed > self.output.len() {
            self.output.resize(self.skip + needed, 0);
        }
        // write the 4-byte length prefix (ciphertext + tag size)
        let ciphertext_len = (len + 16) as u32;
        self.output[self.skip..self.skip + 4].copy_from_slice(&ciphertext_len.to_le_bytes());
        self.skip += 4;
        // write the encrypted ciphertext (the valid region only)
        self.output[self.skip..self.skip + len].copy_from_slice(&self.segment_buf[..len]);
        self.skip += len;
        // write the 16-byte GCM authentication tag
        self.output[self.skip..self.skip + 16].copy_from_slice(tag.as_slice());
        self.skip += 16;
        // reset the write cursor; the scratch buffer stays allocated/initialized
        self.segment_len = 0;
        // advance the segment counter for the next nonce
        self.segment_counter += 1;
        Ok(())
    }

    /// Compress the current input buffer into the segment buffer, flushing
    /// encrypted segments to output as they fill up.
    ///
    /// Compresses into the already-initialized spare region of `segment_buf`
    /// (tracked by `segment_len`), so no `unsafe` and no per-chunk re-zeroing
    /// are needed.
    ///
    /// Returns `true` if the input buffer still has data remaining.
    fn cart_bytes(&mut self) -> Result<bool, Error> {
        // get the current input buffer, or return false if none
        let Some(buff) = self.current.as_mut() else {
            return Ok(false);
        };
        // compress this chunk into the spare region starting at the write cursor
        let status = self
            .zstd
            .run_on_buffers(&buff[..], &mut self.segment_buf[self.segment_len..])
            .map_err(|e| Error::new(format!("zstd compression failed: {e}")))?;
        // advance the write cursor by the compressed bytes produced
        self.segment_len += status.bytes_written;
        // advance the input buffer past consumed bytes
        buff.advance(status.bytes_read);
        let has_remaining = buff.has_remaining();
        // encrypt and flush the segment if it's full
        if self.segment_len >= self.segment_size {
            self.flush_segment()?;
        }
        Ok(has_remaining)
    }
}

impl CartManualVersionSupport for CartStreamManualV2 {
    /// Queue the next buffer of raw bytes and process the previously queued one
    ///
    /// Always keeps one buffer in reserve so the final write can be handled
    /// correctly by `finish`.
    ///
    /// # Arguments
    ///
    /// * `raw` - The raw bytes to add to the cart stream
    fn next_bytes(&mut self, raw: Bytes) -> Result<bool, Error> {
        // keep one buffer in reserve; process the previous one
        if let Some(old) = self.on_deck.replace(raw) {
            // set the previously queued buffer as current and compress it
            self.current = Some(old);
            self.cart_bytes()
        } else {
            // this is the first buffer so there is nothing to process yet
            Ok(false)
        }
    }

    /// Continue processing the current input buffer
    fn process(&mut self) -> Result<bool, Error> {
        // continue compressing the current input buffer
        self.cart_bytes()
    }

    /// Get the number of carted bytes that are ready to be read
    fn ready(&self) -> usize {
        self.skip
    }

    /// Get a slice to the currently carted bytes
    fn carted_bytes(&self) -> &[u8] {
        &self.output[..self.skip]
    }

    /// Mark all currently available carted bytes as consumed
    fn consume(&mut self) {
        // reset our write position so new carted data overwrites the consumed output
        self.skip = 0;
    }

    /// Flush all remaining data and append the `CaRT` footer
    fn finish(&mut self) -> Result<&[u8], Error> {
        // get the last buffered input
        let Some(buff) = self.on_deck.take() else {
            return Err(Error::FinishBeforeData);
        };
        // set it as current and compress all remaining input
        self.current = Some(buff);
        while self.cart_bytes()? {}
        // flush the zstd encoder's internal buffers
        loop {
            // flush compressed data into the spare region at the write cursor
            let mut out_buf =
                zstd::stream::raw::OutBuffer::around(&mut self.segment_buf[self.segment_len..]);
            let remaining = self
                .zstd
                .finish(&mut out_buf, true)
                .map_err(|e| Error::new(format!("zstd finish failed: {e}")))?;
            // advance the write cursor by the bytes flushed
            self.segment_len += out_buf.pos();
            // encrypt and flush if the segment is full
            if self.segment_len >= self.segment_size {
                self.flush_segment()?;
            }
            if remaining == 0 {
                // encoder fully flushed
                break;
            }
        }
        // encrypt and flush the final (possibly partial) segment
        self.flush_segment()?;
        // ensure enough room for the footer
        if self.skip + footer::FOOTER_LEN > self.output.len() {
            self.output.resize(self.skip + footer::FOOTER_LEN, 0);
        }
        // get a reference to the footer region of the output buffer
        let mut footer_buf = &mut self.output[self.skip..self.skip + footer::FOOTER_LEN];
        // zero out the footer region (may overlap previously carted data)
        footer_buf.fill(0);
        // write the CaRT footer magic number
        footer_buf.write_all(footer::MAGIC_NUM)?;
        Ok(&self.output[..self.skip + footer::FOOTER_LEN])
    }
}

/// Builder for constructing a `CartStreamManual` with configurable version.
///
/// Use `build()` or `build_v2()` for a V2 stream (AES-GCM + zstd, the default),
/// or `build_v1()` for a V1 stream (RC4 + zlib).
pub struct CartStreamManualBuilder<'a, T: ArrayLength<u8>> {
    /// The encryption key
    key: &'a GenericArray<u8, T>,
    /// The size of the data buffer to allocate
    buf_len: usize,
    /// The zlib compression level for V1 (0–9)
    compression_level: Option<i32>,
    /// The zstd compression level for V2 (typically 1–22, default 3)
    zstd_level: Option<i32>,
    /// Segment size override for V2
    segment_size: Option<u32>,
}

impl<'a, T: ArrayLength<u8>> CartStreamManualBuilder<'a, T> {
    /// Create a new builder.
    ///
    /// # Arguments
    ///
    /// * `key`     - The 16-byte encryption key
    /// * `buf_len` - The size of the output data buffer to allocate
    pub fn new(key: &'a GenericArray<u8, T>, buf_len: usize) -> Self {
        // start with no overrides; defaults are applied at build time
        Self {
            key,
            buf_len,
            compression_level: None,
            zstd_level: None,
            segment_size: None,
        }
    }

    /// Set the zlib compression level for V1 (0–9, ignored for V2).
    ///
    /// # Arguments
    ///
    /// * `level` - The zlib compression level (0 = no compression, 9 = max)
    pub fn compression_level(mut self, level: i32) -> Self {
        self.compression_level = Some(level);
        self
    }

    /// Set the zstd compression level for V2 (typically 1–22, default 3).
    ///
    /// Lower levels are faster with slightly larger output. Level 1 is a
    /// good choice when throughput matters more than compression ratio.
    /// Ignored for V1.
    ///
    /// # Arguments
    ///
    /// * `level` - The zstd compression level (1 = fastest, 22 = max compression)
    pub fn zstd_level(mut self, level: i32) -> Self {
        self.zstd_level = Some(level);
        self
    }

    /// Set the segment size for V2 encryption (ignored for V1).
    ///
    /// # Arguments
    ///
    /// * `size` - The maximum number of compressed bytes per segment
    pub fn segment_size(mut self, size: u32) -> Self {
        self.segment_size = Some(size);
        self
    }

    /// Build a V1 manual cart stream (RC4 + zlib).
    pub fn build_v1(self) -> Result<CartStreamManual<CartStreamManualV1<T>>, Error> {
        // use the configured zlib level or fall back to the default
        let compression = match self.compression_level {
            Some(level) => Compression::new(level as u32),
            None => Compression::default(),
        };
        // build the V1 internal state and wrap it
        let internal = CartStreamManualV1::new(self.key, self.buf_len, compression)?;
        Ok(CartStreamManual { internal })
    }

    /// Build a V2 manual cart stream (AES-GCM + zstd). This is the default.
    pub fn build_v2(self) -> Result<CartStreamManual<CartStreamManualV2>, Error> {
        // use the configured zstd level or fall back to the default
        let level = self.zstd_level.unwrap_or(3);
        // use the configured segment size or fall back to the default
        let segment_size = self.segment_size.unwrap_or(DEFAULT_SEGMENT_SIZE) as usize;
        // build the V2 internal state and wrap it
        let internal =
            CartStreamManualV2::new(self.key.as_slice(), self.buf_len, level, segment_size)?;
        Ok(CartStreamManual { internal })
    }

    /// Build a manual cart stream using the default version (V2).
    pub fn build(self) -> Result<CartStreamManual<CartStreamManualV2>, Error> {
        // V2 is the default version
        self.build_v2()
    }
}

/// Packs files using the Cart format manually.
///
/// This allows users to cart files on streams of data that do not implement
/// `AsyncRead` and instead are passed in as a stream of `Bytes`.
///
/// Generic over `V` (the version implementation) for zero-cost dispatch.
/// Use `CartStreamManual::builder()` to select version and options.
pub struct CartStreamManual<V: CartManualVersionSupport = CartStreamManualV2> {
    /// Version-specific compressor + cipher state
    internal: V,
}

/// Builder entry point — available without specifying a version type.
impl CartStreamManual {
    /// Create a builder for configuring version and compression.
    ///
    /// # Arguments
    ///
    /// * `key`     - The 16-byte encryption key
    /// * `buf_len` - The size of the output data buffer to allocate
    pub fn builder<'a, T: ArrayLength<u8>>(
        key: &'a GenericArray<u8, T>,
        buf_len: usize,
    ) -> CartStreamManualBuilder<'a, T> {
        CartStreamManualBuilder::new(key, buf_len)
    }
}

/// Public API — delegates to the version-specific implementation.
impl<V: CartManualVersionSupport> CartStreamManual<V> {
    /// Add the next buffer to cart and start processing the previous one.
    ///
    /// Returns `true` if there is more data to process from the current buffer.
    ///
    /// # Arguments
    ///
    /// * `raw` - The raw bytes to add
    pub fn next_bytes(&mut self, raw: Bytes) -> Result<bool, Error> {
        self.internal.next_bytes(raw)
    }

    /// Process the next chunk of bytes in the current buffer.
    ///
    /// Returns `true` if there is more data to process.
    pub fn process(&mut self) -> Result<bool, Error> {
        self.internal.process()
    }

    /// Get the number of carted bytes that are ready to be read.
    pub fn ready(&self) -> usize {
        self.internal.ready()
    }

    /// Get a slice to the currently carted bytes.
    pub fn carted_bytes(&self) -> &[u8] {
        self.internal.carted_bytes()
    }

    /// Consume our currently carted bytes.
    ///
    /// Resets the internal write position so new carted data will overwrite
    /// the consumed output. Call this after writing `carted_bytes()` to disk
    /// or network.
    pub fn consume(&mut self) {
        self.internal.consume();
    }

    /// Finish packing this file and write the CaRT footer.
    ///
    /// Processes the last buffered input, flushes the compressor, encrypts
    /// any remaining data, and appends the footer. Returns a slice containing
    /// the final carted bytes including the footer.
    pub fn finish(&mut self) -> Result<&[u8], Error> {
        self.internal.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use generic_array::typenum::U16;
    use std::io::Cursor;
    use tokio::io::{AsyncReadExt, BufReader};

    fn make_key() -> GenericArray<u8, U16> {
        GenericArray::clone_from_slice(b"SecretCornIsBest")
    }

    async fn round_trip_v1(data: &[u8]) -> Vec<u8> {
        let key = make_key();
        let cursor = Cursor::new(data);
        let stream = CartStream::builder(&key, cursor).build_v1().unwrap();
        let mut carted = Vec::new();
        let mut reader = BufReader::new(stream);
        reader.read_to_end(&mut carted).await.unwrap();

        let cursor = Cursor::new(&carted);
        let mut uncart = UncartStream::new(cursor);
        let mut output = Vec::new();
        uncart.read_to_end(&mut output).await.unwrap();
        output
    }

    async fn round_trip_v2(data: &[u8]) -> Vec<u8> {
        let key = make_key();
        let cursor = Cursor::new(data);
        let stream = CartStreamBuilder::new(&key, cursor).build_v2().unwrap();
        let mut carted = Vec::new();
        let mut reader = BufReader::new(stream);
        reader.read_to_end(&mut carted).await.unwrap();

        let cursor = Cursor::new(&carted);
        let mut uncart = UncartStream::new(cursor);
        let mut output = Vec::new();
        uncart.read_to_end(&mut output).await.unwrap();
        output
    }

    #[tokio::test]
    async fn v1_round_trip_small() {
        let data = b"ImMalware";
        let result = round_trip_v1(data).await;
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn v1_round_trip_medium() {
        let data: Vec<u8> = (0..65_536).map(|i| (i % 251) as u8).collect();
        let result = round_trip_v1(&data).await;
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn v1_round_trip_large() {
        let data: Vec<u8> = (0..10 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        let result = round_trip_v1(&data).await;
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn v2_round_trip_small() {
        let data = b"ImMalware";
        let result = round_trip_v2(data).await;
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn v2_round_trip_1_byte() {
        let data = b"X";
        let result = round_trip_v2(data).await;
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn v2_round_trip_16_bytes() {
        let data = b"0123456789abcdef";
        let result = round_trip_v2(data).await;
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn v2_round_trip_medium() {
        let data: Vec<u8> = (0..65_536).map(|i| (i % 251) as u8).collect();
        let result = round_trip_v2(&data).await;
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn v2_round_trip_one_segment() {
        let data: Vec<u8> = (0..262_144).map(|i| (i % 251) as u8).collect();
        let result = round_trip_v2(&data).await;
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn v2_round_trip_two_segments() {
        let data: Vec<u8> = (0..262_145).map(|i| (i % 251) as u8).collect();
        let result = round_trip_v2(&data).await;
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn v2_round_trip_large() {
        let data: Vec<u8> = (0..10 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        let result = round_trip_v2(&data).await;
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn v1_builder_round_trip() {
        let key = make_key();
        let data = b"BuilderV1Test";
        let cursor = Cursor::new(data.as_slice());
        let stream = CartStreamBuilder::new(&key, cursor).build_v1().unwrap();
        let mut carted = Vec::new();
        BufReader::new(stream)
            .read_to_end(&mut carted)
            .await
            .unwrap();
        let cursor = Cursor::new(&carted);
        let mut uncart = UncartStream::new(cursor);
        let mut output = Vec::new();
        uncart.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, data);
    }

    #[tokio::test]
    async fn v2_builder_default_is_v2() {
        let key = make_key();
        let data = b"DefaultV2Test";
        let cursor = Cursor::new(data.as_slice());
        let stream = CartStreamBuilder::new(&key, cursor).build_v2().unwrap();
        let mut carted = Vec::new();
        BufReader::new(stream)
            .read_to_end(&mut carted)
            .await
            .unwrap();

        // Verify the header says version 2
        assert_eq!(&carted[0..4], b"CART");
        assert_eq!(u16::from_le_bytes([carted[4], carted[5]]), 2);

        let cursor = Cursor::new(&carted);
        let mut uncart = UncartStream::new(cursor);
        let mut output = Vec::new();
        uncart.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, data);
    }

    #[tokio::test]
    async fn v2_custom_segment_size() {
        let key = make_key();
        let data: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
        let cursor = Cursor::new(data.as_slice());
        let stream = CartStreamBuilder::new(&key, cursor)
            .segment_size(1024)
            .build_v2()
            .unwrap();
        let mut carted = Vec::new();
        BufReader::new(stream)
            .read_to_end(&mut carted)
            .await
            .unwrap();
        let cursor = Cursor::new(&carted);
        let mut uncart = UncartStream::new(cursor);
        let mut output = Vec::new();
        uncart.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, data);
    }

    #[tokio::test]
    async fn v1_carted_auto_detected() {
        let key = make_key();
        let data = b"V1AutoDetect";
        let cursor = Cursor::new(data.as_slice());
        let stream = CartStream::builder(&key, cursor).build_v1().unwrap();
        let mut carted = Vec::new();
        BufReader::new(stream)
            .read_to_end(&mut carted)
            .await
            .unwrap();
        // Verify V1 header
        assert_eq!(u16::from_le_bytes([carted[4], carted[5]]), 1);
        // UncartStream auto-detects V1
        let cursor = Cursor::new(&carted);
        let mut uncart = UncartStream::new(cursor);
        let mut output = Vec::new();
        uncart.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, data);
    }

    #[test]
    fn header_v1_parse() {
        let key = b"SecretCornIsBest";
        let mut buf = [0u8; 38];
        Header::write(CartVersion::V1, key, &mut buf).unwrap();
        let hdr = Header::get(&buf).unwrap();
        assert_eq!(hdr.version, CartVersion::V1);
        assert_eq!(hdr.key, key);
    }

    #[test]
    fn header_v2_parse() {
        let key = b"SecretCornIsBest";
        let mut buf = [0u8; 38];
        Header::write(CartVersion::V2, key, &mut buf).unwrap();
        let hdr = Header::get(&buf).unwrap();
        assert_eq!(hdr.version, CartVersion::V2);
        assert_eq!(hdr.key, key);
    }

    #[test]
    fn header_write_sets_correct_version_bytes() {
        let key = b"SecretCornIsBest";

        // V1: Header::write must set version bytes to 1
        let mut v1_buf = [0u8; 38];
        Header::write(CartVersion::V1, key, &mut v1_buf).unwrap();
        let v1_version = u16::from_le_bytes([v1_buf[4], v1_buf[5]]);
        assert_eq!(v1_version, 1, "Header::write(V1, ..) must write version 1");

        // V2: Header::write must set version bytes to 2
        let mut v2_buf = [0u8; 38];
        Header::write(CartVersion::V2, key, &mut v2_buf).unwrap();
        let v2_version = u16::from_le_bytes([v2_buf[4], v2_buf[5]]);
        assert_eq!(v2_version, 2, "Header::write(V2, ..) must write version 2");
        // both must start with the CART magic number
        assert_eq!(&v1_buf[..4], b"CART");
        assert_eq!(&v2_buf[..4], b"CART");
        // the header format is identical aside from the version bytes
        assert_eq!(&v1_buf[6..], &v2_buf[6..]);
    }

    #[tokio::test]
    async fn cart_stream_v1_writes_version_1_header() {
        let key = make_key();
        let cursor = Cursor::new(b"test".as_slice());
        let stream = CartStream::builder(&key, cursor).build_v1().unwrap();
        let mut carted = Vec::new();
        BufReader::new(stream)
            .read_to_end(&mut carted)
            .await
            .unwrap();
        assert_eq!(&carted[..4], b"CART");
        assert_eq!(
            u16::from_le_bytes([carted[4], carted[5]]),
            1,
            "CartStream::new must produce a version 1 header"
        );
    }

    #[tokio::test]
    async fn cart_stream_v2_writes_version_2_header() {
        let key = make_key();
        let cursor = Cursor::new(b"test".as_slice());
        let stream = CartStreamBuilder::new(&key, cursor).build_v2().unwrap();
        let mut carted = Vec::new();
        BufReader::new(stream)
            .read_to_end(&mut carted)
            .await
            .unwrap();
        assert_eq!(&carted[..4], b"CART");
        assert_eq!(
            u16::from_le_bytes([carted[4], carted[5]]),
            2,
            "CartStreamBuilder::build must produce a version 2 header"
        );
    }

    /// Helper: cart data using manual V1, return carted bytes
    fn manual_cart_v1(data: &[u8]) -> Vec<u8> {
        let key = make_key();
        let chunk_size = 32_768;
        let mut cart = CartStreamManual::builder(&key, chunk_size)
            .build_v1()
            .unwrap();
        let mut output = Vec::new();
        for chunk in data.chunks(chunk_size) {
            let bytes = Bytes::copy_from_slice(chunk);
            if cart.next_bytes(bytes).unwrap() {
                while cart.process().unwrap() {
                    if cart.ready() >= chunk_size {
                        output.extend_from_slice(cart.carted_bytes());
                        cart.consume();
                    }
                }
            }
        }
        output.extend_from_slice(cart.finish().unwrap());
        output
    }

    /// Helper: cart data using manual V2, return carted bytes
    fn manual_cart_v2(data: &[u8]) -> Vec<u8> {
        let key = make_key();
        let chunk_size = 32_768;
        let mut cart = CartStreamManual::builder(&key, chunk_size)
            .build_v2()
            .unwrap();
        let mut output = Vec::new();
        for chunk in data.chunks(chunk_size) {
            let bytes = Bytes::copy_from_slice(chunk);
            if cart.next_bytes(bytes).unwrap() {
                while cart.process().unwrap() {
                    if cart.ready() >= chunk_size {
                        output.extend_from_slice(cart.carted_bytes());
                        cart.consume();
                    }
                }
            }
        }
        output.extend_from_slice(cart.finish().unwrap());
        output
    }

    #[tokio::test]
    async fn v1_manual_round_trip() {
        let data: Vec<u8> = (0..65_536).map(|i| (i % 251) as u8).collect();
        let carted = manual_cart_v1(&data);
        let cursor = Cursor::new(&carted);
        let mut uncart = UncartStream::new(cursor);
        let mut output = Vec::new();
        uncart.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, data);
    }

    #[tokio::test]
    async fn v2_manual_round_trip_small() {
        let data = b"ManualV2Test";
        let carted = manual_cart_v2(data);
        let cursor = Cursor::new(&carted);
        let mut uncart = UncartStream::new(cursor);
        let mut output = Vec::new();
        uncart.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, data);
    }

    #[tokio::test]
    async fn v2_manual_round_trip_large() {
        let data: Vec<u8> = (0..10 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        let carted = manual_cart_v2(&data);
        let cursor = Cursor::new(&carted);
        let mut uncart = UncartStream::new(cursor);
        let mut output = Vec::new();
        uncart.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, data);
    }

    #[tokio::test]
    async fn v2_manual_builder_default() {
        let key = make_key();
        let data = b"ManualBuilderDefault";
        let chunk_size = 32_768;
        let mut cart = CartStreamManual::builder(&key, chunk_size).build().unwrap();
        let mut output = Vec::new();
        for chunk in data.chunks(chunk_size) {
            let bytes = Bytes::copy_from_slice(chunk);
            if cart.next_bytes(bytes).unwrap() {
                while cart.process().unwrap() {
                    if cart.ready() >= chunk_size {
                        output.extend_from_slice(cart.carted_bytes());
                        cart.consume();
                    }
                }
            }
        }
        output.extend_from_slice(cart.finish().unwrap());
        // verify the header says V2
        assert_eq!(u16::from_le_bytes([output[4], output[5]]), 2);
        // uncart and verify round-trip
        let cursor = Cursor::new(&output);
        let mut uncart = UncartStream::new(cursor);
        let mut result = Vec::new();
        uncart.read_to_end(&mut result).await.unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn header_unsupported_version() {
        let mut buf = [0u8; 38];
        Header::write(CartVersion::V1, b"SecretCornIsBest", &mut buf).unwrap();
        buf[4] = 3; // change version to 3
        buf[5] = 0;
        let err = Header::get(&buf).unwrap_err();
        assert!(matches!(err, Error::UnsupportedVersion(3)));
    }

    /// Regression test: when the underlying reader hands back a segment in
    /// chunks smaller than the whole segment, `UncartStream` must keep reading
    /// rather than returning a zero-byte read (which the caller would treat as
    /// EOF, exiting immediately with no data). A bare `Cursor` returns its whole
    /// slice at once and never exercises this path, so we wrap the carted bytes
    /// in a tiny-capacity `BufReader` to force chunked `poll_fill_buf` returns.
    #[tokio::test]
    async fn v2_uncart_chunked_reader() {
        let key = make_key();
        // generate ~1 MiB of incompressible pseudo-random data via a simple LCG so
        // the carted segment stays large (compressible data would shrink to a tiny
        // segment that fits in one buffer fill and never exercise the chunked path)
        let mut state: u32 = 0x1234_5678;
        let data: Vec<u8> = (0..1024 * 1024)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect();
        let cursor = Cursor::new(data.as_slice());
        let stream = CartStreamBuilder::new(&key, cursor).build_v2().unwrap();
        let mut carted = Vec::new();
        BufReader::new(stream)
            .read_to_end(&mut carted)
            .await
            .unwrap();
        // uncart through a 512-byte BufReader so each segment spans many fills
        let reader = BufReader::with_capacity(512, Cursor::new(&carted));
        let mut uncart = UncartStream::new(reader);
        let mut output = Vec::new();
        uncart.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, data);
    }

    /// A test reader that serves its input as a fixed list of chunks.
    ///
    /// Used to prove that V1 uncarting handles the trailing footer correctly
    /// even when it arrives in its own chunk after the deflate stream ends.
    struct ChunkedReader {
        /// The chunks to serve in order
        chunks: Vec<Vec<u8>>,
        /// The index of the chunk currently being served
        idx: usize,
        /// The read position within the current chunk
        pos: usize,
    }

    impl AsyncRead for ChunkedReader {
        /// Copy bytes from the current chunk into the caller's buffer
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            let this = self.get_mut();
            // copy from the current chunk if any remain
            if this.idx < this.chunks.len() {
                let cur = &this.chunks[this.idx][this.pos..];
                let n = std::cmp::min(cur.len(), buf.remaining());
                buf.put_slice(&cur[..n]);
                this.pos += n;
                // advance to the next chunk once this one is drained
                if this.pos >= this.chunks[this.idx].len() {
                    this.idx += 1;
                    this.pos = 0;
                }
            }
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncBufRead for ChunkedReader {
        /// Hand back the remaining bytes of the current chunk
        fn poll_fill_buf(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
            let this = self.get_mut();
            // an exhausted reader reports EOF with an empty slice
            if this.idx >= this.chunks.len() {
                return Poll::Ready(Ok(&[]));
            }
            Poll::Ready(Ok(&this.chunks[this.idx][this.pos..]))
        }

        /// Advance past `amt` consumed bytes of the current chunk
        fn consume(self: Pin<&mut Self>, amt: usize) {
            let this = self.get_mut();
            this.pos += amt;
            // advance to the next chunk once this one is drained
            if this.idx < this.chunks.len() && this.pos >= this.chunks[this.idx].len() {
                this.idx += 1;
                this.pos = 0;
            }
        }
    }

    /// Generate incompressible pseudo-random test data via a simple LCG
    ///
    /// # Arguments
    ///
    /// * `seed` - The LCG seed
    /// * `len`  - The number of bytes to generate
    fn lcg_data(seed: u32, len: usize) -> Vec<u8> {
        // run a simple LCG so the compressed stream stays non-trivial
        let mut state = seed;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect()
    }

    /// Regression test for the macOS uncart failure: the trailing 28-byte CaRT
    /// footer must never be handed to the zlib decompressor. Some backends
    /// (notably the hardware accelerated system zlib on macOS) consume the
    /// deflate trailer but defer reporting `StreamEnd` until the next inflate
    /// call, so the uncarter cannot rely on `StreamEnd` arriving before the
    /// footer would be fed; it must withhold the newest footer-sized suffix
    /// until EOF proves what it is.
    ///
    /// We serve the carted `[header + compressed]` body and the footer as two
    /// separate chunks and confirm the data still round-trips exactly.
    #[tokio::test]
    async fn v1_uncart_footer_in_separate_chunk() {
        let key = make_key();
        // incompressible pseudo-random data so the compressed stream is non-trivial
        let data = lcg_data(0x0BAD_F00D, 256 * 1024);
        // cart the data as V1
        let cursor = Cursor::new(data.as_slice());
        let stream = CartStreamBuilder::new(&key, cursor).build_v1().unwrap();
        let mut carted = Vec::new();
        BufReader::new(stream)
            .read_to_end(&mut carted)
            .await
            .unwrap();
        // split off the trailing footer so it lands in its own chunk
        let split = carted.len() - footer::FOOTER_LEN;
        let body = carted[..split].to_vec();
        let footer = carted[split..].to_vec();
        // build a reader that serves the body and footer as separate chunks
        let reader = ChunkedReader {
            chunks: vec![body, footer],
            idx: 0,
            pos: 0,
        };
        // uncart and confirm the data round-trips
        let mut uncart = UncartStream::new(reader);
        let mut output = Vec::new();
        uncart.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, data, "uncarted data must match the original");
    }

    /// The bytes after the deflate trailer must never influence uncarting: the
    /// withheld footer-sized suffix is dropped at EOF without ever reaching the
    /// decompressor, so even a wholly corrupt footer region must not break the
    /// round trip or poison a backend's deferred data check.
    #[tokio::test]
    async fn v1_uncart_ignores_garbage_footer() {
        let key = make_key();
        // incompressible pseudo-random data so the compressed stream is non-trivial
        let data = lcg_data(0xDEAD_BEEF, 256 * 1024);
        // cart the data as V1
        let cursor = Cursor::new(data.as_slice());
        let stream = CartStreamBuilder::new(&key, cursor).build_v1().unwrap();
        let mut carted = Vec::new();
        BufReader::new(stream)
            .read_to_end(&mut carted)
            .await
            .unwrap();
        // overwrite the entire footer with garbage
        let split = carted.len() - footer::FOOTER_LEN;
        carted[split..].fill(0xFF);
        // uncart and confirm the garbage footer never affected the stream
        let cursor = Cursor::new(&carted);
        let mut uncart = UncartStream::new(cursor);
        let mut output = Vec::new();
        uncart.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, data, "uncarted data must match the original");
    }
}

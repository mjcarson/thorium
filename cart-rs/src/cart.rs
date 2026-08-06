//! Writing `CaRT` files
//!
//! There are two ways to cart something and they share a codec, so they produce byte identical
//! output for the same input, key, and options:
//!
//! * [`CartStream`] wraps an [`AsyncRead`] and is itself an [`AsyncRead`] over the carted bytes.
//! * [`CartManual`] is a plain push/pull state machine for callers that already have the bytes,
//!   which is what the S3 multipart upload path needs.
//!
#![cfg_attr(feature = "v2", doc = "```")]
#![cfg_attr(not(feature = "v2"), doc = "```ignore")]
//! use cart_rs::{CartKey, CartStream};
//!
//! # fn main() -> Result<(), cart_rs::Error> {
//! let key = CartKey::from_password("SecretCornIsBest")?;
//! let stream = CartStream::builder(key, &b"EvilCorn\n"[..])
//!     .v2()
//!     .zstd_level(3)
//!     .block_size_log2(20)
//!     .build()?;
//! # let _ = stream;
//! # Ok(())
//! # }
//! ```
//!
//! `.zstd_level()` does not exist on the V1 builder and `.deflate_level()` does not exist on the
//! V2 builder, so mixing them is an unresolved method rather than an option that gets silently
//! thrown away:
//!
//! ```compile_fail
//! use cart_rs::{CartKey, CartStream};
//!
//! let key = CartKey::from_password("SecretCornIsBest").unwrap();
//! // `zstd_level` is a V2 option and this is a V1 builder, so this does not compile
//! let stream = CartStream::builder(key, &b"EvilCorn\n"[..]).v1().zstd_level(3).build();
//! ```
//!
//! Neither does `build` exist before a version has been picked, which is what stops a bare
//! `build()` from quietly defaulting to one:
//!
//! ```compile_fail
//! use cart_rs::{CartKey, CartStream};
//!
//! let key = CartKey::from_password("SecretCornIsBest").unwrap();
//! // no version has been chosen yet, so there is nothing to build
//! let stream = CartStream::builder(key, &b"EvilCorn\n"[..]).build();
//! ```
//!
//! Carting without an [`tokio::io::AsyncRead`]
//!
//! [`CartManual`] is for callers that already have the bytes and want the carted ones back as
//! owned [`Bytes`] — the S3 multipart upload path, which reads from an axum multipart field and
//! writes to a part queue, and so has no reader to wrap.
//!
//! The old version of this API could report "there is more work to do" without having made any
//! progress, which spun a tokio worker at 100% with the request hung and no timeout to break it.
//! That state does not exist here: [`CartManual::push`] always consumes everything it is given,
//! so there is nothing to loop on. Callers bound memory by bounding how much they push, not by
//! sizing an output buffer and hoping the ratio between two crates' constants holds.

use bytes::Bytes;
use std::io;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::io::{AsyncRead, ReadBuf};

use crate::error::Error;
use crate::key::CartKey;

pub mod codec;
pub mod versions;

use codec::{Encode, MAX_INPUT_CHUNK, OutBuf, new_encoder};
use versions::{
    BuilderState, CartOptions, CartVersion, Dynamic, NoVersion, V1, V1Options, V2, V2Options,
};

/// Builds a [`CartStream`]
///
/// `S` tracks which version has been chosen and controls which methods exist. It starts as
/// [`NoVersion`], which has no `build`, and becomes [`V1`], [`V2`], or [`Dynamic`] once one of
/// [`CartStreamBuilder::v1`], [`CartStreamBuilder::v2`], or [`CartStreamBuilder::version`] has
/// been called.
#[derive(Debug)]
pub struct CartStreamBuilder<R, S: BuilderState = NoVersion> {
    /// The reader whose bytes will be carted
    reader: R,
    /// The key to cart with
    key: CartKey,
    /// The version to write, meaningless until `S` is no longer [`NoVersion`]
    version: CartVersion,
    /// The options for every version, of which only the chosen version's are ever read
    options: CartOptions,
    /// The version this builder has been narrowed to
    state: PhantomData<S>,
}

impl<R, S: BuilderState> CartStreamBuilder<R, S> {
    /// Move this builder to a different typestate, keeping everything already set
    ///
    /// # Arguments
    ///
    /// * `version` - The version the new typestate stands for
    fn retarget<T: BuilderState>(self, version: CartVersion) -> CartStreamBuilder<R, T> {
        CartStreamBuilder {
            reader: self.reader,
            key: self.key,
            version,
            options: self.options,
            state: PhantomData,
        }
    }
}

impl<R> CartStreamBuilder<R, NoVersion> {
    /// Start building a `CaRT` stream
    ///
    /// # Arguments
    ///
    /// * `key` - The key to cart with
    /// * `reader` - The reader whose bytes will be carted
    pub(crate) fn new(key: CartKey, reader: R) -> Self {
        CartStreamBuilder {
            reader,
            key,
            // overwritten by whichever of the three version methods gets called
            version: CartVersion::default(),
            options: CartOptions::default(),
            state: PhantomData,
        }
    }

    /// Write the official V1 format, which every `CaRT` tool can read
    #[must_use]
    pub fn v1(self) -> CartStreamBuilder<R, V1> {
        self.retarget(CartVersion::V1)
    }

    /// Write the Thorium only V2 format
    #[must_use]
    pub fn v2(self) -> CartStreamBuilder<R, V2> {
        self.retarget(CartVersion::V2)
    }

    /// Write a version that is only known at runtime
    ///
    /// This is the escape hatch for callers whose version comes from a config file or a command
    /// line flag. It deliberately offers no per option setters, only whole `V1Options`/
    /// `V2Options` values, because a caller that does not know the version cannot know which
    /// individual option applies.
    ///
    /// # Arguments
    ///
    /// * `version` - The version to write
    #[must_use]
    pub fn version(self, version: CartVersion) -> CartStreamBuilder<R, Dynamic> {
        self.retarget(version)
    }
}

impl<R> CartStreamBuilder<R, V1> {
    /// Set the DEFLATE compression level, from 0 (store) to 9 (smallest)
    ///
    /// # Arguments
    ///
    /// * `level` - The DEFLATE level to compress with
    #[must_use]
    pub fn deflate_level(mut self, level: u8) -> Self {
        self.options.v1.deflate_level = level;
        self
    }

    /// Set every V1 option at once
    ///
    /// # Arguments
    ///
    /// * `options` - The V1 options to cart with
    #[must_use]
    pub fn options(mut self, options: V1Options) -> Self {
        self.options.v1 = options;
        self
    }

    /// Build the stream
    ///
    /// # Errors
    ///
    /// Returns an error if V1 was compiled out or an option is out of range.
    pub fn build(self) -> Result<CartStream<R>, Error> {
        CartStream::new(self.reader, self.key, self.version, self.options)
    }
}

impl<R> CartStreamBuilder<R, V2> {
    /// Set the Zstd compression level, from -131072 (fastest) to 22 (smallest)
    ///
    /// # Arguments
    ///
    /// * `level` - The Zstd level to compress with
    #[must_use]
    pub fn zstd_level(mut self, level: i32) -> Self {
        self.options.v2.zstd_level = level;
        self
    }

    /// Set the base 2 logarithm of the block size, from 10 (1 KiB) to 26 (64 MiB)
    ///
    /// Each block is one independent Zstd frame, so this trades compression ratio for how
    /// finely the file can be decompressed in parallel later. It is a logarithm rather than a
    /// size because that is what the format stores, and a size that is not a power of two
    /// simply cannot be written down.
    ///
    /// # Arguments
    ///
    /// * `log2` - The base 2 logarithm of the block size
    #[must_use]
    pub fn block_size_log2(mut self, log2: u8) -> Self {
        self.options.v2.block_size_log2 = log2;
        self
    }

    /// Set every V2 option at once
    ///
    /// # Arguments
    ///
    /// * `options` - The V2 options to cart with
    #[must_use]
    pub fn options(mut self, options: V2Options) -> Self {
        self.options.v2 = options;
        self
    }

    /// Build the stream
    ///
    /// # Errors
    ///
    /// Returns an error if V2 was compiled out or an option is out of range.
    pub fn build(self) -> Result<CartStream<R>, Error> {
        CartStream::new(self.reader, self.key, self.version, self.options)
    }
}

impl<R> CartStreamBuilder<R, Dynamic> {
    /// Set the options to use if the version turns out to be V1
    ///
    /// # Arguments
    ///
    /// * `options` - The V1 options to cart with
    #[must_use]
    pub fn v1_options(mut self, options: V1Options) -> Self {
        self.options.v1 = options;
        self
    }

    /// Set the options to use if the version turns out to be V2
    ///
    /// # Arguments
    ///
    /// * `options` - The V2 options to cart with
    #[must_use]
    pub fn v2_options(mut self, options: V2Options) -> Self {
        self.options.v2 = options;
        self
    }

    /// Set the options for every version at once
    ///
    /// # Arguments
    ///
    /// * `options` - The options to cart with
    #[must_use]
    pub fn options(mut self, options: CartOptions) -> Self {
        self.options = options;
        self
    }

    /// Build the stream
    ///
    /// # Errors
    ///
    /// Returns an error if the chosen version was compiled out or an option is out of range.
    pub fn build(self) -> Result<CartStream<R>, Error> {
        CartStream::new(self.reader, self.key, self.version, self.options)
    }
}

/// An [`AsyncRead`] that carts another [`AsyncRead`] as it is read
///
/// This holds no pinned state of its own, so it is [`Unpin`] whatever the reader is. That is
/// deliberate: four call sites hand the equivalent uncart stream to `tokio_tar::Archive` or
/// `indicatif`, which need `Send + Sync + Unpin`, and making that depend on a private field
/// means a future codec change can break a consumer instead of this crate.
pub struct CartStream<R> {
    /// The reader being carted
    reader: R,
    /// The codec for the version being written
    encoder: Box<dyn Encode>,
    /// Carted bytes waiting to be read
    out: OutBuf,
    /// Somewhere for the reader to land, allocated on first poll
    scratch: Vec<u8>,
    /// Whether the footer has been written
    done: bool,
}

impl<R> CartStream<R> {
    /// Start building a `CaRT` stream
    ///
    /// # Arguments
    ///
    /// * `key` - The key to cart with
    /// * `reader` - The reader whose bytes will be carted
    ///
    /// # Examples
    ///
    #[cfg_attr(feature = "v1", doc = "```")]
    #[cfg_attr(not(feature = "v1"), doc = "```ignore")]
    /// use cart_rs::{CartKey, CartStream};
    ///
    /// # fn main() -> Result<(), cart_rs::Error> {
    /// let key = CartKey::from_password("SecretCornIsBest")?;
    /// let stream = CartStream::builder(key, &b"EvilCorn\n"[..]).v1().build()?;
    /// # let _ = stream;
    /// # Ok(())
    /// # }
    /// ```
    pub fn builder(key: CartKey, reader: R) -> CartStreamBuilder<R, NoVersion> {
        CartStreamBuilder::new(key, reader)
    }

    /// Build a stream around an already chosen version
    ///
    /// # Arguments
    ///
    /// * `reader` - The reader whose bytes will be carted
    /// * `key` - The key to cart with
    /// * `version` - The version to write
    /// * `options` - The options for every version
    ///
    /// # Errors
    ///
    /// Returns an error if the version was compiled out or an option is out of range.
    fn new(
        reader: R,
        key: CartKey,
        version: CartVersion,
        options: CartOptions,
    ) -> Result<Self, Error> {
        Ok(CartStream {
            reader,
            encoder: new_encoder(version, key, options)?,
            out: OutBuf::new(),
            scratch: Vec::new(),
            done: false,
        })
    }

    /// The `CaRT` version this stream is writing
    pub fn version(&self) -> CartVersion {
        self.encoder.version()
    }

    /// Get back the reader being carted
    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R> std::fmt::Debug for CartStream<R> {
    /// Write out a human readable version of this stream
    ///
    /// The reader and the codec are both opaque, so only the parts worth reading are printed.
    ///
    /// # Arguments
    ///
    /// * `f` - The formatter to write to
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CartStream")
            .field("version", &self.encoder.version())
            .field("ready", &self.out.len())
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for CartStream<R> {
    /// Read carted bytes, carting more of the underlying reader as needed
    ///
    /// # Arguments
    ///
    /// * `cx` - The context to poll the underlying reader with
    /// * `buf` - The buffer to write carted bytes into
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
            // hand back whatever is already carted. this is the only place bytes leave the
            // buffer and it returns immediately, so no later branch can drop them on the floor
            if !this.out.is_empty() {
                let take = this.out.len().min(buf.remaining());
                buf.put_slice(&this.out.filled()[..take]);
                this.out.advance(take);
                return Poll::Ready(Ok(()));
            }
            // with the buffer drained and the footer written, this is the one real end of file
            if this.done {
                return Poll::Ready(Ok(()));
            }
            // give the reader somewhere to land the first time we need it
            if this.scratch.is_empty() {
                this.scratch.resize(MAX_INPUT_CHUNK, 0);
            }
            // pull in more plaintext, leaving the waker to the reader if it is not ready
            let mut scratch = ReadBuf::new(this.scratch.as_mut_slice());
            ready!(Pin::new(&mut this.reader).poll_read(cx, &mut scratch))?;
            let plain = scratch.filled();
            if plain.is_empty() {
                // the plaintext ended, so flush the compressor and write the footer
                this.encoder.finish(&mut this.out)?;
                this.done = true;
            } else {
                this.encoder.push(plain, &mut this.out)?;
            }
        }
    }
}

/// Builds a [`CartManual`]
///
/// `S` tracks which version has been chosen and controls which methods exist, exactly as it
/// does on [`crate::CartStreamBuilder`].
#[derive(Debug)]
pub struct CartManualBuilder<S: BuilderState = NoVersion> {
    /// The key to cart with
    key: CartKey,
    /// The version to write, meaningless until `S` is no longer [`NoVersion`]
    version: CartVersion,
    /// The options for every version, of which only the chosen version's are ever read
    options: CartOptions,
    /// The version this builder has been narrowed to
    state: PhantomData<S>,
}

impl<S: BuilderState> CartManualBuilder<S> {
    /// Move this builder to a different typestate, keeping everything already set
    ///
    /// # Arguments
    ///
    /// * `version` - The version the new typestate stands for
    fn retarget<T: BuilderState>(self, version: CartVersion) -> CartManualBuilder<T> {
        CartManualBuilder {
            key: self.key,
            version,
            options: self.options,
            state: PhantomData,
        }
    }
}

impl CartManualBuilder<NoVersion> {
    /// Start building a manual carter
    ///
    /// # Arguments
    ///
    /// * `key` - The key to cart with
    pub(crate) fn new(key: CartKey) -> Self {
        CartManualBuilder {
            key,
            // overwritten by whichever of the three version methods gets called
            version: CartVersion::default(),
            options: CartOptions::default(),
            state: PhantomData,
        }
    }

    /// Write the official V1 format, which every `CaRT` tool can read
    #[must_use]
    pub fn v1(self) -> CartManualBuilder<V1> {
        self.retarget(CartVersion::V1)
    }

    /// Write the Thorium only V2 format
    #[must_use]
    pub fn v2(self) -> CartManualBuilder<V2> {
        self.retarget(CartVersion::V2)
    }

    /// Write a version that is only known at runtime
    ///
    /// # Arguments
    ///
    /// * `version` - The version to write
    #[must_use]
    pub fn version(self, version: CartVersion) -> CartManualBuilder<Dynamic> {
        self.retarget(version)
    }
}

impl CartManualBuilder<V1> {
    /// Set the DEFLATE compression level, from 0 (store) to 9 (smallest)
    ///
    /// # Arguments
    ///
    /// * `level` - The DEFLATE level to compress with
    #[must_use]
    pub fn deflate_level(mut self, level: u8) -> Self {
        self.options.v1.deflate_level = level;
        self
    }

    /// Set every V1 option at once
    ///
    /// # Arguments
    ///
    /// * `options` - The V1 options to cart with
    #[must_use]
    pub fn options(mut self, options: V1Options) -> Self {
        self.options.v1 = options;
        self
    }

    /// Build the manual carter
    ///
    /// # Errors
    ///
    /// Returns an error if V1 was compiled out or an option is out of range.
    pub fn build(self) -> Result<CartManual, Error> {
        CartManual::new(self.key, self.version, self.options)
    }
}

impl CartManualBuilder<V2> {
    /// Set the Zstd compression level, from -131072 (fastest) to 22 (smallest)
    ///
    /// # Arguments
    ///
    /// * `level` - The Zstd level to compress with
    #[must_use]
    pub fn zstd_level(mut self, level: i32) -> Self {
        self.options.v2.zstd_level = level;
        self
    }

    /// Set the base 2 logarithm of the block size, from 10 (1 KiB) to 26 (64 MiB)
    ///
    /// # Arguments
    ///
    /// * `log2` - The base 2 logarithm of the block size
    #[must_use]
    pub fn block_size_log2(mut self, log2: u8) -> Self {
        self.options.v2.block_size_log2 = log2;
        self
    }

    /// Set every V2 option at once
    ///
    /// # Arguments
    ///
    /// * `options` - The V2 options to cart with
    #[must_use]
    pub fn options(mut self, options: V2Options) -> Self {
        self.options.v2 = options;
        self
    }

    /// Build the manual carter
    ///
    /// # Errors
    ///
    /// Returns an error if V2 was compiled out or an option is out of range.
    pub fn build(self) -> Result<CartManual, Error> {
        CartManual::new(self.key, self.version, self.options)
    }
}

impl CartManualBuilder<Dynamic> {
    /// Set the options to use if the version turns out to be V1
    ///
    /// # Arguments
    ///
    /// * `options` - The V1 options to cart with
    #[must_use]
    pub fn v1_options(mut self, options: V1Options) -> Self {
        self.options.v1 = options;
        self
    }

    /// Set the options to use if the version turns out to be V2
    ///
    /// # Arguments
    ///
    /// * `options` - The V2 options to cart with
    #[must_use]
    pub fn v2_options(mut self, options: V2Options) -> Self {
        self.options.v2 = options;
        self
    }

    /// Set the options for every version at once
    ///
    /// # Arguments
    ///
    /// * `options` - The options to cart with
    #[must_use]
    pub fn options(mut self, options: CartOptions) -> Self {
        self.options = options;
        self
    }

    /// Build the manual carter
    ///
    /// # Errors
    ///
    /// Returns an error if the chosen version was compiled out or an option is out of range.
    pub fn build(self) -> Result<CartManual, Error> {
        CartManual::new(self.key, self.version, self.options)
    }
}

/// Carts bytes that are pushed in rather than read from a reader
///
/// Push plaintext with [`CartManual::push`], take carted bytes out with
/// [`CartManual::take_up_to`], and close the file with [`CartManual::finish`]. Output is only
/// ever appended, so a caller may push and take in whatever order suits it.
///
/// # Examples
///
#[cfg_attr(feature = "v2", doc = "```")]
#[cfg_attr(not(feature = "v2"), doc = "```ignore")]
/// use cart_rs::{CartKey, CartManual};
///
/// # fn main() -> Result<(), cart_rs::Error> {
/// let key = CartKey::from_password("SecretCornIsBest")?;
/// let mut cart = CartManual::builder(key).v2().build()?;
/// cart.push(b"EvilCorn\n")?;
/// cart.finish()?;
/// let carted = cart.take();
/// assert_eq!(&carted[..4], b"CART");
/// # Ok(())
/// # }
/// ```
pub struct CartManual {
    /// The codec for the version being written
    encoder: Box<dyn Encode>,
    /// Carted bytes waiting to be taken
    out: OutBuf,
    /// Whether the footer has been written
    finished: bool,
}

impl CartManual {
    /// Start building a manual carter
    ///
    /// # Arguments
    ///
    /// * `key` - The key to cart with
    pub fn builder(key: CartKey) -> CartManualBuilder<NoVersion> {
        CartManualBuilder::new(key)
    }

    /// Build a manual carter around an already chosen version
    ///
    /// # Arguments
    ///
    /// * `key` - The key to cart with
    /// * `version` - The version to write
    /// * `options` - The options for every version
    ///
    /// # Errors
    ///
    /// Returns an error if the version was compiled out or an option is out of range.
    fn new(key: CartKey, version: CartVersion, options: CartOptions) -> Result<Self, Error> {
        Ok(CartManual {
            encoder: new_encoder(version, key, options)?,
            out: OutBuf::new(),
            finished: false,
        })
    }

    /// The `CaRT` version this carter is writing
    pub fn version(&self) -> CartVersion {
        self.encoder.version()
    }

    /// Cart some plaintext
    ///
    /// Every byte is always consumed, so there is no partial push to loop over and no way to
    /// ask for more work without making progress. Whatever comes out is appended to the buffer
    /// [`CartManual::ready`] reports on, so a caller that pushes a gigabyte in one call gets a
    /// gigabyte of carted output to drain.
    ///
    /// # Arguments
    ///
    /// * `data` - The plaintext to cart
    ///
    /// # Errors
    ///
    /// Returns an error if compression fails or the file was already finished.
    pub fn push(&mut self, data: &[u8]) -> Result<(), Error> {
        self.encoder.push(data, &mut self.out)
    }

    /// Flush the compressor and write the `CaRT` footer
    ///
    /// The footer is only appended once the body is genuinely complete, so a caller cannot end
    /// up with a truncated body under a well formed footer.
    ///
    /// # Errors
    ///
    /// Returns an error if compression fails or the file was already finished.
    pub fn finish(&mut self) -> Result<(), Error> {
        self.encoder.finish(&mut self.out)?;
        self.finished = true;
        Ok(())
    }

    /// Whether the footer has been written
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// How many carted bytes are waiting to be taken
    pub fn ready(&self) -> usize {
        self.out.len()
    }

    /// Whether there are no carted bytes waiting to be taken
    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    /// Take up to `want` carted bytes
    ///
    /// This hands over a slice of the buffer rather than copying it, which is what lets the S3
    /// path build an exactly part sized `Bytes` for a retryable upload body without the per
    /// part copy it used to make.
    ///
    /// # Arguments
    ///
    /// * `want` - The most bytes to take, clamped to [`CartManual::ready`]
    pub fn take_up_to(&mut self, want: usize) -> Bytes {
        self.out.take_up_to(want)
    }

    /// Take every carted byte that is waiting
    pub fn take(&mut self) -> Bytes {
        self.out.take()
    }
}

impl std::fmt::Debug for CartManual {
    /// Write out a human readable version of this carter
    ///
    /// # Arguments
    ///
    /// * `f` - The formatter to write to
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CartManual")
            .field("version", &self.encoder.version())
            .field("ready", &self.out.len())
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncReadExt;

    use super::{CartManual, CartVersion};
    use crate::cart::CartStream;
    use crate::key::CartKey;

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

    #[tokio::test]
    /// Both versions cart a stream end to end
    async fn both_versions_cart_a_stream() {
        for &version in VERSIONS {
            let plain = b"EvilCorn\n".repeat(1000);
            let mut stream = CartStream::builder(key(), plain.as_slice())
                .version(version)
                .build()
                .unwrap();
            assert_eq!(stream.version(), version);
            let mut carted = Vec::new();
            stream.read_to_end(&mut carted).await.unwrap();
            // every CaRT file starts with the magic number and the version
            assert_eq!(&carted[..4], b"CART");
            assert_eq!(&carted[4..6], &version.as_u16().to_le_bytes());
            assert_eq!(&carted[carted.len() - 28..carted.len() - 24], b"TRAC");
        }
    }

    #[cfg(feature = "v1")]
    #[tokio::test]
    /// A caller with no room is not told the stream ended
    async fn a_full_buffer_is_not_end_of_file() {
        let mut stream = CartStream::builder(key(), &b"EvilCorn\n"[..])
            .v1()
            .build()
            .unwrap();
        // reading into an empty buffer must report zero bytes without consuming anything
        assert_eq!(stream.read(&mut []).await.unwrap(), 0);
        let mut carted = Vec::new();
        stream.read_to_end(&mut carted).await.unwrap();
        assert_eq!(&carted[..4], b"CART");
    }

    #[tokio::test]
    /// A zero byte file still produces a well formed `CaRT`
    async fn a_zero_byte_file_carts() {
        for &version in VERSIONS {
            let mut stream = CartStream::builder(key(), &[][..])
                .version(version)
                .build()
                .unwrap();
            let mut carted = Vec::new();
            stream.read_to_end(&mut carted).await.unwrap();
            assert_eq!(&carted[..4], b"CART");
            assert_eq!(&carted[carted.len() - 28..carted.len() - 24], b"TRAC");
        }
    }

    #[test]
    /// The stream stays usable by `tokio_tar` and `indicatif`
    fn the_stream_is_send_sync_and_unpin() {
        /// Fail to compile if `T` lost one of the auto traits our consumers need
        const fn assert<T: Send + Sync + Unpin>() {}
        assert::<CartStream<tokio::io::Empty>>();
    }

    /// Build a plaintext of the given length that does not compress to nothing
    fn plaintext(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[tokio::test]
    /// The manual and streaming APIs write byte identical files
    async fn the_manual_api_matches_the_stream() {
        for &version in VERSIONS {
            for len in [0, 1, 4096, 300_000] {
                let plain = plaintext(len);
                // cart it through the stream
                let mut stream = CartStream::builder(key(), plain.as_slice())
                    .version(version)
                    .build()
                    .unwrap();
                let mut streamed = Vec::new();
                stream.read_to_end(&mut streamed).await.unwrap();
                // and cart it again by hand, in awkwardly sized pushes
                let mut cart = CartManual::builder(key()).version(version).build().unwrap();
                let mut manual = Vec::new();
                for chunk in plain.chunks(1000).filter(|chunk| !chunk.is_empty()) {
                    cart.push(chunk).unwrap();
                    manual.extend_from_slice(&cart.take_up_to(333));
                }
                cart.finish().unwrap();
                manual.extend_from_slice(&cart.take());
                assert!(cart.is_empty());
                // V2 salts every file differently, so only V1 can be compared byte for byte
                if version == CartVersion::V1 {
                    assert_eq!(streamed, manual, "{version} at {len} bytes");
                } else {
                    assert_eq!(streamed.len(), manual.len(), "{version} at {len} bytes");
                }
            }
        }
    }

    #[cfg(feature = "v1")]
    #[test]
    /// Taking part of the output leaves the rest in order
    fn a_partial_take_leaves_the_rest() {
        let mut cart = CartManual::builder(key()).v1().build().unwrap();
        cart.push(&plaintext(100_000)).unwrap();
        cart.finish().unwrap();
        assert!(cart.is_finished());
        // drain it 7 bytes at a time and make sure that is the same as taking it all at once
        let mut drained = Vec::new();
        while !cart.is_empty() {
            let before = cart.ready();
            let chunk = cart.take_up_to(7);
            assert_eq!(chunk.len(), 7.min(before));
            assert_eq!(cart.ready(), before - chunk.len());
            drained.extend_from_slice(&chunk);
        }
        assert_eq!(&drained[..4], b"CART");
        assert_eq!(&drained[drained.len() - 28..drained.len() - 24], b"TRAC");
    }

    #[cfg(feature = "v2")]
    #[test]
    /// Finishing twice is an error rather than a second footer
    fn finishing_twice_is_an_error() {
        let mut cart = CartManual::builder(key()).v2().build().unwrap();
        cart.finish().unwrap();
        assert!(cart.finish().is_err());
    }

    #[test]
    /// The carter can be moved between tokio worker threads
    fn the_carter_is_send_sync_and_unpin() {
        /// Fail to compile if `T` lost one of the auto traits our consumers need
        const fn assert<T: Send + Sync + Unpin>() {}
        assert::<CartManual>();
    }
}

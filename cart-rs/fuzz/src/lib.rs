//! Shared scaffolding for the cart-rs fuzz targets
//!
//! Every target needs the same three things: a key, a way to feed a stream its bytes on a
//! schedule the fuzzer controls, and a way to read a stream to the end through a buffer whose
//! size the fuzzer also controls. Those live here so the targets themselves stay short enough
//! to read in one screen.

use std::pin::Pin;
use std::task::{Context, Poll};

use arbitrary::Arbitrary;
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use cart_rs::{CartKey, CartVersion};

/// Build the key every target carts with
///
/// `CaRT` stores its key in the header in plaintext by design, so there is nothing to gain from
/// fuzzing the key itself; a fixed one keeps the corpus focused on the parts that can break.
///
/// # Panics
///
/// Panics if the fixed test password is somehow not a valid key, which would be a bug in
/// [`CartKey::from_password`] rather than in the target.
#[must_use]
pub fn key() -> CartKey {
    CartKey::from_password("SecretCornIsBest").expect("the fixed password is a valid key")
}

/// How the fuzzer wants a stream driven
///
/// Deriving [`Arbitrary`] rather than pulling raw integers means libFuzzer's mutations move the
/// schedule around in structured steps, so it can learn that a one byte read buffer reaches
/// code a large one does not.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub struct Schedule {
    /// The most bytes the reader hands over per poll, before clamping
    chunk: u16,
    /// How often the reader stalls, before clamping
    stall_every: u8,
    /// The size of the buffer to read through, before clamping
    buf: u16,
}

impl Schedule {
    /// The most bytes the reader should hand over in a single poll
    ///
    /// Clamped to at least one so the reader always makes progress.
    #[must_use]
    pub fn chunk(self) -> usize {
        usize::from(self.chunk).max(1)
    }

    /// The size of the buffer to read through
    ///
    /// Clamped to at least one, because a zero length read buffer means end of file to
    /// [`AsyncRead`] and would make every target trivially "pass".
    #[must_use]
    pub fn buf(self) -> usize {
        usize::from(self.buf).max(1)
    }

    /// How many polls happen between stalls, or zero to never stall
    ///
    /// A period of one is folded to zero. A reader that stalls on every poll never makes
    /// progress, so it would hang the target and be reported as a timeout rather than finding
    /// anything.
    #[must_use]
    pub fn stall_every(self) -> u32 {
        match u32::from(self.stall_every) {
            0 | 1 => 0,
            other => other,
        }
    }
}

/// A reader that hands out its data in small pieces and stalls on a schedule
///
/// A `&[u8]` never stalls and never hands back less than it was asked for, so a stream read
/// straight out of a slice never visits its own resumption paths. This makes it do both.
pub struct StallingReader<'a> {
    /// The bytes left to hand out
    data: &'a [u8],
    /// The schedule to hand them out on
    schedule: Schedule,
    /// How many times this reader has been polled
    polls: u32,
}

impl<'a> StallingReader<'a> {
    /// Wrap some bytes in a reader that stalls and short reads
    ///
    /// # Arguments
    ///
    /// * `data` - The bytes to hand out
    /// * `schedule` - How awkwardly to hand them out
    #[must_use]
    pub fn new(data: &'a [u8], schedule: Schedule) -> Self {
        StallingReader {
            data,
            schedule,
            polls: 0,
        }
    }
}

impl AsyncRead for StallingReader<'_> {
    /// Hand over at most one chunk, or stall if this poll is a scheduled stall
    ///
    /// # Arguments
    ///
    /// * `cx` - The context to register a waker against when stalling
    /// * `buf` - The buffer to read into
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // this reader holds nothing pinned, so it can be unpinned freely
        let this = self.get_mut();
        // count this poll
        this.polls = this.polls.wrapping_add(1);
        // stall if this poll is a scheduled stall, waking ourselves so the runtime comes
        // straight back rather than parking forever
        let period = this.schedule.stall_every();
        if period != 0 && this.polls.is_multiple_of(period) {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        // hand over at most a chunk, and never more than the caller asked for
        let take = this.schedule.chunk().min(buf.remaining()).min(this.data.len());
        buf.put_slice(&this.data[..take]);
        this.data = &this.data[take..];
        Poll::Ready(Ok(()))
    }
}

/// Read a stream to the end through a fixed size buffer
///
/// # Arguments
///
/// * `reader` - The stream to read to the end
/// * `buf_size` - The size of the buffer to read through
///
/// # Errors
///
/// Returns whatever error the stream produced.
pub async fn drain<R: AsyncRead + Unpin>(
    mut reader: R,
    buf_size: usize,
) -> std::io::Result<Vec<u8>> {
    // read through a buffer of exactly the size the fuzzer asked for
    let mut scratch = vec![0u8; buf_size];
    let mut out = Vec::new();
    loop {
        let read = reader.read(&mut scratch).await?;
        // a zero length read is the only end of file signal AsyncRead has
        if read == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&scratch[..read]);
    }
}

/// Run a future to completion on a single threaded runtime
///
/// # Arguments
///
/// * `future` - The future to run
///
/// # Panics
///
/// Panics if the runtime cannot be built, which would be an environment failure rather than a
/// finding.
pub fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("failed to build the fuzz runtime")
        .block_on(future)
}

/// Pick one of the `CaRT` versions to write
///
/// This crate depends on cart-rs with its default features, so both codecs are always compiled
/// in and there is no feature gating to do here. The assertion is there because a `cfg(feature
/// = "v1")` written in this crate would silently refer to *this* crate's features rather than
/// cart-rs's, and would compile to a target that quietly never fuzzed anything.
///
/// # Arguments
///
/// * `pick` - An arbitrary byte from the fuzzer to choose with
#[must_use]
pub fn version(pick: u8) -> CartVersion {
    debug_assert!(
        CartVersion::V1.is_enabled() && CartVersion::V2.is_enabled(),
        "the fuzz targets expect a cart-rs built with both codecs",
    );
    if pick.is_multiple_of(2) {
        CartVersion::V1
    } else {
        CartVersion::V2
    }
}

/// The largest plaintext a target will cart
///
/// Carting is quadratic in nothing, but a fuzzer handed an unbounded length will spend all its
/// time on enormous inputs rather than on interesting shapes. Anything past a few blocks adds
/// no new code paths.
pub const MAX_PLAIN: usize = 256 * 1024;

/// The block size log2 targets cart with
///
/// The smallest the format allows, so that even a small fuzz input crosses several block
/// boundaries and exercises the framing rather than one partial block.
pub const BLOCK_SIZE_LOG2: u8 = 10;

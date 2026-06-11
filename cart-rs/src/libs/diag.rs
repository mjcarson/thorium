//! Streaming diagnostics for debugging cross-platform uncart failures
//!
//! These diagnostics exist to debug a macOS-only `CaRT` V1 failure where zlib
//! reports `incorrect data check` (an adler32 mismatch detected at the very end
//! of the deflate stream) on a file that uncarts correctly on Linux. Since the
//! two platforms link different inflate backends, the logs must localize where
//! the platforms diverge: the bytes fed *into* inflate or the bytes it puts out.
//!
//! Diagnostics are disabled unless the `CART_DIAG` environment variable is set
//! and add no allocations or syscalls to the normal uncart path. When enabled,
//! single-line `CARTDIAG key=value ...` records are written to stderr:
//!
//! * `init`     - platform info plus a probe identifying the inflate backend
//! * `head_in`  - the first bytes of the decrypted stream (zlib magic check)
//! * `in_ckpt`  - rolling adler32 of the decrypted stream at exact 64 MiB marks
//! * `out_ckpt` - rolling adler32 of the inflated output at exact 64 MiB marks
//! * `stream_end` / `eof` / `fail` - terminal summaries with all counters
//!
//! Checkpoint updates are split *exactly* at the 64 MiB boundaries, so the
//! checkpoint lines from two machines are bit-comparable no matter how the
//! platforms happened to chunk their reads. The first divergent line pins the
//! corruption to a 64 MiB window on either the input or the output side.

use flate2::{Decompress, FlushDecompress};

/// The largest span of bytes adler32 can ingest before its sums must be reduced
///
/// This matches zlib's `NMAX`: the most bytes that can be added without the
/// 32-bit second sum overflowing.
const ADLER_SPAN: usize = 5552;

/// The modulus used by the adler32 checksum
const ADLER_MOD: u32 = 65_521;

/// Update a rolling adler32 state with the next span of bytes
///
/// This matches zlib's adler32 exactly (initial state 1) because zlib's
/// "data check" *is* the adler32 of the inflated output: the value we roll up
/// here is directly comparable to the check zlib validates at stream end.
///
/// # Arguments
///
/// * `state` - The current adler32 state (1 for a fresh stream)
/// * `bytes` - The next bytes to roll into the checksum
pub(crate) fn adler32(state: u32, bytes: &[u8]) -> u32 {
    // split the packed state into its low/high sums
    let mut a = state & 0xffff;
    let mut b = state >> 16;
    // walk the input in spans small enough to defer the modulo
    for span in bytes.chunks(ADLER_SPAN) {
        // accumulate both sums over this span
        for &byte in span {
            a += u32::from(byte);
            b += a;
        }
        // reduce both sums once per span
        a %= ADLER_MOD;
        b %= ADLER_MOD;
    }
    // repack the sums into a single state value
    (b << 16) | a
}

/// Render a span of bytes as lowercase hex for log lines
///
/// # Arguments
///
/// * `bytes` - The bytes to render
fn hex(bytes: &[u8]) -> String {
    // render each byte as two lowercase hex chars
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Streaming diagnostics for one V1 uncart stream
///
/// Constructed when the V1 header is parsed; enabled only when the `CART_DIAG`
/// environment variable is set. All methods are no-ops when disabled.
pub(crate) struct UncartDiag {
    /// Whether diagnostics are enabled for this stream
    enabled: bool,
    /// The number of raw chunks decrypted so far
    fills: u64,
    /// The smallest chunk handed to the decryptor
    fill_min: usize,
    /// The largest chunk handed to the decryptor
    fill_max: usize,
    /// Rolling adler32 over the decrypted stream fed to the decompressor
    adler_in: u32,
    /// Total decrypted bytes rolled into `adler_in`
    hashed_in: u64,
    /// The byte offset of the next input-side checkpoint line
    next_in_ckpt: u64,
    /// Rolling adler32 over the bytes the decompressor wrote out
    adler_out: u32,
    /// Total inflated bytes rolled into `adler_out`
    hashed_out: u64,
    /// The byte offset of the next output-side checkpoint line
    next_out_ckpt: u64,
    /// The most recent decrypted bytes (context for failure reports)
    tail: [u8; Self::TAIL_LEN],
    /// How many bytes of `tail` are currently valid
    tail_len: usize,
    /// Whether a terminal line (`stream_end`/`eof`) has already been logged
    terminal_logged: bool,
}

impl UncartDiag {
    /// How often checkpoint lines are emitted (64 MiB)
    const CKPT_EVERY: u64 = 67_108_864;

    /// How many trailing decrypted bytes to retain for failure reports
    const TAIL_LEN: usize = 64;

    /// Build the diagnostics state, logging an init line when enabled
    ///
    /// # Arguments
    ///
    /// * `decrypted_buf_size` - The size of the stream's decryption buffer
    pub fn new(decrypted_buf_size: usize) -> Self {
        // diagnostics are opt-in via the CART_DIAG environment variable
        let enabled = std::env::var_os("CART_DIAG").is_some();
        if enabled {
            // probe the linked inflate backend by feeding it a garbage zlib
            // header; the error's debug shape identifies the backend at runtime
            // (C zlib family answers with msg "incorrect header check")
            let mut probe = Decompress::new(true);
            let mut sink = [0u8; 32];
            let shape = probe.decompress(&[0xAA, 0xBB, 0xCC, 0xDD], &mut sink, FlushDecompress::None);
            // log platform info so cross-machine logs are self-identifying
            eprintln!(
                "CARTDIAG init version=V1 os={} arch={} decrypted_buf={} ckpt_every={} probe={:?}",
                std::env::consts::OS,
                std::env::consts::ARCH,
                decrypted_buf_size,
                Self::CKPT_EVERY,
                shape,
            );
        }
        UncartDiag {
            enabled,
            fills: 0,
            fill_min: usize::MAX,
            fill_max: 0,
            // adler32 streams start from state 1 to match zlib
            adler_in: 1,
            hashed_in: 0,
            next_in_ckpt: Self::CKPT_EVERY,
            adler_out: 1,
            hashed_out: 0,
            next_out_ckpt: Self::CKPT_EVERY,
            tail: [0; Self::TAIL_LEN],
            tail_len: 0,
            terminal_logged: false,
        }
    }

    /// Record the size of one decrypted chunk
    ///
    /// # Arguments
    ///
    /// * `size` - The number of bytes decrypted in this fill
    pub fn note_fill(&mut self, size: usize) {
        // skip all tracking when diagnostics are disabled
        if !self.enabled {
            return;
        }
        // track the fill count and size extremes
        self.fills += 1;
        self.fill_min = self.fill_min.min(size);
        self.fill_max = self.fill_max.max(size);
    }

    /// Roll a span of decrypted bytes into the input-side checkpoint stream
    ///
    /// # Arguments
    ///
    /// * `bytes` - The decrypted bytes about to be fed to the decompressor
    pub fn update_in(&mut self, bytes: &[u8]) {
        // skip all tracking when diagnostics are disabled
        if !self.enabled {
            return;
        }
        // log the first decrypted bytes once (a valid stream starts 0x78 ..)
        if self.hashed_in == 0 && !bytes.is_empty() {
            eprintln!("CARTDIAG head_in bytes={}", hex(&bytes[..bytes.len().min(16)]));
        }
        // roll the checksum forward, splitting exactly at checkpoint marks
        let mut rest = bytes;
        while !rest.is_empty() {
            // take only up to the next checkpoint boundary
            let until_ckpt = usize::try_from(self.next_in_ckpt - self.hashed_in).unwrap_or(usize::MAX);
            let take = rest.len().min(until_ckpt);
            self.adler_in = adler32(self.adler_in, &rest[..take]);
            self.hashed_in += take as u64;
            // emit a byte-exact checkpoint line when we land on the boundary
            if self.hashed_in == self.next_in_ckpt {
                eprintln!(
                    "CARTDIAG in_ckpt={} adler_in={:#010x}",
                    self.hashed_in, self.adler_in
                );
                self.next_in_ckpt += Self::CKPT_EVERY;
            }
            rest = &rest[take..];
        }
        // remember the most recent decrypted bytes for failure context
        self.note_tail(bytes);
    }

    /// Roll a span of inflated output bytes into the output-side checkpoints
    ///
    /// # Arguments
    ///
    /// * `bytes` - The bytes the decompressor just wrote out
    pub fn update_out(&mut self, bytes: &[u8]) {
        // skip all tracking when diagnostics are disabled
        if !self.enabled {
            return;
        }
        // roll the checksum forward, splitting exactly at checkpoint marks
        let mut rest = bytes;
        while !rest.is_empty() {
            // take only up to the next checkpoint boundary
            let until_ckpt = usize::try_from(self.next_out_ckpt - self.hashed_out).unwrap_or(usize::MAX);
            let take = rest.len().min(until_ckpt);
            self.adler_out = adler32(self.adler_out, &rest[..take]);
            self.hashed_out += take as u64;
            // emit a byte-exact checkpoint line when we land on the boundary
            if self.hashed_out == self.next_out_ckpt {
                eprintln!(
                    "CARTDIAG out_ckpt={} adler_out={:#010x}",
                    self.hashed_out, self.adler_out
                );
                self.next_out_ckpt += Self::CKPT_EVERY;
            }
            rest = &rest[take..];
        }
    }

    /// Log the end-of-stream summary once the decompressor reports `StreamEnd`
    ///
    /// # Arguments
    ///
    /// * `total_in`  - The decompressor's total consumed byte count
    /// * `total_out` - The decompressor's total produced byte count
    pub fn finish(&mut self, total_in: u64, total_out: u64) {
        // skip when disabled or when a terminal line was already logged
        if !self.enabled || self.terminal_logged {
            return;
        }
        self.terminal_logged = true;
        // log the full final state; adler_out here is the value zlib checked
        eprintln!(
            "CARTDIAG stream_end total_in={} total_out={} hashed_in={} adler_in={:#010x} hashed_out={} adler_out={:#010x} fills={} fill_min={} fill_max={}",
            total_in,
            total_out,
            self.hashed_in,
            self.adler_in,
            self.hashed_out,
            self.adler_out,
            self.fills,
            self.fill_min,
            self.fill_max,
        );
    }

    /// Log an end-of-input summary when the reader runs dry
    ///
    /// Reaching EOF without `StreamEnd` means the deflate stream was cut short,
    /// so this line firing with `finished=false` is itself a finding.
    ///
    /// # Arguments
    ///
    /// * `total_in`  - The decompressor's total consumed byte count
    /// * `total_out` - The decompressor's total produced byte count
    /// * `finished`  - Whether the deflate stream already reached `StreamEnd`
    pub fn eof(&mut self, total_in: u64, total_out: u64, finished: bool) {
        // skip when disabled or when a terminal line was already logged
        if !self.enabled || self.terminal_logged {
            return;
        }
        self.terminal_logged = true;
        // log the full state seen at end of input
        eprintln!(
            "CARTDIAG eof finished={} total_in={} total_out={} hashed_in={} adler_in={:#010x} hashed_out={} adler_out={:#010x} fills={} fill_min={} fill_max={} tail={}",
            finished,
            total_in,
            total_out,
            self.hashed_in,
            self.adler_in,
            self.hashed_out,
            self.adler_out,
            self.fills,
            self.fill_min,
            self.fill_max,
            hex(&self.tail[..self.tail_len]),
        );
    }

    /// Log the full diagnostic state when the decompressor rejects the stream
    ///
    /// # Arguments
    ///
    /// * `err`           - The decompression error returned by the backend
    /// * `total_in`      - The decompressor's total consumed byte count
    /// * `total_out`     - The decompressor's total produced byte count
    /// * `unconsumed`    - The decrypted bytes the decompressor refused
    /// * `decrypt_start` - The stream's current decryption buffer start offset
    /// * `decrypt_end`   - The stream's current decryption buffer end offset
    pub fn fail(
        &mut self,
        err: &dyn std::fmt::Debug,
        total_in: u64,
        total_out: u64,
        unconsumed: &[u8],
        decrypt_start: usize,
        decrypt_end: usize,
    ) {
        // failures are always worth logging exactly once when enabled
        if !self.enabled || self.terminal_logged {
            return;
        }
        self.terminal_logged = true;
        // log every counter plus hex context around the rejected bytes
        eprintln!(
            "CARTDIAG fail err={:?} total_in={} total_out={} hashed_in={} adler_in={:#010x} hashed_out={} adler_out={:#010x} fills={} fill_min={} fill_max={} decrypt_start={} decrypt_end={} window={} tail={}",
            err,
            total_in,
            total_out,
            self.hashed_in,
            self.adler_in,
            self.hashed_out,
            self.adler_out,
            self.fills,
            self.fill_min,
            self.fill_max,
            decrypt_start,
            decrypt_end,
            hex(&unconsumed[..unconsumed.len().min(48)]),
            hex(&self.tail[..self.tail_len]),
        );
    }

    /// Retain the most recent decrypted bytes for failure context
    ///
    /// # Arguments
    ///
    /// * `bytes` - The newest decrypted bytes
    fn note_tail(&mut self, bytes: &[u8]) {
        if bytes.len() >= Self::TAIL_LEN {
            // the new chunk alone fills the tail window
            self.tail.copy_from_slice(&bytes[bytes.len() - Self::TAIL_LEN..]);
            self.tail_len = Self::TAIL_LEN;
        } else {
            // keep as much of the existing tail as still fits before the new bytes
            let keep = (Self::TAIL_LEN - bytes.len()).min(self.tail_len);
            // slide the kept suffix of the old tail to the front
            self.tail.copy_within(self.tail_len - keep..self.tail_len, 0);
            // append the new bytes after the kept prefix
            self.tail[keep..keep + bytes.len()].copy_from_slice(bytes);
            self.tail_len = keep + bytes.len();
        }
    }
}

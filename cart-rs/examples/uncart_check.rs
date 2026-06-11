//! Reference V1 uncart checker: synchronous, no async machinery.
//!
//! This decodes a `CaRT` V1 file with the simplest possible sequential loop
//! (`read_exact` → RC4 → inflate) while using the *same* flate2 backend the
//! crate links, so it isolates whether a platform failure lives in the inflate
//! backend itself or in `UncartStream`'s async state machine. The body bounds
//! are computed up front from the file length, so the trailing footer can never
//! reach the decompressor.
//!
//! It emits the same `CARTDIAG` checkpoint lines as the library's `CART_DIAG`
//! diagnostics (byte-exact adler32 checkpoints every 64 MiB on both the
//! decrypted input stream and the inflated output stream), making its logs
//! directly diffable against `UncartStream` runs and across machines.
//!
//! Usage: `uncart_check <file.cart>`
//!
//! Exit codes: 0 = clean `StreamEnd`, 1 = decompressor rejected the stream,
//! 2 = decompressor stalled without progress, 3 = body ended without
//! `StreamEnd`, 4 = not a `CaRT` V1 file.
use cart_rs::{CartVersion, footer, header, header::Header};
use flate2::{Decompress, FlushDecompress, Status};
use generic_array::GenericArray;
use generic_array::typenum::U16;
use rc4::{KeyInit, Rc4, StreamCipher};
use std::io::{Read, Seek, SeekFrom};

/// How often checkpoint lines are emitted (64 MiB, matching the library)
const CKPT_EVERY: u64 = 67_108_864;

/// The size of the read and inflate scratch buffers (1 MiB)
const BUF_SIZE: usize = 1_048_576;

/// The largest span of bytes adler32 can ingest before its sums must be reduced
const ADLER_SPAN: usize = 5552;

/// The modulus used by the adler32 checksum
const ADLER_MOD: u32 = 65_521;

/// Update a rolling adler32 state with the next span of bytes
///
/// # Arguments
///
/// * `state` - The current adler32 state (1 for a fresh stream)
/// * `bytes` - The next bytes to roll into the checksum
fn adler32(state: u32, bytes: &[u8]) -> u32 {
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

/// A rolling adler32 stream that emits byte-exact checkpoint lines
struct Ckpt {
    /// The stream label used in emitted lines ("in" or "out")
    label: &'static str,
    /// The rolling adler32 state
    adler: u32,
    /// Total bytes rolled into the checksum
    hashed: u64,
    /// The byte offset of the next checkpoint line
    next: u64,
}

impl Ckpt {
    /// Build a fresh checkpoint stream
    ///
    /// # Arguments
    ///
    /// * `label` - The stream label used in emitted lines
    fn new(label: &'static str) -> Self {
        Ckpt {
            label,
            // adler32 streams start from state 1 to match zlib
            adler: 1,
            hashed: 0,
            next: CKPT_EVERY,
        }
    }

    /// Roll bytes into the checksum, splitting exactly at checkpoint marks
    ///
    /// # Arguments
    ///
    /// * `bytes` - The next bytes of this stream
    fn update(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while !rest.is_empty() {
            // take only up to the next checkpoint boundary
            let until_ckpt = usize::try_from(self.next - self.hashed).unwrap_or(usize::MAX);
            let take = rest.len().min(until_ckpt);
            self.adler = adler32(self.adler, &rest[..take]);
            self.hashed += take as u64;
            // emit a byte-exact checkpoint line when we land on the boundary
            if self.hashed == self.next {
                eprintln!(
                    "CARTDIAG {}_ckpt={} adler_{}={:#010x}",
                    self.label, self.hashed, self.label, self.adler
                );
                self.next += CKPT_EVERY;
            }
            rest = &rest[take..];
        }
    }
}

/// Decode the given `CaRT` V1 file and report diagnostics
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // get the path of the cart file to check
    let path = std::env::args()
        .nth(1)
        .expect("usage: uncart_check <file.cart>");
    // probe the linked inflate backend by feeding it a garbage zlib header;
    // the error's debug shape identifies the backend at runtime
    let mut probe = Decompress::new(true);
    let mut sink = [0u8; 32];
    let shape = probe.decompress(&[0xAA, 0xBB, 0xCC, 0xDD], &mut sink, FlushDecompress::None);
    // log platform info so cross-machine logs are self-identifying
    eprintln!(
        "CARTDIAG init version=V1 mode=reference os={} arch={} buf={} ckpt_every={} probe={:?}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        BUF_SIZE,
        CKPT_EVERY,
        shape,
    );
    // open the cart file and get its total length
    let mut file = std::fs::File::open(&path)?;
    let file_len = file.metadata()?.len();
    // read and parse the mandatory 38-byte header
    let mut hdr_buf = [0u8; header::HEADER_LEN];
    file.read_exact(&mut hdr_buf)?;
    let hdr = Header::get(&hdr_buf)?;
    // this checker only understands the V1 (RC4 + zlib) layout
    if hdr.version != CartVersion::V1 {
        eprintln!("CARTDIAG abort reason=not_v1 version={}", hdr.version);
        std::process::exit(4);
    }
    // read and validate the trailing 28-byte footer
    let mut ftr_buf = [0u8; footer::FOOTER_LEN];
    file.seek(SeekFrom::End(-(footer::FOOTER_LEN as i64)))?;
    file.read_exact(&mut ftr_buf)?;
    let footer_ok = &ftr_buf[..4] == footer::MAGIC_NUM.as_slice();
    // compute the body bounds: skip the header (plus any optional header) and
    // trim the footer so the decompressor can never see the trailing bytes
    let skip = hdr.skip() as u64;
    let trim = footer::FOOTER_LEN as u64;
    let body_len = file_len
        .checked_sub(skip + trim)
        .ok_or("file too short to hold a CaRT header + footer")?;
    eprintln!(
        "CARTDIAG layout file_len={} opt_header_len={} body_start={} body_len={} footer_magic_ok={} footer_tail={}",
        file_len,
        hdr.opt_len,
        skip,
        body_len,
        footer_ok,
        hex(&ftr_buf),
    );
    // seek back to the start of the encrypted body
    file.seek(SeekFrom::Start(skip))?;
    // build the RC4 decryptor from the key in the header
    let mut cipher: Rc4<U16> = Rc4::new(GenericArray::from_slice(&hdr.key));
    // build the zlib decompressor (zlib wrapper mode, matching the library)
    let mut zlib = Decompress::new(true);
    // allocate the read and inflate scratch buffers
    let mut inbuf = vec![0u8; BUF_SIZE];
    let mut outbuf = vec![0u8; BUF_SIZE];
    // build the checkpoint streams for the decrypted input and inflated output
    let mut in_ckpt = Ckpt::new("in");
    let mut out_ckpt = Ckpt::new("out");
    // walk the body sequentially in fixed-size chunks
    let mut remaining = body_len;
    while remaining > 0 {
        // read the next chunk of the encrypted body (read_exact loops over any
        // short reads, ruling out platform short-read differences entirely)
        let take = inbuf.len().min(usize::try_from(remaining).unwrap_or(usize::MAX));
        file.read_exact(&mut inbuf[..take])?;
        remaining -= take as u64;
        // decrypt this chunk in place and roll it into the input checkpoints
        cipher.apply_keystream(&mut inbuf[..take]);
        in_ckpt.update(&inbuf[..take]);
        // feed the decrypted chunk to the decompressor until fully consumed
        let mut off = 0;
        let mut stalls = 0;
        while off < take {
            // snapshot the totals so we can compute this round's deltas
            let old_total_in = zlib.total_in();
            let old_total_out = zlib.total_out();
            // inflate the next stretch of this chunk
            let status = zlib.decompress(&inbuf[off..take], &mut outbuf, FlushDecompress::None);
            let consumed = (zlib.total_in() - old_total_in) as usize;
            let written = (zlib.total_out() - old_total_out) as usize;
            // roll the inflated bytes into the output checkpoints
            out_ckpt.update(&outbuf[..written]);
            off += consumed;
            // inspect the decompressor status
            match status {
                // the deflate stream ended cleanly; report and exit
                Ok(Status::StreamEnd) => {
                    // any body bytes past the end of the deflate stream were
                    // never consumed (should be 0 for a cart-rs produced file)
                    let leftover_body = (take - off) as u64 + remaining;
                    eprintln!(
                        "CARTDIAG stream_end total_in={} total_out={} adler_in={:#010x} adler_out={:#010x} leftover_body={}",
                        zlib.total_in(),
                        zlib.total_out(),
                        in_ckpt.adler,
                        out_ckpt.adler,
                        leftover_body,
                    );
                    return Ok(());
                }
                // no error, but watch for a wedged decompressor making no progress
                Ok(_) => {
                    if consumed == 0 && written == 0 {
                        stalls += 1;
                        if stalls >= 2 {
                            eprintln!(
                                "CARTDIAG stall status={status:?} total_in={} total_out={} chunk_off={} chunk_len={}",
                                zlib.total_in(),
                                zlib.total_out(),
                                off,
                                take,
                            );
                            std::process::exit(2);
                        }
                    } else {
                        stalls = 0;
                    }
                }
                // the backend rejected the stream; dump everything and exit
                Err(err) => {
                    eprintln!(
                        "CARTDIAG fail err={:?} total_in={} total_out={} adler_in={:#010x} adler_out={:#010x} file_offset={} window={}",
                        err,
                        zlib.total_in(),
                        zlib.total_out(),
                        in_ckpt.adler,
                        out_ckpt.adler,
                        // the absolute file offset of the rejected input byte
                        skip + zlib.total_in(),
                        hex(&inbuf[off..take.min(off + 48)]),
                    );
                    std::process::exit(1);
                }
            }
        }
    }
    // the body ran out before the deflate stream ended
    eprintln!(
        "CARTDIAG no_stream_end total_in={} total_out={} adler_in={:#010x} adler_out={:#010x}",
        zlib.total_in(),
        zlib.total_out(),
        in_ckpt.adler,
        out_ckpt.adler,
    );
    std::process::exit(3);
}

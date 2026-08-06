//! Flip one bit of a real cart and require that the reader notices
//!
//! This is the target that proves the AEAD does something. V2's whole justification over V1 is
//! that it authenticates, and the previous attempt at V2 did not: the nonce was
//! `"FliffyIs" || counter` under one deployment wide key, so every file reused the same nonces,
//! and GCM nonce reuse with known plaintext hands an attacker the GHASH key. The tags verified
//! and meant nothing. A single flipped bit going undetected is what that failure looks like
//! from the outside, so it gets its own target rather than living inside the round trip one.
//!
//! There is deliberately no V1 half to the assertion. V1 is RC4 over DEFLATE with nothing but
//! zlib's Adler-32 underneath, so a flipped bit is *usually* caught and is not guaranteed to
//! be. Claiming otherwise would be a test that passes for the wrong reason, so V1 is fuzzed
//! here only for the weaker property that it does not panic or hang.

#![no_main]

use libfuzzer_sys::fuzz_target;

use cart_rs::{CartStream, CartVersion, FOOTER_LEN, UncartStream, V2Options};
use cart_rs_fuzz::{
    BLOCK_SIZE_LOG2, MAX_PLAIN, Schedule, StallingReader, drain, key, run, version,
};

/// What the fuzzer gets to control
#[derive(Debug, arbitrary::Arbitrary)]
struct Input {
    /// Which version to cart with
    version: u8,
    /// The bytes to cart before tampering with the result
    plain: Vec<u8>,
    /// Which byte of the carted file to flip a bit in, before clamping into the covered span
    at: u32,
    /// Which bit of that byte to flip
    bit: u8,
    /// How to deliver the tampered cart to the uncarter
    schedule: Schedule,
}

fuzz_target!(|input: Input| {
    // an empty file has no body to tamper with, and a bit flip in its header is covered by the
    // header target, so skip straight past those cases
    if input.plain.is_empty() {
        return;
    }
    // keep the plaintext bounded so a case stays cheap
    let plain = if input.plain.len() > MAX_PLAIN {
        &input.plain[..MAX_PLAIN]
    } else {
        &input.plain[..]
    };
    let chosen = version(input.version);
    // write a clean cart first, so the bytes being flipped are a real file
    let mut carted = run(async {
        let stream = CartStream::builder(key(), plain)
            .version(chosen)
            .v2_options(V2Options {
                zstd_level: 3,
                block_size_log2: BLOCK_SIZE_LOG2,
            })
            .build()?;
        drain(stream, 64 * 1024).await
    })
    .expect("carting a plaintext we generated ourselves must succeed");
    // the covered span is everything after the magic number and before the plaintext footer.
    // The magic number is excluded because breaking it is a "not a CaRT file" error rather than
    // a tamper detection, and the footer because it is plaintext trailing metadata that carries
    // none of the file's data and is not inside the AEAD's authenticated span.
    let span = 4..carted.len().saturating_sub(FOOTER_LEN);
    if span.is_empty() {
        return;
    }
    let index = span.start + (input.at as usize) % span.len();
    carted[index] ^= 1 << (input.bit % 8);
    // read the tampered file back, on whatever schedule the fuzzer picked
    let result = run(drain(
        UncartStream::new(StallingReader::new(&carted, input.schedule)),
        input.schedule.buf(),
    ));
    // V2 authenticates every covered byte, so this has to be an error and not merely different
    // output. Returning anything at all, correct bytes included, means some byte inside the
    // covered span was not actually authenticated. V1 has no authentication to assert against,
    // so for it the property is only that the read returns at all.
    if chosen == CartVersion::V2 {
        assert!(
            result.is_err(),
            "flipping bit {} of byte {index} of a {} byte V2 cart was not detected",
            input.bit % 8,
            carted.len(),
        );
    }
});

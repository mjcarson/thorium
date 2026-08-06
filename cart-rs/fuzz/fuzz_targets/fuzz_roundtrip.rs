//! Cart arbitrary bytes with arbitrary options and require that they come back exactly
//!
//! This is the property the whole crate exists to hold: whatever goes in comes out. It is worth
//! fuzzing rather than only proptesting because the option space is what makes it interesting —
//! a compression level and a block size change how many blocks there are, where the boundaries
//! land, and whether the last block is partial, and libFuzzer's coverage feedback finds the
//! combinations that reach a new branch faster than a uniform sampler does.

#![no_main]

use libfuzzer_sys::fuzz_target;

use cart_rs::{CartStream, UncartStream, V1Options, V2Options};
use cart_rs_fuzz::{MAX_PLAIN, Schedule, StallingReader, drain, key, run, version};

/// The smallest block size the format allows, as a log2
const MIN_LOG2: u8 = 10;

/// The largest block size this target will ask for, as a log2
///
/// The format allows up to 26, but a 64 MiB block buys no new code paths over a 128 KiB one and
/// costs the fuzzer an allocation per case, so this stops well short of the format limit.
const MAX_LOG2: u8 = 17;

/// What the fuzzer gets to control
#[derive(Debug, arbitrary::Arbitrary)]
struct Input {
    /// Which version to cart with
    version: u8,
    /// The bytes to cart
    plain: Vec<u8>,
    /// The deflate level to cart V1 with, before clamping
    deflate_level: u8,
    /// The zstd level to cart V2 with, before clamping
    zstd_level: i8,
    /// The block size to cart V2 with, before clamping
    block_size_log2: u8,
    /// How to deliver the plaintext to the carter
    carting: Schedule,
    /// How to deliver the carted bytes to the uncarter
    uncarting: Schedule,
}

fuzz_target!(|input: Input| {
    // keep the plaintext bounded so the fuzzer spends its time on shapes rather than volume
    let plain = if input.plain.len() > MAX_PLAIN {
        &input.plain[..MAX_PLAIN]
    } else {
        &input.plain[..]
    };
    let chosen = version(input.version);
    // clamp the options into the ranges the builders accept, since an out of range option is
    // already a typed error and rediscovering that is not what this target is for
    let v1 = V1Options {
        deflate_level: input.deflate_level % 10,
    };
    let v2 = V2Options {
        // zstd accepts -131072..=22, so an i8 only ever overshoots at the top end and the whole
        // negative half of its range is a valid fast level
        zstd_level: i32::from(input.zstd_level).min(22),
        block_size_log2: MIN_LOG2 + (input.block_size_log2 % (MAX_LOG2 - MIN_LOG2 + 1)),
    };
    // cart the plaintext, feeding it in on whatever schedule the fuzzer picked
    let carted = run(async {
        let stream = CartStream::builder(key(), StallingReader::new(plain, input.carting))
            .version(chosen)
            .v1_options(v1)
            .v2_options(v2)
            .build()?;
        drain(stream, input.carting.buf()).await
    })
    .expect("carting a plaintext with in range options must succeed");
    // and read it back on a different schedule, so the two state machines never line up
    let recovered = run(drain(
        UncartStream::new(StallingReader::new(&carted, input.uncarting)),
        input.uncarting.buf(),
    ))
    .expect("uncarting a cart we just wrote must succeed");
    assert_eq!(
        recovered.len(),
        plain.len(),
        "a {chosen} round trip changed the length",
    );
    assert_eq!(recovered, plain, "a {chosen} round trip changed the bytes");
});

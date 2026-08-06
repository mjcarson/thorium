//! Corrupt a real cart and uncart it on an arbitrary read schedule
//!
//! This is the deep counterpart to `fuzz_uncart`. Uniformly random bytes nearly always die on
//! the magic number, so that target hammers one branch and rarely reaches a codec. Here the
//! input starts life as a genuine cart, so the header parses, the reader commits to a version,
//! and the mutated bytes land inside the decryptor and the decompressor, which is where the
//! interesting failures are.
//!
//! Every historical resumption defect also lives here: a header arriving one byte at a time, a
//! decompressor that consumed input and produced nothing, and a reader that stalled partway
//! through a block.

#![no_main]

use libfuzzer_sys::fuzz_target;

use cart_rs::{CartStream, CartVersion, UncartStream, V2Options};
use cart_rs_fuzz::{
    BLOCK_SIZE_LOG2, MAX_PLAIN, Schedule, StallingReader, drain, key, run, version,
};

/// What the fuzzer gets to control
#[derive(Debug, arbitrary::Arbitrary)]
struct Input {
    /// Which version to cart with
    version: u8,
    /// The plaintext to cart before corrupting the result
    plain: Vec<u8>,
    /// The bytes to overwrite in the carted output, as (offset, value) pairs
    edits: Vec<(u32, u8)>,
    /// How to deliver the corrupted cart to the uncarter
    schedule: Schedule,
}

fuzz_target!(|input: Input| {
    // keep the plaintext small enough that the fuzzer spends its time on shapes, not on volume
    let plain = if input.plain.len() > MAX_PLAIN {
        &input.plain[..MAX_PLAIN]
    } else {
        &input.plain[..]
    };
    let chosen = version(input.version);
    // cart it cleanly first, so the bytes we corrupt are a real cart rather than noise
    let carted = run(async {
        let stream = CartStream::builder(key(), plain)
            .version(chosen)
            .v2_options(V2Options {
                zstd_level: 3,
                block_size_log2: BLOCK_SIZE_LOG2,
            })
            .build()?;
        drain(stream, 64 * 1024).await
    });
    // carting our own well formed input must never fail, so this one really is an assertion
    let mut carted = carted.expect("carting a plaintext we generated ourselves must succeed");
    // scribble over whatever the fuzzer picked
    for (at, byte) in input.edits {
        let index = (at as usize) % carted.len().max(1);
        if let Some(slot) = carted.get_mut(index) {
            *slot = byte;
        }
    }
    // uncart the result; it may succeed or fail, but it must not panic or hang
    let recovered = run(drain(
        UncartStream::new(StallingReader::new(&carted, input.schedule)),
        input.schedule.buf(),
    ));
    // if nothing was actually corrupted then the round trip has to be exact
    if let Ok(recovered) = recovered {
        // V2 authenticates every byte, so a successful uncart of a V2 cart is a promise that
        // the file was not altered; anything else means the tag check let a change through
        if chosen == CartVersion::V2 {
            assert_eq!(
                recovered, plain,
                "a V2 cart uncarted successfully but gave back different bytes",
            );
        }
    }
});

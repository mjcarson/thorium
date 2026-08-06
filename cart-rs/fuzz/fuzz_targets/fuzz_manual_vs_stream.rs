//! Require the two write APIs to agree
//!
//! cart-rs has two ways to write a cart: `CartStream`, which pulls from an `AsyncRead`, and
//! `CartManual`, which the API's multipart uploader pushes into because it has to interleave
//! hashing and part submission. They are separate state machines over the same codec, and the
//! old implementations quietly disagreed — one flushed the compressor with `None` and the other
//! with `Partial`, so the same input produced different DEFLATE streams depending on which
//! entry point the caller happened to use. That meant a golden vector could only ever pin one
//! of them.
//!
//! V1 is asserted byte for byte. V2 cannot be, because every V2 file gets a fresh random nonce
//! salt, so two carts of the same input are supposed to differ; there the assertion is that the
//! two agree on length and that both decode back to the input.

#![no_main]

use libfuzzer_sys::fuzz_target;

use cart_rs::{CartManual, CartStream, CartVersion, UncartStream, V1Options, V2Options};
use cart_rs_fuzz::{
    BLOCK_SIZE_LOG2, MAX_PLAIN, Schedule, StallingReader, drain, key, run, version,
};

/// What the fuzzer gets to control
#[derive(Debug, arbitrary::Arbitrary)]
struct Input {
    /// Which version to cart with
    version: u8,
    /// The chunks to push into the manual API, concatenated to feed the streaming one
    chunks: Vec<Vec<u8>>,
    /// How to deliver the concatenated plaintext to the streaming API
    schedule: Schedule,
}

fuzz_target!(|input: Input| {
    // flatten the chunks into the single buffer the streaming API reads from, capped so the
    // fuzzer cannot spend a whole case on volume
    let mut plain = Vec::new();
    for chunk in &input.chunks {
        if plain.len() >= MAX_PLAIN {
            break;
        }
        let room = MAX_PLAIN - plain.len();
        plain.extend_from_slice(&chunk[..chunk.len().min(room)]);
    }
    let chosen = version(input.version);
    let v1 = V1Options { deflate_level: 6 };
    let v2 = V2Options {
        zstd_level: 3,
        block_size_log2: BLOCK_SIZE_LOG2,
    };
    // write it once through the streaming API, on an awkward read schedule
    let streamed = run(async {
        let stream = CartStream::builder(key(), StallingReader::new(&plain, input.schedule))
            .version(chosen)
            .v1_options(v1)
            .v2_options(v2)
            .build()?;
        drain(stream, input.schedule.buf()).await
    })
    .expect("the streaming API failed on a plaintext we generated");
    // and once through the manual API, pushing the chunks in the shape the fuzzer chose and
    // draining after every push so the output buffer is exercised rather than just grown
    let mut manual = CartManual::builder(key())
        .version(chosen)
        .v1_options(v1)
        .v2_options(v2)
        .build()
        .expect("the manual builder rejected options the streaming builder accepted");
    let mut pushed = Vec::new();
    let mut manually = Vec::new();
    for chunk in &input.chunks {
        // push only the part of this chunk that made it into the capped plaintext, so both APIs
        // see exactly the same bytes
        let room = MAX_PLAIN.saturating_sub(pushed.len());
        let chunk = &chunk[..chunk.len().min(room)];
        manual.push(chunk).expect("pushing before finish must work");
        pushed.extend_from_slice(chunk);
        manually.extend_from_slice(&manual.take_up_to(input.schedule.chunk()));
    }
    manual.finish().expect("finishing a live cart must work");
    manually.extend_from_slice(&manual.take());
    // the manual API must be drained dry by a take after finish
    assert!(
        manual.is_empty(),
        "the manual API still had {} bytes after finish and a full take",
        manual.ready(),
    );
    // V1 is deterministic, so the two APIs have to emit the same bytes
    if chosen == CartVersion::V1 {
        assert_eq!(
            manually, streamed,
            "the manual and streaming V1 writers disagreed on {} bytes of input",
            plain.len(),
        );
    } else {
        // V2 salts every file, so only the length can be compared directly
        assert_eq!(
            manually.len(),
            streamed.len(),
            "the manual and streaming V2 writers disagreed on output length",
        );
    }
    // and whatever the manual API wrote has to decode back to what was pushed into it
    let recovered = run(drain(
        UncartStream::new(StallingReader::new(&manually, input.schedule)),
        input.schedule.buf(),
    ))
    .expect("uncarting the manual API's output must succeed");
    assert_eq!(
        recovered, pushed,
        "the manual API's output did not decode back to what was pushed",
    );
});

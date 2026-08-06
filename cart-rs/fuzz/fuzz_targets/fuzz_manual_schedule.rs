//! Drive the manual API with an arbitrary sequence of operations
//!
//! The manual API is the one the multipart uploader in `api/src/utils/s3.rs` drives, and it is
//! where the two worst non-crash defects lived. The old `process()` returned "there is more to
//! do" unconditionally once its output buffer was full, and the loop calling it had no `.await`
//! on the spin path, so an output buffer smaller than the flush threshold pinned a tokio worker
//! at 100% with the request hung until the process was killed. Separately, `finish()` called
//! the compressor exactly once and never checked for stream end, so if the buffer lacked room
//! for the DEFLATE tail it appended a well formed footer over a truncated body: a file that
//! looks valid and is not.
//!
//! Both are unrepresentable now — `push` is total and `finish` drains — so this target's job is
//! to keep them that way. It hammers the API with orderings a caller would not write on
//! purpose: taking before pushing, taking zero bytes, pushing empty chunks, taking in one byte
//! slivers, and using it after `finish`.

#![no_main]

use libfuzzer_sys::fuzz_target;

use cart_rs::{CartManual, Error, UncartStream, V2Options};
use cart_rs_fuzz::{
    BLOCK_SIZE_LOG2, MAX_PLAIN, Schedule, StallingReader, drain, key, run, version,
};

/// One thing to do to the manual carter
#[derive(Debug, arbitrary::Arbitrary)]
enum Op {
    /// Push some bytes in, including possibly zero of them
    Push(Vec<u8>),
    /// Take at most this many carted bytes out
    Take(u16),
    /// Take everything that is ready
    TakeAll,
    /// Ask how much is ready without taking it
    Peek,
}

/// What the fuzzer gets to control
#[derive(Debug, arbitrary::Arbitrary)]
struct Input {
    /// Which version to cart with
    version: u8,
    /// The operations to perform before finishing
    ops: Vec<Op>,
    /// The operations to attempt after finishing, which must all be refused or harmless
    after: Vec<Op>,
    /// How to deliver the finished cart to the uncarter
    schedule: Schedule,
}

fuzz_target!(|input: Input| {
    let chosen = version(input.version);
    let mut manual = CartManual::builder(key())
        .version(chosen)
        .v2_options(V2Options {
            zstd_level: 3,
            block_size_log2: BLOCK_SIZE_LOG2,
        })
        .build()
        .expect("the manual builder rejected its own defaults");
    // everything that was pushed, so the round trip at the end has something to compare against
    let mut pushed = Vec::new();
    // everything that was taken, in order, which is the carted file
    let mut carted = Vec::new();
    for op in &input.ops {
        match op {
            Op::Push(data) => {
                // cap the total so a case cannot become a memory test
                let room = MAX_PLAIN.saturating_sub(pushed.len());
                let data = &data[..data.len().min(room)];
                manual.push(data).expect("pushing before finish must work");
                pushed.extend_from_slice(data);
            }
            Op::Take(want) => {
                let taken = manual.take_up_to(usize::from(*want));
                // a take must never hand back more than was asked for
                assert!(
                    taken.len() <= usize::from(*want),
                    "take_up_to({want}) returned {} bytes",
                    taken.len(),
                );
                carted.extend_from_slice(&taken);
            }
            Op::TakeAll => carted.extend_from_slice(&manual.take()),
            Op::Peek => {
                // `ready` and `is_empty` have to agree, since callers branch on both
                assert_eq!(
                    manual.is_empty(),
                    manual.ready() == 0,
                    "is_empty and ready disagreed",
                );
            }
        }
    }
    // finish, then drain, which is the only sequence a caller is ever asked to perform
    manual.finish().expect("finishing a live cart must work");
    carted.extend_from_slice(&manual.take());
    assert!(
        manual.is_empty(),
        "a take after finish left {} bytes behind",
        manual.ready(),
    );
    assert!(manual.is_finished(), "finish did not mark the cart finished");
    // using it afterwards has to be refused rather than corrupt the file we just wrote
    for op in &input.after {
        match op {
            Op::Push(data) => match manual.push(data) {
                Ok(()) => panic!("pushing after finish was accepted"),
                Err(Error::AlreadyFinished) => {}
                Err(err) => panic!("pushing after finish gave the wrong error: {err}"),
            },
            Op::Take(want) => assert!(
                manual.take_up_to(usize::from(*want)).is_empty(),
                "a drained cart produced bytes after finish",
            ),
            Op::TakeAll => assert!(
                manual.take().is_empty(),
                "a drained cart produced bytes after finish",
            ),
            Op::Peek => assert_eq!(manual.ready(), 0, "a drained cart reported bytes ready"),
        }
    }
    // finishing twice is the shape that used to append a second footer
    match manual.finish() {
        Ok(()) => panic!("finishing twice was accepted"),
        Err(Error::AlreadyFinished) => {}
        Err(err) => panic!("finishing twice gave the wrong error: {err}"),
    }
    // and the whole point: whatever ordering the fuzzer picked, the bytes it collected are a
    // valid cart of exactly what was pushed
    let recovered = run(drain(
        UncartStream::new(StallingReader::new(&carted, input.schedule)),
        input.schedule.buf(),
    ))
    .expect("the bytes taken out of the manual API did not form a readable cart");
    assert_eq!(
        recovered, pushed,
        "a {chosen} cart built from {} operations did not round trip",
        input.ops.len(),
    );
});

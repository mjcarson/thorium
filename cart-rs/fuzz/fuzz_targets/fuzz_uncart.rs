//! Feed arbitrary bytes to the uncarter and require that it behaves
//!
//! Uncarting is reachable from a download path, so the only acceptable outcomes for a hostile
//! file are a clean error or a clean read. A panic, a hang, or an allocation sized by a length
//! field the attacker wrote are all denial of service.
//!
//! The allocation case is the one worth naming: the body is a run of length prefixed blocks, and
//! a four byte prefix can ask for four gibibytes. The reader has to cap what it is willing to
//! reserve rather than trusting the prefix, and the memory limit this target runs under is what
//! turns a regression there into a reported crash.

#![no_main]

use libfuzzer_sys::fuzz_target;

use cart_rs::UncartStream;
use cart_rs_fuzz::{Schedule, StallingReader, drain, run};

fuzz_target!(|input: (Schedule, Vec<u8>)| {
    let (schedule, raw) = input;
    // the assertion is that this returns at all, without exhausting memory on the way
    let _ = run(drain(
        UncartStream::new(StallingReader::new(&raw, schedule)),
        schedule.buf(),
    ));
});

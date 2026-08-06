//! Parse arbitrary slices as a header or a footer and require an error rather than a panic
//!
//! Both parsers are `pub`, both take a `&[u8]` of any length, and the old implementations both
//! indexed before they checked: `Header::get` panicked on any 4..=29 byte slice beginning with
//! `CART`, and `Footer::get` sliced `raw[raw.len() - 28..]` with no length check at all. Neither
//! was reachable from a call site at the time, which is exactly why they survived — so this
//! target treats them as the public API they are declared to be rather than as internals.
//!
//! The second half is a round trip: anything the writers emit must parse back to the same
//! values, which is what keeps `to_bytes` and `parse` from drifting apart field by field.

#![no_main]

use libfuzzer_sys::fuzz_target;

use cart_rs::header::MAX_OPT_LEN;
use cart_rs::{CartKey, CartVersion, Footer, Header};

/// What the fuzzer gets to control
#[derive(Debug, arbitrary::Arbitrary)]
struct Input {
    /// The bytes to try to parse as a header, a footer, and a footer tail
    raw: Vec<u8>,
    /// The key to build a header round trip with
    key: [u8; 16],
    /// The reserved bytes to build a header round trip with
    reserved: [u8; 8],
    /// The optional section length to build a header round trip with
    opt_len: u64,
    /// The optional footer position to build a footer round trip with
    opt_pos: u64,
    /// The optional footer length to build a footer round trip with
    opt_footer_len: u64,
    /// Which version to build the header round trip with
    version: u8,
}

fuzz_target!(|input: Input| {
    // none of these may panic for any input of any length, including empty
    let _ = Header::parse(&input.raw);
    let _ = Header::peek_version(&input.raw);
    let _ = Footer::parse(&input.raw);
    let _ = Footer::parse_tail(&input.raw);
    // parsing a prefix of every length is the shape that used to panic, since the old code
    // checked the magic number before it checked the length
    for len in 0..input.raw.len().min(64) {
        let _ = Header::parse(&input.raw[..len]);
        let _ = Footer::parse(&input.raw[..len]);
        let _ = Footer::parse_tail(&input.raw[..len]);
    }
    // now the other direction: whatever the writers emit has to parse back unchanged
    let version = if input.version.is_multiple_of(2) {
        CartVersion::V1
    } else {
        CartVersion::V2
    };
    let key = CartKey::from_bytes(input.key);
    let header = Header::new(version, key)
        .reserved(input.reserved)
        .opt_len(input.opt_len);
    let parsed = Header::parse(&header.to_bytes());
    // an oversized optional section is rejected on purpose, so only assert the round trip for
    // the lengths the parser is willing to accept
    if input.opt_len <= MAX_OPT_LEN {
        let parsed = parsed.expect("a header we wrote ourselves failed to parse");
        assert_eq!(parsed.version, version, "the version did not round trip");
        assert_eq!(parsed.key, key, "the key did not round trip");
        assert_eq!(
            parsed.reserved, input.reserved,
            "the reserved bytes did not round trip",
        );
        assert_eq!(
            parsed.opt_len, input.opt_len,
            "the optional length did not round trip",
        );
    }
    // the footer carries two more attacker reachable u64s, so round trip those too
    let footer = Footer::new().optional(input.opt_pos, input.opt_footer_len);
    if let Ok(parsed) = Footer::parse(&footer.to_bytes()) {
        assert_eq!(
            parsed.opt_footer_pos, input.opt_pos,
            "the optional footer position did not round trip",
        );
        assert_eq!(
            parsed.opt_footer_len, input.opt_footer_len,
            "the optional footer length did not round trip",
        );
    }
});

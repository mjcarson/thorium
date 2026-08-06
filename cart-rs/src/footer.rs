//! The mandatory 28 byte `CaRT` footer
//!
//! Every `CaRT` file, whatever its version, ends with the same 28 bytes:
//!
//! ```text
//! [0..4]   "TRAC"                          magic number
//! [4..12]  reserved                        8 bytes
//! [12..20] opt_footer_pos                  u64 LE, absolute offset of the optional footer
//! [20..28] opt_footer_len                  u64 LE, length of the optional footer
//! ```
//!
//! The [`CaRT` docs](https://bitbucket.org/cse-assemblyline/cart/src/master/) say the footer
//! is 32 bytes. It is 28. Here is `cart corn` from the reference Python tool, which is where
//! the layout above and the compat vectors both come from:
//!
//! ```text
//! 00000000  43 41 52 54 01 00 00 00  00 00 00 00 00 00 03 01  |CART............|
//! 00000010  04 01 05 09 02 06 03 01  04 01 05 09 02 06 0f 00  |................|
//! 00000020  00 00 00 00 00 00 c2 a4  a5 5c 53 d5 43 f7 79 76  |.........\S.C.yv|
//! 00000030  39 d6 6f 11 9d c1 87 b8  10 f5 7c 10 03 74 df b5  |9.o.......|..t..|
//! 00000040  a6 01 23 34 a6 92 c2 a4  a7 58 50 d7 15 a5 79 2f  |..#4.....XP...y/|
//! 00000050  74 9d 23 1f c2 c8 db d3  8b 07 b0 7b 1a 22 79 4a  |t.#........{."yJ|
//! 00000060  0b 8e e2 b9 60 73 74 2b  56 c6 71 67 62 e9 1d 47  |....`st+V.qgb..G|
//! 00000070  3b 28 d8 87 f1 d4 50 17  5c 42 e0 33 3d da e5 07  |;(....P.\B.3=...|
//! 00000080  09 28 ec 4f f0 10 83 58  d3 d7 ef f2 48 fa a4 bd  |.(.O...X....H...|
//! 00000090  87 e4 5e 51 7e e4 d8 6f  66 0b 32 db 69 ef c8 74  |..^Q~..of.2.i..t|
//! 000000a0  c9 ed d1 78 a8 88 07 45  09 13 55 88 b8 48 ee 52  |...x...E..U..H.R|
//! 000000b0  1c ec f4 2e 6b 79 0a 97  0b 75 b3 96 76 21 83 2c  |....ky...u..v!.,|
//! 000000c0  14 a5 e2 17 03 99 d6 82  da 8a 03 e4 32 fe 62 eb  |............2.b.|
//! 000000d0  29 67 b6 95 eb 9c 69 21  8a d3 e0 97 af c4 22 da  |)g....i!......".|
//! 000000e0  ef 58 29 58 4c 8f d7 e0  35 f1 34 7d 55 7a f1 c6  |.X)XL...5.4}Uz..|
//! 000000f0  f6 c7 3d e2 60 d3 8c 14  64 58 d1 54 52 41 43 00  |..=.`...dX.TRAC.|
//! 00000100  00 00 00 00 00 00 00 46  00 00 00 00 00 00 00 b5  |.......F........|
//! 00000110  00 00 00 00 00 00 00                              |.......|
//! 00000117
//! ```
//!
//! Decoding those 279 bytes is what taught us the format, so it is worth writing down:
//!
//! ```text
//! [0..38)     header, plaintext                  opt_len = 15
//! [38..53)    optional header,  RC4              {"name":"corn"}
//! [53..70)    body, RC4( zlib(...) )             -> "EvilCorn\n"
//! [70..251)   optional footer, RC4               {"length":"9","md5":..,"sha1":..,"sha256":..}
//! [251..279)  footer, plaintext                  opt_footer_pos = 70, opt_footer_len = 181
//! ```
//!
//! Two things follow, and neither was handled before. A bare `cart <file>` emits **both** an
//! optional header and an optional footer by default, so `opt_len != 0` is the normal case for
//! anything cart-rs did not write. And the RC4 keystream is **reset at every section
//! boundary** — one continuous RC4 from offset 38 decrypts the optional header and then yields
//! garbage for the body.

use crate::error::Error;

/// The length of the mandatory `CaRT` footer
pub const FOOTER_LEN: usize = 28;

/// The magic number every `CaRT` file ends with
pub const MAGIC_NUM: &[u8; 4] = b"TRAC";

/// The number of reserved bytes in the footer
pub const RESERVED_LEN: usize = 8;

/// The largest optional footer cart-rs is willing to buffer
///
/// The reference tool writes a JSON blob of hashes here, around 181 bytes. The field is a
/// `u64` though, so it needs a cap for the same reason the optional header does.
pub const MAX_OPT_FOOTER_LEN: u64 = 1 << 20;

/// The mandatory footer at the end of every `CaRT` file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Footer {
    /// The 8 reserved bytes
    pub reserved: [u8; RESERVED_LEN],
    /// The absolute offset of the optional footer, or 0 if there is not one
    pub opt_footer_pos: u64,
    /// The length of the optional footer, or 0 if there is not one
    pub opt_footer_len: u64,
}

impl Footer {
    /// Build an empty footer, which is what cart-rs writes
    #[must_use]
    pub const fn new() -> Self {
        Footer {
            reserved: [0; RESERVED_LEN],
            opt_footer_pos: 0,
            opt_footer_len: 0,
        }
    }

    /// Point this footer at an optional footer
    ///
    /// # Arguments
    ///
    /// * `pos` - The absolute offset the optional footer starts at
    /// * `len` - The length of the optional footer
    #[must_use]
    pub const fn optional(mut self, pos: u64, len: u64) -> Self {
        self.opt_footer_pos = pos;
        self.opt_footer_len = len;
        self
    }

    /// Parse a footer from the last [`FOOTER_LEN`] bytes of a `CaRT` file
    ///
    /// This takes exactly the footer, not the whole file, so it never has to slice from the
    /// end of a buffer it did not check the length of.
    ///
    /// # Arguments
    ///
    /// * `raw` - The [`FOOTER_LEN`] bytes to parse a footer from
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] if fewer than [`FOOTER_LEN`] bytes were given,
    /// [`Error::MissingFooter`] if the magic number is wrong, and [`Error::LimitExceeded`] if
    /// the optional footer is larger than [`MAX_OPT_FOOTER_LEN`].
    pub fn parse(raw: &[u8]) -> Result<Self, Error> {
        // we cannot parse a footer we do not have all of
        let raw: &[u8; FOOTER_LEN] = raw
            .get(..FOOTER_LEN)
            .and_then(|slice| slice.try_into().ok())
            .ok_or(Error::Truncated { section: "footer" })?;
        // a CaRT file ends with the TRAC magic number
        if &raw[..4] != MAGIC_NUM {
            return Err(Error::MissingFooter);
        }
        // pull out the reserved bytes
        let mut reserved = [0; RESERVED_LEN];
        reserved.copy_from_slice(&raw[4..4 + RESERVED_LEN]);
        // pull out where the optional footer lives and how long it is
        let opt_footer_pos = u64::from_le_bytes([
            raw[12], raw[13], raw[14], raw[15], raw[16], raw[17], raw[18], raw[19],
        ]);
        let opt_footer_len = u64::from_le_bytes([
            raw[20], raw[21], raw[22], raw[23], raw[24], raw[25], raw[26], raw[27],
        ]);
        // refuse to buffer an optional footer larger than we are willing to allocate for
        if opt_footer_len > MAX_OPT_FOOTER_LEN {
            return Err(Error::LimitExceeded {
                section: "optional footer",
                declared: opt_footer_len,
                max: MAX_OPT_FOOTER_LEN,
            });
        }
        Ok(Footer {
            reserved,
            opt_footer_pos,
            opt_footer_len,
        })
    }

    /// Parse a footer from the tail of a complete `CaRT` file
    ///
    /// # Arguments
    ///
    /// * `raw` - The whole `CaRT` file, or at least its last [`FOOTER_LEN`] bytes
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Footer::parse`].
    pub fn parse_tail(raw: &[u8]) -> Result<Self, Error> {
        // work out where the footer starts without ever subtracting past zero
        let start = raw
            .len()
            .checked_sub(FOOTER_LEN)
            .ok_or(Error::Truncated { section: "footer" })?;
        Footer::parse(&raw[start..])
    }

    /// Serialize this footer into its 28 byte on disk form
    #[must_use]
    pub fn to_bytes(&self) -> [u8; FOOTER_LEN] {
        let mut raw = [0; FOOTER_LEN];
        // write the TRAC magic number
        raw[..4].copy_from_slice(MAGIC_NUM);
        // write the reserved bytes
        raw[4..4 + RESERVED_LEN].copy_from_slice(&self.reserved);
        // write where the optional footer lives and how long it is
        raw[12..20].copy_from_slice(&self.opt_footer_pos.to_le_bytes());
        raw[20..28].copy_from_slice(&self.opt_footer_len.to_le_bytes());
        raw
    }

    /// How many bytes at the end of the file are not part of the body
    ///
    /// This is [`FOOTER_LEN`] plus the optional footer. `opt_footer_len` is capped at
    /// [`MAX_OPT_FOOTER_LEN`] by [`Footer::parse`], so this cannot overflow.
    #[must_use]
    pub const fn trim(&self) -> u64 {
        FOOTER_LEN as u64 + self.opt_footer_len
    }
}

#[cfg(test)]
mod tests {
    use super::{FOOTER_LEN, Footer, MAGIC_NUM, MAX_OPT_FOOTER_LEN};
    use crate::error::Error;

    /// The footer the reference tool wrote for `cart corn`
    fn reference_footer() -> Footer {
        Footer::new().optional(70, 181)
    }

    #[test]
    fn footers_round_trip_through_their_bytes() {
        let footer = reference_footer();
        assert_eq!(Footer::parse(&footer.to_bytes()).unwrap(), footer);
    }

    /// Pin the on disk layout so a refactor cannot quietly move a field
    #[test]
    fn the_on_disk_layout_is_pinned() {
        let raw = reference_footer().to_bytes();
        assert_eq!(raw.len(), FOOTER_LEN);
        assert_eq!(&raw[..4], MAGIC_NUM);
        assert_eq!(&raw[4..12], &[0; 8]);
        assert_eq!(&raw[12..20], &[70, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&raw[20..28], &[181, 0, 0, 0, 0, 0, 0, 0]);
    }

    /// The last 28 bytes of the `cart corn` hexdump in this module's docs
    #[test]
    fn the_reference_tools_footer_parses() {
        let raw = hex::decode(concat!(
            "54524143",         // "TRAC"
            "0000000000000000", // 8 reserved bytes
            "4600000000000000", // opt_footer_pos = 70
            "b500000000000000", // opt_footer_len = 181
        ))
        .unwrap();
        assert_eq!(raw.len(), FOOTER_LEN);
        let footer = Footer::parse(&raw).unwrap();
        assert_eq!(footer, reference_footer());
        assert_eq!(footer.trim(), 28 + 181);
        // and it serializes back to exactly the bytes the reference tool wrote
        assert_eq!(footer.to_bytes().as_slice(), raw.as_slice());
    }

    /// The old `Footer::get` did `raw[raw.len() - 28..]` with no length check
    #[test]
    fn every_short_slice_errors_instead_of_panicking() {
        let full = reference_footer().to_bytes();
        for len in 0..FOOTER_LEN {
            assert!(
                Footer::parse(&full[..len]).is_err(),
                "a {len} byte slice should not parse as a footer"
            );
            assert!(
                Footer::parse_tail(&full[..len]).is_err(),
                "a {len} byte file should not have its tail parse as a footer"
            );
        }
    }

    #[test]
    fn a_file_without_the_trac_magic_number_is_rejected() {
        let mut raw = reference_footer().to_bytes();
        raw[0] = b'C';
        assert!(matches!(Footer::parse(&raw), Err(Error::MissingFooter)));
    }

    #[test]
    fn parse_tail_finds_the_footer_at_the_end_of_a_file() {
        let mut file = b"CART...body bytes that are not a footer...".to_vec();
        file.extend_from_slice(&reference_footer().to_bytes());
        assert_eq!(Footer::parse_tail(&file).unwrap(), reference_footer());
    }

    #[test]
    fn an_oversized_optional_footer_is_rejected() {
        let mut footer = Footer::new();
        footer.opt_footer_len = MAX_OPT_FOOTER_LEN + 1;
        assert!(matches!(
            Footer::parse(&footer.to_bytes()),
            Err(Error::LimitExceeded { .. })
        ));
        // the cap itself is still allowed
        footer.opt_footer_len = MAX_OPT_FOOTER_LEN;
        assert!(Footer::parse(&footer.to_bytes()).is_ok());
    }

    /// An empty footer is what cart-rs itself writes, so pin it
    #[test]
    fn the_empty_footer_is_trac_plus_24_zeros() {
        let raw = Footer::new().to_bytes();
        assert_eq!(&raw[..4], b"TRAC");
        assert_eq!(&raw[4..], &[0; FOOTER_LEN - 4]);
        assert_eq!(Footer::new().trim(), FOOTER_LEN as u64);
    }
}

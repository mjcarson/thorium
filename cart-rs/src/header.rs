//! The mandatory 38 byte `CaRT` header
//!
//! Every `CaRT` file, whatever its version, starts with the same 38 bytes:
//!
//! ```text
//! [0..4]   "CART"                          magic number
//! [4..6]   version                         u16 LE
//! [6..14]  reserved                        8 bytes
//! [14..30] key                             16 bytes, plaintext
//! [30..38] opt_len                         u64 LE, length of the optional header
//! ```
//!
//! The key really is stored in the clear. `CaRT` exists to stop a sample from being executed
//! or matched by a scanner, not to keep it secret, so this is by design.
//!
//! The 8 reserved bytes at `[6..14]` are zero in V1. V2 uses them for a per file random nonce
//! salt, which is what stops every file in a deployment from sharing an AES-GCM keystream.

use crate::error::Error;
use crate::key::{CartKey, KEY_LEN};
use crate::version::CartVersion;

/// The length of the mandatory `CaRT` header
pub const HEADER_LEN: usize = 38;

/// The magic number every `CaRT` file starts with
pub const MAGIC_NUM: &[u8; 4] = b"CART";

/// The number of reserved bytes in the header
pub const RESERVED_LEN: usize = 8;

/// The largest optional header cart-rs is willing to buffer
///
/// The reference tool writes a small JSON blob here, on the order of tens of bytes. The field
/// is a `u64` though, so without a cap a 38 byte file could ask us for 16 exabytes.
pub const MAX_OPT_LEN: u64 = 1 << 20;

/// The mandatory header at the start of every `CaRT` file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// The version of `CaRT` this file uses
    pub version: CartVersion,
    /// The 8 reserved bytes, which V2 uses as its per file nonce salt
    pub reserved: [u8; RESERVED_LEN],
    /// The key this file was encrypted with
    pub key: CartKey,
    /// The length of the optional header that follows these 38 bytes
    pub opt_len: u64,
}

impl Header {
    /// Build a new header
    ///
    /// # Arguments
    ///
    /// * `version` - The `CaRT` version this file uses
    /// * `key` - The key this file is encrypted with
    #[must_use]
    pub const fn new(version: CartVersion, key: CartKey) -> Self {
        Header {
            version,
            reserved: [0; RESERVED_LEN],
            key,
            opt_len: 0,
        }
    }

    /// Set the 8 reserved bytes
    ///
    /// # Arguments
    ///
    /// * `reserved` - The bytes to store in the reserved field
    #[must_use]
    pub const fn reserved(mut self, reserved: [u8; RESERVED_LEN]) -> Self {
        self.reserved = reserved;
        self
    }

    /// Set the length of the optional header that follows this one
    ///
    /// # Arguments
    ///
    /// * `opt_len` - The length of the optional header
    #[must_use]
    pub const fn opt_len(mut self, opt_len: u64) -> Self {
        self.opt_len = opt_len;
        self
    }

    /// Parse a header from the first [`HEADER_LEN`] bytes of a `CaRT` file
    ///
    /// This never panics and never indexes past the end of `raw`, whatever it contains. That
    /// matters because the old `Header::get` was `pub` and panicked for any slice of 4 to 29
    /// bytes that happened to start with `CART`.
    ///
    /// # Arguments
    ///
    /// * `raw` - The bytes to parse a header from
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] if fewer than [`HEADER_LEN`] bytes were given,
    /// [`Error::NotACart`] if the magic number is wrong, [`Error::UnsupportedVersion`] if the
    /// version is one we do not know, and [`Error::LimitExceeded`] if `opt_len` is larger than
    /// [`MAX_OPT_LEN`].
    pub fn parse(raw: &[u8]) -> Result<Self, Error> {
        // we cannot parse a header we do not have all of
        let raw: &[u8; HEADER_LEN] = raw
            .get(..HEADER_LEN)
            .and_then(|slice| slice.try_into().ok())
            .ok_or(Error::Truncated { section: "header" })?;
        // this is only a CaRT file if it starts with the CaRT magic number
        if &raw[..4] != MAGIC_NUM {
            return Err(Error::NotACart);
        }
        // pull out the version, which is a u16 LE and not the u8 the old parser read
        let version = CartVersion::from_u16(u16::from_le_bytes([raw[4], raw[5]]))?;
        // pull out the reserved bytes, which V2 uses as its nonce salt
        let mut reserved = [0; RESERVED_LEN];
        reserved.copy_from_slice(&raw[6..6 + RESERVED_LEN]);
        // pull out the plaintext key
        let mut key = [0; KEY_LEN];
        key.copy_from_slice(&raw[14..14 + KEY_LEN]);
        // pull out the length of the optional header
        let opt_len = u64::from_le_bytes([
            raw[30], raw[31], raw[32], raw[33], raw[34], raw[35], raw[36], raw[37],
        ]);
        // refuse to buffer an optional header larger than we are willing to allocate for
        if opt_len > MAX_OPT_LEN {
            return Err(Error::LimitExceeded {
                section: "optional header",
                declared: opt_len,
                max: MAX_OPT_LEN,
            });
        }
        Ok(Header {
            version,
            reserved,
            key: CartKey::from_bytes(key),
            opt_len,
        })
    }

    /// Serialize this header into its 38 byte on disk form
    #[must_use]
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut raw = [0; HEADER_LEN];
        // write the CaRT magic number
        raw[..4].copy_from_slice(MAGIC_NUM);
        // write the version as a u16 LE
        raw[4..6].copy_from_slice(&self.version.as_u16().to_le_bytes());
        // write the reserved bytes
        raw[6..6 + RESERVED_LEN].copy_from_slice(&self.reserved);
        // write the plaintext key
        raw[14..14 + KEY_LEN].copy_from_slice(self.key.as_bytes());
        // write the length of the optional header
        raw[30..38].copy_from_slice(&self.opt_len.to_le_bytes());
        raw
    }

    /// How many bytes to skip from the start of the file to reach the body
    ///
    /// This is [`HEADER_LEN`] plus the optional header. `opt_len` is capped at
    /// [`MAX_OPT_LEN`] by [`Header::parse`], so this cannot overflow.
    #[must_use]
    pub const fn skip(&self) -> u64 {
        HEADER_LEN as u64 + self.opt_len
    }

    /// Peek at just the version of a `CaRT` file without parsing the rest of the header
    ///
    /// # Arguments
    ///
    /// * `raw` - The first 6 or more bytes of a `CaRT` file
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] if fewer than 6 bytes were given, [`Error::NotACart`] if
    /// the magic number is wrong, and [`Error::UnsupportedVersion`] for a version we do not
    /// know about.
    pub fn peek_version(raw: &[u8]) -> Result<CartVersion, Error> {
        // we need the magic number and the version to say anything at all
        let raw = raw.get(..6).ok_or(Error::Truncated { section: "header" })?;
        // this is only a CaRT file if it starts with the CaRT magic number
        if &raw[..4] != MAGIC_NUM {
            return Err(Error::NotACart);
        }
        CartVersion::from_u16(u16::from_le_bytes([raw[4], raw[5]]))
    }
}

#[cfg(test)]
mod tests {
    use super::{HEADER_LEN, Header, MAGIC_NUM, MAX_OPT_LEN};
    use crate::error::Error;
    use crate::key::CartKey;
    use crate::version::CartVersion;

    /// Build a header with every field set to something distinguishable
    fn full_header() -> Header {
        Header::new(
            CartVersion::V2,
            CartKey::from_password("SecretCornIsBest").unwrap(),
        )
        .reserved([1, 2, 3, 4, 5, 6, 7, 8])
        .opt_len(15)
    }

    #[test]
    fn headers_round_trip_through_their_bytes() {
        let header = full_header();
        assert_eq!(Header::parse(&header.to_bytes()).unwrap(), header);
    }

    /// Pin the on disk layout so a refactor cannot quietly move a field
    #[test]
    fn the_on_disk_layout_is_pinned() {
        let raw = full_header().to_bytes();
        assert_eq!(raw.len(), HEADER_LEN);
        assert_eq!(&raw[..4], MAGIC_NUM);
        assert_eq!(&raw[4..6], &[2, 0]);
        assert_eq!(&raw[6..14], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&raw[14..30], b"SecretCornIsBest");
        assert_eq!(&raw[30..38], &[15, 0, 0, 0, 0, 0, 0, 0]);
    }

    /// The old parser read the version out of a single byte at `[4..5]`; the format says it is
    /// a u16 LE at `[4..6]`
    #[test]
    fn the_version_is_read_as_a_u16() {
        let mut raw = full_header().to_bytes();
        // a high byte of 0 and a low byte of 2 is version 512, not version 2
        raw[4] = 0;
        raw[5] = 2;
        assert!(matches!(
            Header::parse(&raw),
            Err(Error::UnsupportedVersion(512))
        ));
    }

    /// The old `Header::get` panicked on any slice of 4..=29 bytes that started with `CART`
    #[test]
    fn every_short_slice_errors_instead_of_panicking() {
        let full = full_header().to_bytes();
        for len in 0..HEADER_LEN {
            assert!(
                Header::parse(&full[..len]).is_err(),
                "a {len} byte slice should not parse as a header"
            );
        }
    }

    #[test]
    fn non_cart_data_is_rejected() {
        let mut raw = full_header().to_bytes();
        raw[0] = b'D';
        assert!(matches!(Header::parse(&raw), Err(Error::NotACart)));
    }

    #[test]
    fn an_oversized_optional_header_is_rejected() {
        let mut header = full_header();
        header.opt_len = MAX_OPT_LEN + 1;
        assert!(matches!(
            Header::parse(&header.to_bytes()),
            Err(Error::LimitExceeded { .. })
        ));
        // the cap itself is still allowed
        header.opt_len = MAX_OPT_LEN;
        assert!(Header::parse(&header.to_bytes()).is_ok());
    }

    #[test]
    fn trailing_bytes_after_the_header_are_ignored() {
        let mut raw = full_header().to_bytes().to_vec();
        raw.extend_from_slice(b"this is the body");
        assert_eq!(Header::parse(&raw).unwrap(), full_header());
    }

    #[test]
    fn peeking_the_version_agrees_with_a_full_parse() {
        let raw = full_header().to_bytes();
        assert_eq!(Header::peek_version(&raw).unwrap(), CartVersion::V2);
        assert_eq!(Header::peek_version(&raw[..6]).unwrap(), CartVersion::V2);
        assert!(Header::peek_version(&raw[..5]).is_err());
    }

    #[test]
    fn skip_accounts_for_the_optional_header() {
        assert_eq!(full_header().skip(), HEADER_LEN as u64 + 15);
        assert_eq!(
            Header::new(CartVersion::V1, CartKey::from_bytes([0; 16])).skip(),
            HEADER_LEN as u64
        );
    }

    /// The first 38 bytes of the `cart corn` hexdump in `footer.rs`, which came from the
    /// reference Python tool
    #[test]
    fn the_reference_tools_header_parses() {
        let raw = hex::decode(concat!(
            "43415254",                         // "CART"
            "0100",                             // version 1, as a u16 LE
            "0000000000000000",                 // 8 reserved bytes
            "03010401050902060301040105090206", // the 16 byte plaintext key
            "0f00000000000000",                 // opt_len = 15
        ))
        .unwrap();
        assert_eq!(raw.len(), HEADER_LEN);
        let header = Header::parse(&raw).unwrap();
        assert_eq!(header.version, CartVersion::V1);
        assert_eq!(header.opt_len, 15);
        assert_eq!(header.reserved, [0; 8]);
        assert_eq!(
            header.key.as_bytes(),
            &[3, 1, 4, 1, 5, 9, 2, 6, 3, 1, 4, 1, 5, 9, 2, 6]
        );
        // and it serializes back to exactly the bytes the reference tool wrote
        assert_eq!(header.to_bytes().as_slice(), raw.as_slice());
    }
}

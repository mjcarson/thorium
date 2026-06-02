//! The headers for the cart format

use crate::{CartVersion, Error};
use std::io::Write;

/// The length of a `CaRT` header (same for all versions)
pub const HEADER_LEN: usize = 38;
/// The cart magic number preceding the header
pub static MAGIC_NUM: &[u8; 4] = b"CART";
/// The length of the key used for encryption
pub const KEY_LEN: usize = 16;

/// The mandatory header object for cart.
///
/// The header format is identical for all CaRT versions:
///
///   `[0..4]`   Magic ("CART")
///   `[4..6]`   Version (u16 LE)
///   `[6..14]`  Reserved (zeros)
///   `[14..30]` Key (16 bytes)
///   `[30..38]` Opt_len (u64 LE, always 0 when created by Thorium's cart-rs)
///
/// The version field tells the reader which compression and encryption scheme
/// was used for the payload between the header and footer.
#[derive(Debug, Clone)]
pub struct Header {
    /// The version of ``CaRT`` in use
    pub version: CartVersion,
    /// The key used to encrypt this file
    pub key: Vec<u8>,
    /// The length of the optional header
    pub opt_len: usize,
}

impl Header {
    /// Write a `CaRT` header for the given version.
    ///
    /// # Arguments
    ///
    /// * `version` - The CaRT version to write
    /// * `key`     - The 16-byte encryption key
    /// * `buf`     - Destination buffer (must be at least `HEADER_LEN` bytes)
    pub fn write(version: CartVersion, key: &[u8], mut buf: &mut [u8]) -> Result<(), Error> {
        Self::validate_key(key)?;
        // write the CaRT magic number (4 bytes)
        buf.write_all(MAGIC_NUM)?;
        // write the version number (2 bytes)
        let version_num: u16 = match version {
            CartVersion::V1 => 1,
            CartVersion::V2 => 2,
        };
        buf.write_all(&version_num.to_le_bytes())?;
        // write reserved space (8 bytes)
        buf.write_all(&0u64.to_le_bytes())?;
        // write the encryption key (16 bytes)
        buf.write_all(key)?;
        // hardcode an optional header length of 0 (8 bytes)
        buf.write_all(&0u64.to_le_bytes())?;
        Ok(())
    }

    /// Gets the header from the first 38 bytes of the raw binary.
    ///
    /// # Arguments
    ///
    /// * `raw` - The first bytes of the binary containing the header
    ///           (at least 38 bytes)
    pub fn get(raw: &[u8]) -> Result<Self, Error> {
        Self::validate(raw)?;
        if raw.len() < HEADER_LEN {
            return Err(Error::new("Invalid CaRT header: insufficient bytes"));
        }
        let version_num = u16::from_le_bytes(
            raw[4..6]
                .try_into()
                .map_err(|_| Error::new("Invalid header: insufficient bytes for version"))?,
        );
        let version = match version_num {
            1 => CartVersion::V1,
            2 => CartVersion::V2,
            v => return Err(Error::UnsupportedVersion(v)),
        };
        // extract the encryption key
        let key = raw[14..30].to_vec();
        // extract the length of the optional header
        let opt_len = u64::from_le_bytes(
            raw[30..38]
                .try_into()
                .map_err(|_| Error::new("Invalid header: insufficient bytes for opt_len"))?,
        ) as usize;
        Ok(Header {
            version,
            key,
            opt_len,
        })
    }

    pub fn validate(raw: &[u8]) -> Result<(), Error> {
        if raw.len() < 4 {
            return Err(Error::new(
                "Cannot validate Cart file because the given header buffer is empty or too small",
            ));
        } else if raw[..4] != *MAGIC_NUM {
            return Err(Error::new("File does not start with the CART magic number"));
        }
        Ok(())
    }

    /// Checks that the key is valid given CART specifications. The key must be exactly 16
    /// bytes long to be valid.
    ///
    /// # Arguments
    ///
    /// * `key` - The key used for encryption
    pub fn validate_key(key: &[u8]) -> Result<(), Error> {
        if key.len() != KEY_LEN {
            return Err(Error::new(format!(
                "The given key does not have the correct length of {}. Given key length: {}",
                KEY_LEN,
                key.len()
            )));
        }
        Ok(())
    }

    /// Calculate how much of the binary to skip to get past the header
    #[must_use]
    pub fn skip(&self) -> usize {
        HEADER_LEN + self.opt_len
    }
}

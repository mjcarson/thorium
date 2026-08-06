//! The single key type and the single key padding policy for cart-rs
//!
//! `CaRT` stores its encryption key in plaintext in the file header. The key exists to stop a
//! `CaRT`ed sample from being executed or matched by a scanner, not to keep it secret, so
//! nothing here tries to be a secret management story.
//!
//! What this module *does* exist for is to make sure there is exactly one 16 byte key type and
//! exactly one answer to "what happens to a password that isn't 16 bytes". Before this there
//! were two answers: the API panicked on a short password and silently truncated a long one,
//! while Thorctl zero padded a short one and rejected a long one.

use std::str::FromStr;

use crate::error::Error;

/// The length of the key used for encryption
pub const KEY_LEN: usize = 16;

/// The 16 byte key used to encrypt and decrypt a `CaRT` file
///
/// # Examples
///
/// ```
/// use cart_rs::CartKey;
///
/// # fn exec() -> Result<(), cart_rs::Error> {
/// // short passwords are zero padded
/// let key = CartKey::from_password("corn")?;
/// assert_eq!(key.as_bytes(), b"corn\0\0\0\0\0\0\0\0\0\0\0\0");
/// // long passwords are rejected rather than silently truncated
/// assert!(CartKey::from_password("this password is far too long").is_err());
/// # Ok(())
/// # }
/// # exec().unwrap();
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CartKey([u8; KEY_LEN]);

impl CartKey {
    /// The number of bytes in a `CaRT` key
    pub const LEN: usize = KEY_LEN;

    /// Build a key from a password, zero padding it out to 16 bytes
    ///
    /// This is the *only* padding policy in cart-rs. A password longer than 16 bytes is an
    /// error rather than a truncation, because silently truncating a password is how you
    /// write files that nothing can read back.
    ///
    /// # Arguments
    ///
    /// * `password` - The password to build a key from
    ///
    /// # Errors
    ///
    /// Returns [`Error::KeyTooLong`] if the password is longer than 16 bytes.
    pub fn from_password<P: AsRef<[u8]>>(password: P) -> Result<Self, Error> {
        // get the raw bytes of this password
        let raw = password.as_ref();
        // reject any password that cannot fit in a key
        if raw.len() > KEY_LEN {
            return Err(Error::KeyTooLong {
                max: KEY_LEN,
                got: raw.len(),
            });
        }
        // zero pad this password out to a full key
        let mut key = [0_u8; KEY_LEN];
        key[..raw.len()].copy_from_slice(raw);
        Ok(CartKey(key))
    }

    /// Build a key from exactly 16 bytes
    ///
    /// # Arguments
    ///
    /// * `raw` - The 16 bytes to use as this key
    #[must_use]
    pub const fn from_bytes(raw: [u8; KEY_LEN]) -> Self {
        CartKey(raw)
    }

    /// Build a key from a slice of exactly 16 bytes
    ///
    /// # Arguments
    ///
    /// * `raw` - The slice to build a key from
    ///
    /// # Errors
    ///
    /// Returns [`Error::KeyTooLong`] if the slice is not exactly 16 bytes long.
    pub fn from_slice(raw: &[u8]) -> Result<Self, Error> {
        // a slice of the wrong length cannot be a key
        match <[u8; KEY_LEN]>::try_from(raw) {
            Ok(key) => Ok(CartKey(key)),
            Err(_) => Err(Error::KeyTooLong {
                max: KEY_LEN,
                got: raw.len(),
            }),
        }
    }

    /// Get the raw bytes of this key
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl AsRef<[u8]> for CartKey {
    /// Borrow this key as a byte slice
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for CartKey {
    /// Redact the key so it never lands in a log line by accident
    ///
    /// The key is public in the file format, but a log full of them is still noise nobody
    /// wants and it makes grepping for real secrets harder.
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt.write_str("CartKey(<redacted>)")
    }
}

impl FromStr for CartKey {
    type Err = Error;

    /// Parse a key from a password string so clap can use it as a `value_parser`
    ///
    /// # Arguments
    ///
    /// * `raw` - The password to parse
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        CartKey::from_password(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::{CartKey, KEY_LEN};

    #[test]
    fn short_passwords_are_zero_padded() {
        let key = CartKey::from_password("corn").unwrap();
        assert_eq!(&key.as_bytes()[..4], b"corn");
        assert_eq!(&key.as_bytes()[4..], &[0_u8; KEY_LEN - 4]);
    }

    #[test]
    fn empty_passwords_are_allowed() {
        let key = CartKey::from_password("").unwrap();
        assert_eq!(key.as_bytes(), &[0_u8; KEY_LEN]);
    }

    #[test]
    fn exact_length_passwords_are_untouched() {
        let key = CartKey::from_password("SecretCornIsBest").unwrap();
        assert_eq!(key.as_bytes(), b"SecretCornIsBest");
    }

    #[test]
    fn long_passwords_are_rejected_not_truncated() {
        assert!(CartKey::from_password("SecretCornIsBest!").is_err());
    }

    #[test]
    fn from_slice_requires_an_exact_length() {
        assert!(CartKey::from_slice(&[0; KEY_LEN]).is_ok());
        assert!(CartKey::from_slice(&[0; KEY_LEN - 1]).is_err());
        assert!(CartKey::from_slice(&[0; KEY_LEN + 1]).is_err());
    }

    #[test]
    fn debug_does_not_leak_the_key() {
        let key = CartKey::from_password("SecretCornIsBest").unwrap();
        assert_eq!(format!("{key:?}"), "CartKey(<redacted>)");
    }
}

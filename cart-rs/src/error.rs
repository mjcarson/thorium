//! A unified error type for cart-rs

use crate::version::CartVersion;

/// A unified error type for cart-rs
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A generic error with a human readable message
    #[error("{0}")]
    Generic(String),
    /// An error from performing some IO
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// The data handed to us does not begin with the `CART` magic number
    #[error("File does not start with the CaRT magic number")]
    NotACart,
    /// The trailing bytes of this file are not a valid `CaRT` footer
    #[error("File does not end with the CaRT (TRAC) footer magic number")]
    MissingFooter,
    /// The file claims to be a `CaRT` version we do not know how to handle
    #[error("Unsupported CaRT version {0}")]
    UnsupportedVersion(u16),
    /// The file is a `CaRT` version we know about but that was not compiled in
    #[error("CaRT {0} support was not enabled at compile time")]
    VersionDisabled(CartVersion),
    /// The key we were given is not exactly [`crate::key::KEY_LEN`] bytes long
    #[error("A CaRT key must be at most {max} bytes long but {got} bytes were given")]
    KeyTooLong {
        /// The maximum number of bytes a key may be
        max: usize,
        /// The number of bytes that were given
        got: usize,
    },
    /// The file ended in the middle of a structure we were still reading
    #[error("The CaRT file ended unexpectedly while reading the {section}")]
    Truncated {
        /// The section of the file that was being read when the input ran out
        section: &'static str,
    },
    /// The file is structurally invalid in some way
    #[error("Malformed CaRT file: {0}")]
    Malformed(String),
    /// A block failed its authentication check or the compressed data is corrupt
    #[error("CaRT data is corrupt or has been tampered with: {0}")]
    Corrupt(String),
    /// A length field in the file is larger than we are willing to allocate for
    #[error(
        "CaRT file declares a {section} of {declared} bytes which exceeds the {max} byte limit"
    )]
    LimitExceeded {
        /// The section of the file whose declared length was too large
        section: &'static str,
        /// The length the file declared
        declared: u64,
        /// The largest length we will accept
        max: u64,
    },
    /// A compression or decompression option was outside its legal range
    #[error("Invalid CaRT option: {0}")]
    InvalidOption(String),
    /// This cart was already finished and cannot take more data
    #[error("This CaRT has already been finished")]
    AlreadyFinished,
}

impl Error {
    /// Creates a new generic error
    ///
    /// # Arguments
    ///
    /// * `msg` - The error message to return
    #[must_use]
    pub fn new<T: Into<String>>(msg: T) -> Self {
        Error::Generic(msg.into())
    }

    /// Whether this error means the input was not a well formed `CaRT` file
    ///
    /// Callers that map cart errors onto HTTP status codes can use this to tell a bad
    /// request (the user handed us something that is not a `CaRT`) apart from an internal
    /// failure (our own IO broke).
    #[must_use]
    pub fn is_bad_input(&self) -> bool {
        matches!(
            self,
            Error::NotACart
                | Error::MissingFooter
                | Error::UnsupportedVersion(_)
                | Error::Truncated { .. }
                | Error::Malformed(_)
                | Error::Corrupt(_)
                | Error::LimitExceeded { .. }
        )
    }
}

impl From<Error> for std::io::Error {
    /// Convert a cart error into an IO error so it can cross an [`tokio::io::AsyncRead`] boundary
    ///
    /// # Arguments
    ///
    /// * `error` - The cart error to convert
    fn from(error: Error) -> Self {
        // unwrap our IO errors instead of nesting them
        match error {
            Error::Io(err) => err,
            // anything else is a problem with the data we were handed
            other => std::io::Error::new(std::io::ErrorKind::InvalidData, other),
        }
    }
}

//! A unified error class for Cart-rs

use std::{collections::TryReserveError, num::TryFromIntError};

/// a unified error class for Cart-rs
#[derive(Debug)]
pub enum Error {
    /// a Generic error
    Generic(String),
    /// An error from performing some IO
    IO(std::io::Error),
    /// An error from converting an integer
    TryFromInt(TryFromIntError),
    /// An error from reserving more space for data
    TryReserve(TryReserveError),
    /// Finish was called before any data was specified
    FinishBeforeData,
    /// Unsupported CaRT version
    UnsupportedVersion(u16),
    /// An AES-GCM encryption/decryption error
    AesGcm,
}

impl Error {
    /// Creates a new error instance
    ///
    /// # Arguments
    ///
    /// * `msg` - The error message to return
    #[must_use]
    pub fn new<T: Into<String>>(msg: T) -> Self {
        Error::Generic(msg.into())
    }
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Generic(msg) => write!(fmt, "Generic: {msg}"),
            Error::IO(err) => write!(fmt, "IO: {err}"),
            Error::TryFromInt(err) => write!(fmt, "TryFromInt: {err}"),
            Error::TryReserve(err) => write!(fmt, "TryReserve: {err}"),
            Error::FinishBeforeData => write!(fmt, "FinishBeforeData"),
            Error::UnsupportedVersion(v) => write!(fmt, "Unsupported CaRT version: {v}"),
            Error::AesGcm => write!(fmt, "AES-GCM encryption/decryption error"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::IO(error)
    }
}

impl From<TryReserveError> for Error {
    fn from(error: TryReserveError) -> Self {
        Error::TryReserve(error)
    }
}

impl From<TryFromIntError> for Error {
    fn from(error: TryFromIntError) -> Self {
        Error::TryFromInt(error)
    }
}

//! A streaming implementation of the `CaRT` neutering container format
//!
//! `CaRT` stores a file in a form that cannot be executed and that no scanner will match on,
//! while keeping the original bytes recoverable. cart-rs reads and writes two versions of it:
//!
//! | version | cipher | compression | who can read it |
//! |---|---|---|---|
//! | V1 | RC4 | DEFLATE | every `CaRT` tool |
//! | V2 | AES-128-GCM | Zstd | Thorium only |
//!
//! V1 is the official format and is what cart-rs writes by default. V2 is a Thorium specific
//! format that is faster, authenticated, and framed so that it can one day be compressed and
//! decompressed in parallel.

#![forbid(unsafe_code)]

// a CaRT library with every CaRT version compiled out cannot read or write anything, so say so
// here rather than building a crate whose every `build` call returns `Error::VersionDisabled`
#[cfg(not(any(feature = "v1", feature = "v2")))]
compile_error!("cart-rs needs at least one of the `v1` or `v2` features enabled");

pub mod cart;
pub(crate) mod codec;
pub mod error;
pub mod footer;
pub mod header;
pub mod key;
pub mod uncart;
#[cfg(feature = "v1")]
pub(crate) mod v1;
#[cfg(feature = "v2")]
pub(crate) mod v2;
pub mod version;

pub use cart::{CartManual, CartManualBuilder, CartStream, CartStreamBuilder};
pub use error::Error;
pub use footer::{FOOTER_LEN, Footer};
pub use header::{HEADER_LEN, Header};
pub use key::{CartKey, KEY_LEN};
pub use uncart::UncartStream;
pub use version::{CartOptions, CartVersion, V1Options, V2Options};

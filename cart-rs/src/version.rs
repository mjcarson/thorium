//! The `CaRT` versions cart-rs can read and write
//!
//! This module holds two related but distinct things:
//!
//! * [`CartVersion`] — the *runtime* version, used in config files, on the command line, and
//!   in the file header.
//! * [`V1`] / [`V2`] / [`NoVersion`] / [`Dynamic`] — *type level* markers used by the builders
//!   so that a V1 only option cannot be set on a V2 build and vice versa.

use std::str::FromStr;

use crate::error::Error;

/// The default Zstd compression level used by V2
pub const DEFAULT_ZSTD_LEVEL: i32 = 3;

/// The default DEFLATE compression level used by V1
pub const DEFAULT_DEFLATE_LEVEL: u8 = 6;

/// The default base 2 log of the V2 block size
///
/// V2 compresses each block into its own independent Zstd frame so that blocks can one day be
/// compressed and decompressed in parallel. Independence costs compression ratio and the cost
/// shrinks as blocks get bigger; measured against a 119 MB ELF binary and a 4.8 MB source
/// tarball at `zstd -3`, independent frames cost 7-10% at 256 KiB, ~3% at 1 MiB, and <1% at
/// 4 MiB. 1 MiB is the compromise, and because the block size is recorded in the header
/// changing it later is a config change rather than a format change.
pub const DEFAULT_BLOCK_SIZE_LOG2: u8 = 20;

/// The smallest V2 block size we will accept, as a base 2 log
pub const MIN_BLOCK_SIZE_LOG2: u8 = 10;

/// The largest V2 block size we will accept, as a base 2 log
///
/// This caps how much memory a hostile file can make us allocate for a single block.
pub const MAX_BLOCK_SIZE_LOG2: u8 = 26;

/// The version of the `CaRT` format a file uses
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, strum::Display, strum::EnumString)]
#[strum(ascii_case_insensitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum CartVersion {
    /// The official `CaRT` format: RC4 + DEFLATE
    ///
    /// This is the default because it is the only format any other `CaRT` tool can read, and
    /// because every object Thorium has ever written is V1. Switching the write format is an
    /// explicit opt in.
    #[default]
    V1,
    /// The Thorium specific `CaRT` format: AES-128-GCM + Zstd
    V2,
}

impl CartVersion {
    /// Get the value this version is stored as in the `CaRT` header
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        match self {
            CartVersion::V1 => 1,
            CartVersion::V2 => 2,
        }
    }

    /// Build a version from the value stored in a `CaRT` header
    ///
    /// # Arguments
    ///
    /// * `raw` - The raw version number from the header
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedVersion`] for any version cart-rs does not know about.
    pub const fn from_u16(raw: u16) -> Result<Self, Error> {
        match raw {
            1 => Ok(CartVersion::V1),
            2 => Ok(CartVersion::V2),
            other => Err(Error::UnsupportedVersion(other)),
        }
    }

    /// Whether support for this version was compiled into this build of cart-rs
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        match self {
            CartVersion::V1 => cfg!(feature = "v1"),
            CartVersion::V2 => cfg!(feature = "v2"),
        }
    }
}

/// The options that only apply to a V1 (`RC4` + DEFLATE) `CaRT`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V1Options {
    /// The DEFLATE compression level to use, from 0 (store) to 9 (best)
    pub deflate_level: u8,
}

impl Default for V1Options {
    /// Build the default set of V1 options
    fn default() -> Self {
        V1Options {
            deflate_level: DEFAULT_DEFLATE_LEVEL,
        }
    }
}

impl V1Options {
    /// Check that these options are in range
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidOption`] if the DEFLATE level is above 9.
    pub(crate) fn validate(&self) -> Result<(), Error> {
        // DEFLATE levels only go up to 9
        if self.deflate_level > 9 {
            return Err(Error::InvalidOption(format!(
                "DEFLATE level must be between 0 and 9 but {} was given",
                self.deflate_level
            )));
        }
        Ok(())
    }
}

/// The options that only apply to a V2 (`AES-128-GCM` + Zstd) `CaRT`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V2Options {
    /// The Zstd compression level to use
    pub zstd_level: i32,
    /// The base 2 log of the plaintext block size
    pub block_size_log2: u8,
}

impl Default for V2Options {
    /// Build the default set of V2 options
    fn default() -> Self {
        V2Options {
            zstd_level: DEFAULT_ZSTD_LEVEL,
            block_size_log2: DEFAULT_BLOCK_SIZE_LOG2,
        }
    }
}

impl V2Options {
    /// Get the plaintext block size these options describe
    #[must_use]
    pub const fn block_size(&self) -> usize {
        1_usize << self.block_size_log2
    }

    /// Check that these options are in range
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidOption`] if the block size is outside
    /// [`MIN_BLOCK_SIZE_LOG2`]..=[`MAX_BLOCK_SIZE_LOG2`] or the Zstd level is out of range.
    #[cfg(feature = "v2")]
    pub(crate) fn validate(&self) -> Result<(), Error> {
        // reject block sizes we are not willing to allocate for
        if self.block_size_log2 < MIN_BLOCK_SIZE_LOG2 || self.block_size_log2 > MAX_BLOCK_SIZE_LOG2
        {
            return Err(Error::InvalidOption(format!(
                "block_size_log2 must be between {MIN_BLOCK_SIZE_LOG2} and \
                 {MAX_BLOCK_SIZE_LOG2} but {} was given",
                self.block_size_log2
            )));
        }
        // reject Zstd levels outside the range Zstd itself accepts
        if !(-131_072..=22).contains(&self.zstd_level) {
            return Err(Error::InvalidOption(format!(
                "zstd_level must be between -131072 and 22 but {} was given",
                self.zstd_level
            )));
        }
        Ok(())
    }
}

/// Every option cart-rs knows about, split by the version it applies to
///
/// This is what the runtime version builder works with: you hand it a whole [`V1Options`] or a
/// whole [`V2Options`] and only the one matching the chosen version is ever consulted. There
/// are deliberately no per option setters on the runtime builder, because if you do not know
/// the version then you do not know which option applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CartOptions {
    /// The options to use if this turns out to be a V1 cart
    pub v1: V1Options,
    /// The options to use if this turns out to be a V2 cart
    pub v2: V2Options,
}

/// Keeps the builder typestate markers from being implemented outside this crate
mod sealed {
    /// A trait that cannot be named or implemented downstream
    pub trait Sealed {}
}

/// A type level marker for a builder that has not been given a version yet
///
/// A builder in this state has no `build` method, which is what stops a caller from getting a
/// silently defaulted version.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoVersion;

/// A type level marker for a V1 (`RC4` + DEFLATE) builder
#[derive(Debug, Clone, Copy, Default)]
pub struct V1;

/// A type level marker for a V2 (`AES-128-GCM` + Zstd) builder
#[derive(Debug, Clone, Copy, Default)]
pub struct V2;

/// A type level marker for a builder whose version is only known at runtime
#[derive(Debug, Clone, Copy, Default)]
pub struct Dynamic;

impl sealed::Sealed for NoVersion {}
impl sealed::Sealed for V1 {}
impl sealed::Sealed for V2 {}
impl sealed::Sealed for Dynamic {}

/// The state a [`crate::CartStreamBuilder`] or [`crate::CartManualBuilder`] is in
///
/// This is sealed; the only states are [`NoVersion`], [`V1`], [`V2`], and [`Dynamic`].
pub trait BuilderState: sealed::Sealed {}

impl BuilderState for NoVersion {}
impl BuilderState for V1 {}
impl BuilderState for V2 {}
impl BuilderState for Dynamic {}

/// A builder state that has settled on a single `CaRT` version at compile time
///
/// This ties a marker type to its runtime version and to the option set that applies to it,
/// which is the one table the rest of the crate dispatches from.
pub trait Version: BuilderState {
    /// The runtime version this marker stands for
    const VERSION: CartVersion;
    /// The options that apply to this version
    type Options: Clone + Copy + Default + std::fmt::Debug;
}

impl Version for V1 {
    const VERSION: CartVersion = CartVersion::V1;
    type Options = V1Options;
}

impl Version for V2 {
    const VERSION: CartVersion = CartVersion::V2;
    type Options = V2Options;
}

impl FromStr for V1Options {
    type Err = Error;

    /// Parse a bare DEFLATE level into a set of V1 options
    ///
    /// # Arguments
    ///
    /// * `raw` - The DEFLATE level to parse
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        // parse the level and make sure it is in range
        let deflate_level = raw
            .parse::<u8>()
            .map_err(|err| Error::InvalidOption(format!("invalid DEFLATE level: {err}")))?;
        let options = V1Options { deflate_level };
        options.validate()?;
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "v2")]
    use super::V2Options;
    use super::{CartVersion, V1Options};
    use std::str::FromStr;

    #[test]
    fn version_round_trips_through_its_header_value() {
        for version in [CartVersion::V1, CartVersion::V2] {
            assert_eq!(CartVersion::from_u16(version.as_u16()).unwrap(), version);
        }
    }

    #[test]
    fn unknown_versions_are_rejected() {
        for raw in [0_u16, 3, 255, 0xFFFF] {
            assert!(CartVersion::from_u16(raw).is_err());
        }
    }

    /// A hand written `FromStr` over this enum previously shipped `"V2" => V1`, so pin the
    /// mapping rather than trusting the derive
    #[test]
    fn from_str_maps_each_name_to_its_own_variant() {
        assert_eq!(CartVersion::from_str("V1").unwrap(), CartVersion::V1);
        assert_eq!(CartVersion::from_str("v1").unwrap(), CartVersion::V1);
        assert_eq!(CartVersion::from_str("V2").unwrap(), CartVersion::V2);
        assert_eq!(CartVersion::from_str("v2").unwrap(), CartVersion::V2);
        assert!(CartVersion::from_str("V3").is_err());
    }

    #[test]
    fn display_round_trips_through_from_str() {
        for version in [CartVersion::V1, CartVersion::V2] {
            assert_eq!(
                CartVersion::from_str(&version.to_string()).unwrap(),
                version
            );
        }
    }

    /// The default write version must stay V1 so that an upgrade never silently switches a
    /// deployment onto a format older readers cannot handle
    #[test]
    fn the_default_version_is_v1() {
        assert_eq!(CartVersion::default(), CartVersion::V1);
    }

    #[test]
    fn out_of_range_v1_options_are_rejected() {
        assert!(V1Options { deflate_level: 9 }.validate().is_ok());
        assert!(V1Options { deflate_level: 10 }.validate().is_err());
    }

    #[cfg(feature = "v2")]
    #[test]
    fn out_of_range_v2_options_are_rejected() {
        assert!(V2Options::default().validate().is_ok());
        assert!(
            V2Options {
                block_size_log2: 9,
                ..V2Options::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            V2Options {
                block_size_log2: 27,
                ..V2Options::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            V2Options {
                zstd_level: 23,
                ..V2Options::default()
            }
            .validate()
            .is_err()
        );
    }
}

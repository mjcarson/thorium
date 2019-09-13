/// Handles version operations for the Thorium api
use tracing::instrument;

use crate::models::Version;
use crate::utils::ApiError;

impl Version {
    /// Get the current version info
    #[instrument(name = "Version::new", skip_all, err(Debug))]
    pub fn new() -> Result<Self, ApiError> {
        // get the current Thorium version
        let thorium = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;
        // build our version struct
        let version = Version { thorium };
        Ok(version)
    }
}

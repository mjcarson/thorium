use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::bytes::Regex;

use crate::bad;
use crate::models::{HostPath, SystemSettings, User, Volume, VolumeTypes};
use crate::utils::ApiError;

/// Attempts to unwrap opt or returns an error if it's is none
macro_rules! unwrap_opt {
    ($field:expr, $name:expr, $val:expr) => {
        match $field.as_ref() {
            Some(val) => val,
            None => {
                return bad!(format!(
                    "{} volumes require {} options to be set",
                    $name, $val
                ));
            }
        }
    };
}

/// A [`regex::bytes::Regex`] matching any relative traversal in a path (one or more ".'s")
static RELATIVE_DIR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\A\.+\z").unwrap());

impl HostPath {
    /// Returns true if the given path is a valid host path
    ///
    /// # Arguments
    ///
    /// * `path` - The path to validate
    pub fn is_valid<T: AsRef<Path>>(path: T) -> bool {
        let path: &Path = path.as_ref();
        // the host path is invalid if it's relative or has any relative-like components (".", "..", "...", etc.)
        if path.is_relative()
            || path
                .components()
                .any(|comp| RELATIVE_DIR_REGEX.is_match(comp.as_os_str().as_bytes()))
        {
            return false;
        }
        true
    }
}

impl Volume {
    /// validate a volume contains the information needed
    ///
    /// This does not ensure that it wont fail later on (for instance if a NFS server is wrong).
    ///
    /// # Arguments
    ///
    /// * `user` - The user validating this volume
    /// * `settings` - The Thorium [`SystemSettings`]
    pub fn validate(&self, user: &User, settings: &SystemSettings) -> Result<(), ApiError> {
        // in order to prevent Thorium-created volumes from being leaked ban any volumes that start with "thorium"
        if self.name.starts_with("thorium") {
            return bad!("Volume names cannot start with 'thorium'".to_owned());
        }
        // validate specific options
        match self.archetype {
            VolumeTypes::HostPath => {
                // make sure the host path field is set
                let host_path = unwrap_opt!(self.host_path, "HostPath", "host_path");
                // check if unrestricted host paths is set or the host path is on the whitelist
                if !settings.allow_unrestricted_host_paths
                    && !settings.is_whitelisted_host_path(&host_path.path)
                {
                    // determine what the error message should advise depending on if the user is an admin
                    let admin_error_msg = if user.is_admin() {
                        "Add it to the host path whitelist"
                    } else {
                        "Ask an admin to add it"
                    };
                    return bad!(format!(
                        "The host path '{}' in volume '{}' is not in the list of allowed host paths. \
                        {} or choose a different path.",
                        host_path.path,
                        self.name,
                        admin_error_msg
                    ));
                }
                // make sure the host path is valid
                if !HostPath::is_valid(&host_path.path) {
                    return bad!(format!(
                        "The host path '{}' in volume '{}' is invalid! Host paths must be absolute \
                        and must not contain relative traversal ('.', '..', etc.)",
                        host_path.path,
                        self.name
                    ));
                }
                Ok(())
            }
            VolumeTypes::ConfigMap => Ok(()),
            VolumeTypes::Secret => Ok(()),
            VolumeTypes::NFS => {
                // make sure nfs exists
                let _ = unwrap_opt!(self.nfs, "NFS", "nfs");
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_host_path_validate() {
        let mut path = PathBuf::from("relative/path");
        assert!(!HostPath::is_valid(path));
        path = PathBuf::from("..");
        assert!(!HostPath::is_valid(path));
        path = PathBuf::from(".");
        assert!(!HostPath::is_valid(path));
        path = PathBuf::from("");
        assert!(!HostPath::is_valid(path));
        path = PathBuf::from("/absolute/../but/traverses/upward");
        assert!(!HostPath::is_valid(path));
        path = PathBuf::from("/absolute/......../arbitrary/number/dots");
        assert!(!HostPath::is_valid(path));
        path = PathBuf::from("/😎/../");
        assert!(!HostPath::is_valid(path));
        path = PathBuf::from("/😎/valid/");
        assert!(HostPath::is_valid(path));
        path = PathBuf::from("/.valid/path");
        assert!(HostPath::is_valid(path));
        path = PathBuf::from("/...........valid/path");
        assert!(HostPath::is_valid(path));
    }
}

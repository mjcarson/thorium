use crate::{models::ImageScaler, utils::Shared};

/// Keys to use to access system info
pub struct SystemKeys {
    /// The key to store/retrieve system info with
    pub data: String,
    /// The key to store system keys at
    #[allow(dead_code)]
    pub keys: String,
    /// The key to store system settings at
    pub settings: String,
}

impl SystemKeys {
    /// Builds the keys to access system info in redis
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    pub fn new(shared: &Shared) -> Self {
        // build key to store system info at
        let data = Self::data(shared);
        // build key to store system keys at
        let keys = Self::keys(shared);
        // build key to store system settings at
        let settings = Self::settings(shared);
        // build key object
        SystemKeys {
            data,
            keys,
            settings,
        }
    }

    /// Builds key to system info
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    pub fn data(shared: &Shared) -> String {
        format!("{ns}:system_info", ns = shared.config.thorium.namespace,)
    }

    /// Builds key to system keys
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    pub fn keys(shared: &Shared) -> String {
        format!("{ns}:system_keys", ns = shared.config.thorium.namespace,)
    }

    /// Builds key to system settings
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    pub fn settings(shared: &Shared) -> String {
        format!("{ns}:system_settings", ns = shared.config.thorium.namespace,)
    }
}

/// Build the keys to the set of workers for a specific cluster/node/scaler
///
/// # Arguments
///
/// * `cluster` - The cluster these workers are in
/// * `node` - The node these workers are on
/// * `scaler` - The scaler these workers were spawned under
/// * `shared` - Shared Thorium objects
pub fn worker_set(cluster: &str, node: &str, scaler: ImageScaler, shared: &Shared) -> String {
    format!(
        "{ns}:workers:{cluster}:{node}:{scaler}",
        ns = shared.config.thorium.namespace
    )
}

/// Build the keys to the set of workers for a specific cluster/node/scaler
///
/// # Arguments
///
/// * `name` - The name of this worker
/// * `shared` - Shared Thorium objects
pub fn worker_data(name: &str, shared: &Shared) -> String {
    format!(
        "{ns}:worker_data:{name}",
        ns = shared.config.thorium.namespace
    )
}

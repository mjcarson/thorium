use crate::models::ImageScaler;
use crate::utils::Shared;

/// The keys to store/retrieve job data/queues
pub struct StreamKeys;

impl StreamKeys {
    /// Builds the keys to access streams
    ///
    /// # Arguments
    ///
    /// * `group` - The group this stream is in
    /// * `namespace` - The namespace for this stream within this group
    /// * `stream` - The name of this stream
    /// * `shared` - shared Thorium objects
    pub fn stream(group: &str, namespace: &str, stream: &str, shared: &Shared) -> String {
        // base key to build the queue key off of
        format!(
            "{ns}:streams:{group}:{namespace}:{stream}",
            ns = shared.config.thorium.namespace,
            group = group,
            stream = stream,
        )
    }

    /// Builds the keys to access system streams in the global namespace
    ///
    /// # Arguments
    ///
    /// * `stream` - The name of this stream
    /// * `shared` - shared Thorium objects
    pub fn system_global(stream: &str, shared: &Shared) -> String {
        // base key to build the queue key off of
        format!(
            "{ns}:streams:system:global:{stream}",
            ns = shared.config.thorium.namespace,
            stream = stream,
        )
    }

    /// Builds the keys to access system streams that are namespaced by scaler
    ///
    /// # Arguments
    ///
    /// * `stream` - The name of this stream
    /// * `shared` - shared Thorium objects
    pub fn system_scaler(scaler: ImageScaler, stream: &str, shared: &Shared) -> String {
        // base key to build the queue key off of
        format!(
            "{ns}:streams:system:{scaler}:{stream}",
            ns = shared.config.thorium.namespace,
            scaler = scaler,
            stream = stream,
        )
    }
}

//! The keys related to events in Redis
use crate::{models::EventType, utils::Shared};

/// The keys related to events in Redis
#[derive(Debug)]
pub struct EventKeys {
    /// The key to our cache status
    #[allow(dead_code)]
    pub cache: String,
}

impl EventKeys {
    /// Build the keys for events
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    #[allow(dead_code)]
    pub fn new(shared: &Shared) -> Self {
        // build the key to our events cache
        let cache = Self::cache(shared);
        // build our keys
        EventKeys { cache }
    }

    /// Build the key for the event cache map
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    pub fn cache(shared: &Shared) -> String {
        format!(
            "{ns}:event-handler:cache",
            ns = shared.config.thorium.namespace
        )
    }

    /// The queue for a specific type of events
    ///
    /// # Arguments
    ///
    /// * `kind` - The kind of events to start or retrieve
    /// * `shared` - Shared Thorium objects
    pub fn queue(kind: EventType, shared: &Shared) -> String {
        format!(
            "{ns}:event-handler:queue:{kind}",
            ns = shared.config.thorium.namespace,
            kind = kind,
        )
    }

    /// The map of all events that are current in flight
    ///
    /// # Arguments
    ///
    /// * `kind` - The kind of events in this map
    /// * `shared` - Shared Thorium objects
    pub fn in_flight_map(kind: EventType, shared: &Shared) -> String {
        format!(
            "{ns}:event-handler:in_flight_map:{kind}",
            ns = shared.config.thorium.namespace,
            kind = kind,
        )
    }

    /// The queue of all events in the in flight queue
    ///
    /// # Arguments
    ///
    /// * `kind` - The kind of events in this queue
    /// * `shared` - Shared Thorium objects
    pub fn in_flight_queue(kind: EventType, shared: &Shared) -> String {
        format!(
            "{ns}:event-handler:in_flight_queue:{kind}",
            ns = shared.config.thorium.namespace,
            kind = kind,
        )
    }
}

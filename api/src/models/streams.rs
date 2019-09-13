//! Wrappers for interacting with streams within Thorium with different backends
//! Currently only Redis is supported

use chrono::prelude::*;

/// A single point in a Stream sorted by time
#[derive(Serialize, Deserialize, Debug)]
pub struct StreamObj {
    /// The timestamp for where this object exists in the stream
    pub timestamp: i64,
    /// The data within this object
    pub data: String,
}

impl StreamObj {
    /// Creates a new stream object at a specific epoch in milliseconds
    ///
    /// # arguments
    ///
    /// * `timestamp` - Seconds after 1970-01-01T00:00:00Z
    /// * `data` - The data for this stream object
    pub fn new(timestamp: i64, data: String) -> Self {
        StreamObj { timestamp, data }
    }
}

#[derive(Deserialize, Serialize, Debug)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
/// The total number of objects between two points in a stream
pub struct StreamDepth {
    /// The earliest date to count objects from in the stream
    pub start: chrono::DateTime<Utc>,
    /// The latest date to count objects from in the stream
    pub end: chrono::DateTime<Utc>,
    /// The number of objects between the above two points
    pub depth: i64,
}

/// A stream containing objects sorted by timestamps
pub struct Stream;

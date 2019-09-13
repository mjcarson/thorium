//! The messages between the search streamer controller and worker

use chrono::prelude::*;
use thorium::models::ExportError;
use uuid::Uuid;

/// The messages between the search streamer controller and worker
pub enum Msg {
    /// A new chunk of time to try to stream
    New {
        watermark: u64,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    /// A previously errored section of time to retry
    Retry(ExportError),
}

/// The response messages from workers
pub enum Response {
    /// A new chunk reached a completed state (errors included)
    Completed {
        watermark: u64,
        start: DateTime<Utc>,
    },
    /// A previously failed section was completed
    Fixed(Uuid),
}

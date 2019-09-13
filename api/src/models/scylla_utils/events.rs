//! The scylla utils for events
use chrono::prelude::*;
use scylla::DeserializeRow;
use uuid::Uuid;

use crate::models::EventData;

/// An event in Thorium
#[derive(Serialize, Deserialize, Debug, Clone, DeserializeRow)]
#[scylla(flavor = "enforce_order", skip_name_checks)]
pub struct EventRow {
    /// The id for this event
    pub id: Uuid,
    /// The timestamp for this event
    pub timestamp: DateTime<Utc>,
    /// The parent for this event
    pub parent: Option<Uuid>,
    /// The user that caused this trigger
    pub user: String,
    /// This events current trigger depth,
    pub trigger_depth: i16,
    /// this events data
    pub data: EventData,
}

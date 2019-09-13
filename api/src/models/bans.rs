//! Models related to bans
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A ban for an entity in Thorium that at least has a unique ID, a message,
/// and a time that the ban was made; used primarily to convert a ban into
/// a notification message
pub trait Ban<B> {
    /// Returns this ban's unique ID
    fn id(&self) -> &Uuid;

    /// Returns an informative notification message for the user based on
    /// the kind of ban this is
    fn msg(&self) -> String;

    /// Returns the time the entity was banned
    fn time_banned(&self) -> &DateTime<Utc>;
}

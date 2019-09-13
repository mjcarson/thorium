//! The scylla utils for tags
use chrono::prelude::*;
use scylla::DeserializeRow;

/// An internal struct containing one instance or row of a tag in scylla
#[derive(Serialize, Deserialize, Debug, Clone, DeserializeRow)]
#[scylla(flavor = "enforce_order", skip_name_checks)]
pub struct TagRow {
    /// The group this tag is a part of
    pub group: String,
    /// The item this tag is for
    pub item: String,
    /// The key for this tag
    pub key: String,
    /// The value for this tag
    pub value: String,
}

/// An internal struct containing one instance or row of a Tag in scylla
#[derive(Serialize, Deserialize, Debug, Clone, DeserializeRow)]
#[scylla(flavor = "enforce_order", skip_name_checks)]
pub struct TagListRow {
    /// The group this tag is a part of
    pub group: String,
    /// The item this tag is for
    pub item: String,
    /// When this tag was added
    pub uploaded: DateTime<Utc>,
}

/// An internal struct containing one instance or row of a tag in scylla
#[derive(Serialize, Deserialize, Debug, Clone, DeserializeRow)]
#[scylla(flavor = "enforce_order", skip_name_checks)]
pub struct FullTagRow {
    /// The group this tag is a part of
    pub group: String,
    /// The year this tag was submitted
    pub year: i32,
    /// The bucket this tag was submitted in
    pub bucket: i32,
    /// The key for this tag
    pub key: String,
    /// The value for this tag
    pub value: String,
    /// The timestamp this tag was submitted
    pub uploaded: DateTime<Utc>,
    /// The item we are getting tags for
    pub item: String,
}

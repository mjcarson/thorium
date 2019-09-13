//! The different sources of data to stream into a search store

use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use thorium::models::Cursor;
use thorium::{Conf, Error, Thorium};

mod results;

pub use results::SamplesOutput;

#[async_trait::async_trait]
pub trait DataSource: Sync + Send + 'static {
    /// The data this cursor will be pulling
    type DataType: Serialize + for<'a> Deserialize<'a> + Send;

    /// Get the timestamp for this datatype
    ///
    /// # Arguments
    ///
    /// * `data` - The data to get the timestamp from
    fn timestamp(data: &Self::DataType) -> DateTime<Utc>;

    /// Get the name of the index to write these documents too
    fn index(conf: &Conf) -> &str;

    /// Get the earliest data might exist at
    fn earliest(conf: &Conf) -> DateTime<Utc>;

    /// Pull data from a section of time to stream to our search store
    async fn build_cursor(
        thorium: &Thorium,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Cursor<Self::DataType>, Error>;

    /// Cast this data to a serialize Json Value
    fn to_value(
        data: &Self::DataType,
        values: &mut Vec<serde_json::Value>,
        now: DateTime<Utc>,
    ) -> Result<(), Error>;
}

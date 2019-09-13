//! The traits used for taking a census of data in Thorium

use scylla::deserialize::DeserializeRow;
use scylla::prepared_statement::PreparedStatement;
use scylla::transport::errors::QueryError;
use scylla::Session;
use std::fmt::Debug;

/// The trait used for taking a census of data
pub trait Census: 'static + Send {
    /// The type returned by our prepared statement
    type Row: 'static + Send + Debug + for<'frame, 'metadata> DeserializeRow<'frame, 'metadata>;

    /// Build the prepared statement for getting partition count info
    #[allow(async_fn_in_trait)]
    async fn scan_prepared_statement(
        scylla: &Session,
        ns: &str,
    ) -> Result<PreparedStatement, QueryError>;

    /// Get the count for this partition
    fn get_count(row: &Self::Row) -> i64;

    /// Get the bucket for this partition
    fn get_bucket(row: &Self::Row) -> i32;

    /// Build the count key for this partition from a census scan row
    fn count_key_from_row(namespace: &str, row: &Self::Row, grouping: i32) -> String;

    /// Build the sorted set key for this census operation from a census scan row
    fn stream_key_from_row(namespace: &str, row: &Self::Row) -> String;
}

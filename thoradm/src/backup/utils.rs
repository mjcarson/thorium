//! Some utilities to help thoradm do what it needs to do

use scylla::transport::errors::QueryError;
use scylla::{QueryResult, Session};

/// Drop a materialized view in scylla
///
/// # Arguments
///
/// * `name` - The name of the materialized view to delete
/// * `cluster` - The cluster we are deleting a materialized view from
/// * `scylla` - The scylla session to use
pub async fn drop_materialized_view(
    ns: &str,
    name: &str,
    scylla: &Session,
) -> Result<QueryResult, QueryError> {
    // Drop the target table
    let table_drop = format!(
        "drop materialized view if exists {ns}.{name}",
        ns = ns,
        name = name
    );
    // execute our query
    scylla.query_unpaged(table_drop, &[]).await
}

/// The shared functions across all traits
pub trait Utils {
    /// The name of the table we are operating on
    fn name() -> &'static str;
}

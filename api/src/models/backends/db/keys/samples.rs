//! The keys used for sample data in redis

use crate::models::CensusKeys;
use crate::utils::Shared;

/// Build the sorted set key for this census operation
///
/// # Arguments
///
/// * `group` - The group to look for census info for
/// * `year` - The year this sample is in
/// * `shared` - Shared Thorium objects
pub fn census_stream<T: std::fmt::Display>(group: &T, year: i32, shared: &Shared) -> String {
    format!(
        "{namespace}:census:samples:stream:{group}:{year}",
        namespace = shared.config.thorium.namespace,
        group = group,
        year = year,
    )
}

/// Build the keys for this items cursor/census caches
///
/// # Arguments
///
/// * `groups` - The groups these samples submissions are in
/// * `year` - The year this census info is for
/// * `bucket` - This objects bucket
/// * `shared` - Shared Thorium objects
pub fn census_keys(
    groups: &Vec<String>,
    year: i32,
    bucket: i32,
    shared: &Shared,
) -> Vec<CensusKeys> {
    // have a list of keys
    let mut keys = Vec::with_capacity(groups.len());
    // for each group build our key
    for group in groups {
        // build the stream key for this row
        let stream = census_stream(group, year, shared);
        // build our census key object
        let key = CensusKeys { stream, bucket };
        // add our key
        keys.push(key);
    }
    keys
}

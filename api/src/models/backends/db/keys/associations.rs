//! The keys for associations in Redis

use crate::models::CensusKeys;
use crate::utils::Shared;

/// Build the sorted set key for this census operation
///
/// # Arguments
///
/// * `group` - The group to look for census info for
/// * `year` - The year this sample is in
/// * `source` - The source/target entity/object to build a census key for
/// * `shared` - Shared Thorium objects
pub fn census_stream<T: std::fmt::Display>(
    group: &T,
    year: i32,
    source: &str,
    shared: &Shared,
) -> String {
    format!(
        "{namespace}:census:associations:stream:{group}:{source}:{year}",
        namespace = shared.config.thorium.namespace,
        group = group,
        year = year,
        source = source,
    )
}

/// Build the keys for the assocations_from cursor/census caches
///
/// # Arguments
///
/// * `keys` - The list of keys to add our keys too
/// * `groups` - The groups these samples submissions are in
/// * `year` - The year this census info is for
/// * `bucket` - This objects bucket
/// * `source` - The source entity/object to build a census key for
/// * `target` - The target entity/object ot build a census key for
/// * `shared` - Shared Thorium objects
pub fn census_keys(
    keys: &mut Vec<CensusKeys>,
    groups: &Vec<String>,
    year: i32,
    bucket: i32,
    source: &str,
    target: &str,
    shared: &Shared,
) {
    // for each group build our key
    for group in groups {
        // build the stream key for this rows source
        let stream = census_stream(group, year, source, shared);
        // build our census key object
        let key = CensusKeys { stream, bucket };
        // add our key
        keys.push(key);
        // build the stream key for this rows target
        let stream = census_stream(group, year, target, shared);
        // build our census key object
        let key = CensusKeys { stream, bucket };
        // add our key
        keys.push(key);
    }
}

/// Build the keys for the assocations_from cursor/census caches
///
/// # Arguments
///
/// * `keys` - The list of keys to add our keys too
/// * `groups` - The groups these samples submissions are in
/// * `year` - The year this census info is for
/// * `bucket` - This objects bucket
/// * `source` - The source entity/object to build a census key for
/// * `target` - The target entity/object ot build a census key for
/// * `shared` - Shared Thorium objects
pub fn census_keys_ref(
    keys: &mut Vec<CensusKeys>,
    groups: &Vec<&String>,
    year: i32,
    bucket: i32,
    source: &str,
    target: &str,
    shared: &Shared,
) {
    // for each group build our key
    for group in groups {
        // build the stream key for this row
        let stream = census_stream(*group, year, source, shared);
        // build our census key object
        let key = CensusKeys { stream, bucket };
        // add our key
        keys.push(key);
        // build the stream key for this rows target
        let stream = census_stream(group, year, target, shared);
        // build our census key object
        let key = CensusKeys { stream, bucket };
        // add our key
        keys.push(key);
    }
}

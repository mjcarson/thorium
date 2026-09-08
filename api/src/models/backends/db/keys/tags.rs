//! The keys used for tag data in redis

use crate::models::TagType;
use crate::utils::Shared;

/// Build the sorted set key for this census operation
///
/// # Arguments
///
/// * `kind` - The kind of tag we are getting/setting census info for
/// * `group` - The group to look for census info for
/// * `key` - The tag key to use
/// * `value` - The tag value to use
/// * `year` - The year this tag is in
/// * `shared` - Shared Thorium objects
pub fn census_stream<T: std::fmt::Display>(
    kind: TagType,
    group: &T,
    key: &str,
    value: &str,
    year: i32,
    shared: &Shared,
) -> String {
    format!(
        "{namespace}:census:tags:stream:{kind}:{group}:{key}:{value}:{year}",
        namespace = shared.config.thorium.namespace,
        group = group,
        kind = kind,
        key = key,
        value = value,
        year = year,
    )
}

/// Build the sorted set key for this census operation
///
/// # Arguments
///
/// * `kind` - The kind of tag we are getting/setting census info for
/// * `group` - The group to look for census info for
/// * `key` - The tag key to use
/// * `value` - The tag value to use
/// * `year` - The year this tag is in
/// * `shared` - Shared Thorium objects
pub fn census_stream_case_insensitive<T: std::fmt::Display>(
    kind: TagType,
    group: &T,
    key: &str,
    value: &str,
    year: i32,
    shared: &Shared,
) -> String {
    // lowercase our key and value
    let lower_key = key.to_lowercase();
    let lower_value = value.to_lowercase();
    format!(
        "{namespace}:census:tags_case_insensitive:stream:{kind}:{group}:{lower_key}:{lower_value}:{year}",
        namespace = shared.config.thorium.namespace,
        group = group,
        kind = kind,
        lower_key = lower_key,
        lower_value = lower_value,
        year = year,
    )
}

//! Handles tag operations in Thorium
//!
//! This does not handle create operations as they are very specific to the type
//! of data they are tied too. This is largely becuause how they determine the
//! timestamp each tag should be uploaded at.

use chrono::prelude::*;
use std::collections::HashMap;
use tracing::{event, instrument, Level};

use super::keys::tags;
use crate::models::backends::TagSupport;
use crate::models::{
    Event, FullTagRow, TagDeleteRequest, TagMap, TagRequest, TagRow, TagType, User,
};
use crate::utils::{helpers, ApiError, Shared};
use crate::{bad, conn, log_scylla_err};

/// Save new tags into scylla
///
/// # Arguments
///
/// * `user` - The user that is creating tags
/// * `key` - The key to the item we are tagging
/// * `req` - The request containing the tags to create and groups to save them in
/// * `earliest` - The earliest each group has seen this item
/// * `shared` - Shared Thorium objects
#[rustfmt::skip]
#[instrument(
    name = "db::tags::create",
    skip(user, req, earliest, shared),
    fields(kind = T::tag_kind().as_str()),
    err(Debug)
)]
pub async fn create<T: TagSupport>(
    user: &User,
    key: String,
    req: TagRequest<T>,
    earliest: &HashMap<&String, DateTime<Utc>>,
    shared: &Shared,
) -> Result<(), ApiError> {
    // get the type of tag we are creating
    let kind = T::tag_kind();
    // get the chunk size for Thorium tags
    let chunk = shared.config.thorium.tags.partition_size;
    // build a redis pipe to update our tag counts
    let mut pipe = redis::pipe();
    // crawl over the groups we are submitting tags for
    for group in &req.groups {
        // skip any groups we can't get earliest info on
        match earliest.get(group) {
            Some(timestamp) => {
                let year = timestamp.year();
                let bucket = helpers::partition(*timestamp, year, chunk);
                // crawl over all tags and save them
                for (tag_key, tag_values) in &req.tags {
                    // save each tag values for this key
                    for tag_value in tag_values {
                        // save this tag into scylla
                        shared
                            .scylla
                            .session
                            .execute_unpaged(
                                &shared.scylla.prep.tags.insert,
                                (
                                    kind, group, &key, year, bucket, tag_key, tag_value, *timestamp,
                                ),
                            )
                            .await?;
                        // build the keys for this tags census info
                        let count_key = tags::census_count(
                            T::tag_kind(),
                            group,
                            tag_key,
                            tag_value,
                            year,
                            bucket,
                            shared,
                        );
                        let stream_key = tags::census_stream(
                            T::tag_kind(),
                            group,
                            tag_key,
                            tag_value,
                            year,
                            shared,
                        );
                        // add data into redis
                        pipe.cmd("hincrby").arg(count_key).arg(bucket).arg(1)
                            .cmd("zadd").arg(stream_key).arg(bucket).arg(bucket);
                    }
                }
            }
            None => {
                // throw an error because we failed to get earliest info for this group
                return bad!(format!("Failed to get earliest info for {}", group));
            }
        }
    }
    // execute our redis pipeline
    let _:() = pipe.query_async(conn!(shared)).await?;
    // create our tag event
    let event = Event::new_tag(user, key, req);
    // save our event
    super::events::create(&event, shared).await?;
    Ok(())
}

/// Save new tags into scylla where both values in earliest are owned
///
/// # Arguments
///
/// * `user` - The user that is creating tags
/// * `key` - The key to the item we are tagging
/// * `req` - The request containing the tags to create and groups to save them in
/// * `earliest` - The earliest each group has seen this item
/// * `shared` - Shared Thorium objects
#[instrument(
    name = "db::tags::create_owned",
    skip(user, req, earliest, shared),
    fields(kind = T::tag_kind().as_str()),
    err(Debug)
)]
pub async fn create_owned<T: TagSupport>(
    user: &User,
    key: String,
    req: TagRequest<T>,
    earliest: &HashMap<String, DateTime<Utc>>,
    shared: &Shared,
) -> Result<(), ApiError> {
    // get the type of tag we are creating
    let kind = T::tag_kind();
    // get the chunk size for Thorium tags
    let chunk = shared.config.thorium.tags.partition_size;
    // build a redis pipe to update our tag counts
    let mut pipe = redis::pipe();
    // crawl over the groups we are submitting tags for
    for group in &req.groups {
        // skip any groups we can't get earliest info on
        match earliest.get(group) {
            Some(timestamp) => {
                let year = timestamp.year();
                let bucket = helpers::partition(*timestamp, year, chunk);
                // crawl over all tags and save them
                for (tag_key, tag_values) in &req.tags {
                    // save each tag values for this key
                    for tag_value in tag_values {
                        // save this tag into scylla
                        shared
                            .scylla
                            .session
                            .execute_unpaged(
                                &shared.scylla.prep.tags.insert,
                                (
                                    kind, group, &key, year, bucket, tag_key, tag_value, *timestamp,
                                ),
                            )
                            .await?;
                        // build the keys for this tags census info
                        let count_key = tags::census_count(
                            T::tag_kind(),
                            group,
                            tag_key,
                            tag_value,
                            year,
                            bucket,
                            shared,
                        );
                        let stream_key = tags::census_stream(
                            T::tag_kind(),
                            group,
                            tag_key,
                            tag_value,
                            year,
                            shared,
                        );
                        // add data into redis
                        pipe.cmd("hincrby")
                            .arg(count_key)
                            .arg(bucket)
                            .arg(1)
                            .cmd("zadd")
                            .arg(stream_key)
                            .arg(bucket)
                            .arg(bucket);
                    }
                }
            }
            None => {
                // throw an error because we failed to get earliest info for this group
                return bad!(format!("Failed to get earliest info for {}", group));
            }
        }
    }
    // execute our redis pipeline
    let _: () = pipe.query_async(conn!(shared)).await?;
    // create our tag event
    let event = Event::new_tag(user, key, req);
    // save our event
    super::events::create(&event, shared).await?;
    Ok(())
}

/// A map of repo tags and the info needed to delete them
pub type TagDeleteMap = HashMap<String, TagValueMap>;
pub type TagValueMap = HashMap<String, TagGroupMap>;
pub type TagGroupMap = HashMap<String, Vec<(i32, i32, DateTime<Utc>)>>;

/// Get the full tag rows for some specific tags
///
/// # Arguments
///
/// * `tag_type` - The type of tags to get rows for
/// * `groups` - The group to get tag rows for
/// * `item` - The item we are getting tags for
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::tags::get_tag_rows", skip(shared), err(Debug))]
async fn get_tag_rows(
    tag_type: TagType,
    groups: &Vec<String>,
    item: &str,
    shared: &Shared,
) -> Result<TagDeleteMap, ApiError> {
    // default to 30 tags
    let mut map = HashMap::with_capacity(30);
    // if we have more then 100 groups then chunk it into bathes of 100  otherwise just get our tag rows
    if groups.len() > 100 {
        // break our groups into chunks of 100
        for chunk in groups.chunks(100) {
            // turn our group chunk into a vec
            let chunk_vec = chunk.to_vec();
            // get the tag rows for this item
            let query = shared
                .scylla
                .session
                .execute_unpaged(
                    &shared.scylla.prep.tags.get_rows,
                    (tag_type, chunk_vec, item),
                )
                .await?;
            // enable casting to types for this query
            let query_rows = query.into_rows_result()?;
            // set the type to cast this stream too
            let mut typed_stream = query_rows.rows::<FullTagRow>()?;
            // cast our rows to typed values
            while let Some(row_result) = typed_stream.next() {
                // raise any errors from casting
                if let Some(row) = log_scylla_err!(row_result) {
                    // get an entry to this tags value map or create it
                    let value_map: &mut TagValueMap = map.entry(row.key).or_default();
                    // get an entry to this tags group map or create it
                    let group_map: &mut TagGroupMap = value_map.entry(row.value).or_default();
                    // get an entry to this groups row list
                    let row_list = group_map.entry(row.group).or_default();
                    // add our row info to this list
                    row_list.push((row.year, row.bucket, row.uploaded));
                }
            }
        }
    } else {
        // get the tag rows for this item
        let query = shared
            .scylla
            .session
            .execute_unpaged(&shared.scylla.prep.tags.get_rows, (tag_type, groups, item))
            .await?;
        // enable casting to types for this query
        let query_rows = query.into_rows_result()?;
        // set the type to cast this stream too
        let mut typed_stream = query_rows.rows::<FullTagRow>()?;
        // cast our rows to typed values
        while let Some(row_result) = typed_stream.next() {
            // raise any errors from casting
            if let Some(row) = log_scylla_err!(row_result) {
                // get an entry to this tags value map or create it
                let value_map: &mut TagValueMap = map.entry(row.key).or_default();
                // get an entry to this tags group map or create it
                let group_map: &mut TagGroupMap = value_map.entry(row.value).or_default();
                // get an entry to this groups row list
                let row_list = group_map.entry(row.group).or_default();
                // add our row info to this list
                row_list.push((row.year, row.bucket, row.uploaded));
            }
        }
    }
    Ok(map)
}

/// Deletes tags from scylla
///
/// # Arguments
///
/// * `key` - The key to the item to delete tags from
/// * `req` - The request containing the tags to delete and the groups to delete them from
/// * `shared` - Shared Thorium objects
#[instrument(
    name = "db::tags::delete",
    skip(req, shared),
    fields(kind = T::tag_kind().as_str()),
    err(Debug)
)]
pub async fn delete<T: TagSupport>(
    key: &str,
    req: &TagDeleteRequest<T>,
    shared: &Shared,
) -> Result<(), ApiError> {
    // get the type of tag we are deleting
    let kind = T::tag_kind();
    // get all tag rows for this object
    let tag_map = get_tag_rows(kind, &req.groups, key, shared).await?;
    // build a redis pipeline to decrement this tags counts
    let mut pipe = redis::pipe();
    // crawl over the tags we want to delete and delete them
    // so this is pretty ugly but theres lots of nesting and so I am not sure of a better way to do it.
    for (tag_key, values) in &req.tags {
        // get this tags current value info
        if let Some(old_values) = tag_map.get(tag_key) {
            // crawl the values we want to delete
            for value in values {
                // get this tag values current info
                if let Some(old_info) = old_values.get(value) {
                    // crawl over the groups we want to delete tags from
                    for group in &req.groups {
                        // get this group's current rows
                        if let Some(old_rows) = old_info.get(group) {
                            // delete these rows
                            for (year, bucket, uploaded) in old_rows {
                                // log the tag we are deleting
                                event!(
                                    Level::INFO,
                                    tag_type = kind.as_str(),
                                    year = year,
                                    bucket = bucket,
                                    uploaded = uploaded.to_rfc3339(),
                                    key = &tag_key,
                                    value = &value
                                );
                                // delete this tag row
                                shared
                                    .scylla
                                    .session
                                    .execute_unpaged(
                                        &shared.scylla.prep.tags.delete,
                                        (
                                            kind, group, year, bucket, tag_key, &value, *uploaded,
                                            key,
                                        ),
                                    )
                                    .await?;
                                // build the key for this tags census count
                                let count_key = tags::census_count(
                                    T::tag_kind(),
                                    group,
                                    tag_key,
                                    value,
                                    *year,
                                    *bucket,
                                    shared,
                                );
                                // add data into redis
                                pipe.cmd("hincrby").arg(count_key).arg(bucket).arg(-1);
                            }
                        }
                    }
                }
            }
        }
    }
    // execute our redis pipeline
    let _: () = pipe.query_async(conn!(shared)).await?;
    // TODO remove any buckets with no data
    Ok(())
}

/// Gets tags for a specific item
///
/// # Arguments
///
/// * `tag_type` - The type of tags to get
/// * `groups` - The groups to restrict our returned tags too
/// * `item` - The item to get tags for
/// * `map` - The hashmap to add our tags too
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::tags::get", skip(shared), err(Debug))]
pub async fn get(
    tag_type: TagType,
    groups: &Vec<String>,
    item: &str,
    map: &mut TagMap,
    shared: &Shared,
) -> Result<(), ApiError> {
    // if we have more then 100 groups then chunk it into bathes of 100  otherwise just get our info
    if groups.len() > 100 {
        // break our groups into chunks of 100
        for chunk in groups.chunks(100) {
            // turn our group chunk into a vec
            let chunk_vec = chunk.to_vec();
            // get this chunks data
            let query = shared
                .scylla
                .session
                .execute_unpaged(&shared.scylla.prep.tags.get, (tag_type, chunk_vec, item))
                .await?;
            // enable casting to types for this query
            let query_rows = query.into_rows_result()?;
            // set the type to cast this stream too
            let mut typed_stream = query_rows.rows::<TagRow>()?;
            // cast our rows to typed values
            while let Some(row) = typed_stream.next() {
                // raise any errors from casting
                if let Some(tag) = log_scylla_err!(row) {
                    // get our key map oroinsert a default one
                    let key_map = map.entry(tag.key).or_default();
                    // get our value map or insert a default one
                    let group_list = key_map.entry(tag.value).or_default();
                    // extend our group list for this tag
                    group_list.insert(tag.group);
                }
            }
        }
    } else {
        // we have less then 100 groups so just get their data
        let query = shared
            .scylla
            .session
            .execute_unpaged(&shared.scylla.prep.tags.get, (tag_type, groups, item))
            .await?;
        // enable casting to types for this query
        let query_rows = query.into_rows_result()?;
        // set the type to cast this stream too
        let mut typed_stream = query_rows.rows::<TagRow>()?;
        // cast our rows to typed values
        while let Some(row) = typed_stream.next() {
            // raise any errors from casting
            if let Some(tag) = log_scylla_err!(row) {
                // get our key map oroinsert a default one
                let key_map = map.entry(tag.key).or_default();
                // get our value map or insert a default one
                let group_list = key_map.entry(tag.value).or_default();
                // extend our group list for this tag
                group_list.insert(tag.group);
            }
        }
    }
    Ok(())
}

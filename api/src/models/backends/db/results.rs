//! Saves results into the backend

use chrono::prelude::*;
use futures::stream::{self, StreamExt};
use itertools::Itertools;
use std::collections::{BTreeMap, HashMap};
use tracing::{event, instrument, span, Level, Span};
use uuid::Uuid;

use super::{ElasticCursor, ScyllaCursor};
use crate::models::backends::OutputSupport;
use crate::models::{
    ApiCursor, ElasticDoc, ElasticSearchParams, Output, OutputBundle, OutputChunk,
    OutputDisplayType, OutputForm, OutputId, OutputIdRow, OutputKind, OutputListLine, OutputMap,
    OutputRow, OutputStreamRow, ResultListParams,
};
use crate::utils::{helpers, ApiError, Shared};
use crate::{log_scylla_err, unauthorized};

/// Saves a files result into the backend
///
/// # Arguments
///
/// * `key` - The key to use when saving our results.
/// * `form` - The results to save to scylla
/// * `shared` - Shared Thorium objects
pub async fn create<O: OutputSupport>(
    key: &str,
    form: &OutputForm<O>,
    shared: &Shared,
    span: &Span,
) -> Result<(), ApiError> {
    // get the type of tag we are creating
    let kind = O::output_kind();
    // start our create results span
    let span = span!(parent: span, Level::INFO, "Save Results To Scylla");
    // wrap our tool in a vec
    let tools = vec![form.tool.clone()];
    // get our previous results
    let mut past = get(kind, &form.groups, key, &tools, true, shared).await?;
    // downselect to just this tools results
    let past = past.results.remove(&form.tool).unwrap_or_default();
    // get the current year and number of hours so far this year
    let now = Utc::now();
    let year = now.year();
    // get the chunk size for results
    let chunk_size = shared.config.thorium.results.partition_size;
    // determine what bucket to add data too
    let bucket = helpers::partition(now, year, chunk_size);
    // get the current timestamp
    let now = Utc::now();
    // save the result object
    shared
        .scylla
        .session
        .execute_unpaged(
            &shared.scylla.prep.results.insert,
            (
                &form.id,
                now,
                &form.tool,
                &form.tool_version,
                &form.cmd,
                &form.result,
                &form.files,
                form.display_type,
            ),
        )
        .await?;
    // save the stream rows for this result into scylla
    for group in &form.groups {
        shared
            .scylla
            .session
            .execute_unpaged(
                &shared.scylla.prep.results.insert_stream,
                (
                    kind,
                    &group,
                    year,
                    bucket,
                    key,
                    &form.tool,
                    &form.tool_version,
                    form.display_type,
                    now,
                    &form.cmd,
                    &form.id,
                ),
            )
            .await?;
    }
    // if we have more then our max results stored then delete any past that
    if past.len() >= shared.config.thorium.retention.results {
        // prune any results in groups with more then 3 values
        prune(kind, &form.groups, key, &past, shared, &span).await?;
    }
    Ok(())
}

/// Authorize a user has access to a specific result_id
///
/// # Arguments
///
/// * `groups` - The groups a user is in
/// * `sha256` - The sha256 this result is for
/// * `tool` - The tool this result is from
/// * `result_id` - The result id to authorize
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::results::authorize", skip(kind, shared), err(Debug))]
pub async fn authorize(
    kind: OutputKind,
    groups: &Vec<String>,
    key: &str,
    tool: &str,
    result_id: &Uuid,
    shared: &Shared,
) -> Result<(), ApiError> {
    // if we have more then 100 groups then break this into chunks of 100
    if groups.len() > 100 {
        // we have more then 100 groups so break this into chunks of 100
        for chunk in groups.chunks(100) {
            // cast our chunk array into a vec
            let chunk_vec = chunk.to_vec();
            // check if any of our groups have access to this sample
            let query = shared
                .scylla
                .session
                .execute_unpaged(
                    &shared.scylla.prep.results.get_id,
                    (key, &kind, chunk_vec, tool),
                )
                .await?;
            // cast this query to a rows query
            let query_rows = query.into_rows_result()?;
            // crawl over our rows and return early if we find our result id
            for row in query_rows.rows::<(Uuid,)>()? {
                // try to cast our id into a uuid
                if let Some(cast) = log_scylla_err!(row) {
                    // check if this matches our id
                    if cast.0 == *result_id {
                        // the id matches so return early
                        return Ok(());
                    }
                }
            }
        }
    } else {
        // we have less then 100 groups so just query them all at aonce
        // check if any of our groups have access to this sample
        let query = shared
            .scylla
            .session
            .execute_unpaged(
                &shared.scylla.prep.results.get_id,
                (key, kind, groups, tool),
            )
            .await?;
        // cast this query to a rows query
        let query_rows = query.into_rows_result()?;
        // crawl over our rows and return early if we find our result id
        for row in query_rows.rows::<(Uuid,)>()? {
            // try to cast our id into a uuid
            if let Some(cast) = log_scylla_err!(row) {
                // check if this matches our id
                if cast.0 == *result_id {
                    // the id matches so return early
                    return Ok(());
                }
            }
        }
    }
    // no matching result ids were found so bounce this request
    unauthorized!()
}

/// Gets result ids from scylla
///
/// If no tools are passed then all tool results will be returned.
///
/// # Arguments
///
/// * `kind` - The kind of results we are getting ids for
/// * `groups` - The groups to get result ids for
/// * `key` - The key to get result ids for
/// * `tools` - The tools to optionally restrict results retrieved to
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::results::get_ids", skip_all, err(Debug))]
async fn get_ids(
    kind: OutputKind,
    groups: &Vec<String>,
    key: &str,
    tools: &Vec<String>,
    hidden: bool,
    shared: &Shared,
) -> Result<Vec<OutputId>, ApiError> {
    // build a list of queries
    let mut queries = Vec::with_capacity(groups.len() * tools.len() / 50);
    // get the number of tools or default to 10
    let tools_len = if tools.len() > 0 { tools.len() } else { 1 };
    event!(Level::INFO, groups = groups.len(), tools = tools.len());
    // check if can do this all in one request or not
    if groups.len() * tools_len < 100 {
        // get our result ids from scylla
        let query = if tools.is_empty() {
            // get our get result ids query
            shared
                .scylla
                .session
                .execute_unpaged(
                    &shared.scylla.prep.results.get_with_key,
                    (kind, groups, key),
                )
                .await?
        } else {
            // get our get result ids restricted by tools query
            shared
                .scylla
                .session
                .execute_unpaged(
                    &shared.scylla.prep.results.get_with_key_and_tool,
                    (kind, groups, key, tools),
                )
                .await?
        };
        // add our query
        queries.push(query);
    } else if tools.len() > 0 {
        // our combined cartesian product is too high so we need to break up this query
        // crawl our groups 50 at a time and our tools 50 at a time
        for (groups_chunk, tools_chunk) in groups[..]
            .chunks(10)
            .cartesian_product(tools[..].chunks(10))
        {
            // turn our chunks into vecs
            let groups_vec = groups_chunk.to_vec();
            let tools_vec = tools_chunk.to_vec();
            // send get our get result ids restricted by tools query
            let query = shared
                .scylla
                .session
                .execute_unpaged(
                    &shared.scylla.prep.results.get_with_key_and_tool,
                    (kind, groups_vec, key, tools_vec),
                )
                .await?;
            // add our query
            queries.push(query);
        }
    } else {
        // we have no tools so just crawl our groups
        for chunk in groups.chunks(100) {
            // turn our chunks into vecs
            let groups_vec = chunk.to_vec();
            // send our query
            let query = shared
                .scylla
                .session
                .execute_unpaged(
                    &shared.scylla.prep.results.get_with_key,
                    (kind, groups_vec, key),
                )
                .await?;
            // add our query
            queries.push(query);
        }
    }
    // build a vec to store our queries in
    let mut deduped = Vec::with_capacity(tools_len * 3);
    // build a map of timestamps and uuids
    let mut map: HashMap<Uuid, OutputId> = HashMap::with_capacity(10);
    // crawl our queries
    for query in queries {
        // enable rows in this query
        let query_rows = query.into_rows_result()?;
        // turn our query results into a typed iter
        let typed_iter = query_rows.rows::<OutputIdRow>()?;
        // dudplicate our rows based on id and build the correct order to return them in
        let mut order: BTreeMap<DateTime<Utc>, Uuid> = BTreeMap::default();
        // crawl over our output id rows
        for row in typed_iter {
            // error on any invalid casts
            let row = row?;
            // skip any hidden rows unless we should return them
            if !hidden && row.display_type == OutputDisplayType::Hidden {
                // we are not showing hidden results so skip this result
                continue;
            }
            // if this id has already been seen then just add a new group
            if let Some(item) = map.get_mut(&row.id) {
                item.groups.push(row.group);
            } else {
                // build and insert our base deduplicated id
                let output_id = OutputId {
                    id: row.id,
                    tool: row.tool,
                    cmd: row.cmd,
                    groups: vec![row.group],
                    uploaded: row.uploaded,
                };
                map.insert(row.id, output_id);
                // add this row to our sorted btree map
                order.insert(row.uploaded, row.id);
            }
        }
        // build the deduplicated iter of ids
        let ids_iter = order.iter().rev().filter_map(|(_, id)| map.remove(id));
        // extend our deduped vec
        deduped.extend(ids_iter);
        // clear our map
        map.clear();
    }
    Ok(deduped)
}

/// Gets results from scylla
///
/// If no tools are passed then all tool results will be returned.
///
/// # Arguments
///
/// * `kind` - The kind of results to get
/// * `groups` - The groups to get results for
/// * `key` - The key to get results with
/// * `tools` - The tools to optionally restrict results retrieved to
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::results::get", skip_all, err(Debug))]
pub async fn get(
    kind: OutputKind,
    groups: &Vec<String>,
    key: &str,
    tools: &Vec<String>,
    hidden: bool,
    shared: &Shared,
) -> Result<OutputMap, ApiError> {
    // get the ids we want to pull for our results
    let ids = get_ids(kind, groups, key, tools, hidden, shared).await?;
    // if we found no ids then short circuit with empty results
    if ids.is_empty() {
        return Ok(OutputMap::default());
    }
    // build a list of just ids
    let id_list = ids.iter().map(|row| &row.id).collect::<Vec<&Uuid>>();
    // instance an empty result map to insert any retrieved rows into
    let mut temp = HashMap::with_capacity(id_list.len());
    // if we have more then 100 ids then chunk it into bathes of 100  otherwise just get our info
    if id_list.len() > 100 {
        // break our ids into chunks of 100
        for chunk in id_list.chunks(100) {
            // turn our id chunk into a vec
            let chunk_vec = chunk.to_vec();
            // get this chunks data
            let query = shared
                .scylla
                .session
                .execute_unpaged(&shared.scylla.prep.results.get, (chunk_vec,))
                .await?;
            // cast this query to a rows query
            let query_rows = query.into_rows_result()?;
            // cast our rows to an output and insert into our map
            query_rows
                .rows::<OutputRow>()?
                .filter_map(|res| log_scylla_err!(res))
                .for_each(|row| {
                    temp.insert(row.id, row);
                });
        }
    } else {
        // get this chunks data
        let query = shared
            .scylla
            .session
            .execute_unpaged(&shared.scylla.prep.results.get, (&id_list,))
            .await?;
        // cast this query to a rows query
        let query_rows = query.into_rows_result()?;
        // cast our rows to an output and insert into our map
        query_rows
            .rows::<OutputRow>()?
            .filter_map(|res| log_scylla_err!(res))
            .for_each(|row| {
                temp.insert(row.id, row);
            });
    }
    // build the ordered output map
    let mut outputs = OutputMap::default();
    // crawl our results and add them into our output map
    for id in ids {
        // skip any results that aren't in our result map
        if let Some(output) = temp.remove(&id.id) {
            // add this result to our output map
            outputs.add(output, id.groups);
        } else {
            // we are missing this result so log the error
            event!(
                Level::ERROR,
                result_id = id.id.to_string(),
                msg = "Missing output"
            );
        }
    }
    Ok(outputs)
}

/// Help the result pruner prune a specific result if its no longer reachable
///
/// # Arguments
///
/// * `kind` - The kind of results we are pruning
/// * `key` - The key to prune results from
/// * `result_id` - The id of the result to possibly prune
/// * `files` - The files in s3 to prune if neccesary
/// * `shared` - Shared Thorium objects
async fn prune_helper(
    kind: OutputKind,
    key: &str,
    result_id: &Uuid,
    files: &[String],
    shared: &Shared,
) -> Result<(), ApiError> {
    // query scylla to see if this result is reachable
    let query = shared
        .scylla
        .session
        .execute_unpaged(&shared.scylla.prep.results.count, (kind, key, result_id))
        .await?;
    // cast this query to a rows query
    let query_rows = query.into_rows_result()?;
    // delete any backing objects if its  no longer reachable
    if query_rows.rows_num() == 0 {
        // delete this result from scylla
        shared
            .scylla
            .session
            .execute_unpaged(&shared.scylla.prep.results.delete, (result_id,))
            .await?;
        // try to delete all result files from s3
        // if any fail this will orphan things but orphaned result files are better
        // then dangling ones
        for name in files {
            // build our result id path
            let s3_path = format!("{}/{}", &result_id, name);
            shared.s3.results.delete(&s3_path).await?;
        }
    }
    Ok(())
}

/// Prune any older results and their files
///
/// # Arguments
///
/// * `kind` - The kind of results we are pruning
/// * `groups` - The groups to prune results from
/// * `key` - The key to prune results from
/// * `results` - The results we are deciding whether to prune or not
/// * `shared` - Shared Thorium objects
/// * `span` - The span to log traces under
pub async fn prune(
    kind: OutputKind,
    groups: &[String],
    key: &str,
    results: &[Output],
    shared: &Shared,
    span: &Span,
) -> Result<(), ApiError> {
    // create our span
    span!(parent: span, Level::INFO, "Pruning Results");
    // track how many results for each group we have
    let mut stock: HashMap<&str, usize> = HashMap::default();
    // track the results we prune
    let mut pruned = Vec::default();
    // increment the groups for the results we just added
    for group in groups.iter() {
        stock.insert(group, 1);
    }
    // crawl these results and prune any that are not longer needed
    for result in results.iter() {
        // get the year and hour this result was uploaded
        let year = result.uploaded.year();
        // get the partition size for results
        let chunk_size = shared.config.thorium.results.partition_size;
        // get the partition to for this result
        let bucket = helpers::partition(result.uploaded, year, chunk_size);
        // track if we pruned this result for a group
        let mut prune_flag = false;
        for group in result.groups.iter() {
            // get this group if it wasn' already added
            let entry = stock.entry(group).or_insert(0);
            // if we are already at our retention limit then prune this result
            if *entry >= shared.config.thorium.retention.results {
                // get the year and hour for this result
                shared
                    .scylla
                    .session
                    .execute_unpaged(
                        &shared.scylla.prep.results.delete_stream,
                        (kind, group, year, bucket, result.uploaded, result.id),
                    )
                    .await?;
                // track that we pruned a result
                prune_flag = true;
            } else {
                // increment the number of results for this group
                *entry += 1;
            }
        }
        // if a result was pruned then add this to our list of pruned ids
        if prune_flag {
            pruned.push((&result.id, &result.files));
        }
    }
    // crawl ove the result files that might need to be pruned
    for (result_id, files) in pruned.iter() {
        // prune this result if its no longer needed
        prune_helper(kind, key, result_id, files, shared).await?;
    }
    Ok(())
}

//// Gets a chunk of the most recently updated results stream
///
/// # Arguments
///
/// * `kind` - The kind of results to list
/// * `params` - The query params for listing results
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::results::list", skip(kind, shared), err(Debug))]
pub async fn list(
    kind: OutputKind,
    params: ResultListParams,
    shared: &Shared,
) -> Result<ScyllaCursor<OutputListLine>, ApiError> {
    // get our cursor
    let mut cursor = ScyllaCursor::from_params_extra(params, kind, false, shared).await?;
    // get the next page of data for this cursor
    cursor.next(shared).await?;
    // save this cursor
    cursor.save(shared).await?;
    Ok(cursor)
}

/// Filter a list of result hashes down to only the latest versions
///
/// # Arguments
///
/// * `cursor` - The cursor to filter
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::results::latest_filter", skip_all, fields(input_rows = cursor.data.len()))]
pub async fn latest_filter(
    kind: OutputKind,
    cursor: &mut ScyllaCursor<OutputListLine>,
    shared: &Shared,
) {
    // build a list of futures to execute of the expected size
    let mut futures = Vec::with_capacity((cursor.data.len() / 100) * cursor.retain.group_by.len());
    // break our sha256s into chunks of 100
    for chunk in cursor.data.chunks(100) {
        // cast our chunk to a vec
        let chunk_vec = chunk
            .iter()
            .map(|line| line.key.to_owned())
            .collect::<Vec<String>>();
        // perform this search for each group
        for group in cursor.retain.group_by.iter() {
            // build our latest result query
            let query = shared.scylla.session.execute_unpaged(
                &shared.scylla.prep.results.get_uploaded,
                (kind, group, chunk_vec.clone()),
            );
            // create the future for this query and add it to our future list
            futures.push(query);
        }
    }
    // execute our futures
    let queries = stream::iter(futures)
        .buffer_unordered(10)
        .collect::<Vec<Result<_, _>>>()
        .await
        .into_iter()
        .filter_map(|res| log_scylla_err!(res));
    // get the latest timestamp for each sha256
    let mut latest = HashMap::with_capacity(cursor.data.len());
    // handle each query response
    for query in queries {
        // enable rows for this query
        if let Some(query_rows) = log_scylla_err!(query.into_rows_result()) {
            // set the type for our returned rows
            if let Some(typed_iter) = log_scylla_err!(query_rows.rows::<(String, DateTime<Utc>)>())
            {
                // log and skip any failures
                typed_iter
                    .filter_map(|res| log_scylla_err!(res))
                    // build a map containing the latest timestamp for each sample
                    .for_each(|(sha256, uploaded)| {
                        // if this sha isn't in latest yet then add it
                        latest
                            .entry(sha256)
                            .and_modify(|time| {
                                if *time < uploaded {
                                    *time = uploaded
                                }
                            })
                            .or_insert(uploaded);
                    });
            }
        }
    }
    // remove any sha256s whose latest timestamps are not the same as our
    cursor
        .data
        .retain(|chunk| latest.get(&chunk.key) == Some(&chunk.uploaded));
}

/// A temporary nested map to determine the latest id for each sha256/group/tool with timestamps
pub type TimedOutputMap = HashMap<String, HashMap<String, HashMap<String, (Uuid, DateTime<Utc>)>>>;

/// Get the latest tool result ids for all sha256s by group
///
/// # Arguments
///
/// * `kind` - The kind of results to get
/// * `groups` - The groups to get the latest ids from
/// * `data` - The cursor data to get the latest ids from
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::results::latest_ids", skip(groups, data, shared))]
async fn latest_ids(
    kind: OutputKind,
    groups: &[String],
    data: &[OutputListLine],
    shared: &Shared,
) -> TimedOutputMap {
    // build a list of futures to execute of the expected size
    let mut futures = Vec::with_capacity((data.len() / 100) * groups.len());
    // break our keys into chunks of 100
    for chunk in data.chunks(100) {
        // build a list of keys for our outpus
        let keys = chunk
            .iter()
            .map(|output| &output.key)
            .collect::<Vec<&String>>();
        // perform this search for each group
        for group in groups.iter() {
            // create the future for this query and add it to our future list
            futures.push(shared.scylla.session.execute_unpaged(
                &shared.scylla.prep.results.get_stream,
                (kind, group, keys.clone()),
            ));
        }
    }
    // execute our futures
    let queries = stream::iter(futures)
        .buffer_unordered(10)
        .collect::<Vec<Result<_, _>>>()
        .await
        .into_iter()
        .filter_map(|res| log_scylla_err!(res));
    // get the latest ids for each key tool/group
    let mut timed: TimedOutputMap = HashMap::with_capacity(data.len());
    // handle each query response
    for query in queries {
        // enable rows for this query
        if let Some(query_rows) = log_scylla_err!(query.into_rows_result()) {
            // set the type for our returned rows
            if let Some(typed_iter) = log_scylla_err!(query_rows.rows::<OutputStreamRow>()) {
                // log and skip any failures
                typed_iter
                    .filter_map(|res| log_scylla_err!(res))
                    // build a map containing the latest timestamp for each sample
                    .for_each(|row| {
                        // if this key isn't already in our map then add it with a map of groups
                        let groups = timed.entry(row.key).or_default();
                        // if this group isn't already in the map then add it
                        let tools = groups.entry(row.group).or_default();
                        // get the entry for this tool in our tools map
                        tools.entry(row.tool).
                // if this tool does exist then check if this result is newer
                and_modify(|result| {
                    if result.1 < row.uploaded {
                        *result = (row.id, row.uploaded);
                    }
                })
            .or_insert((row.id, row.uploaded));
                    });
            }
        }
    }
    timed
}

/// Get the results for a map of tools and ids
///
/// # Arguments
///
/// * `temp` - The latest reuslts to get output chunks for
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::results::get_output_chunks", skip_all)]
pub async fn get_output_chunks(
    temp: &TimedOutputMap,
    shared: &Shared,
) -> Result<HashMap<Uuid, OutputChunk>, ApiError> {
    // get a list of all ids
    let ids = temp
        .values()
        .flat_map(|groups| groups.values())
        .flat_map(|tools| tools.values())
        .map(|(res_id, _)| res_id)
        .collect::<Vec<&Uuid>>();
    // build a list of futures to execute
    let mut futures = Vec::with_capacity(ids.len());
    // build a map of outputs
    let mut outputs = HashMap::with_capacity(ids.len());
    // crawl over these result ids 100 at a time
    for chunk in ids.chunks(100) {
        // create the future for this query and add it to our future list
        futures.push(
            shared
                .scylla
                .session
                .execute_unpaged(&shared.scylla.prep.results.get, (chunk,)),
        );
    }
    // build a stream of futures to execute
    let mut query_stream = stream::iter(futures).buffer_unordered(10);
    // cast our results as they come in
    while let Some(query) = query_stream.next().await {
        // return any errors from our query
        let query = query?;
        // enable rows on this query
        let query_rows = query.into_rows_result()?;
        // cast our rows to the right row type
        for cast in query_rows.rows::<OutputRow>()? {
            // return any casting errors
            let cast = cast?;
            // this rows to our output map
            outputs.insert(cast.id, OutputChunk::from(cast));
        }
    }
    Ok(outputs)
}

/// Convert the latest result ids to [`OutputBundle`]s
///
/// # Arguments
///
/// * `kind` - The kind of results to get
/// * `cursor` - The cursor to get bundled results for
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::results::bundle_cursor", skip(cursor, shared), fields(rows = cursor.data.len()))]
async fn bundle_cursor(
    kind: OutputKind,
    cursor: ScyllaCursor<OutputListLine>,
    shared: &Shared,
) -> Result<Vec<OutputBundle>, ApiError> {
    // get the latest result ids for our sha256s
    let mut latest = latest_ids(kind, &cursor.retain.group_by, &cursor.data, shared).await;
    // get our result chunks
    let mut chunks = get_output_chunks(&latest, shared).await?;
    // build the output details list
    let mut details = Vec::with_capacity(latest.len());
    for item in cursor.data {
        // remove this sha256 from our latest stream
        if let Some(groups) = latest.remove(&item.key) {
            // build an empty output list map for this sha256
            let mut output = OutputBundle::from(item);
            // iterate over the groups
            for (group, tools) in groups {
                // create a hashmap for this sha256
                let entry = output
                    .map
                    .entry(group)
                    .or_insert_with(|| HashMap::with_capacity(tools.len()));
                // iterate over the tools
                for (tool, (chunk_id, uploaded)) in tools {
                    // if this chunk is in our chunk map then insert it into our results map
                    if let Some((res_id, result)) = chunks.remove_entry(&chunk_id) {
                        // check if this results id is newer then our current one
                        if output.latest < uploaded {
                            // this results timestamp is newer then our other results so upate our latest
                            output.latest = uploaded;
                        }
                        output.results.insert(res_id, result);
                    }
                    // add this tool and id into our tools map
                    entry.insert(tool, chunk_id);
                }
            }
            // add this output to our details list
            details.push(output);
        }
    }
    Ok(details)
}

/// Gets a chunk of results list deduped to the latest result for each group and tool
///
/// # Arguments
///
/// * `kind` - The kind of results to get bundles for
/// * `params` - The query params for listing results
/// * `shared` - Shared Thorium objects
/// * `span` - The span to log traces under
#[instrument(name = "db::results::bundle", skip(kind, shared), err(Debug))]
pub async fn bundle(
    kind: OutputKind,
    params: ResultListParams,
    shared: &Shared,
) -> Result<ApiCursor<OutputBundle>, ApiError> {
    // get our cursor
    let mut cursor = ScyllaCursor::from_params_extra(params, kind, false, shared).await?;
    // loop over this cursor and get more pages until our data vector is full or we have exchausted it
    while cursor.data.len() < cursor.limit {
        // get the next page of data for this cursor
        cursor.next(shared).await?;
        // filter out any results which were already streamed
        latest_filter(kind, &mut cursor, shared).await;
        // check if this cursor has been exhausted
        if cursor.exhausted() {
            break;
        }
    }
    // save this cursor
    cursor.save(shared).await?;
    // get the id for this cursor and determine if its exhausted or not
    let cursor_id = cursor.id;
    let exhausted = cursor.exhausted();
    // build a details stream from our cursor
    let data = bundle_cursor(kind, cursor, shared).await?;
    // build the external cursor object for this flattened list
    let api_cursor = if exhausted {
        ApiCursor { cursor: None, data }
    } else {
        ApiCursor {
            cursor: Some(cursor_id),
            data,
        }
    };
    Ok(api_cursor)
}

/// Search for results matching a query in elastic
///
/// # Arguments
///
/// * `params` - The query params for searching results
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::results::search", skip(shared), err(Debug))]
pub async fn search(
    params: ElasticSearchParams,
    shared: &Shared,
) -> Result<ApiCursor<ElasticDoc>, ApiError> {
    // get our cursor or build a new one
    let mut cursor = ElasticCursor::from_params(params, shared).await?;
    //  get the next page of data
    cursor.next(shared).await?;
    // save this cursor
    cursor.save(shared).await?;
    let data = std::mem::take(&mut cursor.data);
    // determine if this cursor has been exhausted or not
    if data.len() < cursor.limit as usize {
        // this cursor is exhausted so omit the cursor ID
        Ok(ApiCursor { cursor: None, data })
    } else {
        // this cursor is exhausted so omit the cursor ID
        Ok(ApiCursor {
            cursor: Some(cursor.id),
            data,
        })
    }
}

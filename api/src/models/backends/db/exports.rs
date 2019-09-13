//! Handles export objects in Scylla

use chrono::prelude::*;
use tracing::{instrument, span, Level};
use uuid::Uuid;

use crate::models::exports::ExportErrorRequest;
use crate::models::{
    Export, ExportError, ExportErrorRow, ExportListParams, ExportOps, ExportRequest, ExportUpdate,
};
use crate::utils::{helpers, ApiError, Shared};
use crate::{conflict, not_found};

use super::ScyllaCursor;

/// Check if an export object exists by name
pub async fn exists(export_type: ExportOps, name: &str, shared: &Shared) -> Result<bool, ApiError> {
    // query scylla to see if this export exists
    let query = shared
        .scylla
        .session
        .execute_unpaged(&shared.scylla.prep.exports.exists, (export_type, name))
        .await?;
    // enable rows on this query response
    let query_rows = query.into_rows_result()?;
    // make sure we got at least one row from scylla
    Ok(query_rows.rows_num() > 0)
}

/// Creates a new export operation in scylla
///
/// # Arguments
///
/// * `user` - The user that is creating an export
/// * `req` - The export to create
/// * `export_type` - The type of export to create
/// * `shared` - Shared Thorium objects
pub async fn create(
    user: &str,
    req: ExportRequest,
    export_type: ExportOps,
    shared: &Shared,
) -> Result<Export, ApiError> {
    // if this id is unused then use it
    if exists(export_type, &req.name, shared).await? {
        return conflict!("This export already exists!".to_string());
    }
    // save this export operations status info to scylla
    shared
        .scylla
        .session
        .execute_unpaged(
            &shared.scylla.prep.exports.insert,
            (export_type, &req.name, user, req.start, req.end, req.end),
        )
        .await?;
    // build our export object
    let export = Export {
        name: req.name,
        user: user.to_owned(),
        start: req.start,
        current: req.end,
        end: req.end,
    };
    Ok(export)
}

/// Get an existing export object by id
///
/// This object will have an empty export cursor map
///
/// # Arguments
///
/// * `export_name` - The name of the export to get
/// * `export_type` - The type of export operation to get
/// * `groups` - The groups to search in
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::exports::get", skip(shared), err(Debug))]
pub async fn get(
    export_name: &str,
    export_type: ExportOps,
    shared: &Shared,
) -> Result<Export, ApiError> {
    // query scylla for this export operations info
    let query = shared
        .scylla
        .session
        .execute_unpaged(&shared.scylla.prep.exports.get, (export_type, export_name))
        .await?;
    // enable rows in this response
    let query_rows = query.into_rows_result()?;
    // try to cast the first returned row to an export row
    match query_rows.maybe_first_row::<Export>()? {
        Some(export) => Ok(export),
        // no export info was found to return a 404
        None => not_found!(format!("Export {} not found", export_name)),
    }
}

/// Updates the curernt timestamp for an ongoing export operation
///
/// # Arguments
///
/// * `req_id` - The id of the export to update
/// * `export_type` - The type of export operation to add a cursor too
/// * `update` - The update to apply to this cursor
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::exports::update", skip(update, shared), err(Debug))]
pub async fn update(
    name: &str,
    export_type: ExportOps,
    update: &ExportUpdate,
    shared: &Shared,
) -> Result<(), ApiError> {
    // update this export operation cursor in scylla
    shared
        .scylla
        .session
        .execute_unpaged(
            &shared.scylla.prep.exports.update,
            (update.current, export_type, name),
        )
        .await?;
    Ok(())
}

/// Adds a new error to an export operation
///
/// # Arguments
///
/// * `export` - The export to save an error for
/// * `export_type` - The type of export operation to add a cursor for
/// * `req` - The error to save to the db
/// * `shared` - Shared Thorium objects
pub async fn create_error(
    export: &Export,
    export_type: ExportOps,
    req: &ExportErrorRequest,
    shared: &Shared,
) -> Result<Uuid, ApiError> {
    // get the year and hour for our start time
    let year = req.start.year();
    // get the chunk size for results
    let chunk = shared.config.thorium.results.partition_size;
    let bucket = helpers::partition(req.start, year, chunk);
    // generate a random uuid for this request
    let id = Uuid::new_v4();
    // save this export operation error to scylla
    shared
        .scylla
        .session
        .execute_unpaged(
            &shared.scylla.prep.exports.insert_error,
            (
                &export.name,
                &export_type,
                year,
                bucket,
                req.start,
                req.end,
                &id,
                #[allow(clippy::cast_lossless)]
                req.code.map(|code| code as i32),
                &req.msg,
            ),
        )
        .await?;
    Ok(id)
}

/// Gets info on a specfic export error
///
/// # Arguments
///
/// * `export` - The export operation to get an error from
/// * `op` - The type of export operation to get an error from
/// * `error_id` - The id of the error to get info on
/// * `shared` - Shared Thorium objects
pub async fn get_error(
    export: &Export,
    op: ExportOps,
    error_id: &Uuid,
    shared: &Shared,
) -> Result<ExportError, ApiError> {
    // create our span
    span!(Level::INFO, "Getting Export Error");
    // try to get the errors for this export
    let query = shared
        .scylla
        .session
        .execute_unpaged(
            &shared.scylla.prep.exports.get_error_by_id,
            (&error_id, &export.name, op),
        )
        .await?;
    // enable rows on this query response
    let query_rows = query.into_rows_result()?;
    // try to cast this row
    match query_rows.maybe_first_row::<ExportErrorRow>()? {
        Some(typed_row) => Ok(ExportError::from(typed_row)),
        None => not_found!(format!("Export Error {} not found", error_id)),
    }
}

/// List errors for a specific export operation and groups
///
/// # Arguments
///
/// * `export` - The id of the export operation to list errors from
/// * `op` - The type of export operation to list errors from
/// * `params` - The query params to use when listing errors
/// * `shared` - Shared Thorium objects
#[instrument(name = "db::exports::list_errors", skip(shared), err(Debug))]
pub async fn list_errors(
    export: String,
    op: ExportOps,
    params: ExportListParams,
    shared: &Shared,
) -> Result<ScyllaCursor<ExportError>, ApiError> {
    // get our cursor
    let mut cursor = ScyllaCursor::from_params_extra(params, (export, op), false, shared).await?;
    // get the next page of data for this cursor
    cursor.next(shared).await?;
    // save this cursor
    cursor.save(shared).await?;
    Ok(cursor)
}

/// Delete an error for an ongoing export operation
///
/// # Arguments
///
/// * `export` - The export to delete an error from
/// * `export_type` - The type of export operation to delete an error from
/// * `error_id` - The ID of the error to delete
/// * `shared` - Shared Thorium objects
pub async fn delete_error(
    export: &Export,
    export_type: ExportOps,
    error_id: &Uuid,
    shared: &Shared,
) -> Result<(), ApiError> {
    // try to get info on our export error
    let export_error = get_error(export, export_type, error_id, shared).await?;
    // get the year and bucket for our start time
    let year = export_error.start.year();
    // get the chunk size for results
    let chunk = shared.config.thorium.results.partition_size;
    let bucket = helpers::partition(export_error.start, year, chunk);
    // delete this export operation error from scylla
    shared
        .scylla
        .session
        .execute_unpaged(
            &shared.scylla.prep.exports.delete_error,
            (
                &export.name,
                export_type,
                year,
                bucket,
                export_error.start,
                error_id,
            ),
        )
        .await?;
    Ok(())
}

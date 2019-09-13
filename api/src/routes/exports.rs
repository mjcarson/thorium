//! The exports related routes for Thorium

use axum::extract::{Json, Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::Router;
use tracing::instrument;
use utoipa::OpenApi;
use uuid::Uuid;

use super::OpenApiSecurity;
use crate::models::exports::ExportErrorRequest;
use crate::models::{
    ApiCursor, Export, ExportError, ExportErrorResponse, ExportListParams, ExportOps,
    ExportRequest, ExportUpdate, User,
};
use crate::utils::{ApiError, AppState};

/// Create a new results export operation
///
/// # Arguments
///
/// * `user` - The user submitting these results
/// * `state` - Shared Thorium objects
/// * `export` - The export to create
#[utoipa::path(
    post,
    path = "/api/exports/",
    params(
        ("export" = ExportRequest, description = "The export to create")
    ),
    responses(
        (status = 200, description = "Export created", body = Export),
        (status = 401, description = "This user is not authorized to access this route"),
    ),
    security(
        ("basic" = []),
    )
)]
#[instrument(name = "routes::exports::create", skip_all, err(Debug))]
async fn create(
    user: User,
    State(state): State<AppState>,
    Json(export): Json<ExportRequest>,
) -> Result<Json<Export>, ApiError> {
    // create this export operation in the backend
    let export = Export::create(&user, export, ExportOps::Results, &state.shared).await?;
    Ok(Json(export))
}

/// Gets info on a results export for the current user by name
///
/// # Arguments
///
/// * `user` - The user getting info on this export operation
/// * `name` - The name of the export to get
/// * `state` - Shared Thorium objects
#[utoipa::path(
    get,
    path = "/api/exports/:name",
    params(
        ("name" = String, Path, description = "The name of the export to get")
    ),
    responses(
        (status = 200, description = "Export retrieved", body = Export),
        (status = 401, description = "This user is not authorized to access this route"),
    ),
    security(
        ("basic" = []),
    )
)]
#[instrument(name = "routes::exports::get", skip_all, err(Debug))]
async fn get_export(
    user: User,
    Path(name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Export>, ApiError> {
    // get info on our export operation by name
    let export = Export::get(&user, &name, ExportOps::Results, &state.shared).await?;
    Ok(Json(export))
}

/// Updates an export operation
///
/// # Arguments
///
/// * `user` - The user getting info on this export operation
/// * `name` - The name of the export to update
/// * `state` - Shared Thorium objects
/// * `req_id` - This requests ID
/// * `update` - The update to apply to this export operation
#[utoipa::path(
    patch,
    path = "/api/exports/:name",
    params(
        ("name" = String, Path, description = "The name of the export to update"),
        ("update" = ExportUpdate, description = "The update to apply to this export operation"),
    ),
    responses(
        (status = 200, description = "Export updated", body = Export),
        (status = 401, description = "This user is not authorized to access this route"),
    ),
    security(
        ("basic" = []),
    )
)]
#[instrument(name = "routes::exports::update", skip_all, err(Debug))]
async fn update(
    user: User,
    Path(name): Path<String>,
    State(state): State<AppState>,
    Json(update): Json<ExportUpdate>,
) -> Result<Json<Export>, ApiError> {
    // get info on our export operation by name
    let export = Export::get(&user, &name, ExportOps::Results, &state.shared).await?;
    // update this export operation
    export
        .update(ExportOps::Results, &update, &state.shared)
        .await?;
    Ok(Json(export))
}

/// Saves an error from a specific export
///
/// # Arguments
///
/// * `user` - The user updating this export cursor
/// * `name` - The name of the export save an error for
/// * `state` - Shared Thorium objects
/// * `error` - The error to save
#[utoipa::path(
    post,
    path = "/api/exports/:name/error",
    params(
        ("name" = String, Path, description = "The name of the export save an error for"),
        ("error" = ExportErrorRequest, description = "The error to save"),
    ),
    responses(
        (status = 200, description = "Export error saved", body = ExportErrorResponse),
        (status = 401, description = "This user is not authorized to access this route"),
    ),
    security(
        ("basic" = []),
    )
)]
#[instrument(name = "routes::exports::create_error", skip_all, err(Debug))]
async fn create_error(
    user: User,
    Path(name): Path<String>,
    State(state): State<AppState>,
    Json(error): Json<ExportErrorRequest>,
) -> Result<Json<ExportErrorResponse>, ApiError> {
    // get info on our export operation by name
    let export = Export::get(&user, &name, ExportOps::Results, &state.shared).await?;
    // save this error to scylla
    let id = export
        .create_error(ExportOps::Results, &error, &state.shared)
        .await?;
    Ok(Json(ExportErrorResponse { id }))
}

/// Lists the errors for a specific export operation
///
/// # Arguments
///
/// * `user` - The user that is listing submissions
/// * `name` - The export to list errors from
/// * `params` - The query params to use for this request
/// * `state` - Shared Thorium objects
#[utoipa::path(
    get,
    path = "/api/exports/:name/error",
    params(
        ("name" = String, Path, description = "The export to list errors from"),
        ("params" = ExportListParams, description = "The query params to use for the error list request"),
    ),
    responses(
        (status = 200, description = "List of errors for export", body = ApiCursor<ExportError>),
        (status = 401, description = "This user is not authorized to access this route"),
    ),
    security(
        ("basic" = []),
    )
)]
#[instrument(name = "routes::exports::list_errors", skip_all, err(Debug))]
async fn list_errors(
    user: User,
    Path(name): Path<String>,
    params: ExportListParams,
    State(state): State<AppState>,
) -> Result<Json<ApiCursor<ExportError>>, ApiError> {
    // get info on our export operation by name
    let export = Export::get(&user, &name, ExportOps::Results, &state.shared).await?;
    // get a page of errors for this export operation
    let cursor = export
        .list_error(ExportOps::Results, params, &state.shared)
        .await?;
    Ok(Json(cursor))
}

/// Deletes an error from an export operation
///
/// # Arguments
///
/// * `user` - The user deleting this export error
/// * `name` - The name of the export to delete an error from
/// * `cursor_id` - The id of the error to delete
/// * `state` - Shared Thorium objects
#[utoipa::path(
    delete,
    path = "/api/exports/:name/error/:error_id",
    params(
        ("name" = String, Path, description = "The name of the export to delete an error from"),
        ("error_id" = Uuid, Path, description = "The Uuid of the error to delete"),
    ),
    responses(
        (status = 204, description = "Error deleted"),
        (status = 401, description = "This user is not authorized to access this route"),
    ),
    security(
        ("basic" = []),
    )
)]
#[instrument(name = "routes::exports::delete_error", skip_all, err(Debug))]
async fn delete_error(
    user: User,
    Path((name, error_id)): Path<(String, Uuid)>,
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    // get info on our export operation by name
    let export = Export::get(&user, &name, ExportOps::Results, &state.shared).await?;
    // delete this from scylla
    export
        .delete_error(ExportOps::Results, &error_id, &state.shared)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The struct containing our openapi docs
#[derive(OpenApi)]
#[openapi(
    paths(create, get_export, update, create_error, list_errors, delete_error),
    components(schemas(ApiCursor<ExportError>, Export, ExportErrorRequest, ExportErrorResponse, ExportListParams, ExportOps, ExportRequest, ExportUpdate)),
    modifiers(&OpenApiSecurity),
)]
pub struct ExportApiDocs;

/// Return the openapi docs for these routes
#[allow(dead_code)]
async fn openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(ExportApiDocs::openapi())
}

/// Add the exports routes to our router
///
/// # Arguments
///
// * `router` - The router to add routes too
pub fn mount(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/exports/", post(create))
        .route("/api/exports/:name", get(get_export).patch(update))
        .route(
            "/api/exports/:name/error",
            post(create_error).get(list_errors),
        )
        .route("/api/exports/:name/error/:error_id", delete(delete_error))
}

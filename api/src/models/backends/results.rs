//! Handles saving results into the backend

use aws_sdk_s3::primitives::ByteStream;
use axum::extract::multipart::Field;
use axum::extract::{FromRequestParts, Multipart};
use axum::http::request::Parts;
use axum::http::StatusCode;
use chrono::prelude::*;
use futures_util::Future;
use scylla::transport::errors::QueryError;
use scylla::QueryResult;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use tracing::{instrument, Span};
use uuid::Uuid;

use super::db::{self, CursorCore, ScyllaCursorSupport};
use crate::models::backends::OutputSupport;
use crate::models::{
    ApiCursor, AutoTag, AutoTagUpdate, ElasticDoc, ElasticSearchParams, ImageVersion, Output,
    OutputBundle, OutputChunk, OutputCollection, OutputCollectionUpdate, OutputDisplayType,
    OutputForm, OutputFormBuilder, OutputKind, OutputListLine, OutputMap, OutputRow,
    OutputStreamRow, Repo, ResultGetParams, ResultListParams, Sample, User,
};
use crate::utils::{ApiError, Shared};
use crate::{bad, deserialize, update, update_clear, update_opt};

impl<O: OutputSupport> OutputFormBuilder<O> {
    /// Adds a multipart field to our sample form
    ///
    /// # Arguments
    ///
    /// * `field` - The field to try to add
    pub async fn add<'a>(&'a mut self, field: Field<'a>) -> Result<Option<Field<'a>>, ApiError> {
        // get the name of this field
        if let Some(name) = field.name() {
            // add this fields value to our form
            match name {
                "groups" => self.groups.push(field.text().await?),
                "tool" => self.tool = Some(field.text().await?),
                "tool_version" => {
                    self.tool_version = Some(ImageVersion::from(&field.text().await?))
                }
                "cmd" => self.cmd = Some(field.text().await?),
                "result" => self.result = Some(field.text().await?),
                "display_type" => {
                    self.display_type = Some(OutputDisplayType::from_str(&field.text().await?[..])?)
                }
                "extra" => self.extra = Some(deserialize!(&field.text().await?)),
                // this is the data so return it so we can stream it to s3
                "files" => return Ok(Some(field)),
                _ => return bad!(format!("{} is not a valid form name", name)),
            }
            // we found and consumed a valid form entry
            return Ok(None);
        }
        bad!(format!("All form entries must have a name!"))
    }

    ///  Validate and convert this [`OutputFormBuilder`] to an [`OutputForm`]
    ///
    /// This takes a mutable ref and takes most of the values in the form builder
    /// but leaves files so that we can safely clean them up in case of errors.
    fn build(&mut self) -> Result<OutputForm<O>, ApiError> {
        // make sure that all of our required options are set
        if self.tool.is_none() || self.display_type.is_none() || !O::validate_extra(&self.extra) {
            // reject this invalid request
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                Some("OutputRequest is missing fields!".to_owned()),
            ));
        }
        // build our output request
        let valid = OutputForm {
            id: self.id,
            groups: std::mem::take(&mut self.groups),
            tool: self.tool.take().unwrap(),
            tool_version: std::mem::take(&mut self.tool_version),
            cmd: self.cmd.take(),
            result: self.result.take().unwrap(),
            display_type: self.display_type.take().unwrap(),
            files: self.files.clone(),
            extra: O::extract_extra(self.extra.take()),
        };
        Ok(valid)
    }

    /// Save a result to the backend for specific samples
    ///
    /// # Arguments
    ///
    /// * `user` - The user that is adding new results
    /// * `upload` - The mutlipart form containing our results
    /// * `form` - The results form to add our multipart entries too
    /// * `shared` - Shared objects in Thorium
    #[instrument(
        name = "OutputForm::create_results_helper",
        skip(self, user, object, upload, shared),
        err(Debug)
    )]
    async fn create_results_helper(
        &mut self,
        user: &User,
        key: O::Key,
        object: &O,
        mut upload: Multipart,
        shared: &Shared,
    ) -> Result<(), ApiError> {
        // copy our results id
        let result_id = self.id;
        // begin crawling over our multipart form upload
        while let Some(field) = upload.next_field().await? {
            // try to consume our fields
            if let Some(data_field) = self.add(field).await? {
                // throw an error if the correct content type is not used
                if data_field.content_type().is_none() {
                    return bad!("A content type must be set for the data form entry!".to_owned());
                }
                // try to get the name for this file
                let file_name = data_field.file_name().map_or_else(
                    || Uuid::new_v4().to_string(),
                    std::borrow::ToOwned::to_owned,
                );
                // build the path to save this attachment at in s3
                let s3_path = format!("{}/{}", &result_id, file_name);
                // cart and stream this file into s3
                shared.s3.results.stream(&s3_path, data_field).await?;
                // add this file name to our form
                self.files.push(file_name);
            }
        }
        // validate and cast our results
        let mut form = self.build()?;
        // make sure these groups are valid for this result
        object
            .validate_groups_editable(user, &mut form.groups, shared)
            .await?;
        // build the key to save results and tags too
        let key = O::build_key(key.clone(), &form.extra);
        // get our current span
        let span = Span::current();
        // save these results to the backend
        db::results::create(&key, &form, shared, &span).await?;
        // build the tag request for this results tags
        let tag_req = O::tag_req()
            .groups(form.groups.clone())
            .add("Results", &form.tool);
        // get the earliest each group has seen this object
        let earliest = object.earliest();
        // add the tags for this result
        db::tags::create(user, key, tag_req, &earliest, shared).await?;
        Ok(())
    }

    /// Save a result to the backend for a specific kind of data
    ///
    /// # Arguments
    ///
    /// * `user` - The user that is adding new results
    /// * `kind` - The kind of data we are saving results for
    /// * `key` - The key for the data we are saving results for
    /// * `upload` - The mutlipart form containing our results
    /// * `shared` - Shared objects in Thorium
    #[instrument(
        name = "OutputForm::create_results",
        skip(self, user, object, upload, shared),
        err(Debug)
    )]
    pub async fn create_results(
        mut self,
        user: &User,
        key: O::Key,
        object: &O,
        upload: Multipart,
        shared: &Shared,
    ) -> Result<Uuid, ApiError> {
        // try to save this result to the backend
        match self
            .create_results_helper(user, key, object, upload, shared)
            .await
        {
            Ok(()) => Ok(self.id),
            Err(err) => {
                // delete all our dangling comment attachments
                for name in self.files {
                    // build the path to delete this attachment at in s3
                    let s3_path = format!("{}/{}", self.id, name);
                    // delete this result file from s3
                    shared.s3.results.delete(&s3_path).await?;
                }
                Err(err)
            }
        }
    }
}

impl OutputMap {
    /// Get results for a specific object
    ///
    /// # Arguments
    ///
    /// * `key` - The full key to get our results at
    /// * `item` - The object we are getting results for
    /// * `user` - The user that is getting results
    /// * `params` - The query params for getting results
    /// * `shared` - Shared Thorium objects
    #[instrument(name = "OutputMap::get", skip_all, err(Debug))]
    pub async fn get<T: OutputSupport>(
        key: &str,
        item: &T,
        user: &User,
        mut params: ResultGetParams,
        shared: &Shared,
    ) -> Result<Self, ApiError> {
        // authorize this user can get results from the requested groups
        item.validate_groups_viewable(user, &mut params.groups, shared)
            .await?;
        // get our results
        db::results::get(
            T::output_kind(),
            &params.groups,
            key,
            &params.tools,
            params.hidden,
            shared,
        )
        .await
    }
}

impl OutputMap {
    /// Add an output row to this map
    ///
    /// # Arguments
    ///
    /// * `row` - The output to add to this map
    /// * `groups` - The groups this result is from
    pub(super) fn add(&mut self, row: OutputRow, groups: Vec<String>) {
        // get an entry to this tools command map
        let results = self.results.entry(row.tool.clone()).or_default();
        // try to deserialize our string as a json Value
        let (result, deserialization_error) = match serde_json::from_str(&row.result) {
            Ok(value) => (value, None),
            Err(e) => (serde_json::Value::String(row.result), Some(e.to_string())),
        };
        // build our output object for this row
        let output = Output {
            id: row.id,
            groups,
            tool_version: row.tool_version,
            cmd: row.cmd,
            uploaded: row.uploaded,
            deserialization_error,
            result,
            files: row.files.unwrap_or_default(),
            display_type: row.display_type,
            children: row.children.unwrap_or_default(),
        };
        // push our results
        results.push(output);
    }

    /// limit our output map to at most N results for each tool
    ///
    /// # Arguments
    ///
    /// * `limit` - The max number of results to keep for each tool
    pub fn limit(&mut self, limit: usize) {
        for (_, results) in self.results.iter_mut() {
            results.truncate(limit);
        }
    }
}

impl Output {
    /// Downloads a result file
    ///
    /// # Arguments
    ///
    /// * `user` - The user submitting these results
    /// * `sha256` - The sha256 we are trying to download results from
    /// * `tool` - The name of the tool these results are from
    /// * `result_id` - The ID for the result to download files from
    /// * `name` - The name of the file to download
    /// * `shared` - Shared Thorium objects
    #[instrument(name = "Output::download", skip(kind, user, shared), err(Debug))]
    pub async fn download(
        kind: OutputKind,
        user: &User,
        key: &str,
        tool: &str,
        result_id: &Uuid,
        file_path: PathBuf,
        shared: &Shared,
    ) -> Result<ByteStream, ApiError> {
        // make sure that this user has access to this repo or sample
        kind.authorize(user, key, shared).await?;
        // authorize this user has access to this result id if we are not an admin
        if !user.is_admin() {
            // we are not an admin so make sure we can see this result
            db::results::authorize(kind, &user.groups, key, tool, result_id, shared).await?;
        }
        // build the path to this file in s3
        let path = format!("{}/{}", result_id, file_path.to_string_lossy());
        // download this result file
        shared.s3.results.download(&path).await
    }

    /// Get a chunk of the Result list
    /// # Arguments
    ///
    /// * `user` - The user that is listing results
    /// * `kind` - The kind of results to list
    /// * `params` - The query params for listing results
    /// * `shared` - Shared Thorium objects
    #[instrument(name = "Output::list", skip(kind, user, shared), err(Debug))]
    pub async fn list(
        user: &User,
        kind: OutputKind,
        mut params: ResultListParams,
        shared: &Shared,
    ) -> Result<ApiCursor<OutputListLine>, ApiError> {
        // authorize the groups to list results from
        user.authorize_groups(&mut params.groups, shared).await?;
        // get a chunk of the results stream
        let scylla_cursor = db::results::list(kind, params, shared).await?;
        // convert our scylla cursor to a user facing crusor
        Ok(ApiCursor::from(scylla_cursor))
    }

    /// Search results in elastic and return a list of sha256s
    ///
    /// # Arguments
    ///
    /// * `user` - The user that is listing samples
    /// * `params` - The query params for searching results
    /// * `shared` - Shared objects in Thorium
    pub async fn search(
        user: &User,
        mut params: ElasticSearchParams,
        shared: &Shared,
    ) -> Result<ApiCursor<ElasticDoc>, ApiError> {
        // authorize the groups to list files from
        user.authorize_groups(&mut params.groups, shared).await?;
        // search for results documents in elastic
        db::results::search(params, shared).await
    }

    /// Get a bundled chunk of the results stream
    ///
    /// Bundling the results stream means that we get the latest result for each
    /// tool + group for each sample in our list
    ///
    /// # Arguments
    ///
    /// * `user` - The user that is listing results
    /// * `kind` - The kind of results to get bundles for
    /// * `params` - The query params for getting bundles of results
    /// * `shared` - Shared Thorium objects
    #[instrument(name = "Output::bundle", skip(kind, user, shared), err(Debug))]
    pub async fn bundle(
        user: &User,
        kind: OutputKind,
        mut params: ResultListParams,
        shared: &Shared,
    ) -> Result<ApiCursor<OutputBundle>, ApiError> {
        // authorize the groups to list results from
        user.authorize_groups(&mut params.groups, shared).await?;
        // get a bundled chunk of the results stream
        db::results::bundle(kind, params, shared).await
    }
}

impl AutoTag {
    /// Update this auto tag settings object
    ///
    /// # Arguments
    ///
    /// * `update` - The updates to apply
    pub fn update(&mut self, mut update: AutoTagUpdate) {
        // update these auto tag settings
        update!(self.logic, update.logic);
        update_opt!(self.key, update.key);
        update_clear!(self.key, update.clear_key);
    }
}

impl OutputCollection {
    /// Update this output collection settings object
    ///
    /// # Arguments
    ///
    /// * `update` - The update to apply
    pub fn update(&mut self, update: OutputCollectionUpdate) {
        update!(self.handler, update.handler);
        update!(self.files.results, update.files.results);
        update!(self.files.result_files, update.files.result_files);
        update!(self.files.tags, update.files.tags);
        update!(self.children, update.children);
        // update the names in the files handler
        self.files
            .names
            .retain(|name| !update.files.remove_names.contains(name));
        self.files.names.extend(update.files.add_names);
        // clear names if requested
        if update.files.clear_names {
            self.files.names = Vec::default();
        }
        // update the groups in the groups restrictions if they were specified
        if !update.groups.is_empty() {
            self.groups = update.groups;
        }
        // clear group restrictions if thats requested
        if update.clear_groups {
            self.groups = Vec::default();
        }
        // crawl over all auto tag updates
        for (key, update) in update.auto_tag {
            // if this auto tag is set to be deleted then delete it and skip to the next update
            if update.delete {
                self.auto_tag.remove(&key);
                continue;
            }
            // if this auto tag setting doesn't exist then create it
            let entry = self.auto_tag.entry(key).or_default();
            // determine if this auto tag setting should be deleted or updated
            entry.update(update);
        }
    }
}

impl From<OutputRow> for OutputChunk {
    /// Convert a [`OutputRow`] to a [`OutputChunk`]
    ///
    /// # Arguments
    ///
    /// * `row` - The row to convert
    fn from(row: OutputRow) -> Self {
        // try to deserialize our string as a json Value
        let (result, deserialization_error) = match serde_json::from_str(&row.result) {
            Ok(value) => (value, None),
            Err(e) => (serde_json::Value::String(row.result), Some(e.to_string())),
        };
        OutputChunk {
            id: row.id,
            cmd: row.cmd,
            tool_version: row.tool_version,
            uploaded: row.uploaded,
            deserialization_error,
            result,
            files: row.files.unwrap_or_default(),
            children: row.children.unwrap_or_default(),
        }
    }
}

impl From<OutputStreamRow> for OutputListLine {
    /// Convert an [`OutputStreamRow`] to an [`OutputListLine`]
    ///
    /// # Arguments
    ///
    /// * `row` - The search row to convert
    fn from(row: OutputStreamRow) -> Self {
        OutputListLine {
            groups: vec![row.group],
            key: row.key,
            tool: row.tool,
            id: row.id,
            uploaded: row.uploaded,
        }
    }
}

// implement cursor for our results stream
#[async_trait::async_trait]
impl CursorCore for OutputListLine {
    /// The params to build this cursor from
    type Params = ResultListParams;

    /// The extra info to filter with
    type ExtraFilters = OutputKind;

    /// The type of data to group our rows by
    type GroupBy = String;

    /// The data structure to store tie info in
    type Ties = HashMap<String, Uuid>;

    /// Get our cursor id from params
    ///
    /// # Arguments
    ///
    /// * `params` - The params to use to build this cursor
    fn get_id(params: &mut Self::Params) -> Option<Uuid> {
        params.cursor.take()
    }

    // Get our start and end timestamps
    ///
    /// # Arguments
    ///
    /// * `params` - The params to use to build this cursor
    fn get_start_end(
        params: &Self::Params,
        shared: &Shared,
    ) -> Result<(DateTime<Utc>, DateTime<Utc>), ApiError> {
        // get our end timestmap
        let end = params.end(shared)?;
        Ok((params.start, end))
    }

    /// Get any group restrictions from our params
    ///
    /// # Arguments
    ///
    /// * `params` - The params to use to build this cursor
    fn get_group_by(params: &mut Self::Params) -> Vec<Self::GroupBy> {
        std::mem::take(&mut params.groups)
    }

    /// Get our extra filters from our params
    ///
    /// # Arguments
    ///
    /// * `params` - The params to use to build this cursor
    fn get_extra_filters(_params: &mut Self::Params) -> Self::ExtraFilters {
        unimplemented!("USE FROM PARAMS EXTRA INSTEAD!")
    }

    /// Get our the max number of rows to return
    ///
    /// # Arguments
    ///
    /// * `params` - The params to use to build this cursor
    fn get_limit(params: &Self::Params) -> usize {
        params.limit
    }

    /// Get the partition size for this cursor
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    fn partition_size(shared: &Shared) -> u16 {
        // get our partition size
        shared.config.thorium.results.partition_size
    }

    /// Add an item to our tie breaker map
    ///
    /// # Arguments
    ///
    /// * `ties` - Our current ties
    fn add_tie(&self, ties: &mut Self::Ties) {
        // if its not already in the tie map then add each of its groups to our map
        for group in &self.groups {
            // if this group doesn't already have a tie entry then add it
            ties.entry(group.clone()).or_insert_with(|| self.id.clone());
        }
    }

    /// Determines if a new item is a duplicate or not
    ///
    /// # Arguments
    ///
    /// * `set` - The current set of deduped data
    fn dedupe_item(&self, dedupe_set: &mut HashSet<String>) -> bool {
        // if this is already in our dedupe set then skip it
        if dedupe_set.contains(&self.key) {
            // we already have this result so skip it
            false
        } else {
            // add this new result to our dedupe set
            dedupe_set.insert(self.key.clone());
            // keep this new result
            true
        }
    }
}

// implement cursor for our results stream
#[async_trait::async_trait]
impl ScyllaCursorSupport for OutputListLine {
    /// The intermediate list row to use
    type IntermediateRow = OutputStreamRow;

    /// The unique key for this cursors row
    type UniqueType<'a> = Uuid;

    /// Get the timestamp from this items intermediate row
    ///
    /// # Arguments
    ///
    /// * `intermediate` - The intermediate row to get a timestamp for
    fn get_intermediate_timestamp(intermediate: &Self::IntermediateRow) -> DateTime<Utc> {
        intermediate.uploaded
    }

    /// Get the timestamp for this item
    ///
    /// # Arguments
    ///
    /// * `item` - The item to get a timestamp for
    fn get_timestamp(&self) -> DateTime<Utc> {
        self.uploaded
    }

    /// Get the unique key for this intermediate row if it exists
    ///
    /// # Arguments
    ///
    /// * `intermediate` - The intermediate row to get a unique key for
    fn get_intermediate_unique_key<'a>(
        intermediate: &'a Self::IntermediateRow,
    ) -> Self::UniqueType<'a> {
        intermediate.id
    }

    /// Get the unique key for this row if it exists
    fn get_unique_key<'a>(&'a self) -> Self::UniqueType<'a> {
        self.id
    }

    /// Add a group to a specific returned line
    ///
    /// # Arguments
    ///
    /// * `group` - The group to add to this line
    fn add_group_to_line(&mut self, group: String) {
        // add this group
        self.groups.push(group);
    }

    /// Add a group to a specific returned line
    fn add_intermediate_to_line(&mut self, intermediate: Self::IntermediateRow) {
        // add this intermediate rows group
        self.groups.push(intermediate.group);
    }

    /// builds the query string for getting data from ties in the last query
    ///
    /// # Arguments
    ///
    /// * `group` - The group that this query is for
    /// * `_filters` - Any filters to apply to this query
    /// * `year` - The year to get data for
    /// * `bucket` - The bucket to get data for
    /// * `uploaded` - The timestamp to get the remaining tied values for
    /// * `breaker` - The value to use as a tie breaker
    /// * `limit` - The max number of rows to return
    /// * `shared` - Shared Thorium objects
    fn ties_query(
        ties: &mut Self::Ties,
        extra: &Self::ExtraFilters,
        year: i32,
        bucket: i32,
        uploaded: DateTime<Utc>,
        limit: i32,
        shared: &Shared,
    ) -> Result<Vec<impl Future<Output = Result<QueryResult, QueryError>>>, ApiError> {
        // allocate space for 300 futures
        let mut futures = Vec::with_capacity(ties.len());
        // if any ties were found then get the rest of them and add them to data
        for (group, id) in ties.drain() {
            // execute our query
            let future = shared.scylla.session.execute_unpaged(
                &shared.scylla.prep.results.list_ties_stream,
                (extra, group, year, bucket, uploaded, id, limit),
            );
            // add this future to our set
            futures.push(future);
        }
        Ok(futures)
    }

    /// builds the query string for getting the next page of values
    ///
    /// # Arguments
    ///
    /// * `group` - The group to restrict our query too
    /// * `_filters` - Any filters to apply to this query
    /// * `year` - The year to get data for
    /// * `bucket` - The bucket to get data for
    /// * `start` - The earliest timestamp to get data from
    /// * `end` - The oldest timestamp to get data from
    /// * `limit` - The max amount of data to get from this query
    /// * `shared` - Shared Thorium objects
    #[allow(clippy::too_many_arguments)]
    async fn pull(
        group: &Self::GroupBy,
        extra: &Self::ExtraFilters,
        year: i32,
        bucket: Vec<i32>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: i32,
        shared: &Shared,
    ) -> Result<QueryResult, QueryError> {
        // execute our query
        shared
            .scylla
            .session
            .execute_unpaged(
                &shared.scylla.prep.results.list_pull_stream,
                (extra, group, year, bucket, start, end, limit),
            )
            .await
    }
}

impl From<OutputListLine> for OutputBundle {
    /// Convert an [`&OutputLineLine`] to an [`OutputBundle`]
    ///
    /// # Arguments
    ///
    /// * `row` - The output list line to convert
    fn from(line: OutputListLine) -> Self {
        OutputBundle {
            sha256: line.key,
            latest: line.uploaded,
            results: HashMap::with_capacity(5),
            map: HashMap::with_capacity(5),
        }
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for ResultGetParams
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // try to extract our query
        if let Some(query) = parts.uri.query() {
            // try to deserialize our query string
            Ok(serde_qs::Config::new(5, false).deserialize_str(query)?)
        } else {
            Ok(Self::default())
        }
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for ResultListParams
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // try to extract our query
        if let Some(query) = parts.uri.query() {
            // try to deserialize our query string
            Ok(serde_qs::Config::new(5, false).deserialize_str(query)?)
        } else {
            Ok(Self::default())
        }
    }
}

impl ResultListParams {
    pub fn end(&self, shared: &Shared) -> Result<DateTime<Utc>, ApiError> {
        match self.end {
            Some(end) => Ok(end),
            None => match Utc.timestamp_opt(shared.config.thorium.results.earliest, 0) {
                chrono::LocalResult::Single(default_end) => Ok(default_end),
                _ => crate::internal_err!(format!(
                    "default earliest results timestamp is invalid or ambigous - {}",
                    shared.config.thorium.results.earliest
                )),
            },
        }
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for ElasticSearchParams
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // try to extract our query
        if let Some(query) = parts.uri.query() {
            // try to deserialize our query string
            Ok(serde_qs::Config::new(5, false).deserialize_str(query)?)
        } else {
            Ok(Self::default())
        }
    }
}

impl ElasticSearchParams {
    pub fn end(&self, shared: &Shared) -> Result<DateTime<Utc>, ApiError> {
        match self.end {
            Some(end) => Ok(end),
            None => match Utc.timestamp_opt(shared.config.thorium.results.earliest, 0) {
                chrono::LocalResult::Single(default_end) => Ok(default_end),
                _ => crate::internal_err!(format!(
                    "default earliest results timestamp is invalid or ambigous - {}",
                    shared.config.thorium.results.earliest
                )),
            },
        }
    }
}

impl OutputKind {
    /// Authorize access to a result
    ///
    /// # Arguments
    ///
    /// * `user` - The user that we are authorizing
    /// * `key` - The key to determine what we are authorizing access too
    /// * `shared` - Shared Thorium objects
    #[instrument(name = "ResultKind::authorize", skip(user, shared), err(Debug))]
    pub async fn authorize(&self, user: &User, key: &str, shared: &Shared) -> Result<(), ApiError> {
        // if we are an admin then short circuit and authorize access
        if user.is_admin() {
            return Ok(());
        }
        // check if this user has access to this file
        match self {
            // authorize access to this file
            OutputKind::Files => Sample::authorize(user, &vec![key.to_owned()], shared).await,
            // authorize access to this repo
            OutputKind::Repos => Repo::authorize(user, &vec![key.to_owned()], shared).await,
        }
    }
}

/// The query params for downloading result files
#[derive(Deserialize, Debug)]
pub struct ResultFileDownloadParams {
    /// The path to the result file to download
    pub result_file: PathBuf,
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for ResultFileDownloadParams
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // try to extract our query
        if let Some(query) = parts.uri.query() {
            // try to deserialize our query string
            Ok(serde_qs::Config::new(5, false).deserialize_str(query)?)
        } else {
            bad!("result file query paramter required but was not given".to_string())
        }
    }
}

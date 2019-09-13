use uuid::Uuid;

use super::Error;
use crate::models::exports::ExportErrorRequest;
use crate::models::{Cursor, Export, ExportError, ExportRequest, ExportUpdate, ResultListOpts};
use crate::{add_date, add_query, send, send_build};

/// A handler for the exports routes in Thorium
#[derive(Clone)]
pub struct Exports {
    /// The host/url that Thorium can be reached at
    host: String,
    /// token to use for auth
    token: String,
    /// A reqwest client for reqwests
    client: reqwest::Client,
}

impl Exports {
    /// Creates a new exports handler
    ///
    /// Instead of directly creating this handler you likely want to simply create a
    /// `thorium::Thorium` and use the handler within that instead.
    ///
    /// # Arguments
    ///
    /// * `host` - url/ip of the Thorium api
    /// * `token` - The token used for authentication
    /// * `client` - The reqwest client to use
    ///
    /// # Examples
    ///
    /// ```
    /// use thorium::client::Exports;
    ///
    /// let client = reqwest::Client::new();
    /// let exports = Exports::new("http://127.0.0.1", "token", &client);
    /// ```
    #[must_use]
    pub fn new(host: &str, token: &str, client: &reqwest::Client) -> Self {
        // build basic route handler
        Exports {
            host: host.to_owned(),
            token: token.to_owned(),
            client: client.clone(),
        }
    }
}

// only inlcude blocking structs if the sync feature is enabled
cfg_if::cfg_if! {
    if #[cfg(feature = "sync")] {
        /// A blocking handler for the exports routes in Thorium
        #[derive(Clone)]
        #[allow(dead_code)]
        pub struct ExportsBlocking {
            /// The host/url that Thorium can be reached at
            host: String,
            /// token to use for auth
            token: String,
            /// A reqwest client for reqwests
            client: reqwest::Client,
        }

        impl ExportsBlocking {
            /// creates a new blocking exports handler
            ///
            /// Instead of directly creating this handler you likely want to simply create a
            /// `thorium::ThoriumBlocking` and use the handler within that instead.
            ///
            ///
            /// # Arguments
            ///
            /// * `host` - url/ip of the Thorium api
            /// * `token` - The token used for authentication
            /// * `client` - The reqwest client to use
            ///
            /// # Examples
            ///
            /// ```
            /// use thorium::client::ExportsBlocking;
            ///
            /// let exports = ExportsBlocking::new("http://127.0.0.1", "token");
            /// ```
            pub fn new(host: &str, token: &str, client: &reqwest::Client) -> Self {
                // build basic route handler
                ExportsBlocking {
                    host: host.to_owned(),
                    token: token.to_owned(),
                    client: client.clone(),
                }
            }
        }
    }
}

#[syncwrap::clone_impl]
impl Exports {
    /// Creates a new exports export operation
    ///
    /// # Arguments
    ///
    /// * `export` - The exports export operation to create
    ///
    /// # Examples
    ///
    /// ```
    /// use thorium::{Thorium, SearchDate};
    /// use thorium::models::ExportRequest;
    /// # use thorium::Error;
    ///
    /// # async fn exec() -> Result<(), Error> {
    /// // create Thorium client
    /// let thorium = Thorium::build("http://127.0.0.1").token("<token>").build().await?;
    /// // Build an export object for any object from 2022 or newer;
    /// let req = ExportRequest::new("ExportsExport", SearchDate::year(2022, false)?);
    /// // Create our exports export operation
    /// thorium.exports.create(&req).await?;
    /// # // allow test code to be compiled but don't unwrap as no API instance would be up
    /// # Ok(())
    /// # }
    /// # tokio_test::block_on(async {
    /// #    exec().await
    /// # });
    /// ```
    pub async fn create(&self, export: &ExportRequest) -> Result<Export, Error> {
        // build the url for listing files
        let url = format!("{}/api/exports/", self.host);
        // build request
        let req = self
            .client
            .post(&url)
            .header("authorization", &self.token)
            .json(export);
        // send this request and build a image from the response
        send_build!(self.client, req, Export)
    }

    /// Gets info about a exports export operation by id
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the exports export operation
    ///
    /// # Examples
    ///
    /// ```
    /// use thorium::Thorium;
    /// use uuid::Uuid;
    /// # use thorium::Error;
    ///
    /// # async fn exec() -> Result<(), Error> {
    /// // create Thorium client
    /// let thorium = Thorium::build("http://127.0.0.1").token("<token>").build().await?;
    /// // Get info on an export operation
    /// thorium.exports.get("SearchStream").await?;
    /// # // allow test code to be compiled but don't unwrap as no API instance would be up
    /// # Ok(())
    /// # }
    /// # tokio_test::block_on(async {
    /// #    exec().await
    /// # });
    /// ```
    pub async fn get(&self, name: &str) -> Result<Export, Error> {
        // build the url for getting info on an export operation
        let url = format!("{}/api/exports/{}", self.host, name);
        // build request
        let req = self.client.get(&url).header("authorization", &self.token);
        // send this request and build a image from the response
        send_build!(self.client, req, Export)
    }

    /// Updates an export operation
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the exports export operation to update
    /// * `update` - The update to apply to this export operation
    ///
    /// # Examples
    ///
    /// ```
    /// use thorium::Thorium;
    /// use thorium::models::ExportUpdate;
    /// use chrono::prelude::*;
    /// use uuid::Uuid;
    /// # use thorium::Error;
    ///
    /// # async fn exec() -> Result<(), Error> {
    /// // create Thorium client
    /// let thorium = Thorium::build("http://127.0.0.1").token("<token>").build().await?;
    /// // Build the update to apply to this export operation
    /// let update = ExportUpdate::new(Utc::now());
    /// // Update our exports export operation
    /// thorium.exports.update("SearchStream", &update).await?;
    /// # // allow test code to be compiled but don't unwrap as no API instance would be up
    /// # Ok(())
    /// # }
    /// # tokio_test::block_on(async {
    /// #    exec().await
    /// # });
    /// ```
    pub async fn update(
        &self,
        name: &str,
        update: &ExportUpdate,
    ) -> Result<reqwest::Response, Error> {
        // build the url for updating an export operation
        let url = format!("{}/api/exports/{}", self.host, name);
        // build request
        let req = self
            .client
            .patch(&url)
            .header("authorization", &self.token)
            .json(update);
        // send this request and build a image from the response
        send!(self.client, req)
    }

    /// Saves an error from an export operation
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the exports export operation to save an error from
    /// * `error` - The error to save
    ///
    /// # Examples
    ///
    /// ```
    /// use thorium::Thorium;
    /// use thorium::models::ExportErrorRequest;
    /// # use thorium::Error;
    /// use uuid::Uuid;
    /// use chrono::prelude::*;
    ///
    /// # async fn exec() -> Result<(), Error> {
    /// // create Thorium client
    /// let thorium = Thorium::build("http://127.0.0.1").token("<token>").build().await?;
    /// // Build our export error
    /// let start = Utc::now();
    /// let end = Utc::now();
    /// let msg = "I am an error message!";
    /// let error = ExportErrorRequest::new(start, end, msg);
    /// // Add this error to our export operation
    /// thorium.exports.add_error("SearchStream", &error).await?;
    /// # // allow test code to be compiled but don't unwrap as no API instance would be up
    /// # Ok(())
    /// # }
    /// # tokio_test::block_on(async {
    /// #    exec().await
    /// # });
    /// ```
    pub async fn add_error(
        &self,
        name: &str,
        error: &ExportErrorRequest,
    ) -> Result<reqwest::Response, Error> {
        // build the url for adding a new export cursor
        let url = format!("{}/api/exports/{}/error", self.host, name);
        // build request
        let req = self
            .client
            .post(&url)
            .header("authorization", &self.token)
            .json(error);
        // send this request
        send!(self.client, req)
    }

    /// Lists the errors for an export operation
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the exports export operation to list errors from
    /// * `opts` - The options for listing export errors
    ///
    /// # Examples
    ///
    /// ```
    /// use thorium::{Thorium, SearchDate};
    /// use thorium::models::ResultListOpts;
    /// use uuid::Uuid;
    /// # use thorium::Error;
    ///
    /// # async fn exec() -> Result<(), Error> {
    /// // create Thorium client
    /// let thorium = Thorium::build("http://127.0.0.1").token("<token>").build().await?;
    /// // build the list opts for all errors from streaming data from 2020
    /// let search = ResultListOpts::default()
    ///     .start(SearchDate::year(2020, false)?)
    ///     .end(SearchDate::year(2020, true)?)
    ///     // limit it to 100 files
    ///     .limit(100);
    /// // list up to 100 errors from 2020
    /// thorium.exports.list_errors("SearchStream", search).await?;
    /// # // allow test code to be compiled but don't unwrap as no API instance would be up
    /// # Ok(())
    /// # }
    /// # tokio_test::block_on(async {
    /// #    exec().await
    /// # });
    /// ```
    pub async fn list_errors(
        &self,
        name: &str,
        opts: ResultListOpts,
    ) -> Result<Cursor<ExportError>, Error> {
        // build the url for adding a new export cursor
        let url = format!("{}/api/exports/{}/error", self.host, name);
        // get the correct page size if our limit is smaller then our page_size
        let page_size = opts.limit.map_or_else(
            || opts.page_size,
            |limit| std::cmp::min(opts.page_size, limit),
        );
        // build our query params
        let mut query = vec![("limit", page_size.to_string())];
        add_date!(query, "start", opts.start);
        add_date!(query, "end", opts.end);
        add_query!(query, "cursor", opts.cursor);
        // get the data for this request and create our cursor
        Cursor::new(
            &url,
            opts.page_size,
            opts.limit,
            &self.token,
            &query,
            &self.client,
        )
        .await
    }

    /// Deletes an error from an export operation
    ///
    /// # Arguments
    ///
    /// * `export_name` - The name of the exports export operation to delete an error from
    /// * `error_id` - The id of the error to delete
    ///
    /// # Examples
    ///
    /// ```
    /// use thorium::Thorium;
    /// # use thorium::Error;
    /// use uuid::Uuid;
    /// use chrono::prelude::*;
    ///
    /// # async fn exec() -> Result<(), Error> {
    /// // create Thorium client
    /// let thorium = Thorium::build("http://127.0.0.1").token("<token>").build().await?;
    /// // Have an ongoing export cursor id and an error id
    /// let error_id = Uuid::new_v4();
    /// // Delete this export error
    /// thorium.exports.delete_error("SearchStream", &error_id).await?;
    /// # // allow test code to be compiled but don't unwrap as no API instance would be up
    /// # Ok(())
    /// # }
    /// # tokio_test::block_on(async {
    /// #    exec().await
    /// # });
    /// ```
    pub async fn delete_error(
        &self,
        export_name: &str,
        error_id: &Uuid,
    ) -> Result<reqwest::Response, Error> {
        // build the url for delete an export cursor
        let url = format!(
            "{}/api/exports/{}/error/{}",
            self.host, export_name, error_id
        );
        // build request
        let req = self
            .client
            .delete(&url)
            .header("authorization", &self.token);
        // send this request
        send!(self.client, req)
    }
}

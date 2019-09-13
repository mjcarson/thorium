//! A worker that streams data to a search store

use std::marker::PhantomData;

use chrono::prelude::*;
use kanal::{AsyncReceiver, AsyncSender};
use serde_json::Value;
use thorium::models::{ExportError, ExportErrorRequest};
use thorium::{Conf, Error, Thorium};
use tracing::{event, instrument, Level};

use crate::msg::{Msg, Response};
use crate::streamer::{DataSource, SearchStore};

pub struct StreamWorker<D: DataSource, S: SearchStore> {
    /// A client for Thorium
    thorium: Thorium,
    /// The channel to pull jobs from
    jobs_rx: AsyncReceiver<Msg>,
    /// The channel to send progress updates on
    progress_rx: AsyncSender<Response>,
    /// A list of documents to send to our search store
    docs: Vec<Value>,
    /// The source of data to index
    source: PhantomData<D>,
    /// The store for data
    store: S,
    /// Our export cursors name
    export: String,
}

impl<D: DataSource, S: SearchStore> StreamWorker<D, S> {
    /// Create a new stream worker
    ///
    /// # Arguments
    ///
    /// * `conf` - The Thorium conf to use
    /// * `thorium` - A client for the Thorium API
    /// * `jobs_rx` - The channel to poll for jobs
    /// * `progress_rx` - The channel to send progress updates over
    /// * `export` - The export we are streaming
    pub fn new(
        conf: &Conf,
        thorium: &Thorium,
        jobs_rx: &AsyncReceiver<Msg>,
        progress_rx: &AsyncSender<Response>,
        export: &str,
    ) -> Self {
        // get our index
        let index = D::index(conf);
        // build our store client
        let store = S::new(conf, index).unwrap();
        Self {
            thorium: thorium.clone(),
            jobs_rx: jobs_rx.clone(),
            progress_rx: progress_rx.clone(),
            docs: Vec::with_capacity(50),
            source: PhantomData,
            store,
            export: export.to_owned(),
        }
    }

    /// Build a cursor and stream its data to our search store
    ///
    /// # Arguments
    ///
    /// * `watermark` - This chunks watermark
    /// * `start` - The start of the chunk to stream to our search store
    /// * `end` - The end of the chunk to stream to our search store
    #[instrument(name = "StreamWorker::handle_chunk_helper", skip(self), err(Debug))]
    async fn handle_chunk_helper(
        &mut self,
        watermark: u64,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), Error> {
        // get our cursor
        let mut cursor = D::build_cursor(&self.thorium, start, end).await?;
        // track how many objects were streamed in total
        let mut total = 0;
        // crawl this cursor until its exhausted
        loop {
            // stream data if we have any to stream to our store
            if !cursor.data.is_empty() {
                // get the current timestamp
                let now = Utc::now();
                // crawl over the results in this cursor
                for data in &cursor.data {
                    // add this to our doc list
                    D::to_value(data, &mut self.docs, now)?;
                }
                // send this data to our search store
                self.store.send(&mut self.docs).await?;
                // increment our total counter
                total += cursor.data.len();
                // log our progress
                event!(
                    Level::INFO,
                    watermark = watermark,
                    streamed = cursor.data.len()
                );
            }
            // check if this cursor has been exhausted
            if cursor.exhausted() {
                // this cursor is out of data so break
                break;
            }
            // get the next page of data for our cursor
            cursor.refill().await?;
        }
        // log our progress and that this is the last part of this chunk
        event!(
            Level::INFO,
            watermark = watermark,
            end = true,
            total = total
        );
        Ok(())
    }

    /// Stream this chunks data to our search store
    ///
    /// # Arguments
    ///
    /// * `watermark` - This chunks watermark
    /// * `start` - The start of the chunk to stream to our search store
    /// * `end` - The end of the chunk to stream to our search store
    ///
    /// # Panics
    ///
    /// This will panic if we fail to add an error to thorium or push a message onto our progress channel
    #[instrument(name = "StreamWorker::handle_chunk", skip(self))]
    async fn handle_chunk(&mut self, watermark: u64, start: DateTime<Utc>, end: DateTime<Utc>) {
        // stream this chunk of results
        if let Err(error) = self.handle_chunk_helper(watermark, start, end).await {
            // log this error
            event!(Level::ERROR, error = error.to_string());
            // build the error to save
            let mut error_req = ExportErrorRequest::new(start, end, error.to_string());
            // if an error code was set then also add that
            if let Some(code) = error.status() {
                error_req.code_mut(code.as_u16());
            }
            // set this section of the data to be retried later
            self.thorium
                .exports
                .add_error(&self.export, &error_req)
                .await
                .unwrap();
        }
        // tell the controller this chunk was completed regardless of it it worked or not
        self.progress_rx
            .send(Response::Completed { watermark, start })
            .await
            .unwrap();
    }

    /// Retry an errored section of the stream
    ///
    /// # Arguments
    ///
    /// * `error` - The export error to retry
    #[instrument(name = "StreamWorker::retry_error_helper", skip(self), err(Debug))]
    async fn retry_error_helper(&mut self, error: ExportError) -> Result<(), Error> {
        // build the cursor for this export error
        let mut cursor = D::build_cursor(&self.thorium, error.start, error.end).await?;
        // track how many objects were streamed in total
        let mut total = 0;
        // crawl this cursor until its exhausted
        loop {
            // stream data if we have any to stream to our store
            if !cursor.data.is_empty() {
                // get the current timestamp
                let now = Utc::now();
                // crawl over the results in this cursor
                for data in &cursor.data {
                    // add this to our doc list
                    D::to_value(data, &mut self.docs, now)?;
                }
                // send this data to our search store
                self.store.send(&mut self.docs).await?;
                // increment our total counter
                total += cursor.data.len();
                // log our progress
                event!(Level::INFO, total = cursor.data.len());
                println!(
                    "RETRY: {} -> {} -> {}",
                    error.start,
                    error.end,
                    cursor.data.len()
                );
            }
            // check if this cursor has been exhausted
            if cursor.exhausted() {
                // this cursor is out of data so break
                break;
            }
            // get the next page of data for our cursor
            cursor.refill().await?;
        }
        // this section was completed so clear this error
        self.thorium
            .exports
            .delete_error(&self.export, &error.id)
            .await?;
        // tell the controller this chunk was completed
        self.progress_rx.send(Response::Fixed(error.id)).await?;
        // log our progress
        event!(Level::INFO, total = total);
        Ok(())
    }

    /// Retry an errored section of the stream
    ///
    /// # Arguments
    ///
    /// * `error` - The export error to retry
    #[instrument(name = "StreamWorker::retry_error", skip(self))]
    async fn retry_error(&mut self, error: ExportError) {
        // get a copy of our start and end
        let start = error.start;
        let end = error.end;
        // retry this stream
        if let Err(error) = self.retry_error_helper(error).await {
            // log this error
            println!("{end} -> {start}: {:#?}", error);
            event!(Level::ERROR, error = error.to_string());
        }
    }

    /// Poll our job queue and stream data to our search store
    pub async fn start(mut self) -> Result<(), Error> {
        // pop messages in our queue until it closes
        while let Ok(msg) = self.jobs_rx.recv().await {
            // handle this message
            match msg {
                Msg::New {
                    watermark,
                    start,
                    end,
                } => self.handle_chunk(watermark, start, end).await,
                Msg::Retry(error) => self.retry_error(error).await,
            }
        }
        Ok(())
    }
}

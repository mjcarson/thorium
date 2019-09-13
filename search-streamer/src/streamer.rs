//! Stream data to our search store

use chrono::prelude::*;
use futures::stream::{FuturesUnordered, StreamExt};
use kanal::{AsyncReceiver, AsyncSender};
use std::marker::PhantomData;
use thorium::{
    models::{Export, ExportRequest},
    Conf, Error, Thorium,
};
use tokio::task::JoinHandle;

use crate::msg::{Msg, Response};
pub(crate) use crate::sources::DataSource;
pub use crate::sources::SamplesOutput;
pub use crate::stores::Elastic;
pub(crate) use crate::stores::SearchStore;
use crate::worker::StreamWorker;
use crate::{args::Args, monitor::Monitor};

/// Get an existing export or create a new one with this name
///
/// # Arguments
///
/// * `thorium` - A Thorium client
/// * `args` - The command line args that were passed in
/// * `current` - The current timestamp to set for new exports
async fn get_export(
    thorium: &Thorium,
    args: &Args,
    current: DateTime<Utc>,
) -> Result<Export, Error> {
    // try to get an export with this name
    match thorium.exports.get(&args.name).await {
        // we got an existing export so start using that
        Ok(export) => Ok(export),
        // check if this is a 404 error
        Err(error) => {
            // try to get a status code for this error
            match error.status() {
                // this error has a status code
                Some(code) => {
                    // if this is a 404 then create a new export
                    if code.as_u16() == 404 {
                        // build our new export request
                        let export_req = ExportRequest::new(&args.name, current);
                        // get or create a new export
                        thorium.exports.create(&export_req).await
                    } else {
                        Err(error)
                    }
                }
                None => Err(error),
            }
        }
    }
}

/// Stream data from Thorium to a full text search store
pub struct SearchStreamer<D: DataSource, S: SearchStore> {
    /// The command line args passed in
    args: Args,
    /// The thorium config for this cluster
    conf: Conf,
    /// A Thorium client
    thorium: Thorium,
    /// The transmit side of the queue of time chunks to stream
    jobs_tx: AsyncSender<Msg>,
    /// The receive side of the queue of time chunks to stream
    jobs_rx: AsyncReceiver<Msg>,
    /// The channel to send progress updates on
    progress_tx: AsyncSender<Response>,
    /// The channel to listen for progress updates on
    progress_rx: AsyncReceiver<Response>,
    /// Our exports info
    export: Export,
    /// The source of our data
    data_source: PhantomData<D>,
    /// The search store to stream data too,
    search_store: PhantomData<S>,
}

impl<D: DataSource, S: SearchStore> SearchStreamer<D, S> {
    /// Create a new search streamer
    ///
    /// # Arguments
    ///
    /// * `args` - The command line args for our search streamer
    /// * `conf` - The Thorium config
    pub async fn new(args: &Args, conf: Conf) -> Result<Self, Error> {
        // get a Thorium client
        let thorium = Thorium::from_key_file(&args.keys).await?;
        // get the earliest to stream data at
        let current = D::earliest(&conf);
        // build our job queue channels
        let (jobs_tx, jobs_rx) = kanal::bounded_async(1000);
        // build our progress queue channels
        let (progress_tx, progress_rx) = kanal::bounded_async(10000);
        // get or create an export
        let export = get_export(&thorium, args, current).await?;
        // build our search streamer
        let streamer = SearchStreamer {
            args: args.clone(),
            conf,
            thorium,
            jobs_tx,
            jobs_rx,
            progress_tx,
            progress_rx,
            export,
            data_source: PhantomData,
            search_store: PhantomData,
        };
        Ok(streamer)
    }

    /// Spawn this streamers monitor
    ///
    /// # Arguments
    ///
    /// * `futures` - A set of futures to track
    fn spawn_monitor(&self, futures: &mut FuturesUnordered<JoinHandle<Result<(), Error>>>) {
        // build our monitor
        let monitor = Monitor::new(
            &self.thorium,
            &self.export,
            &self.jobs_tx,
            &self.progress_rx,
        );
        // spawn our monitor
        futures.push(tokio::spawn(monitor.start()));
    }

    /// Spawn this streamers workers
    ///
    /// # Arguments
    ///
    /// * `futures` - A set of futures to track
    fn spawn_workers(&mut self, futures: &mut FuturesUnordered<JoinHandle<Result<(), Error>>>) {
        // create and spawn this streamers workers
        for _ in 0..self.args.workers {
            // create a new worker
            let worker: StreamWorker<D, S> = StreamWorker::new(
                &self.conf,
                &self.thorium,
                &self.jobs_rx,
                &self.progress_tx,
                &self.export.name,
            );
            // spawn this worker
            futures.push(tokio::spawn(worker.start()));
        }
    }

    /// Spawn our feeder
    fn spawn_feeder(&mut self, futures: &mut FuturesUnordered<JoinHandle<Result<(), Error>>>) {
        // spawn our feeder
        let handle = tokio::spawn(feed_queue(self.export.current, self.jobs_tx.clone()));
        // add this future to our futures set
        futures.push(handle);
    }

    /// Start streaming data to our search store
    pub async fn start(mut self) -> Result<(), Error> {
        // keep a set of all of our futures
        let mut futures = FuturesUnordered::default();
        // spawn our workers
        self.spawn_workers(&mut futures);
        // spawn our monitor
        self.spawn_monitor(&mut futures);
        // spawn our feeder
        self.spawn_feeder(&mut futures);
        // wait for all of our futures to complete
        while let Some(result) = futures.next().await {
            // check if any of our futures failed
            result??;
        }
        Ok(())
    }
}

/// Feed new chunks to stream our forever
///
/// # Arguments
///
/// * `current` - The timestamp to start streaming at
/// * `jobs_tx` - The channel to push jobs into
async fn feed_queue(mut current: DateTime<Utc>, jobs_tx: AsyncSender<Msg>) -> Result<(), Error> {
    // track the proposed watermark
    let mut watermark = 1;
    loop {
        // get the current timestamp
        let now = Utc::now();
        // check if we are within 10 minutes of now
        if current + chrono::Duration::minutes(10) > now {
            // we are within 10 minutes of now so just add that to our job queue
            jobs_tx
                .send(Msg::New {
                    watermark,
                    start: now,
                    end: current,
                })
                .await?;
            // increment our current timestamp
            current = now;
            // sleep for 30 seconds
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        } else {
            // we are not within 10 minutes of now so scan in 10 minute chunks
            let start = current + chrono::Duration::minutes(10);
            // add this chunk to our job queue
            jobs_tx
                .send(Msg::New {
                    watermark,
                    start,
                    end: current,
                })
                .await?;
            // increment our current timestamp
            current = start;
        }
        // increment our watermark
        watermark += 1;
    }
}

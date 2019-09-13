//! Monitor our exports for errors and push them into the queue

use chrono::prelude::*;
use chrono::Duration;
use kanal::{AsyncReceiver, AsyncSender};
use std::collections::{BTreeMap, HashSet};
use thorium::models::{Export, ExportUpdate, ResultListOpts};
use thorium::{Error, Thorium};
use uuid::Uuid;

use crate::msg::{Msg, Response};

pub struct Monitor {
    /// A client for Thorium
    thorium: Thorium,
    /// The export to monitor
    export: Export,
    /// The channel to push jobs into
    jobs_tx: AsyncSender<Msg>,
    /// The channel to listen for worker progress updates on
    progress_rx: AsyncReceiver<Response>,
    /// The current watermark
    watermark: u64,
    /// The pending watermarks
    pending_marks: BTreeMap<u64, DateTime<Utc>>,
    /// The currently pending retries
    pending_retries: HashSet<Uuid>,
    /// Keep track of the last time we updated our watermark in scylla
    last_updated: DateTime<Utc>,
}

impl Monitor {
    /// Create a new monitor
    ///
    /// # Arguments
    ///
    /// * `thorium` - A Thorium client
    /// * `export` - The export we are monitoring
    /// * `jobs_tx` - The channel to send new jobs on
    /// * `progress_rx` - The channel to listen for progress on
    pub fn new(
        thorium: &Thorium,
        export: &Export,
        jobs_tx: &AsyncSender<Msg>,
        progress_rx: &AsyncReceiver<Response>,
    ) -> Self {
        Monitor {
            thorium: thorium.clone(),
            export: export.clone(),
            jobs_tx: jobs_tx.clone(),
            progress_rx: progress_rx.clone(),
            watermark: 0,
            pending_marks: BTreeMap::default(),
            pending_retries: HashSet::default(),
            last_updated: Utc::now(),
        }
    }

    /// Check if theres any new errors to retry
    async fn check_errors(&mut self) -> Result<(), Error> {
        // build our result list opts
        let opts = ResultListOpts::default();
        // check if there is any export errors to retry
        let mut cursor = self
            .thorium
            .exports
            .list_errors(&self.export.name, opts)
            .await?;
        // loop over our cursor
        loop {
            // crawl over the errors in this cursor
            for error in cursor.data.drain(..) {
                // add this retry job to our queue
                self.jobs_tx.send(Msg::Retry(error.clone())).await?;
                // add this error to our current pending retries
                self.pending_retries.insert(error.id);
            }
            // if our cursor exhausted then break out
            if cursor.exhausted() {
                break;
            }
            // get the next page of errors
            cursor.refill().await?;
        }
        Ok(())
    }

    /// Handle a watermark update
    async fn check_watermark(&mut self) {
        // get the first watermark entry
        if let Some((watermark, mut start)) = self.pending_marks.pop_first() {
            // check if this is the next watermark
            if self.watermark == watermark - 1 {
                // increment our watermark
                self.watermark += 1;
                // crawl our pending watermark until we have a gap in marks
                for (mark, timestamp) in &self.pending_marks {
                    // if this watermark is the next one then update our watermark
                    if self.watermark == mark - 1 {
                        // increment our watermark
                        self.watermark += 1;
                        // update our start time
                        start = *timestamp;
                    } else {
                        // there is a gap in watermarks
                        break;
                    }
                }
                // drop any pending marks lower then our new watermark
                self.pending_marks.retain(|mark, _| *mark > self.watermark);
                // build our export update
                let update = ExportUpdate::new(start);
                // update our export
                if let Err(error) = self
                    .thorium
                    .exports
                    .update(&self.export.name, &update)
                    .await
                {
                    // log that we failed to update our export
                    println!("Failed to update export: {error:#?}");
                }
            } else {
                // add the first mark back in
                self.pending_marks.insert(watermark, start);
            }
        }
    }

    /// Monitor our export for any errors and send them to workers
    pub async fn start(mut self) -> Result<(), Error> {
        loop {
            // check if we have any new errors to retry
            self.check_errors().await?;
            // track how many iterations it has been since we checked watermarks
            let mut since_check = 0;
            // handle any responses
            while let Some(resp) = self.progress_rx.try_recv()? {
                // handle our message
                match resp {
                    Response::Completed { watermark, start } => {
                        self.pending_marks.insert(watermark, start);
                    }
                    Response::Fixed(error_id) => {
                        self.pending_retries.remove(&error_id);
                    }
                }
                // increment our counter
                since_check += 1;
                // if there has been more then 5000 iterations since our check then break out
                if since_check > 5000 {
                    break;
                }
            }
            // check our watermarks every minute
            if self.last_updated + Duration::seconds(60) < Utc::now() {
                // check our watermarks and update them in scylla
                self.check_watermark().await;
            }
            // sleep for 1 second if we emptied our queue
            if self.progress_rx.is_empty() {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

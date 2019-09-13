//! A backup scrubbing worker for Thoradm
use data_encoding::HEXLOWER;
use indicatif::ProgressBar;
use kanal::{AsyncReceiver, AsyncSender};
use sha2::{Digest, Sha256};
use std::marker::PhantomData;
use std::path::PathBuf;

use super::{ArchiveReader, MonitorUpdate, Utils};
use crate::Error;

/// The worker that handles scrubbing data in Thorium
pub struct ScrubWorker<S: Scrub> {
    /// The sha256 hasher to use
    hasher: Sha256,
    /// The type we are scrubbing
    phantom: PhantomData<S>,
    /// The kanal channel workers should send scrub updates over
    updates: AsyncSender<MonitorUpdate>,
    /// The progress bar to track progress with
    progress: ProgressBar,
}

impl<S: Scrub> ScrubWorker<S> {
    /// Create a new ScrubWorker
    ///
    /// # Arguments
    ///
    /// * `updates` - The kanal channel to send scrub updates over
    /// * `progress` - The progress bar to track progress with
    pub fn new(updates: AsyncSender<MonitorUpdate>, progress: ProgressBar) -> Self {
        ScrubWorker {
            hasher: Sha256::new(),
            phantom: PhantomData::default(),
            updates,
            progress,
        }
    }

    /// Scrub an archive for bitrot
    ///
    /// # Arguments
    ///
    /// * `orders` - The channel to receive map paths on
    pub async fn scrub(mut self, orders: AsyncReceiver<PathBuf>) -> Result<Self, Error> {
        // handle messages in our channel until its closed
        loop {
            // get the next message in the queue
            let map_path = match orders.recv().await {
                Ok(path) => path,
                Err(kanal::ReceiveError::Closed) => break,
                Err(kanal::ReceiveError::SendClosed) => break,
            };
            // build our reader for this archive
            let mut reader = ArchiveReader::new(map_path).await?;
            // crawl over the data for our partitions and scrub them
            while let Some((scrub_slice, partition)) = reader.next_partition().await? {
                // hash this partitions data
                self.hasher.update(scrub_slice);
                // finalize our hash and cast it to a string
                let sha256 = HEXLOWER.encode(&self.hasher.finalize_reset());
                // set our new writer position
                self.progress.inc(scrub_slice.len() as u64);
                // log any scrub failures
                if sha256 != partition.sha256 {
                    // log that a partition failed a scrub
                    self.progress.println(format!(
                        "Failed scrub for partition {}",
                        partition.partition
                    ));
                }
                // build our monitor update
                let update = MonitorUpdate::Update {
                    items: 1,
                    bytes: scrub_slice.len() as u64,
                };
                // send our update to the global tracker
                self.updates.send(update).await?;
            }
        }
        Ok(self)
    }

    /// Shutdown this worker
    pub fn shutdown(self) {
        // shutdown our progress bar
        self.progress.finish();
    }
}

pub trait Scrub: Utils + std::fmt::Debug + 'static + Send {}

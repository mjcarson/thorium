//! The global monitors for backup/restore capabilities in thoradm

use indicatif::ProgressBar;
use kanal::AsyncReceiver;
use num_format::{Locale, ToFormattedString};

/// An update for our table backup monitor
pub enum MonitorUpdate {
    /// Update our monitors status
    #[allow(dead_code)]
    Update {
        items: usize,
        bytes: u64,
    },
    Finished,
}

pub struct Monitor {
    /// The progress bar to report on
    progress: ProgressBar,
    /// The receiver to get updates on
    receiver: AsyncReceiver<MonitorUpdate>,
}

impl Monitor {
    /// Create a new table monitor
    ///
    /// # Arguments
    ///
    /// * `progress` - The progress bar to report status with
    /// * `receiver` - The channel to get status updates on
    pub fn new(progress: ProgressBar, receiver: AsyncReceiver<MonitorUpdate>) -> Self {
        Monitor { progress, receiver }
    }

    /// Start monitor our channel for updates
    pub async fn start(self) {
        // track the number of items we have backed up/restored
        let mut total_items = 0;
        // track the number of updates since our last message update
        let mut since_msg_update = 0;
        // handle messages in our channel until its closed
        loop {
            // get the next message in the queue
            let msg = match self.receiver.recv().await {
                Ok(msg) => msg,
                Err(kanal::ReceiveError::Closed) => break,
                Err(kanal::ReceiveError::SendClosed) => break,
            };
            // handle the different message types
            match msg {
                // we have an update to our progress bar
                MonitorUpdate::Update { items, bytes } => {
                    // update our progress bar
                    self.progress.inc(bytes);
                    // update the number of rows we have backed up/restored
                    total_items += items;
                }
                // this monitor is finished and should exit
                MonitorUpdate::Finished => break,
            }
            // update our message if its been a while since we did or we have more no more updates
            if since_msg_update >= 100 || self.receiver.is_empty() {
                // set our new message
                self.progress
                    .set_message(total_items.to_formatted_string(&Locale::en));
                // reset our messages since message update counter
                since_msg_update = 0;
            } else {
                // increment our messages since update counter
                since_msg_update += 1;
            }
        }
        // set our new message before exiting
        self.progress
            .set_message(total_items.to_formatted_string(&Locale::en));
        // finish this bar
        self.progress.finish();
    }
}

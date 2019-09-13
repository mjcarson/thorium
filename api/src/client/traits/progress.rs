//! Add progress bar support to various opts structs

use crate::models::{FileDownloadOpts, RepoDownloadOpts};

use super::TransferProgress;

impl TransferProgress for RepoDownloadOpts {
    fn update_progress_bytes(&self, transferred: &[u8]) {
        // get our progress bar if we have one
        if let Some(bar) = &self.progress {
            bar.inc(transferred.len() as u64);
        }
    }
}

impl TransferProgress for FileDownloadOpts {
    fn update_progress_bytes(&self, transferred: &[u8]) {
        // get our progress bar if we have one
        if let Some(bar) = &self.progress {
            bar.inc(transferred.len() as u64);
        }
    }
}

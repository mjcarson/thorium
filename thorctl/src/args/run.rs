//! Arguments for run-related Thorctl commands

use clap::Parser;
use std::path::PathBuf;

/// The commands to send to the repos task handler
#[derive(Parser, Debug)]
pub struct Run {
    /// The pipeline to run
    pub pipeline: String,
    /// The sample SHA256 or repo to run the pipeline on
    pub sha256_or_repo: String,
    /// The group that the pipeline is in (required if a pipeline with the same
    /// name exists in another group)
    #[clap(long)]
    pub group: Option<String>,
    /// How many seconds allowed to wait before starting the job
    #[clap(short, long, default_value_t = 1)]
    pub sla: u64,
    /// The path to save the results to [default: `<SHA256/REPO>_<PIPELINE>`]
    #[clap(short, long)]
    pub output: Option<PathBuf>,
}

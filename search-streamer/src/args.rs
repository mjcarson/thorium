//! The command line args for the search streamer

use clap::Parser;

/// The command line args for the search streamer
#[derive(Parser, Debug, Clone)]
#[clap(version, author)]
pub struct Args {
    /// The base name of our search stream exports
    #[clap(short, long, default_value = "results")]
    pub name: String,
    /// The path to the thorium config
    #[clap(long, default_value = "thorium.yml")]
    pub config: String,
    /// The path to the single user auth keys to use in place of the thorctl config
    #[clap(short, long, default_value = "keys.yml")]
    pub keys: String,
    /// The number of parallel async actions to process at once
    #[clap(short, long, default_value = "10")]
    pub workers: usize,
}

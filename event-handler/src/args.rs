use clap::Parser;

/// The Command line args to pass to the event handler
#[derive(Parser, Debug, Clone)]
#[clap(version, author)]
pub struct Args {
    /// The path to load the config file from
    #[clap(short, long, default_value = "thorium.yml")]
    pub config: String,
    /// The path to load the auth keys for Thorium from
    #[clap(short, long, default_value = "keys.yml")]
    pub auth: String,
}

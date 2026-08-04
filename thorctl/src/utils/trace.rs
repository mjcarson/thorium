//! Sets up logging for Thorctl
//!
//! Unlike the agent, the scaler, and the reactor, Thorctl has no tracing config file and no
//! trace exporter. It is a CLI, so the only thing it can usefully do with a span is print it,
//! and the only knob anyone needs is how much of it to print. Logging is off by default and
//! everything it does print goes to stderr so it never gets mixed into the tables and paths
//! Thorctl writes to stdout.

use clap::ValueEnum;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, fmt};

use crate::Args;

/// How much logging Thorctl should print to stderr
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevel {
    /// Don't print any logs at all
    #[default]
    Off,
    /// Only print errors
    Error,
    /// Print errors and warnings
    Warn,
    /// Print the progress of the requests Thorctl is making
    Info,
    /// Print the details of the requests Thorctl is making
    Debug,
    /// Print everything
    Trace,
}

impl LogLevel {
    /// Convert this log level to the filter the subscriber wants
    fn to_filter(self) -> LevelFilter {
        match self {
            LogLevel::Off => LevelFilter::OFF,
            LogLevel::Error => LevelFilter::ERROR,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Trace => LevelFilter::TRACE,
        }
    }
}

/// Setup logging for Thorctl if the user asked for any
///
/// No subscriber is installed at all when logging is off, which keeps Thorctl's default
/// behavior byte for byte identical to what it was before logging existed. The client only
/// spawns its upload watchdogs when something is listening, so this also decides whether
/// those get spawned.
///
/// # Arguments
///
/// * `args` - The arguments passed to Thorctl
pub fn setup(args: &Args) {
    // don't install a subscriber at all if the user didn't ask for logs
    if args.log_level == LogLevel::Off {
        return;
    }
    // log to stderr so our logs never get mixed into the output we print to stdout
    let layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_filter(args.log_level.to_filter());
    // install our subscriber for the rest of this run
    tracing_subscriber::registry().with(layer).init();
}

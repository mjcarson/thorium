//! Handles the launching of jobs for specific reactor types
//!
//! Currently only windows is supported;

use std::collections::{HashMap, HashSet};
use thorium::models::{Node, Worker};
use thorium::{Error, Thorium};
use tracing::Span;

#[cfg(target_os = "linux")]
mod bare_metal;
//#[cfg(feature = "kvm")]
#[cfg(target_os = "linux")]
#[cfg(feature = "kvm")]
mod kvm;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use bare_metal::BareMetal;
//#[cfg(feature = "kvm")]
#[cfg(feature = "kvm")]
#[cfg(target_os = "linux")]
use kvm::Kvm;
#[cfg(target_os = "windows")]
use windows::Windows;

use crate::args::{Args, Launchers};

#[async_trait::async_trait]
pub trait Launcher {
    /// Spawn a worker and then return a process id that can be used to track it
    ///
    /// # Arguments
    ///
    /// * `thorium` - A Thorium client
    /// * `worker` - The worker to launch
    /// * `span` - The span to log traces under
    async fn launch(
        &mut self,
        thorium: &Thorium,
        worker: &Worker,
        span: &Span,
    ) -> Result<(), Error>;

    /// Check if any of our current workers have completed or died
    ///
    /// # Arguments
    ///
    /// * `thorium` - A Thorium client
    /// * `info` - Info about our node and its workers
    /// * `active` - The names of the currently active workers in the reactor
    /// * `span` - The span to log traces under
    async fn check(
        &mut self,
        thorium: &Thorium,
        info: &mut Node,
        active: &mut HashMap<String, Worker>,
        span: &Span,
    ) -> Result<(), Error>;

    /// Shutdown a list of workers
    ///
    /// # Arguments
    ///
    /// * `thorium` - A Thorium client
    /// * `workers` - The workers to shutdown
    /// * `span` - The span to log traces under
    async fn shutdown(
        &mut self,
        thorium: &Thorium,
        mut workers: HashSet<String>,
        span: &Span,
    ) -> Result<(), Error>;
}

/// Creates a new instance of the launcher
pub fn new(args: &Args) -> Box<dyn Launcher> {
    match &args.launchers {
        #[cfg(target_os = "linux")]
        Launchers::BareMetal => {
            // get our node name
            let node = args.node().expect("Failed to get node name");
            Box::new(BareMetal::new(&args.cluster, node))
        }
        #[cfg(target_os = "windows")]
        Launchers::Windows => Box::new(Windows::default()),
        #[cfg(feature = "kvm")]
        #[cfg(target_os = "linux")]
        Launchers::Kvm(kvm) => {
            // build our kvm launcher
            let kvm = Kvm::new(kvm).expect("Failed to build kvm launcher");
            // box and return our kvm launcher
            Box::new(kvm)
        }
    }
}

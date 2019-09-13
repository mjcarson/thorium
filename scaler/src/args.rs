use clap::Parser;
use thorium::models::ImageScaler;

/// The Command line args to pass to the scaler
#[derive(Parser, Debug, Clone)]
#[clap(version, author)]
pub struct Args {
    /// The path to load the config file from
    #[clap(short, long, default_value = "thorium.yml")]
    pub config: String,
    /// The path to load the auth keys for Thorium from
    #[clap(short, long, default_value = "keys.yml")]
    pub auth: String,
    /// The target to schedule jobs on (K8s or BareMetal)
    #[clap(short, long, default_value_t, ignore_case = true)]
    pub scaler: ImageScaler,
    /// Don't actually schedule workers
    #[clap(long, default_value_t)]
    pub dry_run: bool,
    /// The context name to use when not loading kube config for k8s
    #[clap(long, default_value = "kubernetes-admin@cluster.local")]
    pub context_name: String,
}

//! Handles groups commands
use thorium::{Error, Thorium};

use crate::args::groups::{DescribeGroups, GetGroups, Groups};
use crate::args::{Args, DescribeCommand};
use crate::utils;

/// Get and print a list of groups to which the user belongs
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The [`GetGroups`] command that was run
async fn get(thorium: Thorium, cmd: &GetGroups) -> Result<(), Error> {
    // get the current user's groups
    let mut groups = thorium.users.info().await?.groups;
    if cmd.alpha {
        // alphabetize if the flag was set
        groups.sort_unstable();
    }
    for group in &groups {
        println!("{group}");
    }
    Ok(())
}

/// Describe groups by displaying/saving all of their JSON-formatted details
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The [`DescribeGroups`] command to run
async fn describe(thorium: Thorium, cmd: &DescribeGroups) -> Result<(), Error> {
    cmd.describe(&thorium).await
}

/// Handle all groups commands or print groups docs
///
/// # Arguments
///
/// * `args` - The arguments passed to Thorctl
/// * `cmd` - The groups command to execute
pub async fn handle(args: &Args, cmd: &Groups) -> Result<(), Error> {
    // load our config and instance our client
    let (conf, thorium) = utils::get_client(args).await?;
    // warn about insecure connections if not set to skip
    if !conf.skip_insecure_warning.unwrap_or_default() {
        utils::warn_insecure_conf(&conf)?;
    }
    // call the right groups handler
    match cmd {
        Groups::Get(cmd) => get(thorium, cmd).await,
        Groups::Describe(cmd) => describe(thorium, cmd).await,
    }
}

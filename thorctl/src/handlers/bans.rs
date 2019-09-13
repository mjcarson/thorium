//! The images related features for Thoradm

use thorium::{
    models::{ImageBan, ImageBanKind, ImageBanUpdate, ImageUpdate},
    Thorium,
};

use crate::{
    args::{AddImageBan, Args, BansSubCommands, RemoveImageBan},
    Error,
};

/// Handle the images command
///
/// # Arguments
///
/// * `sub` - The images subcommand
/// * `args` - The Thoradm args
pub async fn handle(sub: &BansSubCommands, args: &Args) -> Result<(), Error> {
    // generate a Thorium client from the given Thorctl conf file
    let thorium = Thorium::from_ctl_conf_file(&args.ctl_conf).await?;
    match sub {
        BansSubCommands::Add(add_ban_cmd) => add_ban(add_ban_cmd, thorium).await,
        BansSubCommands::Remove(remove_ban_cmd) => remove_ban(remove_ban_cmd, thorium).await,
    }
}

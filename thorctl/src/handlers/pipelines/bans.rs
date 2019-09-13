//! Handle pipeline ban related commands

use thorium::{
    models::{PipelineBan, PipelineBanKind, PipelineBanUpdate, PipelineUpdate},
    Thorium,
};

use crate::Error;
use crate::{
    args::pipelines::{AddPipelineBan, PipelineBans, RemovePipelineBan},
    err_not_admin,
};

/// Add a ban to a pipeline in Thorium
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The add pipeline ban command that was run
async fn add_ban(thorium: Thorium, cmd: &AddPipelineBan) -> Result<(), Error> {
    // create a new ban
    let ban = PipelineBan::new(PipelineBanKind::generic(&cmd.msg));
    // add the ban to a pipeline update
    let update = PipelineUpdate::default().bans(PipelineBanUpdate::default().add_ban(ban));
    // send the update
    err_not_admin!(
        thorium
            .pipelines
            .update(&cmd.group, &cmd.pipeline, &update)
            .await,
        "ban pipelines"
    );
    Ok(())
}

/// Remove a ban from a pipeline in Thorium
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The remove pipeline ban command that was run
async fn remove_ban(thorium: Thorium, cmd: &RemovePipelineBan) -> Result<(), Error> {
    // add the ban to remove to a pipeline update
    let update = PipelineUpdate::default().bans(PipelineBanUpdate::default().remove_ban(cmd.id));
    // send the update
    err_not_admin!(
        thorium
            .pipelines
            .update(&cmd.group, &cmd.pipeline, &update)
            .await,
        "remove pipeline bans"
    );
    Ok(())
}

/// Handle pipeline ban commands
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The pipeline bans sub command that was run
pub async fn handle(thorium: Thorium, cmd: &PipelineBans) -> Result<(), Error> {
    match cmd {
        PipelineBans::Add(cmd) => add_ban(thorium, cmd).await,
        PipelineBans::Remove(cmd) => remove_ban(thorium, cmd).await,
    }
}

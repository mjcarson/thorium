use itertools::Itertools;
use thorium::{models::Pipeline, Error, Thorium};

use super::update;
use crate::args::pipelines::{DescribePipelines, GetPipelines, Pipelines};
use crate::args::{Args, DescribeCommand};
use crate::utils;

mod bans;
mod notifications;

struct GetPipelinesLine;

impl GetPipelinesLine {
    /// Print this log lines header
    pub fn header() {
        println!(
            "{:<30} | {:<20} | {:<50}",
            "PIPELINE NAME", "GROUP", "DESCRIPTION",
        );
        println!("{:-<31}+{:-<22}+{:-<50}", "", "", "");
    }

    /// Print a pipeline's info
    ///
    /// # Arguments
    ///
    /// * `pipeline` - The pipeline to print
    pub fn print_pipeline(pipeline: &Pipeline) {
        println!(
            "{:<30} | {:<20} | {}",
            pipeline.name,
            pipeline.group,
            pipeline.description.as_ref().unwrap_or(&"-".to_string())
        );
    }
}

/// Get pipeline info from Thorium
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The pipeline get command to execute
async fn get(thorium: Thorium, cmd: &GetPipelines) -> Result<(), Error> {
    GetPipelinesLine::header();
    // get the current user's groups if no groups were specified
    let groups = if cmd.groups.is_empty() {
        utils::groups::get_all_groups(&thorium).await?
    } else {
        cmd.groups.clone()
    };
    // get pipeline cursors for all groups specified
    let pipeline_cursors = groups.iter().map(|group| {
        thorium
            .pipelines
            .list(group)
            .limit(cmd.limit)
            .page(cmd.page_size)
            .details()
    });
    // retrieve the pipelines in each cursor until we've reached our limit
    // or all cursors are exhausted
    let mut pipelines: Vec<Pipeline> = Vec::new();
    for mut cursor in pipeline_cursors {
        while !cursor.exhausted {
            cursor.next().await?;
            if cmd.alpha {
                // save for later if we need to alphabetize
                pipelines.append(&mut cursor.details);
            } else {
                // print immediately if no need to alphabetize
                cursor
                    .details
                    .iter()
                    .for_each(GetPipelinesLine::print_pipeline);
            }
        }
    }
    // sort and print in alphabetical order if alpha flag was set
    if cmd.alpha {
        pipelines
            .iter()
            .sorted_unstable_by(|a, b| Ord::cmp(&a.name, &b.name))
            .for_each(GetPipelinesLine::print_pipeline);
    }
    Ok(())
}

/// Describe pipelines by displaying/saving all of their JSON-formatted details
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The describe pipeline command to execute
async fn describe(thorium: Thorium, cmd: &DescribePipelines) -> Result<(), Error> {
    cmd.describe(&thorium).await
}

/// Handle all pipelines commands
///
/// # Arguments
///
/// * `args` - The arguments passed to Thorctl
/// * `cmd` - The pipelines command to execute
pub async fn handle(args: &Args, cmd: &Pipelines) -> Result<(), Error> {
    // load our config and instance our client
    let (conf, thorium) = utils::get_client(args).await?;
    // warn about insecure connections if not set to skip
    if !conf.skip_insecure_warning.unwrap_or_default() {
        utils::warn_insecure_conf(&conf)?;
    }
    // check if we need to update
    if !args.skip_update && !conf.skip_update.unwrap_or_default() {
        update::ask_update(&thorium).await?;
    }
    // call the right pipelines handler
    match cmd {
        Pipelines::Get(cmd) => get(thorium, cmd).await,
        Pipelines::Describe(cmd) => describe(thorium, cmd).await,
        Pipelines::Notifications(cmd) => notifications::handle(thorium, cmd).await,
        Pipelines::Bans(cmd) => bans::handle(thorium, cmd).await,
    }
}

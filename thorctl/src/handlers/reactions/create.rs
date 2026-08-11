//! Handles creating reactions

use colored::Colorize;
use futures::StreamExt;
use futures::stream::{self, FuturesUnordered};
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use thorium::models::{
    BulkReactionResponse, Pipeline, Reaction, ReactionArgs, ReactionRequest, ReactionStatus,
    RepoDependencyRequest,
};
use thorium::{Error, Thorium};

use crate::args::{
    SearchParameterized,
    reactions::{BUNDLE_DELIMITER, CreateReactions},
    repos::RepoTarget,
};
use crate::handlers::progress;
use crate::utils;
use crate::utils::ephemeral::{self, EphemeralFiles};

/// prints out a single create reaction line
macro_rules! create_print {
    ($code:expr, $pipeline:expr, $dep:expr, $id:expr, $msg:expr) => {
        println!(
            "{:<4} | {:<25} | {:<64} | {:<36} | {:<32}",
            $code, $pipeline, $dep, $id, $msg
        )
    };
}

/// prints out a single create reaction line for samples
macro_rules! create_print_samples {
    ($code:expr, $pipeline:expr, $samples:expr, $id:expr, $msg:expr) => {
        if $samples.len() > 1 {
            // build a truncated string of the first 8 characters in each sample + ...,
            let mut truncated = $samples.iter().fold(String::new(), |truncated, sample| {
                truncated + sample.chars().take(8).collect::<String>().as_str() + "...,"
            });
            // pop the final ','
            truncated.pop();
            create_print!($code, $pipeline, truncated, $id, $msg);
        } else if let Some(sample) = $samples.first() {
            create_print!($code, $pipeline, sample, $id, $msg)
        }
    };
}

/// prints out a single create reaction line for repos
macro_rules! create_print_repos {
    ($code:expr, $pipeline:expr, $repos:expr, $id:expr, $msg:expr) => {
        if $repos.len() > 1 {
            // build a truncated string of name of the repo and 4 of commit in each repo + ...,
            let mut truncated = $repos.iter().fold(String::new(), |truncated, repo| {
                let name = repo.url.split('/').last().unwrap_or(repo.url.as_str());
                let short: String = if let Some(commitish) = &repo.commitish {
                    // if our commitish is longer then 12 chars then use the first 4 chars
                    if commitish.len() > 8 {
                        // set the text to be first 4 chars from the commitish
                        format!("{}:{}", name, commitish.chars().take(8).collect::<String>())
                    } else {
                        format!("{name}{commitish}")
                    }
                } else {
                    name.to_string()
                };
                truncated + short.as_str() + "...,"
            });
            // pop the final ','
            truncated.pop();
            create_print!($code, $pipeline, truncated, $id, $msg)
        } else if let Some(repo) = $repos.first() {
            // otherwise include all of the url + 12 of the commitish
            if let Some(commit) = &repo.commitish {
                let repo = format!(
                    "{}:{}",
                    repo.url,
                    commit.chars().take(12).collect::<String>()
                );
                create_print!($code, $pipeline, repo, $id, $msg);
            } else {
                create_print!($code, $pipeline, repo.url, $id, $msg)
            };
        }
    };
}

/// A single line for a reaction creation log
struct CreateLine;

impl CreateLine {
    /// Print this log lines header
    pub fn header() {
        println!(
            "{} | {:<25} | {:<64} | {:<36} | {:<32}",
            "CODE", "PIPELINE", "SAMPLE/REPO", "ID", "MESSAGE"
        );
        println!("{:-<5}+{:-<27}+{:-<66}+{:-<38}+{:-<34}", "", "", "", "", "");
    }

    /// Print a log line for a created reaction
    pub fn created(reqs: &[ReactionRequest], creates: &BulkReactionResponse) {
        // track what index we are at for created reaction ids
        let mut ok_index = 0;
        // crawl over the reaction requests we tried to create
        for (index, req) in reqs.iter().enumerate() {
            // check if this request had an error and build the right info for our log lines
            let (code, id, msg) = if let Some(error) = creates.errors.get(&index) {
                ("ERROR".bright_red(), "-".to_string(), error.as_str())
            } else {
                // this request didn't run into an error so get our created reaction id
                let id = &creates.created[ok_index];
                // increment our ok index
                ok_index += 1;
                ("200".bright_green(), id.to_string(), "-")
            };
            // log sample reaction creations
            create_print_samples!(code, req.pipeline, req.samples, id, msg);
            // log repo reaction creations
            create_print_repos!(code, req.pipeline, req.repos, id, msg);
        }
    }

    /// Print a log line for a created reaction in dry-run mode
    pub fn created_dry_run(reqs: &[ReactionRequest]) {
        // print out the pipelines an samples/repos:commitishes from each request
        for req in reqs {
            // print samples if there are any
            create_print_samples!("-", req.pipeline, req.samples, "-", "-");
            // print repos if there are any
            create_print_repos!("-", req.pipeline, req.repos, "-", "-");
        }
    }
}

/// Whether we've already warned about the size of this run's ephemeral payload
///
/// `create_bulk` runs once per page of a search cursor, so without this the same warning would
/// print on every page.
static WARNED_EPHEMERAL: AtomicBool = AtomicBool::new(false);

/// Warn if the ephemeral data duplicated into a full bulk request would be large
///
/// Every reaction request in a bulk call carries its own copy of the ephemeral buffers, so the
/// payload scales with the number of reactions per call rather than with the file count. The
/// exact count isn't known until each call is assembled, so this estimates a full search page as
/// the representative case. This is advisory only, we never refuse to send an ephemeral file.
///
/// # Arguments
///
/// * `files` - The ephemeral files loaded from the command
/// * `cmd` - The full reaction creation command/args
/// * `pipelines` - The number of pipelines we're creating reactions for
fn warn_estimated_ephemeral(files: &EphemeralFiles, cmd: &CreateReactions, pipelines: usize) {
    // stay quiet if there are no ephemeral files or the user asked us not to warn
    if files.is_empty() || cmd.skip_ephemeral_warning {
        return;
    }
    // a bulk call holds one request per target per pipeline, and a search sends a full page
    let reqs_per_bulk = cmd.page_size.saturating_mul(pipelines) as u64;
    // every one of those requests carries its own copy of the encoded files
    let per_req = files.encoded_len();
    let total = per_req.saturating_mul(reqs_per_bulk);
    // only speak up once the duplicated payload gets big enough to matter
    if total > ephemeral::WARN_ENCODED_BYTES {
        // note that we've warned so create_bulk doesn't immediately repeat us
        WARNED_EPHEMERAL.store(true, Ordering::Relaxed);
        progress::warn(format!(
            "{} ephemeral file(s) ({} base64 encoded) are copied into every reaction, so a full \
             bulk request of {reqs_per_bulk} reactions will send about {}. Lower --page-size or \
             narrow your targets to shrink each request.",
            files.len(),
            ephemeral::fmt_bytes(per_req),
            ephemeral::fmt_bytes(total),
        ));
    }
}

/// Warn if an assembled bulk request carries a large amount of ephemeral data
///
/// Unlike [`warn_estimated_ephemeral`] this knows exactly how many reactions are in the request,
/// which matters for the file/repo list paths that send every entry in a single call. This is
/// advisory only, we always send the request no matter how large it is.
///
/// # Arguments
///
/// * `reqs` - The assembled reaction requests about to be sent
/// * `cmd` - The full reaction creation command/args
fn warn_exact_ephemeral(reqs: &[ReactionRequest], cmd: &CreateReactions) {
    // stay quiet if the user asked us not to warn or we've already said our piece
    if cmd.skip_ephemeral_warning || WARNED_EPHEMERAL.load(Ordering::Relaxed) {
        return;
    }
    // every request carries the same buffers, so measure the first and scale by the request count
    if let Some(first) = reqs.first() {
        let per_req: u64 = first.buffers.values().map(|buf| buf.len() as u64).sum();
        let total = per_req.saturating_mul(reqs.len() as u64);
        // only speak up once the duplicated payload gets big enough to matter
        if total > ephemeral::WARN_ENCODED_BYTES {
            // warn at most once, since this runs for every page of a search cursor
            WARNED_EPHEMERAL.store(true, Ordering::Relaxed);
            progress::warn(format!(
                "Sending {} of ephemeral data in one request ({} reactions x {} each)",
                ephemeral::fmt_bytes(total),
                reqs.len(),
                ephemeral::fmt_bytes(per_req),
            ));
        }
    }
}

/// Create several reactions in bulk or just print them if dry-run mode is on
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `reqs` - The reaction requests to send
/// * `batch` - The batch the reaction might be connected to
/// * `args_info` - Info on the args to give to created reactions
/// * `cmd` - The full reaction creation command/args
async fn create_bulk(
    thorium: &Thorium,
    reqs: Vec<ReactionRequest>,
    batch: &Option<String>,
    args_info: &Option<ReactionArgsInfo>,
    cmd: &CreateReactions,
) -> Result<(), Error> {
    // the file/repo list paths send every entry in a single call, so the up front estimate can
    // badly understate a request's real size; warn with the exact numbers now that we have them
    warn_exact_ephemeral(&reqs, cmd);
    // add other settings derived from the run command to each request before sending
    let reqs: Vec<ReactionRequest> = reqs
        .into_iter()
        .map(|mut req| {
            // build our base reaction request
            req = req.tags(cmd.reaction_tags.clone());
            // if we have a batch name then set that
            if let Some(batch) = batch {
                req = req.tag(batch);
            }
            // set the SLA if one was given
            if let Some(sla) = cmd.sla {
                req = req.sla(sla);
            }
            // add any args to the request if they were given
            if let Some(args_info) = args_info {
                // see if this pipeline has any args
                if let Some(args) = args_info.get(&req.pipeline) {
                    for (image, args) in args {
                        // add the args to the request for this pipeline
                        req = req.args(image.clone(), args.clone());
                    }
                }
            }
            req
        })
        .collect();
    if cmd.dry_run {
        // only display files/repos for which reactions will be created if dry-run mode is on
        CreateLine::created_dry_run(&reqs);
    } else {
        // create the reactions
        match thorium.reactions.create_bulk(&reqs).await {
            Ok(creates) => CreateLine::created(&reqs, &creates),
            Err(err) => return Err(Error::new(format!("Unable to create reactions: {err}"))),
        }
    }
    Ok(())
}

/// Parse file bundles using the [`BUNDLE_DELIMITER`]
///
/// # Arguments
///
/// * `raw_bundles` - The raw file bundles containing `BUNDLE_DELIMITER`(s)
fn parse_bundles<I, T>(raw_bundles: I) -> Vec<Vec<String>>
where
    I: IntoIterator<Item = T>,
    T: AsRef<str>,
{
    // split bundles using the delimiter, map to owned Strings and collect to a list of lists
    raw_bundles
        .into_iter()
        .map(|bundle| {
            bundle
                .as_ref()
                .split(BUNDLE_DELIMITER)
                .map(str::to_string)
                .collect_vec()
        })
        .collect()
}

/// Info used to apply reaction args
pub type ReactionArgsInfo = HashMap<String, ReactionArgs>;

/// prints out a single watch reaction line
macro_rules! watch_print {
    ($status:expr, $pipeline:expr, $id:expr) => {
        println!("{:<12} | {:<25} | {:<36}", $status, $pipeline, $id)
    };
}

/// A single line for a reaction creation log
struct WatchLine;

impl WatchLine {
    /// Print this log lines header
    pub fn header() {
        println!("{:<12} | {:<25} | {:<36}", "STATUS", "PIPELINE", "ID");
        println!("{:-<13}+{:-<27}+{:-<38}", "", "", "");
    }

    /// Print a log line for a created reaction
    pub fn change(details: &Reaction) {
        match details.status {
            ReactionStatus::Created => {
                watch_print!("Created".bright_magenta(), details.pipeline, details.id);
            }
            ReactionStatus::Started => {
                watch_print!("Started".bright_blue(), details.pipeline, details.id);
            }
            ReactionStatus::Completed => {
                watch_print!("Completed".bright_green(), details.pipeline, details.id);
            }
            ReactionStatus::Failed => {
                watch_print!("Failed".bright_red(), details.pipeline, details.id);
            }
        }
    }
}

/// Watch batched reactions in a given group
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `group` - The group to watch reactions in
/// * `batch` - The batch of reactions to watch
async fn watch_group(thorium: Arc<Thorium>, group: String, batch: String) -> Result<(), Error> {
    // keep a map of all status to detect changes
    let mut statuses = HashMap::with_capacity(100);
    // loop over all reactions in this batch until all have reached terminal states
    loop {
        // track whether any unfinished reactions were found
        let mut unfinished = false;
        // get a cursor over the reactions in this batch
        let mut cursor = thorium
            .reactions
            .list_tag(&group, &batch)
            .page_size(100)
            .details();
        // crawl this cursor until its exhausted
        while !cursor.exhausted {
            // get the next page of data
            cursor.next().await?;
            // check for any changes in the status these reactions
            for reaction in cursor.details.drain(..) {
                // skip any reactions in the created state
                if reaction.status == ReactionStatus::Created {
                    // we found an unfinished reaction
                    unfinished = true;
                    continue;
                }
                // check if this reaction is already in our status list
                if let Some(old_status) = statuses.get(&reaction.id) {
                    // if we have the same status then continue on
                    if old_status == &reaction.status {
                        // check if this is an unfinished reaction
                        if reaction.status == ReactionStatus::Started {
                            // this is still an unfinished reaction so set that
                            unfinished = true;
                        }
                        continue;
                    }
                }
                // we haven't logged this reaction yet or it changed statuses so log it again
                WatchLine::change(&reaction);
                // check if this was a terminal status or not
                if reaction.status == ReactionStatus::Started {
                    unfinished = true;
                }
                // set this reactions status
                statuses.insert(reaction.id, reaction.status);
            }
        }
        // stop watching reactions if we saw no unfinished reactions
        if !unfinished {
            break;
        }
        // sleep for 2 seconds between checks
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    Ok(())
}

/// Watch for a batch of reactions to complete
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `batch` - The batch tag used to find the running reactions
/// * `groups` - All of the groups our reactions are running in
pub async fn watch<I>(thorium: Thorium, batch: &str, groups: I) -> Result<(), Error>
where
    I: Iterator<Item = String>,
{
    // print the create reaction header
    WatchLine::header();
    // create an Arc for the Thorium client
    let thorium = Arc::new(thorium);
    let mut handles = FuturesUnordered::new();
    for group in groups {
        // create tasks to watch the reactions in each group
        handles.push(tokio::spawn(watch_group(
            thorium.clone(),
            group,
            batch.to_owned(),
        )));
    }
    // wait for all of the watching tasks to complete
    while let Some(ret) = handles.next().await {
        // log any errors
        if let Err(error) = ret {
            eprintln!("Error watching reactions: {error}");
        }
    }
    Ok(())
}

async fn get_args_info(
    thorium: &Thorium,
    cmd: &CreateReactions,
    pipelines_with_groups: &HashMap<String, String>,
) -> Result<ReactionArgsInfo, Error> {
    // get all info on all of our pipelines
    let pipelines = stream::iter(pipelines_with_groups.iter())
        .map(|(pipeline, group)| thorium.pipelines.get(group, pipeline))
        .buffer_unordered(10)
        .collect::<Vec<Result<_, _>>>()
        .await
        .into_iter()
        .collect::<Result<Vec<Pipeline>, Error>>()
        .map_err(|err| Error::new(format!("Error retreiving info on pipelines: {err}")))?;
    let pipelines_images: HashMap<String, HashSet<String>> = pipelines
        .into_iter()
        .map(|pipeline| {
            (
                pipeline.name,
                pipeline.order.into_iter().flatten().collect::<HashSet<_>>(),
            )
        })
        .collect();
    // build reaction args based on the args we were given and the images we have
    cmd.build_reaction_args(&pipelines_images)
}

async fn parse_pipelines(
    thorium: &Thorium,
    cmd: &CreateReactions,
) -> Result<HashMap<String, String>, Error> {
    // parse pipeline targets from our command
    let pipelines = cmd.parse_pipelines()?;
    // get groups for all the pipelines that don't have them
    let mut pipelines_with_groups = utils::pipelines::find_pipelines_groups(
        thorium,
        pipelines
            .iter()
            .filter_map(|p| p.group.is_none().then_some(&p.pipeline)),
    )
    .await?;
    // add all the pipelines that already have groups
    pipelines_with_groups.extend(
        pipelines
            .into_iter()
            .filter_map(|p| p.group.map(|g| (p.pipeline, g))),
    );
    Ok(pipelines_with_groups)
}

/// Create reactions for files specified in the command
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The create reactions command that was run
/// * `base_reqs` - Base reaction requests for each requested pipeline
/// * `batch` - An optional batch to track the reactions we're creating
/// * `args_info` - Any args to pass to the reactions
async fn create_files(
    thorium: &Thorium,
    cmd: &CreateReactions,
    base_reqs: &[ReactionRequest],
    batch: &Option<String>,
    args_info: &Option<ReactionArgsInfo>,
) -> Result<(), Error> {
    // create requests for each specific file target for each pipeline
    let reqs = cmd
        .files
        .iter()
        .flat_map(|sha256| {
            base_reqs
                .iter()
                .map(|base_req| base_req.clone().sample(sha256.clone()))
        })
        .collect();
    // create reactions
    create_bulk(thorium, reqs, batch, args_info, cmd).await?;
    Ok(())
}

/// Create reactions for file bundles specified in the command
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The create reactions command that was run
/// * `base_reqs` - Base reaction requests for each requested pipeline
/// * `batch` - An optional batch to track the reactions we're creating
/// * `args_info` - Any args to pass to the reactions
async fn create_file_bundles(
    thorium: &Thorium,
    cmd: &CreateReactions,
    base_reqs: &[ReactionRequest],
    batch: &Option<String>,
    args_info: &Option<ReactionArgsInfo>,
) -> Result<(), Error> {
    // parse the file bundles
    let bundles = parse_bundles(&cmd.file_bundles);
    // create requests for each bundle
    let reqs = bundles
        .iter()
        .flat_map(|bundle| {
            base_reqs
                .iter()
                .map(|base_req| base_req.clone().samples(bundle.clone()))
        })
        .collect();
    // create reactions
    create_bulk(thorium, reqs, batch, args_info, cmd).await?;
    Ok(())
}

/// Create reactions for repos specified in the command
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The create reactions command that was run
/// * `repo_targets` - Parsed repo targets from the command
/// * `base_reqs` - Base reaction requests for each requested pipeline
/// * `batch` - An optional batch to track the reactions we're creating
/// * `args_info` - Any args to pass to the reactions
async fn create_repos(
    thorium: &Thorium,
    cmd: &CreateReactions,
    repo_targets: HashSet<RepoTarget>,
    base_reqs: &[ReactionRequest],
    batch: &Option<String>,
    args_info: &Option<ReactionArgsInfo>,
) -> Result<(), Error> {
    // create requests for each specific file target for each pipeline
    let repo_deps: Vec<RepoDependencyRequest> = repo_targets
        .iter()
        .cloned()
        .map(RepoDependencyRequest::from)
        .collect();
    let reqs = repo_deps
        .iter()
        .flat_map(|repo| {
            base_reqs
                .iter()
                .map(|base_req| base_req.clone().repo(repo.clone()))
        })
        .collect();
    // create reactions
    create_bulk(thorium, reqs, batch, args_info, cmd).await?;
    Ok(())
}

/// Create reactions from a list of files (SHA256's) in a file
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The create reactions command that was run
/// * `repo_list` - The list of repos to read from
/// * `base_reqs` - Base reaction requests for each requested pipeline
/// * `batch` - An optional batch to track the reactions we're creating
/// * `args_info` - Any args to pass to the reactions
async fn create_file_list(
    thorium: &Thorium,
    cmd: &CreateReactions,
    file_list: &Path,
    base_reqs: &[ReactionRequest],
    batch: &Option<String>,
    args_info: &Option<ReactionArgsInfo>,
) -> Result<(), Error> {
    // get the set of all lines from the file
    let lines = utils::fs::lines_set_from_file(file_list).await?;
    // convert each line to bundles of SHA256's
    let bundles = parse_bundles(lines);
    // create requests for each bundle
    let reqs = base_reqs
        .iter()
        .flat_map(|base_req| {
            bundles
                .clone()
                .into_iter()
                .map(|bundle| base_req.clone().samples(bundle))
        })
        .collect();
    // create reactions
    create_bulk(thorium, reqs, batch, args_info, cmd).await?;
    Ok(())
}

/// Create reactions from a list of repos in a file
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The create reactions command that was run
/// * `repo_list` - The list of repos to read from
/// * `base_reqs` - Base reaction requests for each requested pipeline
/// * `batch` - An optional batch to track the reactions we're creating
/// * `args_info` - Any args to pass to the reactions
async fn create_repo_list(
    thorium: &Thorium,
    cmd: &CreateReactions,
    repo_list: &Path,
    base_reqs: &[ReactionRequest],
    batch: &Option<String>,
    args_info: &Option<ReactionArgsInfo>,
) -> Result<(), Error> {
    // create reactions for all repo URL's in a list file if one was given
    // read the file to a raw String
    let repos_raw = tokio::fs::read_to_string(repo_list).await?;
    // separate the file by lines, filter out all empty lines, and collect to a set of RepoTargets
    let repos_set: HashSet<RepoTarget> = repos_raw
        .lines()
        .filter(|line| !line.is_empty())
        .map(RepoTarget::try_from)
        .collect::<Result<HashSet<RepoTarget>, Error>>()?;
    // create requests for each repo target
    let reqs = base_reqs
        .iter()
        .flat_map(|base_req| {
            repos_set
                .clone()
                .into_iter()
                .map(RepoDependencyRequest::from)
                // build our base reaction request
                .map(|repo_dep| base_req.clone().repo(repo_dep))
        })
        .collect();
    // create reactions
    create_bulk(thorium, reqs, batch, args_info, cmd).await?;
    Ok(())
}

/// Create reactions by doing a search for files/repos
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The create reactions command that was run
/// * `base_reqs` - Base reaction requests for each requested pipeline
/// * `batch` - An optional batch to track the reactions we're creating
/// * `args_info` - Any args to pass to the reactions
async fn create_search(
    thorium: &Thorium,
    cmd: &CreateReactions,
    base_reqs: &[ReactionRequest],
    batch: &Option<String>,
    args_info: &Option<ReactionArgsInfo>,
) -> Result<(), Error> {
    if !cmd.repos_only {
        // search for files if repos_only hasn't been set
        let opts = cmd.build_file_opts()?;
        // build a cursor object
        let mut cursor = thorium.files.list(&opts).await?;
        // crawl over this cursor until its exhausted
        loop {
            // convert our SHA256's to requests
            let reqs = base_reqs
                .iter()
                .flat_map(|base_req| {
                    cursor
                        .data
                        .iter()
                        .map(|line| base_req.clone().sample(line.sha256.clone()))
                })
                .collect();
            // create reactions
            create_bulk(thorium, reqs, batch, args_info, cmd).await?;
            // check if this cursor has been exhausted
            if cursor.exhausted() {
                break;
            }
            // get the next page of data
            cursor.refill().await?;
        }
    }
    if cmd.include_repos || cmd.repos_only {
        // search for repos if they should be included in the search
        let opts = cmd.build_repo_opts()?;
        let mut cursor = thorium.repos.list(&opts).await?;
        loop {
            // create requests for each repo
            let reqs = base_reqs
                .iter()
                .flat_map(|base_req| {
                    cursor.data.iter().map(|line| {
                        base_req
                            .clone()
                            .repo(RepoDependencyRequest::new(line.url.clone()))
                    })
                })
                .collect();
            // create reactions
            create_bulk(thorium, reqs, batch, args_info, cmd).await?;
            // check if this cursor has been exhausted
            if cursor.exhausted() {
                break;
            }
            // get the next page of data
            cursor.refill().await?;
        }
    }
    Ok(())
}

/// Creates any requested reactions
///
/// # Arguments
///
/// * `thorium` - A Thorium client
/// * `cmd` - The full reaction creation command/args
pub async fn create(thorium: Thorium, cmd: &CreateReactions) -> Result<(), Error> {
    // make sure the command has targets or at least one search parameter
    cmd.validate_search()?;
    // read any ephemeral files up front so a bad path or name fails before we hit the API
    let ephemeral_files = EphemeralFiles::load(&cmd.ephemeral, cmd.delimiter).await?;
    // attempt to parse to our list of repo targets to a set of structured RepoTarget
    let repo_targets: HashSet<RepoTarget> = cmd
        .repos
        .iter()
        .map(RepoTarget::try_from)
        .collect::<Result<HashSet<RepoTarget>, Error>>()?;
    // get our batch name if needed
    let batch = cmd.batch_name();
    // parse pipelines and get groups for each one
    let pipelines_with_groups = parse_pipelines(&thorium, cmd).await?;
    // get info on reaction args if we need to
    let args_info = cmd
        .has_reaction_args()
        .then_some(get_args_info(&thorium, cmd, &pipelines_with_groups).await)
        .transpose()?;
    // warn if the ephemeral files copied into each reaction will make our requests large
    warn_estimated_ephemeral(&ephemeral_files, cmd, pipelines_with_groups.len());
    // ephemeral files don't appear in the table, so name them explicitly under --dry-run
    if cmd.dry_run && !ephemeral_files.is_empty() {
        progress::note(format!(
            "Each reaction would carry ephemeral file(s): {}",
            ephemeral_files.names().collect::<Vec<&str>>().join(", ")
        ));
    }
    // print the create reaction header
    CreateLine::header();
    // build base reaction requests for each pipeline
    let mut base_reqs: Vec<ReactionRequest> = pipelines_with_groups
        .iter()
        .map(|(pipeline, group)| ReactionRequest::new(group, pipeline))
        .collect();
    // set our parent uuid if we have one
    if let Some(parent) = cmd.parent {
        for base_req in &mut base_reqs {
            base_req.parent = Some(parent);
        }
    }
    // attach the ephemeral files once here so every request cloned from these bases inherits
    // them, no matter which target source builds it
    let base_reqs: Vec<ReactionRequest> = base_reqs
        .into_iter()
        .map(|base_req| ephemeral_files.attach(base_req))
        .collect();
    // create reactions for any files
    create_files(&thorium, cmd, &base_reqs, &batch, &args_info).await?;
    // create reactions for any file bundles
    create_file_bundles(&thorium, cmd, &base_reqs, &batch, &args_info).await?;
    // create reactions for any repos
    create_repos(&thorium, cmd, repo_targets, &base_reqs, &batch, &args_info).await?;
    // if the user gave a file list, create reactions from the repo list
    if let Some(file_list) = &cmd.file_list {
        create_file_list(&thorium, cmd, file_list, &base_reqs, &batch, &args_info).await?;
    }
    // if the user gave a repo list, create reactions from the repo list
    if let Some(repo_list) = &cmd.repo_list {
        create_repo_list(&thorium, cmd, repo_list, &base_reqs, &batch, &args_info).await?;
    }
    // if the user provided any search parameters, perform a search and create reactions
    if cmd.has_parameters() || cmd.apply_to_all() {
        create_search(&thorium, cmd, &base_reqs, &batch, &args_info).await?;
    }
    // if we want to watch all our jobs complete then do that
    if cmd.watch {
        // make sure we have a batch name
        if let Some(batch) = batch {
            // close our create print
            println!("{:-<5}+{:-<27}+{:-<66}+{:-<38}+{:-<34}", "", "", "", "", "");
            // put an empty line of space between our create prints and our watch log
            println!("\n\tWATCHING REACTIONS\t\n");
            watch(thorium, &batch, pipelines_with_groups.into_values()).await?;
        }
    }
    Ok(())
}

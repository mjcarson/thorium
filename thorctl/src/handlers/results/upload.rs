use futures::TryStreamExt;
use http::StatusCode;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use thorium::client::ResultsClient;
use thorium::models::{Buffer, EntityKinds, EntityRequest, OnDiskFile, OutputRequest, Sample};
use thorium::{Error, Thorium};

use crate::args::results::UploadResults;

/// prints out a single downloaded result line
macro_rules! upload_print {
    ($code:expr, $sample:expr, $tool:expr, $msg:expr) => {
        println!(
            "{:<4} | {:<64} | {:<20} | {:<32} ",
            $code, $sample, $tool, $msg
        )
    };
}

struct UploadLine;

impl UploadLine {
    /// Print this log lines header
    #[rustfmt::skip]
    #[allow(clippy::print_literal)]
    pub fn header() {
        println!(
            "{} | {:<64} | {:<20} | {:<32}",
            "CODE", "SAMPLE", "TOOL", "MESSAGE"
        );
        println!("{:-<5}+{:-<66}+{:-<22}+{:-<34}", "", "", "", "");
    }

    /// Print a log line for an uploaded result
    ///
    /// # Arguments
    ///
    /// * `sample` - The SHA256 this result was uploaded for
    /// * `tool` - The tool this result was uploaded for
    /// * `msg` - A message describing what was uploaded with this result
    pub fn uploaded<S: Display, T: Display, M: Display>(sample: S, tool: T, msg: M) {
        // log this line
        upload_print!(200.bright_green(), sample, tool, msg);
    }

    /// Print a log line for an uploaded result
    ///
    /// # Arguments
    ///
    /// * `sample` - The SHA256 this result would be uploaded for
    /// * `tool` - The tool this result would be uploaded for
    /// * `msg` - A message describing what would be uploaded with this result
    pub fn uploaded_dry_run<S: Display, T: Display, M: Display>(sample: S, tool: T, msg: M) {
        // log this line
        upload_print!("-".bright_green(), sample, tool, msg);
    }

    /// Print an error log line for a result that could not be uploaded
    pub fn error<S: Display, T: Display>(sample: S, tool: T, err: &Error) {
        // get the error message for this line
        let msg = err.msg().unwrap_or_else(|| "-".to_owned());
        // get the error status
        let status = err.status();
        // get a default "-" if no status, otherwise map to a str
        let status_str = status.as_ref().map_or("-", StatusCode::as_str);
        // log this line
        upload_print!(status_str.bright_red(), sample, tool, msg);
    }
}

/// Attempt to upload a result for a file, outputting a log line on success or
/// error, or just output a successful line if we're in dry run mode
macro_rules! upload {
    ($thorium:expr, $req:expr, $sha256:expr, $tool:expr, $cmd:expr) => {
        async {
            // bind our request so we can describe it before its consumed by the upload
            let req = $req;
            // build the message describing the entities attached to this result
            let msg = entities_msg(&req.entities);
            if $cmd.dry_run {
                // just log a line if we're in dry run mode
                UploadLine::uploaded_dry_run($sha256, $tool, msg);
            } else {
                match $thorium.files.create_result(req).await {
                    Ok(_) => UploadLine::uploaded($sha256, $tool, msg),
                    Err(err) => UploadLine::error($sha256, $tool, &err),
                }
            }
        }
    };
}

/// Returns true if the string is a valid SHA256 (is 64 ASCII hex digits)
///
/// # Arguments
///
/// * `s` - The string to check if it's probably a SHA256
fn is_sha256<T: AsRef<str>>(s: T) -> bool {
    let s = s.as_ref();
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Group a raw JSON list of entity requests into a serialized buffer for each entity kind
///
/// An empty list of entity requests produces an empty map so we never send an entity
/// kind with a count of 0, which the API rejects.
///
/// # Arguments
///
/// * `data` - The raw JSON data containing a list of entity requests
/// * `path` - The path this raw data was read from, used for error context
fn group_entities(
    data: &[u8],
    path: &Path,
) -> Result<HashMap<EntityKinds, (usize, Buffer)>, Error> {
    // deserialize the list of entity requests in this raw data
    let parsed: Vec<EntityRequest> = serde_json::from_slice(data).map_err(|err| {
        Error::new(format!(
            "Error deserializing entities file '{}': {}",
            path.display(),
            err
        ))
    })?;
    // break our list of requests up based on what kind of entity each one is
    let mut kind_map = HashMap::<EntityKinds, Vec<EntityRequest>>::default();
    for req in parsed {
        // add this request to the list for its kind
        kind_map.entry(req.kind()).or_default().push(req);
    }
    // serialize each kinds requests into its own buffer
    let mut entities = HashMap::with_capacity(kind_map.len());
    for (kind, reqs) in kind_map {
        // serialize the requests for this kind
        let serialized = serde_json::to_string(&reqs).map_err(|err| {
            Error::new(format!(
                "Error serializing {kind} entities from '{}': {}",
                path.display(),
                err
            ))
        })?;
        // wrap our serialized requests in a buffer and track how many entities it holds
        entities.insert(kind, (reqs.len(), Buffer::new(serialized)));
    }
    Ok(entities)
}

/// Read and group the entities discovered for a set of results if any were found
///
/// Results directories with no entities file just have no entities.
///
/// # Arguments
///
/// * `path` - The path to the entities file for these results
async fn collect_entities(path: &Path) -> Result<HashMap<EntityKinds, (usize, Buffer)>, Error> {
    // check if an entities file exists for these results
    if !tokio::fs::try_exists(path).await.map_err(|err| {
        Error::new(format!(
            "Error checking that entities exist at '{}': {}",
            path.display(),
            err
        ))
    })? {
        // no entities file was found so these results just have no entities
        return Ok(HashMap::default());
    }
    // read the raw entities data from disk
    let data = tokio::fs::read(path).await.map_err(|err| {
        Error::new(format!(
            "Error reading entities file '{}': {}",
            path.display(),
            err
        ))
    })?;
    // group our entities by kind and serialize them into a buffer for each kind
    group_entities(&data, path)
}

/// Build the message describing the entities attached to a result
///
/// # Arguments
///
/// * `entities` - The entities attached to this result by kind
fn entities_msg(entities: &HashMap<EntityKinds, (usize, Buffer)>) -> String {
    // no entities were attached to this result so just use our placeholder
    if entities.is_empty() {
        return "-".to_owned();
    }
    // sum how many entities we found across every kind
    let total: usize = entities.values().map(|(count, _)| count).sum();
    // report the total and how many kinds those entities were split across
    format!("{total} entities/{} kinds", entities.len())
}

/// Build the request to upload a single tools results
///
/// # Arguments
///
/// * `cmd` - The upload results command
/// * `sha256` - The SHA256 of the file these results are for
/// * `tool` - The tool these results are for
/// * `results` - The main results to display in the UI
/// * `files` - Any result files to upload as attachments
/// * `entities` - Any entities discovered by this tool by kind
fn build_request(
    cmd: &UploadResults,
    sha256: &str,
    tool: &str,
    results: String,
    files: Vec<OnDiskFile>,
    entities: HashMap<EntityKinds, (usize, Buffer)>,
) -> OutputRequest<Sample> {
    // build the base request for this tools results
    let mut req = OutputRequest::<Sample>::new(sha256.to_string(), tool, results, cmd.display_type)
        .groups(cmd.result_groups.clone())
        .files(files);
    // attach each kind of entity this tool discovered
    for (kind, (count, buff)) in entities {
        req = req.entities(kind, count, buff);
    }
    req
}

/// Uploads results for a file based on tool sub-directories
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The upload results command
/// * `sha256` - The SHA256 of the file to upload results to
/// * `tool_subdirs` - The list of tool sub-directories to upload
async fn upload_tool_subdirs(
    thorium: &Thorium,
    cmd: &UploadResults,
    sha256: &str,
    tool_subdirs: Vec<PathBuf>,
) -> Result<(), Error> {
    // walk each tool subdirectory recursively and upload results for that tool
    for tool_subdir in tool_subdirs {
        // get the tool name (the name of the sub-directory)
        let tool = match tool_subdir.file_name() {
            Some(file_name) => file_name.to_string_lossy().to_string(),
            // there is no file name; just proceed to the next tool
            None => continue,
        };
        // construct the path to the main results file
        let results_path = tool_subdir.join(&cmd.results);
        // construct the path to the entities this tool discovered
        let entities_path = tool_subdir.join(&cmd.entities);
        // check if the main results exist
        if !tokio::fs::try_exists(&results_path).await.map_err(|err| {
            Error::new(format!(
                "Error checking that results exist at '{}': {}",
                results_path.display(),
                err
            ))
        })? {
            // there are no main results, so log an error for each tool and move on to the next tool
            UploadLine::error(
                sha256,
                tool,
                &Error::new(format!(
                    "No results file found at '{}'",
                    results_path.display()
                )),
            );
            continue;
        }
        // try to read the results file to a string
        let results_string = match tokio::fs::read_to_string(&results_path).await {
            Ok(results_string) => results_string,
            Err(err) => {
                // log an error that we couldn't read this results file for each tool and move on
                UploadLine::error(
                    sha256,
                    tool,
                    &Error::new(format!(
                        "Error reading results file '{}': {}",
                        results_path.display(),
                        err
                    )),
                );
                continue;
            }
        };
        // collect any entities this tool discovered
        let entities = match collect_entities(&entities_path).await {
            Ok(entities) => entities,
            Err(err) => {
                // log an error that we couldn't collect this tools entities and move on
                UploadLine::error(sha256, tool, &err);
                continue;
            }
        };
        // walk the directory recursively
        let walkdir = async_walkdir::WalkDir::new(&tool_subdir);
        let result_files = walkdir
            .try_fold(Vec::new(), |mut result_files, entry| {
                let results_path_ref = &results_path;
                let entities_path_ref = &entities_path;
                let tool_subdir_ref = &tool_subdir;
                async move {
                    let path = entry.path();
                    // only add this path if it's not the main result or the entities we
                    // already collected *and* it's a file
                    if &path != results_path_ref && &path != entities_path_ref && path.is_file() {
                        // trim the tool subdir so we only include the nested part
                        let on_disk = OnDiskFile::new(path).trim_prefix(tool_subdir_ref);
                        result_files.push(on_disk);
                    }
                    Ok(result_files)
                }
            })
            .await
            .map_err(|err| {
                Error::new(format!(
                    "Error reading tool sub-directory '{}': {}",
                    tool_subdir.display(),
                    err
                ))
            })?;
        // upload the results for this tool
        let results_req = build_request(cmd, sha256, &tool, results_string, result_files, entities);
        upload!(thorium, results_req, sha256, tool, cmd).await;
    }
    Ok(())
}

/// Uploads result to all tools given by flags, not organized in tool
/// sub directories
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The upload results command
/// * `sha256` - The SHA256 of the file to upload results to
/// * `path` - The path to the results
/// * `unnested_result_files` - The result files found in the path collected previously
/// * `entities` - The entities found in the path collected previously by kind
async fn upload_tool_flags(
    thorium: &Thorium,
    cmd: &UploadResults,
    sha256: &str,
    path: &Path,
    unnested_result_files: Vec<OnDiskFile>,
    entities: HashMap<EntityKinds, (usize, Buffer)>,
) -> Result<(), Error> {
    // build a path to the main results file as defined by the user
    let results = path.join(&cmd.results);
    // check if we even have an unnested results file
    if !tokio::fs::try_exists(&results).await.map_err(|err| {
        Error::new(format!(
            "Error checking that results exist at '{}': {}",
            results.display(),
            err
        ))
    })? {
        // there is no unnested results, so log an error for each tool and exit
        for tool in &cmd.tools {
            UploadLine::error(
                sha256,
                tool,
                &Error::new(format!("No results file found at '{}'", results.display())),
            );
        }
        return Ok(());
    }
    // try to read the results file to a string
    let results_string = match tokio::fs::read_to_string(&results).await {
        Ok(results_string) => results_string,
        Err(err) => {
            // log an error that we couldn't read this results file for each tool and move on
            for tool in &cmd.tools {
                UploadLine::error(
                    sha256,
                    tool,
                    &Error::new(format!(
                        "Error reading results file '{}': {}",
                        results.display(),
                        err
                    )),
                );
            }
            return Ok(());
        }
    };
    // upload unnested files for each tool given in the command
    for tool in &cmd.tools {
        // clone our results/files/entities since each tool gets its own request
        let results_req = build_request(
            cmd,
            sha256,
            tool,
            results_string.clone(),
            unnested_result_files.clone(),
            entities.clone(),
        );
        upload!(thorium, results_req, sha256, tool, cmd).await;
    }
    Ok(())
}

/// Uploads results for a single file to Thorium
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The upload results command
/// * `sha256` - The SHA256 of the file to upload results to
/// * `path` - The path to the results
async fn upload_helper(
    thorium: &Thorium,
    cmd: &UploadResults,
    sha256: &str,
    path: &Path,
) -> Result<(), Error> {
    // check if this is a file or a directory
    if path.is_file() {
        // this is a sha256 file, so just upload it as a result for each given tool
        if cmd.tools.is_empty() {
            // log an error and move on to the next target if no tools were given
            UploadLine::error(
                sha256,
                "-",
                &Error::new("Results file found but no tools were supplied with flag '--tools'"),
            );
            return Ok(());
        }
        for tool in &cmd.tools {
            // try to read in this sha256 file as the results file
            let results_string = tokio::fs::read_to_string(path).await.map_err(|err| {
                Error::new(format!(
                    "Error reading results file '{}': {}",
                    path.display(),
                    err
                ))
            })?;
            // this target is a results file rather than a results directory so it has
            // no directory of its own to collect entities from
            let results_req = build_request(
                cmd,
                sha256,
                tool,
                results_string,
                Vec::new(),
                HashMap::default(),
            );
            upload!(thorium, results_req, sha256, tool, cmd).await;
        }
    } else {
        // build the path to the entities found outside of any tool sub-directory
        let entities_path = path.join(&cmd.entities);
        let mut unnested_result_files = Vec::new();
        let mut tool_subdirs = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&path).await.map_err(|err| {
            Error::new(format!(
                "Error reading directory '{}': {}",
                path.display(),
                err
            ))
        })?;
        while let Some(entry) = read_dir.next_entry().await.map_err(|err| {
            Error::new(format!(
                "Error reading directory '{}': {}",
                path.display(),
                err
            ))
        })? {
            let inner_path = entry.path();
            // never sweep the entities file up as a result file attachment
            if inner_path == entities_path {
                continue;
            }
            // check if the entry is a file
            if entry
                .file_type()
                .await
                .map_err(|err| {
                    Error::new(format!(
                        "Error reading metadata for file '{}': {}",
                        inner_path.display(),
                        err
                    ))
                })?
                .is_file()
            {
                // add this file to the list of files not nested in tool directories
                let on_disk = OnDiskFile::new(inner_path).trim_prefix(path);
                unnested_result_files.push(on_disk);
            } else {
                // this is a directory, so assume it's a tool and add it to the list
                tool_subdirs.push(inner_path);
            }
        }
        // upload results by tool sub-directory
        upload_tool_subdirs(thorium, cmd, sha256, tool_subdirs).await?;
        if !unnested_result_files.is_empty() && cmd.tools.is_empty() {
            // log an error if there are unnested result files but no --tools were given
            UploadLine::error(
                sha256,
                "-",
                &Error::new(
                    "Found result files not in tool sub-directories but no tools were given with flag '--tools'",
                ),
            );
        } else if !cmd.tools.is_empty() {
            // remove the main results file so we don't upload it twice (as main results and as attachment)
            if let Some(index) = unnested_result_files.iter().position(|file| {
                file.path
                    .file_name()
                    // this is the main results file if its name matches the results set in the command
                    .is_some_and(|file_name| cmd.results.as_str() == file_name)
            }) {
                unnested_result_files.swap_remove(index);
            }
            // collect any entities found outside of a tool sub-directory
            let entities = match collect_entities(&entities_path).await {
                Ok(entities) => entities,
                Err(err) => {
                    // log an error that we couldn't collect these entities for each tool
                    for tool in &cmd.tools {
                        UploadLine::error(sha256, tool, &err);
                    }
                    return Ok(());
                }
            };
            // upload unnested files to every tool given by the cmd flags
            upload_tool_flags(thorium, cmd, sha256, path, unnested_result_files, entities).await?;
        }
    }
    Ok(())
}

/// Crawl target directories and upload their results to Thorium
///
/// # Arguments
///
/// * `thorium` - A Thorium client
/// * `cmd` - The full result upload command/args
pub async fn upload(thorium: &Thorium, cmd: &UploadResults) -> Result<(), Error> {
    // the same file can't be both the main results and the entities for those results
    if cmd.entities == cmd.results {
        return Err(Error::new(format!(
            "The entities file name cannot match the results file name: '{}'",
            cmd.results
        )));
    }
    // print the header
    UploadLine::header();
    // crawl over each path and upload them if they are new
    for target in &cmd.targets {
        // check if this file name in the path is already a sha256
        let path = Path::new(target);
        if let Some(file_name) = path.file_name().map(OsStr::to_string_lossy)
            && is_sha256(&file_name)
        {
            // upload this sha256
            upload_helper(thorium, cmd, &file_name, path).await?;
        } else {
            // not a sha256, so look for sha256 sub-directories/files
            let mut read_dir = tokio::fs::read_dir(target)
                .await
                .map_err(|err| Error::new(format!("Error reading target '{target}': {err}")))?;
            while let Some(entry) = read_dir
                .next_entry()
                .await
                .map_err(|err| Error::new(format!("Error reading target '{target}': {err}")))?
            {
                let path = entry.path();
                // only process files/directories that are sha256's
                if let Some(file_name) = path.file_name().map(OsStr::to_string_lossy)
                    && is_sha256(&file_name)
                {
                    upload_helper(thorium, cmd, &file_name, &path).await?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{collect_entities, entities_msg, group_entities, is_sha256};
    use std::collections::HashMap;
    use std::path::Path;
    use thorium::models::{Buffer, EntityKinds, EntityMetadataRequest, EntityRequest};

    /// Serialize a list of entity requests the way a tool would write them to disk
    ///
    /// # Arguments
    ///
    /// * `reqs` - The entity requests to serialize
    fn serialize_reqs(reqs: Vec<EntityRequest>) -> Vec<u8> {
        serde_json::to_vec(&reqs).expect("failed to serialize entity requests")
    }

    /// Build a simple entity request of a specific kind
    ///
    /// # Arguments
    ///
    /// * `name` - The name to give this entity
    /// * `metadata` - The metadata determining what kind of entity this is
    fn gen_req(name: &str, metadata: EntityMetadataRequest) -> EntityRequest {
        EntityRequest::new(name, metadata, ["corn"])
    }

    /// Deserialize the entity requests in a buffer back into a list
    ///
    /// # Arguments
    ///
    /// * `buff` - The buffer containing the serialized entity requests
    fn parse_buffer(buff: &Buffer) -> Vec<EntityRequest> {
        serde_json::from_slice(&buff.data).expect("failed to deserialize entity requests")
    }

    #[test]
    fn test_group_entities_empty() {
        // serialize an empty list of entity requests
        let data = serialize_reqs(Vec::new());
        // group our entities by kind
        let entities = group_entities(&data, Path::new("entities.json")).unwrap();
        // an empty list must not add any kinds since the API rejects a count of 0
        assert!(entities.is_empty());
    }

    #[test]
    fn test_group_entities_single_kind() {
        // serialize a few entity requests that are all the same kind
        let data = serialize_reqs(vec![
            gen_req("corn", EntityMetadataRequest::Other),
            gen_req("maize", EntityMetadataRequest::Other),
            gen_req("sweetcorn", EntityMetadataRequest::Other),
        ]);
        // group our entities by kind
        let entities = group_entities(&data, Path::new("entities.json")).unwrap();
        // all 3 entities should be in a single kind
        assert_eq!(entities.len(), 1);
        // get the entities for our single kind
        let (count, buff) = entities.get(&EntityKinds::Other).expect("missing Other");
        // all 3 of our entities should have been counted
        assert_eq!(*count, 3);
        // all 3 of our entities should be in this buffer
        assert_eq!(parse_buffer(buff).len(), 3);
    }

    #[test]
    fn test_group_entities_mixed_kinds() {
        // serialize some entity requests spread across two different kinds
        let data = serialize_reqs(vec![
            gen_req("corn", EntityMetadataRequest::Other),
            gen_req("tree", EntityMetadataRequest::WindowsProcessTree),
            gen_req("maize", EntityMetadataRequest::Other),
        ]);
        // group our entities by kind
        let entities = group_entities(&data, Path::new("entities.json")).unwrap();
        // our entities should have been split across two kinds
        assert_eq!(entities.len(), 2);
        // check that each kind has the right count and only contains its own entities
        for (kind, expected) in [
            (EntityKinds::Other, 2),
            (EntityKinds::WindowsProcessTree, 1),
        ] {
            // get the entities for this kind
            let (count, buff) = entities.get(&kind).expect("missing kind");
            // this kind should have the number of entities we added for it
            assert_eq!(*count, expected);
            // deserialize the entities in this kinds buffer
            let parsed = parse_buffer(buff);
            // the buffer should agree with the count we sent alongside it
            assert_eq!(parsed.len(), expected);
            // every entity in this buffer should actually be of this kind
            assert!(parsed.iter().all(|req| req.kind() == kind));
        }
    }

    #[test]
    fn test_group_entities_malformed() {
        // try to group entities from data that isn't valid json
        let error = group_entities(b"{ not json", Path::new("/corn/entities.json"))
            .expect_err("malformed entities should be an error");
        // the error should tell the user which file failed to parse
        assert!(
            error
                .msg()
                .is_some_and(|msg| msg.contains("/corn/entities.json"))
        );
    }

    #[test]
    fn test_group_entities_not_a_list() {
        // a single entity request instead of a list of them isn't a valid entities file
        let data = serde_json::to_vec(&gen_req("corn", EntityMetadataRequest::Other)).unwrap();
        // try to group entities from a single request
        group_entities(&data, Path::new("entities.json"))
            .expect_err("a single entity request should be an error");
    }

    #[test]
    fn test_entities_msg_empty() {
        // a result with no entities should just use our placeholder
        assert_eq!(entities_msg(&HashMap::default()), "-");
    }

    #[test]
    fn test_entities_msg_counts() {
        // build a map of entities spread across two kinds
        let entities = HashMap::from([
            (EntityKinds::Other, (3, Buffer::new("[]"))),
            (EntityKinds::WindowsProcessTree, (1, Buffer::new("[]"))),
        ]);
        // build the message describing these entities
        let msg = entities_msg(&entities);
        // the message should report the total entities and how many kinds they span
        assert_eq!(msg, "4 entities/2 kinds");
    }

    #[tokio::test]
    async fn test_collect_entities_missing_file() {
        // build a path to an entities file that doesn't exist
        let path = std::env::temp_dir().join("thorctl-test-entities-that-do-not-exist.json");
        // collecting entities that don't exist should just find no entities
        let entities = collect_entities(&path)
            .await
            .expect("a missing entities file should not be an error");
        assert!(entities.is_empty());
    }

    #[test]
    fn test_is_sha256_valid() {
        // 64 hex digits
        let s = "0123456789abcdefABCDEF0123456789abcdefABCDEF0123456789abcdefABCD";
        // make sure there are actully 64 characters above
        assert_eq!(s.len(), 64);
        // test the function itself
        assert!(is_sha256(s));
    }

    #[test]
    fn test_is_sha256_invalid_nonhex() {
        // 64 hex digits (except an invalid 'G' at the end)
        let s = "0123456789abcdefABCDEF0123456789abcdefABCDEF0123456789abcdefABCG";
        // make sure there are actully 64 characters above
        assert_eq!(s.len(), 64);
        // test the function itself
        assert!(!is_sha256(s));
    }

    #[test]
    fn test_is_sha256_invalid_length() {
        let short = "abcd";
        assert!(!is_sha256(short));
        let long = "a".repeat(65);
        assert!(!is_sha256(&long));
    }

    #[test]
    fn test_is_sha256_invalid_characters() {
        let invalid = "!".repeat(64);
        assert!(!is_sha256(&invalid));
    }

    #[test]
    fn test_is_sha256_mixed_invalid() {
        let mut mixed = "a".repeat(63);
        mixed.push('!');
        assert!(!is_sha256(&mixed));
    }

    #[test]
    fn test_is_sha256_invalid_spaces() {
        let mut mixed = "a".repeat(63);
        mixed.push(' ');
        assert!(!is_sha256(&mixed));
    }
}

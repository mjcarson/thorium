//! Utilities for loading ephemeral files to attach to reaction requests
//!
//! Ephemeral files are uploaded inline with a reaction instead of being stored as samples: the
//! API base64 encodes them into `ReactionRequest.buffers`, writes them to S3 under the
//! reaction's id, hands them to the agent at job setup, and discards them once the reaction
//! finishes.
//!
//! Size is advisory here. This module reports how large a payload will be but never refuses to
//! build one; only malformed input (an unreadable path, a name the API would reject, or a
//! duplicate name) is an error.

use std::collections::HashSet;
use std::path::PathBuf;
use thorium::Error;
use thorium::models::ReactionRequest;

/// The delimiter separating an ephemeral file's name from its path
///
/// `thorctl reactions create` overrides this with its own `--delimiter`, which it already
/// shares with `--kwargs`/`--tags`. Commands with no delimiter flag of their own use this.
pub const DEFAULT_DELIMITER: char = '=';

/// The base64 encoded ephemeral payload of a single request at which we warn the user
pub const WARN_ENCODED_BYTES: u64 = 8 * 1024 * 1024;

/// The maximum length, in bytes, of an ephemeral file name
///
/// Mirrors the API's `bounder::file_name(name, "ephemeral file names", 1, 32)`.
const MAX_NAME_LEN: usize = 32;

/// A set of ephemeral files read off disk and ready to attach to reaction requests
///
/// The raw bytes stay un-encoded so [`ReactionRequest::buffer`] remains the single place that
/// base64 encodes them.
#[derive(Debug, Default)]
pub struct EphemeralFiles {
    /// Each file's ephemeral name and its raw contents, in the order the user gave them
    files: Vec<(String, Vec<u8>)>,
}

impl EphemeralFiles {
    /// Parse every `--ephemeral` target into an ephemeral name and the path to read it from
    ///
    /// This is split out from [`EphemeralFiles::load`] so all of the parsing and validation
    /// happens before we touch the filesystem, letting a typo fail instantly instead of after
    /// reading a pile of files.
    ///
    /// # Arguments
    ///
    /// * `raw` - The raw `--ephemeral` targets from the command
    /// * `delimiter` - The character separating an explicit name from its path
    pub fn plan_targets<'a, I>(raw: I, delimiter: char) -> Result<Vec<(String, PathBuf)>, Error>
    where
        I: IntoIterator<Item = &'a String>,
    {
        let raw = raw.into_iter();
        // pre-size off the target count since every target yields exactly one file
        let (size_hint, _) = raw.size_hint();
        let mut planned = Vec::with_capacity(size_hint);
        let mut seen: HashSet<String> = HashSet::with_capacity(size_hint);
        for target in raw {
            // split this target into an ephemeral name and the path to read it from
            let (name, path) = parse_target(target, delimiter)?;
            // reject a repeated name since the API's S3 write skips a key that already exists,
            // silently keeping only the first file's contents
            if !seen.insert(name.clone()) {
                return Err(Error::new(format!(
                    "Duplicate ephemeral file name '{name}' in '--ephemeral {target}'! \
                     Rename one of them with '--ephemeral <NAME>{delimiter}<PATH>'"
                )));
            }
            planned.push((name, path));
        }
        Ok(planned)
    }

    /// Parse and read every `--ephemeral` target the user gave us
    ///
    /// # Arguments
    ///
    /// * `raw` - The raw `--ephemeral` targets from the command
    /// * `delimiter` - The character separating an explicit name from its path
    pub async fn load<'a, I>(raw: I, delimiter: char) -> Result<Self, Error>
    where
        I: IntoIterator<Item = &'a String>,
    {
        // parse and validate every target before reading anything off disk
        let planned = Self::plan_targets(raw, delimiter)?;
        let mut files = Vec::with_capacity(planned.len());
        for (name, path) in planned {
            // read the file, reporting the path the user actually typed when it fails
            let bytes = tokio::fs::read(&path).await.map_err(|err| {
                Error::new(format!(
                    "Unable to read ephemeral file '{}': {err}",
                    path.to_string_lossy()
                ))
            })?;
            files.push((name, bytes));
        }
        Ok(Self { files })
    }

    /// Returns true if the user gave us no ephemeral files
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The number of ephemeral files we loaded
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// The ephemeral names we will send, for logging in dry-run mode
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|(name, _)| name.as_str())
    }

    /// The total size in bytes these files occupy in a reaction request once base64 encoded
    ///
    /// Base64 turns every 3 raw bytes into 4 and pads the final group, so an `n` byte file
    /// serializes to `4 * ceil(n / 3)` bytes. The base64 alphabet needs no JSON string
    /// escaping, so the only thing this ignores is the handful of framing bytes per file.
    pub fn encoded_len(&self) -> u64 {
        self.files
            .iter()
            .map(|(_, bytes)| 4 * (bytes.len() as u64).div_ceil(3))
            .sum()
    }

    /// Attach every loaded file to a reaction request as a base64 encoded buffer
    ///
    /// # Arguments
    ///
    /// * `req` - The reaction request to attach our ephemeral files to
    #[must_use]
    pub fn attach(&self, mut req: ReactionRequest) -> ReactionRequest {
        // base64 encode each file into the request's buffer map
        for (name, bytes) in &self.files {
            req = req.buffer(name.clone(), bytes);
        }
        req
    }
}

/// Split a single `--ephemeral` target into its ephemeral name and the path to read
///
/// A target containing the delimiter is split on its *first* occurrence into
/// `<NAME><DELIMITER><PATH>`, so a path may itself contain the delimiter as long as the name is
/// given explicitly. Anything else is a bare path whose base name becomes the name.
///
/// # Arguments
///
/// * `target` - The raw `--ephemeral` target
/// * `delimiter` - The character separating an explicit name from its path
fn parse_target(target: &str, delimiter: char) -> Result<(String, PathBuf), Error> {
    // an explicit target names the file before the first delimiter
    if let Some((name, path)) = target.split_once(delimiter) {
        // an empty path is almost certainly a typo, so say so rather than failing on read
        if path.is_empty() {
            return Err(Error::new(format!(
                "Invalid ephemeral file '--ephemeral {target}': no path given after '{delimiter}'"
            )));
        }
        // make sure the explicit name is one the API will accept
        validate_name(name).map_err(|err| {
            Error::new(format!(
                "Invalid ephemeral file '--ephemeral {target}': {}",
                err.msg().unwrap_or_default()
            ))
        })?;
        return Ok((name.to_owned(), PathBuf::from(path)));
    }
    // otherwise this is a bare path, so derive the ephemeral name from its base name
    let path = PathBuf::from(target);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| {
            Error::new(format!(
                "Unable to derive an ephemeral file name from '--ephemeral {target}'! \
                 Name it explicitly with '--ephemeral <NAME>{delimiter}{target}'"
            ))
        })?;
    // plenty of ordinary file names ('_' or spaces) break the API's rules, so when a derived
    // name fails point the user straight at the explicit form instead of just rejecting it
    validate_name(&name).map_err(|err| {
        Error::new(format!(
            "Cannot use '{name}' as an ephemeral file name: {}. \
             Give it a valid name with '--ephemeral <NAME>{delimiter}{target}'",
            err.msg().unwrap_or_default()
        ))
    })?;
    Ok((name, path))
}

/// Check an ephemeral file name against the same rules the API enforces
///
/// This mirrors the API's `bounder::file_name(name, "ephemeral file names", 1, 32)`: the name
/// is bounded by *byte* length, must be 1-32 long, cannot be made up entirely of '.'s, and may
/// only contain alphanumeric characters, '-', and '.'. Checking locally means a bad name fails
/// before we read a single file or open a connection.
///
/// # Arguments
///
/// * `name` - The ephemeral file name to check
fn validate_name(name: &str) -> Result<(), Error> {
    // the API bounds this by byte length, so measure bytes and not chars to match it exactly
    if name.is_empty() || name.len() > MAX_NAME_LEN {
        return Err(Error::new(format!(
            "ephemeral file names must be between 1 and {MAX_NAME_LEN} characters"
        )));
    }
    // a name of all '.'s would resolve to the reaction's own directory in S3
    if name.chars().all(|chr| chr == '.') {
        return Err(Error::new("ephemeral file names cannot just be '.'s"));
    }
    // only characters that are safe in an S3 key and on a worker's disk are allowed
    if let Some(bad) = name
        .chars()
        .find(|chr| !(chr.is_alphanumeric() || *chr == '-' || *chr == '.'))
    {
        return Err(Error::new(format!(
            "ephemeral file names must be only alphanumeric or '-'/'.' (found '{bad}')"
        )));
    }
    Ok(())
}

/// Format a byte count as a human readable size for warnings and errors
///
/// # Arguments
///
/// * `bytes` - The byte count to format
pub fn fmt_bytes(bytes: u64) -> String {
    // the binary unit prefixes we can scale up through
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    // walk up the prefixes until the value fits in three digits
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    // whole bytes read better without a decimal point
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the owned target list `plan_targets`/`load` expect
    ///
    /// # Arguments
    ///
    /// * `targets` - The raw targets to own
    fn targets(targets: &[&str]) -> Vec<String> {
        targets.iter().map(|target| (*target).to_string()).collect()
    }

    #[test]
    fn parse_target_bare_path_uses_base_name() {
        // a bare path takes its ephemeral name from the file name
        let (name, path) = parse_target("./dir/rules.yara", '=').unwrap();
        assert_eq!(name, "rules.yara");
        assert_eq!(path, PathBuf::from("./dir/rules.yara"));
    }

    #[test]
    fn parse_target_explicit_name() {
        // an explicit name overrides the base name
        let (name, path) = parse_target("cfg.json=./a/prod-config.json", '=').unwrap();
        assert_eq!(name, "cfg.json");
        assert_eq!(path, PathBuf::from("./a/prod-config.json"));
    }

    #[test]
    fn parse_target_splits_on_first_delimiter_only() {
        // splitting on the first delimiter lets an explicitly named path contain more of them
        let (name, path) = parse_target("cfg.json=./a=b/c.json", '=').unwrap();
        assert_eq!(name, "cfg.json");
        assert_eq!(path, PathBuf::from("./a=b/c.json"));
    }

    #[test]
    fn parse_target_honors_delimiter() {
        // a command with a different delimiter splits on that instead
        let (name, path) = parse_target("cfg.json|./a=b.json", '|').unwrap();
        assert_eq!(name, "cfg.json");
        assert_eq!(path, PathBuf::from("./a=b.json"));
    }

    #[test]
    fn parse_target_rejects_empty_path() {
        // a name with nothing after the delimiter is a typo
        assert!(parse_target("cfg.json=", '=').is_err());
    }

    #[test]
    fn parse_target_rejects_invalid_derived_name() {
        // '_' is not in the API's allowed charset, so a bare path with one must fail...
        let err = parse_target("./my_file.txt", '=').unwrap_err();
        // ...and the error has to point at the explicit form that works around it
        assert!(
            err.msg()
                .unwrap_or_default()
                .contains("--ephemeral <NAME>=./my_file.txt")
        );
    }

    #[test]
    fn parse_target_rejects_paths_without_a_base_name() {
        // there's no file name to derive from either of these
        assert!(parse_target("..", '=').is_err());
        assert!(parse_target("/", '=').is_err());
    }

    #[test]
    fn validate_name_accepts_valid_names() {
        // every name the API's bounder would allow
        for name in ["a", "rules.yara", "my-file.1", &"a".repeat(MAX_NAME_LEN)] {
            assert!(validate_name(name).is_ok(), "'{name}' should be valid");
        }
    }

    #[test]
    fn validate_name_rejects_invalid_names() {
        // every name the API's bounder would reject
        for name in [
            "",
            &"a".repeat(MAX_NAME_LEN + 1),
            ".",
            "..",
            "...",
            "my_file",
            "my file",
            "a/b",
            "a=b",
        ] {
            assert!(validate_name(name).is_err(), "'{name}' should be invalid");
        }
    }

    #[test]
    fn validate_name_bounds_by_bytes_not_chars() {
        // the API bounds by byte length, so 32 multibyte chars must fail here too or we'd
        // pass locally and take a 400 from the server
        let name = "é".repeat(MAX_NAME_LEN);
        assert_eq!(name.chars().count(), MAX_NAME_LEN);
        assert!(name.len() > MAX_NAME_LEN);
        assert!(validate_name(&name).is_err());
    }

    #[test]
    fn plan_targets_rejects_duplicate_names() {
        // two targets resolving to the same name would silently drop one server side
        assert!(EphemeralFiles::plan_targets(&targets(&["a.txt=./x", "a.txt=./y"]), '=').is_err());
        // the same collision by way of two different directories
        assert!(EphemeralFiles::plan_targets(&targets(&["./x/a.txt", "./y/a.txt"]), '=').is_err());
    }

    #[test]
    fn plan_targets_allows_distinct_names() {
        // distinct names are fine, and come back in the order they were given
        let planned =
            EphemeralFiles::plan_targets(&targets(&["./x/a.txt", "b.txt=./y/z"]), '=').unwrap();
        assert_eq!(planned.len(), 2);
        assert_eq!(planned[0].0, "a.txt");
        assert_eq!(planned[1].0, "b.txt");
    }

    #[tokio::test]
    async fn load_reads_files_and_attaches_them() {
        // this crate's own Cargo.toml is a real file with a name the API accepts
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let files = EphemeralFiles::load(&targets(&[path]), '=').await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files.names().collect::<Vec<&str>>(), vec!["Cargo.toml"]);
        // attaching it puts the encoded contents in the request under that name
        let req = files.attach(ReactionRequest::new("group", "pipeline"));
        assert!(req.buffers.contains_key("Cargo.toml"));
        // base64 has no padding to trim here beyond the final group, so sizes line up
        assert_eq!(req.buffers["Cargo.toml"].len() as u64, files.encoded_len());
    }

    #[tokio::test]
    async fn load_reports_the_path_it_could_not_read() {
        // a path that doesn't exist must name itself in the error, not just say "not found"
        let err = EphemeralFiles::load(&targets(&["./no-such-ephemeral.txt"]), '=')
            .await
            .unwrap_err();
        assert!(
            err.msg()
                .unwrap_or_default()
                .contains("./no-such-ephemeral.txt")
        );
    }

    #[test]
    fn encoded_len_matches_base64_growth() {
        // base64 pads to a multiple of 4, so check every remainder of the 3 byte group
        for (raw, encoded) in [(0, 0), (1, 4), (2, 4), (3, 4), (4, 8)] {
            let files = EphemeralFiles {
                files: vec![("a".to_string(), vec![0; raw])],
            };
            assert_eq!(files.encoded_len(), encoded, "{raw} raw bytes");
        }
    }

    #[test]
    fn fmt_bytes_scales_units() {
        // whole bytes stay whole and larger values scale to one decimal place
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_bytes(1536), "1.5 KiB");
        assert_eq!(fmt_bytes(4 * 1024 * 1024), "4.0 MiB");
    }
}

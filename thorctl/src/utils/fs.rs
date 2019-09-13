//! Utility functions relating to file system interactions

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use regex::{Regex, RegexSet};
use thorium::Error;
use walkdir::{DirEntry, WalkDir};

/// Recursively walks through the target and returns all file
/// entries filtered based on user preference
///
/// # Arguments
///
/// * `target` - The raw string path to the target file/directory
/// * `filter` - Regex set used to determine which files to include
/// * `skip` - Regex set used to determine which files to skip
/// * `include_hidden` - When set, hidden files/folders will not be filtered
/// * `filter_dirs` - When set, include/skip filters will be applied to *directories* as well as files
pub fn get_filtered_entries(
    target: &String,
    filter: &RegexSet,
    skip: &RegexSet,
    mut include_hidden: bool,
    filter_dirs: bool,
) -> Vec<DirEntry> {
    // include hidden directories/files if the target itself is a hidden file/directory
    if is_hidden(target) {
        include_hidden = true;
    }
    let target_path = Path::new(target);
    // if target is a directory, recursively walk, including/ignoring entries based on filter/skip settings;
    // if target is a file, it will be the only entry
    WalkDir::new(target)
        .into_iter()
        // filter based on filter/skip/include_hidden/filter_dirs settings;
        // don't filter the target; assume that the user-given target should always be walked
        .filter_entry(|entry| {
            if entry.path() == target_path {
                true
            } else {
                filter_entry(entry, filter, skip, include_hidden, filter_dirs)
            }
        })
        .filter_map(std::result::Result::ok)
        // include only files
        .filter(|entry| {
            if let Ok(metadata) = entry.metadata() {
                return metadata.is_file();
            }
            false
        })
        .collect()
}

lazy_static::lazy_static! {
    /// Regex used to pattern match hidden files/directories
    static ref HIDDEN_REGEX: Regex = Regex::new(r"^\.+[^\.]+\b").unwrap();
}

/// Checks if a target file/directory is hidden
///
/// # Arguments
///
/// * `target` - The target file/directory path
fn is_hidden<T: AsRef<str>>(target: T) -> bool {
    return HIDDEN_REGEX.is_match(target.as_ref());
}

/// Returns true if a [`DirEntry`] should be included in a directory walk
///
/// Whether or not an entry is filtered depends on user settings, given as
/// parameters to this function.
///
/// # Arguments
///
/// * `entry` - The file/folder to check
/// * `filter` - A set of regular expressions to use to determine what files to act on
/// * `skip` - A set of regular expressions to use to determine what files to skip
/// * `include_hidden` - When set, hidden files/folders will not be filtered
/// * `filter_dirs` - When set, include/skip filters will be applied to *directories* as well as files
fn filter_entry(
    entry: &DirEntry,
    filter: &RegexSet,
    skip: &RegexSet,
    include_hidden: bool,
    filter_dirs: bool,
) -> bool {
    // get the name of the file as a string
    let name = match entry.file_name().to_str() {
        Some(name) => name,
        None => return false,
    };
    // if directories are not filtered, just check if hidden
    if entry.file_type().is_dir() && !filter_dirs {
        // if hidden should be included, don't filter; otherwise filter hidden
        if include_hidden {
            return true;
        }
        return !is_hidden(name);
    }
    // skip hidden files/directories if include_hidden is not set
    let skip_with_hidden =
        RegexSet::new([skip.patterns(), &[HIDDEN_REGEX.to_string()]].concat()).unwrap();
    let skip = if include_hidden {
        skip
    } else {
        &skip_with_hidden
    };
    // depending on what args are set use the right filters
    match (filter.is_empty(), skip.is_empty()) {
        (false, false) => filter.is_match(name) && !skip.is_match(name),
        (true, false) => !skip.is_match(name),
        (false, true) => filter.is_match(name),
        (true, true) => true,
    }
}

/// Retrieve a set of lines from a file as Strings
///
/// # Arguments
///
/// * `path` - The path to the file to read from
pub async fn lines_set_from_file(path: &Path) -> Result<HashSet<String>, Error> {
    // read the file into a raw String
    let raw = match tokio::fs::read_to_string(path).await {
        Ok(raw) => raw,
        Err(err) => {
            return Err(Error::new(format!(
                "Unable to read file \"{}\": {}",
                path.to_string_lossy(),
                err
            )))
        }
    };
    // separate the file by lines, filter out all empty lines, and collect to a set
    Ok(raw
        .lines()
        .filter(|&line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

/// Prepend "./" or ".\" to relative paths to make it clearer that the
/// output is a path
///
/// # Arguments
///
/// * `output` - The output path to save file details in
pub fn prepend_current_dir(output: &Path) -> String {
    if output.is_relative() {
        // set patterns for Unix-style operating systems
        #[cfg(unix)]
        const CURRENT_DIR_PATTERN: &str = "./";
        #[cfg(unix)]
        const PARENT_DIR_PATTERN: &str = "../";
        // set patterns for Windows
        #[cfg(target_os = "windows")]
        const CURRENT_DIR_PATTERN: &str = ".\\";
        #[cfg(target_os = "windows")]
        const PARENT_DIR_PATTERN: &str = "..\\";
        if !output.starts_with(CURRENT_DIR_PATTERN) && !output.starts_with(PARENT_DIR_PATTERN) {
            return PathBuf::from(".")
                .join(output)
                .to_string_lossy()
                .to_string();
        }
    }
    output.to_string_lossy().to_string()
}

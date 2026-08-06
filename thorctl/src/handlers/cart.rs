use cart_rs::{CartKey, CartStream};
use regex::RegexSet;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use thorium::Error;
use tokio::{
    fs::{File, OpenOptions},
    io::BufStream,
    task::JoinError,
};
use uuid::Uuid;

use crate::args::{Args, cart::Cart};
use crate::utils;

/// Handle the cart command
///
/// # Arguments
///
/// * `args` - The arguments passed to Thorctl
/// * `cmd` - The cart command to execute
pub async fn handle(args: &Args, cmd: &Cart) -> Result<(), Error> {
    // build the set of regexs to determine which files to include or skip
    let filter = RegexSet::new(&cmd.filter)?;
    let skip = RegexSet::new(&cmd.skip)?;
    // prepare data for saving to between tasks
    let cmd = Arc::new(cmd.clone());
    // derive our cart key; CartKey is Copy and 16 bytes so it needs no Arc to share between tasks
    let key = CartKey::from_password(&cmd.password)?;
    // construct base output path
    let base_out_path = if cmd.in_place {
        &cmd.temp_dir
    } else {
        &cmd.output
    };
    tokio::fs::create_dir_all(base_out_path).await?;
    // print headers
    CartLine::header();
    // iterate over targets and uncart them
    utils::fs::process_async_walk(
        cmd.targets.clone().into_iter(),
        |path| {
            // make a copy of the path for printing errors
            let path_copy = path.clone();
            // copy data for this task
            let base_out_path = base_out_path.clone();
            let cmd = cmd.clone();
            async move {
                // cart the entry in a new task
                let cart_result: Result<Result<PathBuf, Error>, JoinError> =
                    tokio::spawn(cart_path(path, base_out_path, cmd, key)).await;
                // log the result
                match cart_result {
                    Ok(Ok(out_path)) => CartLine::success(&path_copy, &out_path),
                    Ok(Err(err)) => CartLine::error(&path_copy, &err),
                    Err(err) => CartLine::error(&path_copy, &Error::from(err)),
                }
            }
        },
        CartLine::error,
        &filter,
        &skip,
        cmd.include_hidden,
        // run process concurrently based on the number of workers set
        args.workers,
    )
    .await;
    // remove temporary drectory after in-place conversion
    if cmd.in_place {
        tokio::fs::remove_dir_all(base_out_path).await?;
    }
    Ok(())
}

/// Cart the file at the given path
///
/// Returns the path to the carted file or an error on failure
///
/// # Arguments
///
/// * `path` - The path to cart
/// * `target_path` - The path to the target
/// * `base_out_path` - The base output path
/// * `cmd` - The cart command including user options
/// * `key` - The key used to cart the file
async fn cart_path(
    path: PathBuf,
    base_out_path: PathBuf,
    cmd: Arc<Cart>,
    key: CartKey,
) -> Result<PathBuf, Error> {
    // read input file
    let input: File = OpenOptions::new()
        .read(true)
        .write(false)
        .open(&path)
        .await?;
    // generate output path and create necessary directories
    let mut out_path = construct_out_path(
        &path,
        &base_out_path,
        cmd.preserve_dir_structure,
        cmd.no_extension,
        cmd.in_place,
    )?;
    if let Some(out_path_parent) = out_path.parent() {
        tokio::fs::create_dir_all(&out_path_parent).await?;
    }
    // open output carted file
    let mut output_cart = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&out_path)
        .await?;
    // create a stream to cart the file and copy the stream's contents to the output path
    let mut cart_stream = CartStream::builder(key, BufStream::new(input))
        .version(cmd.cart_version)
        .build()?;
    if let Err(err) = tokio::io::copy(&mut cart_stream, &mut output_cart).await {
        // if an error occurred while carting, delete the output file and return the error
        drop(output_cart);
        tokio::fs::remove_file(&out_path).await?;
        return Err(Error::from(err));
    }
    // if conversion is in-place, replace the input file with the output cart
    if cmd.in_place {
        // replace the original file
        tokio::fs::rename(&out_path, &path).await?;
        if cmd.no_extension {
            out_path = path;
        } else {
            out_path.clone_from(&path);
            out_path.as_mut_os_string().push(".cart");
            // rename the file to have the '.cart' extension
            tokio::fs::rename(&path, &out_path).await?;
        }
    }
    Ok(out_path)
}

/// Construct the output path for the carted file
///
/// # Arguments
///
/// * `path` - The path to the input cart file
/// * `base_out_path` - The base output path
/// * `preserve_dir_structure` - User flag to preserve the directory structure of the target
/// * `no_extension` - User flag to refrain from adding the ".cart" extension to the file
/// * `in_place` - User flag to convert in-place
fn construct_out_path(
    path: &Path,
    base_out_path: &Path,
    preserve_dir_structure: bool,
    no_extension: bool,
    in_place: bool,
) -> Result<PathBuf, Error> {
    // start with the base output path
    let mut out_path = PathBuf::from(base_out_path);
    // preserve the target structure if flag is set
    if preserve_dir_structure {
        // append target path
        if path.is_absolute() {
            // path already has leading '/' so push the raw OsStr
            out_path.as_mut_os_string().push(path);
        } else {
            // path does not have leading '/' so push it as a path
            out_path.push(path);
        }
    } else {
        // if directory structure is not preserved, just add the filename;
        // unchecked because every entry given to this function is a file
        let file_name = path.file_name().ok_or(Error::Generic(String::from(
            "Error creating output path! The given entry is not a file",
        )))?;
        out_path.push(file_name);
    }
    if in_place {
        out_path.set_file_name(format!(".temp-{}.cart", Uuid::new_v4()));
    } else if !no_extension {
        out_path.as_mut_os_string().push(".cart");
    }
    Ok(out_path)
}

/// A single line for a file uncart log
struct CartLine;

impl CartLine {
    /// Print this log lines header
    pub fn header() {
        println!("{:<48} | {:<48} | {:<48}", "INPUT", "OUTPUT", "MESSAGE",);
        println!("{:-<49}+{:-<50}+{:-<48}", "", "", "");
    }

    /// Log successful carting
    ///
    /// # Arguments
    ///
    /// * `input_path` - The path to the input cart file
    /// * `output_path` - The path to the output uncarted file
    pub fn success<P: AsRef<Path>>(input_path: P, output_path: P) {
        Self::print(input_path.as_ref(), output_path.as_ref(), "");
    }

    /// Log an error in carting
    ///
    /// # Arguments
    ///
    /// * `input_path` - The path to the input cart file
    /// * `output_path` - The path to the output uncarted file
    pub fn error(input_path: &Path, err: &Error) {
        Self::print(
            input_path,
            Path::new("."),
            err.msg()
                .unwrap_or(String::from("Unknown error carting file")),
        );
    }

    /// Print a line
    ///
    /// # Arguments
    ///
    /// * `input_path` - The path to the input file
    /// * `output_path` - The path to the output file
    /// * `msg` - The message to print
    fn print<T: AsRef<str>>(input_path: &Path, output_path: &Path, msg: T) {
        if let (Some(cart_path_str), Some(output_path_str)) =
            (input_path.to_str(), output_path.to_str())
        {
            println!(
                "{:<48} | {:<48} | {}",
                cart_path_str,
                output_path_str,
                msg.as_ref()
            );
        } else {
            println!(
                "{:<48} | {:<48} | Paths contain non-ASCII characters",
                "-", "-"
            );
        }
    }
}

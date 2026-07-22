//! Handles scoped token commands

use chrono::{DateTime, NaiveDateTime, Utc};
use thorium::client::conf::ActiveScopedToken;
use thorium::models::{ScopedToken, ScopedTokenRequest, ScopedTokenUpdate};
use thorium::{CtlConf, Error, Thorium};

use crate::args::Args;
use crate::args::scoped_tokens::{
    ActivateScopedToken, CreateScopedToken, CurrentScopedToken, DeleteScopedTokens,
    ListScopedTokens, ScopedTokens, UpdateScopedToken,
};
use crate::utils;

/// Parse an optional expiration date with the given format
///
/// # Arguments
///
/// * `expires` - The raw expiration date to parse if one was given
/// * `date_fmt` - The chrono strftime format the date is in
fn parse_expires(expires: Option<&String>, date_fmt: &str) -> Result<Option<DateTime<Utc>>, Error> {
    // parse our expiration date if one was given
    match expires {
        Some(raw) => Ok(Some(
            NaiveDateTime::parse_from_str(raw, date_fmt)?.and_utc(),
        )),
        None => Ok(None),
    }
}

/// Print a scoped tokens info
///
/// The scoped tokens value is only printed when `show_token` is set.
///
/// # Arguments
///
/// * `scoped` - The scoped token to print
/// * `show_token` - Whether to show this scoped tokens value
fn print_token(scoped: &ScopedToken, show_token: bool) {
    // print this scoped tokens name
    println!("{}:", scoped.name);
    // print the groups this scoped token is limited to
    println!("  groups: {}", scoped.groups.join(", "));
    // print when this scoped tokens value rotates
    println!("  token expiration: {}", scoped.token_expiration);
    // print when this scoped token permanently expires if its ephemeral
    match scoped.expires {
        Some(expires) => println!("  expires: {expires}"),
        None => println!("  expires: never"),
    }
    // only show this scoped tokens value if it was requested
    if show_token {
        println!("  token: {}", scoped.token);
    }
}

/// Write an updated Thorctl config to disk
///
/// # Arguments
///
/// * `args` - The arguments passed to Thorctl
/// * `config` - The config to write to disk
fn write_config(args: &Args, config: &CtlConf) -> Result<(), Error> {
    // open the config file for writing
    let conf_file = std::fs::File::create(&args.config)?;
    // write our updated config to disk
    serde_norway::to_writer(conf_file, config)?;
    Ok(())
}

/// Make sure this command was not run in `--keys` mode
///
/// Activation state lives in the Thorctl config file which does not exist
/// when authenticating with a raw keys file.
///
/// # Arguments
///
/// * `args` - The arguments passed to Thorctl
fn require_config(args: &Args) -> Result<(), Error> {
    // bail if a keys file is in use since we have no config file to update
    if args.keys.is_some() {
        return Err(Error::new(
            "Scoped token activation requires a Thorctl config file and cannot be used with --keys",
        ));
    }
    Ok(())
}

/// Create a new scoped token
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The create command to execute
async fn create(thorium: Thorium, cmd: &CreateScopedToken) -> Result<(), Error> {
    // parse our expiration date if one was given
    let expires = parse_expires(cmd.expires.as_ref(), &cmd.date_fmt)?;
    // build our scoped token request
    let mut req = ScopedTokenRequest::new(&cmd.name).groups(cmd.groups.clone());
    // set our expiration date if one was given
    if let Some(expires) = expires {
        req = req.expires(expires);
    }
    // create our scoped token
    let scoped = thorium.users.create_scoped_token(&req).await?;
    // print our new scoped tokens info
    print_token(&scoped, cmd.show_token);
    // tell the user how to see this scoped tokens value if it was hidden
    if !cmd.show_token {
        println!("Rerun with --show-token to see this scoped tokens value");
    }
    Ok(())
}

/// List all available scoped tokens
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The list command to execute
async fn list(thorium: Thorium, cmd: &ListScopedTokens) -> Result<(), Error> {
    // list all of our scoped tokens
    let tokens = thorium.users.list_scoped_tokens().await?;
    // tell the user if there are no scoped tokens to list
    if tokens.is_empty() {
        println!("No scoped tokens found");
        return Ok(());
    }
    // print each of our scoped tokens
    for scoped in &tokens {
        print_token(scoped, cmd.show_token);
    }
    Ok(())
}

/// Update a scoped token
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The update command to execute
async fn update(thorium: Thorium, cmd: &UpdateScopedToken) -> Result<(), Error> {
    // parse our expiration date if one was given
    let expires = parse_expires(cmd.opts.expires.as_ref(), &cmd.date_fmt)?;
    // build our scoped token update
    let mut update = ScopedTokenUpdate::default()
        .add_groups(cmd.opts.add_groups.clone())
        .remove_groups(cmd.opts.remove_groups.clone());
    // set our new expiration date if one was given
    if let Some(expires) = expires {
        update = update.expires(expires);
    }
    // clear our expiration date if requested
    if cmd.opts.clear_expires {
        update = update.clear_expires();
    }
    // update our scoped token
    let scoped = thorium
        .users
        .update_scoped_token(&cmd.name, &update)
        .await?;
    // print our updated scoped tokens info
    print_token(&scoped, cmd.show_token);
    Ok(())
}

/// Delete scoped tokens
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `cmd` - The delete command to execute
async fn delete(thorium: Thorium, cmd: &DeleteScopedTokens) -> Result<(), Error> {
    // track whether any deletes failed
    let mut failed = false;
    // try to delete each of the target scoped tokens
    for name in &cmd.names {
        match thorium.users.delete_scoped_token(name).await {
            // we deleted this scoped token
            Ok(_) => println!("Deleted scoped token {name}"),
            // we failed to delete this scoped token
            Err(error) => {
                // print this error and keep deleting the remaining tokens
                eprintln!("Failed to delete scoped token {name}: {error}");
                failed = true;
            }
        }
    }
    // error out if any deletes failed
    if failed {
        return Err(Error::new("Failed to delete one or more scoped tokens"));
    }
    Ok(())
}

/// Activate a scoped token making Thorctl authenticate with it
///
/// # Arguments
///
/// * `args` - The arguments passed to Thorctl
/// * `config` - The Thorctl config to update
/// * `thorium` - The Thorium client
/// * `cmd` - The activate command to execute
async fn activate(
    args: &Args,
    mut config: CtlConf,
    thorium: Thorium,
    cmd: &ActivateScopedToken,
) -> Result<(), Error> {
    // make sure we have a config file to store our activation in
    require_config(args)?;
    // get this scoped tokens info rotating its value if it has expired
    let scoped = thorium.users.get_scoped_token(&cmd.name).await?;
    // save this scoped token to our config
    config.scoped_token = Some(ActiveScopedToken {
        name: scoped.name.clone(),
        token: scoped.token.clone(),
    });
    // write our updated config to disk
    write_config(args, &config)?;
    // print this scoped tokens info
    print_token(&scoped, cmd.show_token);
    // tell the user this scoped token is now active
    println!(
        "Activated scoped token {}; Thorctl commands will now authenticate with it \
        until 'thorctl scoped-tokens deactivate' is run",
        scoped.name
    );
    Ok(())
}

/// Deactivate the currently activated scoped token
///
/// # Arguments
///
/// * `args` - The arguments passed to Thorctl
/// * `config` - The Thorctl config to update
fn deactivate(args: &Args, mut config: CtlConf) -> Result<(), Error> {
    // make sure we have a config file to store our activation in
    require_config(args)?;
    // check if a scoped token is currently active
    match config.scoped_token.take() {
        // a scoped token was active so clear it from our config
        Some(active) => {
            // write our updated config to disk
            write_config(args, &config)?;
            // tell the user this scoped token is no longer active
            println!(
                "Deactivated scoped token {}; Thorctl commands will now authenticate \
                with your primary credentials",
                active.name
            );
            Ok(())
        }
        // no scoped token is active so there is nothing to do
        None => {
            println!("No scoped token is currently active");
            Ok(())
        }
    }
}

/// Get info on the currently activated scoped token if one is active
///
/// # Arguments
///
/// * `args` - The arguments passed to Thorctl
/// * `config` - The Thorctl config to inspect
/// * `thorium` - The Thorium client
/// * `cmd` - The current command to execute
async fn current(
    args: &Args,
    config: CtlConf,
    thorium: Thorium,
    cmd: &CurrentScopedToken,
) -> Result<(), Error> {
    // make sure we have a config file that could contain an activation
    require_config(args)?;
    // check if a scoped token is currently active
    let Some(active) = &config.scoped_token else {
        println!("No scoped token is currently active");
        return Ok(());
    };
    // get this scoped tokens current info with our primary credentials
    match thorium.users.get_scoped_token(&active.name).await {
        Ok(scoped) => {
            // print this scoped tokens info
            print_token(&scoped, cmd.show_token);
            // warn if our stored value no longer matches the server side value
            if scoped.token != active.token {
                println!(
                    "WARNING: The activated value for {} is stale because this scoped token \
                    was rotated; rerun 'thorctl scoped-tokens activate {}' to fix it",
                    active.name, active.name
                );
            }
            Ok(())
        }
        Err(error) => {
            // warn if this scoped token no longer exists server side
            if error.status() == Some(http::StatusCode::NOT_FOUND) {
                println!(
                    "The activated scoped token {} no longer exists; run \
                    'thorctl scoped-tokens deactivate' to clear it",
                    active.name
                );
                return Ok(());
            }
            // some other error occurred so bubble it up
            Err(error)
        }
    }
}

/// Handle all scoped token commands
///
/// # Arguments
///
/// * `args` - The arguments passed to Thorctl
/// * `cmd` - The scoped tokens command to execute
pub async fn handle(args: &Args, cmd: &ScopedTokens) -> Result<(), Error> {
    // load our config and instance a client that always uses our primary
    // credentials since scoped tokens cannot manage scoped tokens
    let (conf, thorium) = utils::get_primary_client(args).await?;
    // warn about insecure connections if not set to skip
    if !conf.skip_insecure_warning.unwrap_or_default() {
        utils::warn_insecure_conf(&conf)?;
    }
    // call the right scoped tokens handler
    match cmd {
        ScopedTokens::Create(cmd) => create(thorium, cmd).await,
        ScopedTokens::List(cmd) => list(thorium, cmd).await,
        ScopedTokens::Update(cmd) => update(thorium, cmd).await,
        ScopedTokens::Delete(cmd) => delete(thorium, cmd).await,
        ScopedTokens::Activate(cmd) => activate(args, conf, thorium, cmd).await,
        ScopedTokens::Deactivate => deactivate(args, conf),
        ScopedTokens::Current(cmd) => current(args, conf, thorium, cmd).await,
    }
}

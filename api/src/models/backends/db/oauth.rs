//! Stores and retreives data needed to authenticate with oauth in the API

use openidconnect::{CsrfToken, Nonce, PkceCodeVerifier};
use tracing::{Level, event, instrument};

use super::keys::oauth;
use crate::models::backends::oauth::OAuthRegistrationSession;
use crate::utils::{ApiError, Shared};
use crate::{conn, deserialize, serialize, unauthorized};

/// The info needed to validate an OAuth Authentication flow
#[derive(Debug, Serialize, Deserialize)]
struct OAuthContext {
    /// The nonce for this authentication request
    pub nonce: Nonce,
    /// The PKCE verifier
    pub pkce_verifier: PkceCodeVerifier,
}

/// Store a CSRF token/OAuth state and its nonce
///
/// # Arguments
///
/// * `provider` - The provider this oauth context is for
/// * `csrf_token` - The CSRF token/OAuth state
/// * `nonce` - The nonce to store
/// * `pkce_verifier` - The pkce verifier for this authentication flow
/// * `shared` - Shared Thorium objects
#[rustfmt::skip]
#[instrument(name = "db::oauth::store_context", skip(csrf_token, nonce, pkce_verifier, shared), err(Debug))]
pub async fn store_context(
    provider: &str,
    csrf_token: &CsrfToken,
    nonce: Nonce,
    pkce_verifier: PkceCodeVerifier,
    shared: &Shared,
) -> Result<(), ApiError> {
    // serialize our oauth context
    let ctx = OAuthContext { nonce, pkce_verifier };
    // get the key to store our nonce at
    let key = oauth::context(provider, csrf_token, shared);
    // get how long this session should be valid for
    let expire = shared.config.thorium.auth.oauth.as_ref()
        .map(|conf| conf.register_expire)
        .unwrap_or(crate::conf::default_csrf_expire());
    // save this oauth sessions context for 10 minutes
    let redis_result = redis::cmd("set").arg(key).arg(serialize!(&ctx)).arg("NX").arg("EX").arg(expire)
        .exec_async(conn!(shared))
        .await;
    // if we ran into an error then just return a 401 and log this error internally
    // this is done to prevent any attacker from forcing alias collisions to enumerate
    // user aliases. This should be impossible unless the attacker already controls
    // the OAuth provider but its better to be defensive.
    match redis_result {
        Ok(()) => Ok(()),
        // saving this oauth context ran into a problem
        Err(error) => {
            // log this error interanally
            event!(Level::ERROR, error=error.to_string());
            // return a 401 no matter what the error was
            unauthorized!()
        },
    }
}

/// Get and consume the oauth context for a specific CSRF token/OAuth State
///
/// # Arguments
///
/// * `provider` - The provider this oauth context is for
/// * `csrf_token` - The CSRF token/OAuth state
/// * `shared` - Shared Thorium objects
#[rustfmt::skip]
#[instrument(name = "db::oauth::consume_context", skip(csrf_token, shared), err(Debug))]
pub async fn consume_context(provider: &str, csrf_token: &CsrfToken, shared: &Shared) -> Result<(Nonce, PkceCodeVerifier), ApiError> {
    // get the key to load our nonce at
    let key = oauth::context(provider, csrf_token, shared);
    // load and remove this nonce to prevent anyone else using it
    let ctx_str: Option<String> = redis::cmd("getdel").arg(key).query_async(conn!(shared)).await?;
    // deserialize our context or return unauthorized
    let ctx: OAuthContext = match &ctx_str {
        Some(ctx_str) => deserialize!(ctx_str),
        // return a 401 if we can't find the context for this oauth flow
        None => return unauthorized!(),
    };
    // wrap our nonce to prevent it from leaking
    Ok((ctx.nonce, ctx.pkce_verifier))
}

/// Save a registration sessions info
///
/// # Arguments
///
/// * `provider` - The OAuth provider this session is for
/// * `session` - The session to save
/// * `shared` - Shared Thorium objects
#[rustfmt::skip]
#[instrument(name = "db::oauth::save_registration_session", skip(session, shared), err(Debug))]
pub async fn save_registration_session(
    provider: &str,
    session: &OAuthRegistrationSession,
    shared: &Shared,
) -> Result<(), ApiError> {
    // get the key to store alias to username mapping
    let key = oauth::registration_session(provider, &session.token, shared);
    // get how long this session should be valid for
    let expire = shared.config.thorium.auth.oauth.as_ref()
        .map(|conf| conf.register_expire)
        .unwrap_or(crate::conf::default_csrf_expire());
    // save this alias to username mapping
    let redis_result = redis::cmd("set").arg(key).arg(serialize!(session)).arg("NX").arg("EX").arg(expire)
        .exec_async(conn!(shared))
        .await;
    // if we ran into an error then just return a 401 and log this error internally
    // this is done to prevent any attacker from forcing alias collisions to enumerate
    // registration sessions. This should be impossible unless the attacker already controls
    // the OAuth provider but its better to be defensive.
    match redis_result {
        Ok(()) => Ok(()),
        // saving this registration session ran into a problem
        Err(error) => {
            // log this error interanally
            event!(Level::ERROR, error=error.to_string());
            // return a 401 no matter what the error was
            unauthorized!()
        },
    }
}

/// Get and consume a registration sessions info by token
///
/// # Arguments
///
/// * `provider` - The OAuth provider this session is for
/// * `token` - The token to the session to load
/// * `shared` - Shared Thorium objects
#[rustfmt::skip]
#[instrument(name = "db::oauth::consume_registration_session", skip(token, shared), err(Debug))]
pub async fn consume_registration_session(
    provider: &str,
    token: &str,
    shared: &Shared,
) -> Result<OAuthRegistrationSession, ApiError> {
    // get the key to store alias to username mapping
    let key = oauth::registration_session(provider, token, shared);
    // get the session tied to this session token
    let maybe_session: Option<String> = redis::cmd("getdel").arg(key)
        .query_async(conn!(shared))
        .await?;
    // if this session wasn't found then return a 401
    match &maybe_session {
        // we got a session deserialize it
        Some(session_str) => Ok(deserialize!(session_str)),
        None => unauthorized!(),
    }
}

/// Validate that a registration session exists
/// 
/// # Arguments
///
/// * `provider` - The OAuth provider this session is for
/// * `token` - The token to the session to load
/// * `shared` - Shared Thorium objects
#[rustfmt::skip]
#[instrument(name = "db::oauth::validate_registration_token", skip(token, shared), err(Debug))]
pub async fn validate_registration_session(
    provider: &str,
    token: &str,
    shared: &Shared,
) -> Result<(), ApiError> {
    // get the key to store alias to username mapping
    let key = oauth::registration_session(provider, token, shared);
    // get the session tied to this session token
    let maybe_session: Option<String> = redis::cmd("get").arg(key)
        .query_async(conn!(shared))
        .await?;
    // if this session wasn't found then return a 401
    match &maybe_session {
        // we got a session deserialize it
        Some(session_str) => {
            // try to deserialize this data just to make sure its valid
            let _: OAuthRegistrationSession = deserialize!(session_str);
            // we were able to deserialize this session so it exists and is valid
            Ok(())
        },
        None => unauthorized!(),
    }
}

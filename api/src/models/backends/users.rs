//! Wrappers for interacting with users within Thorium with different backends
//! Currently only Redis is supported

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Version};
use axum::extract::{FromRef, FromRequestParts};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use chrono::prelude::*;
use headers::{Header, HeaderName, HeaderValue};
use ldap3::{Scope, SearchEntry};
use rand::prelude::*;
use std::collections::{HashMap, HashSet};
use std::str;
use tracing::{Level, Span, event, instrument};
use url::Url;

use super::db;
use crate::conf::Ldap;
use crate::models::{
    AiEndpoint, AiEndpointUpdate, AiSettings, AiSettingsUpdate, AuthResponse, Group, ImageScaler,
    Key, ScrubbedUser, UnixInfo, User, UserCreate, UserRole, UserSettings, UserSettingsUpdate,
    UserUpdate,
};
use crate::utils::shared::EmailClient;
use crate::utils::{ApiError, AppState, Shared, bounder};
use crate::{bad, conflict, is_admin, ldap, unauthorized, unavailable, update};

/// The header name for our secret key
static SECRET_KEY_HEADER: HeaderName = HeaderName::from_static("secret-key");
static SECRET_KEY_HEADER_REF: &HeaderName = &SECRET_KEY_HEADER;

/// The reserved alias provider name used for LDAP based auth
///
/// LDAP participates in the same alias system as OAuth providers; this is the provider
/// key its aliases are stored under. It is reserved so an OAuth provider cannot reuse
/// the same name and collide with LDAP aliases.
pub const LDAP_PROVIDER: &str = "ldap";

/// Return unauthorized if a function return an error
macro_rules! check_unauth {
    ($func:expr) => {
        match $func {
            Ok(val) => val,
            Err(_) => return crate::unauthorized!(),
        }
    };
}

impl Header for Key {
    fn name() -> &'static HeaderName {
        SECRET_KEY_HEADER_REF
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
    {
        // get our header value
        let value = values.next().ok_or_else(headers::Error::invalid)?;
        // cast this header into a secret key
        let key = Key {
            key: value.to_str().unwrap().to_owned(),
        };
        Ok(key)
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        // wrap our secret key
        let value = HeaderValue::from_str(&self.key);
        // add this to the header
        values.extend(value);
    }
}

/// Hashes a users password
#[macro_export]
macro_rules! hash_pw {
    ($raw:expr, $secret_key:expr) => {
        // hash this password with our salt
        Argon2::new_with_secret(
            $secret_key.as_bytes(),
            Algorithm::Argon2id,
            Version::V0x13,
            argon2::Params::default(),
        )?
        .hash_password($raw.as_bytes(), &SaltString::generate(&mut OsRng))?
        .to_string()
    };
}

/// generate a token
#[macro_export]
macro_rules! token {
    () => {{
        let mut rng = rand::rng();
        let token: [u8; 32] = rng.random();
        hex::encode(token)
    }};
}

/// get the time a new token should expire
#[macro_export]
macro_rules! token_expire {
    ($shared:expr) => {
        // update token expiration
        Utc::now() + chrono::Duration::days($shared.config.thorium.auth.token_expire as i64)
    };
}

/// Authenticate a user by token
///
/// # Arguments
///
/// * `token` - The token to authenticate with
/// * `shared` - Shared objects in Thorium
#[instrument(name = "backends::user::token_auth", skip_all, err(Debug))]
async fn token_auth<'a>(token: &str, shared: &Shared) -> Result<User, ApiError> {
    // get user
    let mut user = db::users::get_token(token, shared).await?;
    // throw unauthorized if token doesn't match
    // this should only happen if the token map is somehow wrong
    // which should never happen
    if user.token != token {
        event!(Level::ERROR, msg = "Token Map Corruption Likely");
        return unauthorized!();
    }
    // Check if this users token has expired
    if user.token_expiration < Utc::now() {
        // token is expired so generate a new one and bounce this user
        event!(Level::INFO, msg = "Regenerating Expired Token");
        user.regen_token(shared).await?;
        return unauthorized!();
    }
    // return authed user
    Ok(user)
}

/// Authenticate a user with basic auth stored in redis
///
/// # Arguments
///
/// * `possible` - The user data we are authenticating against
/// * `password` - The password to check against
/// * `shared` - Shared objects in Thorium
#[instrument(name = "backends::user::basic_auth_redis", skip_all, err(Debug))]
async fn basic_auth_redis(
    username: &str,
    password: &str,
    password_hash: &str,
    shared: &Shared,
) -> Result<(), ApiError> {
    // parse our password hash
    let parsed_hash = PasswordHash::new(password_hash)?;
    // get our key
    let secret_key = shared.config.thorium.secret_key.as_bytes();
    // build an argon hasher
    let argon = Argon2::new_with_secret(
        secret_key,
        Algorithm::Argon2id,
        Version::V0x13,
        argon2::Params::default(),
    )?;
    // verify this user provided the correct password
    match argon.verify_password(password.as_bytes(), &parsed_hash) {
        Ok(()) => Ok(()),
        Err(error) => {
            // log this authorization failure
            event!(Level::ERROR, user = username, error = error.to_string(),);
            // This user is not authorized to view this route/data
            unauthorized!()
        }
    }
}

/// Authenticate a user with basic auth using LDAP
///
/// # Arguments
///
/// * `username` - The name of the user to authenticate
/// * `password` - The password to authenticate with
/// * `shared` - Shared objects in Thorium
#[instrument(
    name = "backends::user::basic_auth_ldap",
    skip(password, shared),
    err(Debug)
)]
async fn basic_auth_ldap(
    username: &str,
    password: &str,
    shared: &Shared,
) -> Result<ldap3::Ldap, ApiError> {
    if let Some(ldap_conf) = &shared.config.thorium.auth.ldap {
        //  build an ldap connection
        //  we do this on demand instead of having it in shared because it nees to be mutable
        let (conn, mut ldap) = ldap!(ldap_conf).await?;
        ldap3::drive!(conn);
        // try to bind to ldap with this users creds
        let query = format!(
            "{}{username}{}",
            ldap_conf.user_prepend, ldap_conf.user_append
        );
        let res = ldap.simple_bind(&query, password).await?.success();
        // check if the bind failed or ot
        if let Err(err) = res {
            // log this auth failure
            event!(Level::ERROR, user = username, err = &err.to_string());
            // the bind failed return unauthorized
            return unauthorized!();
        }
        Ok(ldap)
    } else {
        unavailable!("ldap is not configured!".to_owned())
    }
}

/// Authenticate a user with basic auth stored in redis or using ldap
///
/// # Arguments
///
/// * `username` - The name of the user to authenticate
/// * `password` - The password to authenticate with
/// * `shared` - Shared objects in Thorium
#[instrument(
    name = "backends::users::password_auth",
    skip(password, shared),
    err(Debug)
)]
async fn password_auth(username: &str, password: &str, shared: &Shared) -> Result<User, ApiError> {
    // get the user doc we are authenticating against, supporting ldap aliases for
    // accounts whose ldap identity differs from their Thorium username (e.g. an
    // OAuth-registered account that has linked ldap based auth)
    let mut possible = match db::users::get(username, shared).await {
        // we found a user directly by the provided name
        Ok(user) => user,
        // no user has this name directly so try to resolve it as an ldap alias
        Err(error) => {
            // only attempt alias resolution when ldap is configured
            if shared.config.thorium.auth.ldap.is_some() {
                // try to resolve this name as an ldap alias to a Thorium username
                match db::users::get_username_by_alias(LDAP_PROVIDER, username, shared).await? {
                    // load the account this ldap identity is linked to
                    Some(resolved) => db::users::get(&resolved, shared).await?,
                    // this name is neither a user nor a linked ldap alias
                    None => return Err(error),
                }
            } else {
                // ldap is not configured so there are no aliases to resolve
                return Err(error);
            }
        }
    };
    event!(
        Level::INFO,
        user = &possible.username,
        msg = "Attempting authentication",
    );
    // try to authenticate against redis or ldap based on if a password is set
    if let Some(password_hash) = &possible.password {
        // a password is set so use basic auth against the resolved user
        basic_auth_redis(&possible.username, password, password_hash, shared).await?;
    } else {
        // no password was set so bind to ldap with the provided ldap identity (which
        // is the alias for an account that linked ldap, or the username for a pure
        // ldap user)
        let mut ldap = basic_auth_ldap(username, password, shared).await?;
        // if no unix info is set then try to get it and save it
        if possible.unix.is_none() {
            // get this users unix info from ldap using their ldap identity
            let unix = get_unix_info(
                username,
                shared.config.thorium.auth.ldap.as_ref().unwrap(),
                &mut ldap,
            )
            .await?;
            // unbind our ldap socket
            ldap.unbind().await?;
            // set our new unix info
            possible.unix = Some(unix);
            // save the updated user object to redis
            db::users::save(&possible, shared).await?;
        }
    }

    // check if our token is expired and regenerate it if it is
    if possible.token_expiration < Utc::now() {
        event!(Level::INFO, msg = "refreshing token");
        // token is expired so generate a new one and bounce this user
        possible.regen_token(shared).await?;
    }
    Ok(possible)
}

/// Build an ldap connection with our system user for listing info (not auth checks)
#[instrument(name = "backends::users::bind_to_ldap_system_user", skip_all, fields(creds_set = ldap_conf.credentials.is_some()), err(Debug))]
pub async fn bind_to_ldap_system_user(
    ldap: &mut ldap3::Ldap,
    ldap_conf: &Ldap,
) -> Result<(), ApiError> {
    // if we have any system creds set then use them
    if let Some(creds) = &ldap_conf.credentials {
        // try to bind to ldap with this users creds
        let query = format!(
            "{}{}{}",
            ldap_conf.user_prepend, creds.user, ldap_conf.user_append
        );
        let res = ldap.simple_bind(&query, &creds.password).await?.success();
        // check if the bind failed or ot
        if let Err(err) = res {
            // log this auth failure
            event!(Level::ERROR, user = &creds.user, err = &err.to_string());
            // the bind failed return unavailable
            return unavailable!(format!("Failed to bind with user {}", &creds.user));
        }
    }
    Ok(())
}

/// Pull the unix info about a user from ldap
///
/// # Argumens
///
/// * `username` - The username to get info
/// * `conf` - The Thorium Ldap config
/// * `ldap` - A bound and authenticated ldap connection
#[instrument(name = "backends::users::get_unix_info", skip(conf, ldap), err(Debug))]
async fn get_unix_info(
    username: &str,
    conf: &Ldap,
    ldap: &mut ldap3::Ldap,
) -> Result<UnixInfo, ApiError> {
    // build a filter to get info on this user
    let filter = format!(
        "(&({}{username})(objectClass=*))",
        conf.search_filter_prepend
    );
    // search for this users info in ldap
    let mut stream = ldap
        .streaming_search(&conf.scope, Scope::Subtree, &filter, vec!["*"])
        .await?;
    // crawl over the search entries for our user and pull their info
    if let Some(entry) = stream.next().await? {
        // try to cast this entry to strings instead of binary arrays
        let mut entry = SearchEntry::construct(entry);
        // get the group and user id if they exist
        let user = conf.user_unix_id.attr.get(&mut entry);
        let group = conf.group_unix_id.attr.get(&mut entry);
        // build UnixInfo object if they exist or error out
        if let (Some(user), Some(group)) = (user, group) {
            // cast our user and group ids
            let user = conf.user_unix_id.cast.cast(user)?;
            let group = conf.group_unix_id.cast.cast(group)?;
            return Ok(UnixInfo { user, group });
        }
        // we didn't get the attrs we expected so log what we got to jaeger
        let ldap_str = format!("{:#?}", entry);
        event!(Level::ERROR, ldap_entry = ldap_str);
    }
    unavailable!(format!("Ldap did not return UNIX info for {}", username))
}

/// The different support auth methods
enum AuthMethods {
    /// Authenticate with a token
    Token(String),
    /// Authenticate with a password
    Password { username: String, password: String },
}

impl AuthMethods {
    /// Authenticates a user based on an auth header
    ///
    /// # Arguments
    ///
    /// * `verify_email` - Whether to require a verified email or not
    /// * `shared` - Shared Thorium objects
    #[instrument(name = "User::authenticate", skip_all, err(Debug))]
    pub async fn authenticate(
        &self,
        verify_email: bool,
        shared: &Shared,
    ) -> Result<User, ApiError> {
        // try to authenticate this user
        let user = match self {
            Self::Token(token) => token_auth(token, shared).await,
            Self::Password { username, password } => {
                password_auth(username, password, shared).await
            }
        }?;
        // make sure this user's email has been verified
        if verify_email && !user.verified {
            // our user has not been verified yet so reject this request
            return unauthorized!("Email has not been verified".to_owned());
        }
        Ok(user)
    }

    /// Build our auth method from a str
    ///
    /// # Arguments
    ///
    /// * `raw` - The str to build our auth method from
    /// * `span` - The span to log traces under
    #[instrument(name = "User::from_str", skip_all, err(Debug))]
    pub fn from_str(raw: &str) -> Result<Self, ApiError> {
        // get our current span
        let span = Span::current();
        // get the first index where a space exists
        if let Some(index) = raw.find(' ') {
            // make sure this isn't the last character
            if raw.len() < index + 1 {
                event!(parent: span, Level::ERROR, error = "Auth header is too short", len = raw.len(), index = index);
                return unauthorized!();
            }
            // try to get the correct auth method
            match &raw[..index] {
                "token" | "Token" | "bearer" | "Bearer" => Self::token(&raw[index + 1..]),
                "basic" | "Basic" => Self::password(&raw[index + 1..]),
                _ => {
                    event!(parent: span, Level::ERROR, error = "Unknown auth type", auth_type = &raw[..index]);
                    unauthorized!()
                }
            }
        } else {
            event!(parent: span, Level::ERROR, error = "Couldn't find space to split auth header");
            unauthorized!()
        }
    }

    /// Builds the token auth method
    ///
    /// # Arguments
    ///
    /// * `raw` - The str to pull our token from
    fn token(raw: &str) -> Result<Self, ApiError> {
        // try to decode this token value
        let decoded = b64_decode(raw)?;
        Ok(AuthMethods::Token(decoded))
    }

    /// Builds the password auth method
    ///
    /// # Arguments
    ///
    /// * `raw` - The str to pull our username and password from
    fn password(raw: &str) -> Result<Self, ApiError> {
        // try to decode this token value
        let decoded = b64_decode(raw)?;
        // find the first ':' that we should split on
        if let Some(index) = decoded.find(':') {
            // split this decoded string into a username and password
            let username = decoded[..index].to_owned();
            let password = decoded[index + 1..].to_owned();
            Ok(AuthMethods::Password { username, password })
        } else {
            // we couldn't find a ':' to split on
            unauthorized!()
        }
    }
}

impl AiEndpointUpdate {
    /// Create a full endpoing from this update
    ///
    /// If all required fields are not set this will error
    pub fn to_endpoint(self) -> Result<AiEndpoint, ApiError> {
        // convert this update into a new endpoint
        match (self.url, self.api_key, self.model) {
            (Some(url), Some(api_key), Some(model)) => Ok(AiEndpoint {
                url,
                api_key,
                model,
            }),
            _ => bad!(format!("Missing required fields for new endpoint")),
        }
    }

    /// Apply this update to an existing endpoint
    ///
    /// # Arguments
    ///
    /// * `endpoint` - The endpoint to apply this update too
    pub fn apply(self, mut endpoint: AiEndpoint) -> AiEndpoint {
        // update this endpoints settings
        update!(endpoint.url, self.url);
        update!(endpoint.api_key, self.api_key);
        update!(endpoint.model, self.model);
        endpoint
    }
}

impl AiSettingsUpdate {
    /// Apply any updated AI settings
    ///
    /// # Arguments
    ///
    /// * `existing` - Any existing settings to update
    pub fn apply(mut self, existing: &mut Option<AiSettings>) -> Result<(), ApiError> {
        // check if we have existing existing to update
        let mut settings = match existing.take() {
            Some(settings) => settings,
            None => {
                // we don't have any existing settings so well need to make new ones
                // We have to make sure that users provide us enough info to build our new settings
                let default_endpoint = match self.default_endpoint.take() {
                    Some(default) => default,
                    None => {
                        return bad!(
                        "A default endpoint must be provided if no previous AI settings were set"
                            .to_owned()
                    );
                    }
                };
                // make sure they provided us settings for our default endpoint
                let endpoint = match self.endpoints.remove(&default_endpoint) {
                    // turn this endpoint update into an endpoint
                    Some(endpoint_update) => endpoint_update.to_endpoint()?,
                    None => {
                        return bad!(format!(
                            "Settings for {default_endpoint} must be specified to set it as a default"
                        ));
                    }
                };
                // insert our default endpoints settings into a map
                let mut endpoints = HashMap::with_capacity(self.endpoints.len());
                // add our new default endpoint
                endpoints.insert(default_endpoint.clone(), endpoint);
                // build the base ai settings object
                AiSettings {
                    endpoints,
                    default_endpoint,
                }
            }
        };
        // apply the rest of our updates to our settings
        update!(settings.default_endpoint, self.default_endpoint);
        // apply our endpoint updates
        for (name, endpoint_update) in self.endpoints {
            // get and update this endpoint if it already exists or create a new one
            let endpoint = match settings.endpoints.remove(&name) {
                Some(endpoint) => endpoint_update.apply(endpoint),
                // this endpoint does not yet exist so create it
                None => endpoint_update.to_endpoint()?,
            };
            // reinsert our endpoint
            settings.endpoints.insert(name, endpoint);
        }
        // drop any endpoints that we want to delete
        settings
            .endpoints
            .retain(|name, _| !self.remove_endpoints.contains(name));
        // if we have no more ai endpoints then clear our ai settings
        if settings.endpoints.is_empty() {
            // clear any existing ai settings
            *existing = None;
        } else {
            // make sure we still have a default endpoint
            if !settings.endpoints.contains_key(&settings.default_endpoint) {
                return bad!(format!(
                    "Please change your default endpoint from {} before removing its config!",
                    settings.default_endpoint
                ));
            }
            // set our new ai settings for this user
            *existing = Some(settings);
        }
        Ok(())
    }
}

impl UserSettingsUpdate {
    /// Apply any updated settings to this user
    ///
    /// # Arguments
    ///
    /// * `settings` - The user settings to update
    pub fn apply(self, settings: &mut UserSettings) -> Result<(), ApiError> {
        // update our theme if an update was set
        update!(settings.theme, self.theme);
        // apply any AI settings updates
        if let Some(ai_update) = self.ai {
            ai_update.apply(&mut settings.ai)?;
        }
        Ok(())
    }
}

/// The outcome of an LDAP registration attempt for a given identity and email
#[derive(Debug, PartialEq, Eq)]
enum LdapRegOutcome {
    /// Create a brand new pure-LDAP user
    Create,
    /// The email belongs to an existing account; link this LDAP alias to it
    Link {
        /// The username of the existing account to link too
        existing: String,
    },
    /// This LDAP identity is already linked to an account
    AliasTaken,
}

/// Decide what an LDAP registration should do based on existing alias/email owners
///
/// This is the pure decision logic behind LDAP based account linking so it can be unit
/// tested without a live LDAP server or Redis.
///
/// # Arguments
///
/// * `alias_owner` - The account already linked to this LDAP identity if any
/// * `email_owner` - The account that already owns this email if any
fn ldap_registration_outcome(
    alias_owner: Option<String>,
    email_owner: Option<String>,
) -> LdapRegOutcome {
    // if this ldap identity is already linked then we can neither link nor create
    if alias_owner.is_some() {
        return LdapRegOutcome::AliasTaken;
    }
    // otherwise force linking if the email is taken or create a brand new user
    match email_owner {
        // this email already belongs to an account so force linking to it
        Some(existing) => LdapRegOutcome::Link { existing },
        // this is a brand new pure-ldap user
        None => LdapRegOutcome::Create,
    }
}

impl User {
    /// Creates a new user in the backend
    ///
    /// # Arguments
    ///
    /// * `req` - The user registration request
    /// * `key` - The secret key from the request
    /// * `shared` - Shared Thorium objects
    #[instrument(name = "User::create", skip_all, fields(user = req.username, key_set = key.is_some()), err(Debug))]
    pub async fn create(
        mut req: UserCreate,
        key: Option<Key>,
        shared: &Shared,
    ) -> Result<User, ApiError> {
        // ensure username is alphanumeric
        bounder::string_lower(&req.username, "username", 1, 50)?;
        // make sure this isn't using any reserved usernames
        if req.username == "external" {
            return bad!("external is a reserved username".to_owned());
        }
        // make sure this user doesn't already exist
        if User::exists(&req.username, shared).await? {
            return conflict!(format!("User {} already exists", req.username));
        }
        // only allow users with the secret key to create admins with this route
        if req.role == UserRole::Admin || req.local {
            // bounce users without the key
            if let Some(key) = key {
                // return unauthorized for users with invalid keys
                if shared.config.thorium.secret_key != key.key {
                    return unauthorized!();
                }
            } else {
                return unauthorized!();
            }
        }
        // if ldap is configured then authenticated against ldap and pull unix info
        let (password, unix) = match (&shared.config.thorium.auth.ldap, req.local) {
            // ldap config is setup and a local account was not requested
            (Some(conf), false) => {
                // authenticate against ldap to prove ownership of this ldap identity
                let mut ldap = basic_auth_ldap(&req.username, &req.password, shared).await?;
                // look up who, if anyone, already owns this ldap identity and this email
                let alias_owner =
                    db::users::get_username_by_alias(LDAP_PROVIDER, &req.username, shared).await?;
                let email_owner = db::users::get_username_for_email(&req.email, shared).await?;
                // decide what to do based on existing alias/email ownership
                match ldap_registration_outcome(alias_owner, email_owner) {
                    // this ldap identity is already linked to an account
                    LdapRegOutcome::AliasTaken => {
                        // unbind our ldap socket before returning
                        ldap.unbind().await?;
                        // tell the user this ldap identity is already in use
                        return conflict!(
                            "This LDAP identity is already linked to an account".to_owned()
                        );
                    }
                    // this email already belongs to an account so force account linking
                    LdapRegOutcome::Link { existing } => {
                        // send an account link email to the existing account so they can
                        // confirm linking this ldap identity to it
                        User::send_alias_link_email(
                            LDAP_PROVIDER,
                            &existing,
                            &req.username,
                            &req.email,
                            shared,
                        )
                        .await?;
                        // unbind our ldap socket before returning
                        ldap.unbind().await?;
                        // tell the user to check their email to finish linking
                        return conflict!(
                            "A user with this email already exists. Please check your email for an account link email!".to_owned()
                        );
                    }
                    // this is a brand new pure-ldap user
                    LdapRegOutcome::Create => {
                        // get unix info for this user
                        let unix = get_unix_info(&req.username, conf, &mut ldap).await?;
                        // unbind our ldap socket
                        ldap.unbind().await?;
                        // pure ldap users have no password and no alias
                        (None, Some(unix))
                    }
                }
            }
            // ldap is not configured or a local account was requested
            (_, _) => {
                // get password from request and replace it with an empty str
                let pw = std::mem::take(&mut req.password);
                // get our secret key
                let key = &shared.config.thorium.secret_key;
                // hash password
                (Some(hash_pw!(pw, key)), None)
            }
        };
        // create user object
        let mut cast = User {
            username: req.username,
            password,
            email: req.email,
            groups: Vec::default(),
            role: req.role,
            token: token!(),
            unix,
            token_expiration: token_expire!(shared),
            settings: req.settings,
            verified: false,
            verification_token: None,
            verification_sent: None,
            aliases: HashMap::default(),
        };
        // send a verification email if needed
        match (req.skip_verification, &shared.email) {
            (true, _) | (_, None) => cast.verified = true,
            (false, Some(client)) => {
                // send our verification email
                cast.send_verification_email(client, shared).await?;
            }
        };
        // inject user into the backend
        let user = db::users::create(cast, shared).await?;
        // sync all groups in ldap if ldap is enabled
        if shared.config.thorium.auth.ldap.is_some() {
            // sync ldap data for all groups
            Group::sync_ldap(shared).await?;
        }
        Ok(user)
    }

    /// Send a verification email to an unverified user
    ///
    /// # Arguments
    ///
    /// *
    pub async fn send_verification_email(
        &mut self,
        client: &EmailClient,
        shared: &Shared,
    ) -> Result<(), ApiError> {
        // check if this user has an approved email
        if let Some(approved) = &client.approved {
            if !approved.is_match(&self.email) {
                // reject any emails that are not approved
                return unauthorized!(format!("{} is not an approved email", self.email));
            }
        }
        // if they are not yet verified then send the verification email
        if self.verified {
            // this user is already verified so return a conflict
            return conflict!(format!(
                "{} has already verified their email",
                &self.username
            ));
        }
        // make sure we have email verification settings
        let email_conf = match &shared.config.thorium.auth.email {
            Some(email_conf) => email_conf,
            None => return unavailable!("Email verification is not enabled".to_owned()),
        };
        // make sure this user has not recently sent a verification email
        // this helps prevent using Thorium to spam unverified users
        email_conf.can_send_verification(self)?;
        // generate a special token for our email verification
        let verification_token = token!();
        // save this verification token to redis
        db::users::set_verification_token(&self.username, &verification_token, shared).await?;
        // build our verification link to embed in the email
        let link = format!(
            "{}/users/verify/{}/email/{}",
            email_conf.base_url, self.username, verification_token
        );
        // update our user object
        self.verification_token = Some(verification_token);
        self.verification_sent = Some(Utc::now());
        // build the subject for email verification email
        let subject = "🦀🎉 Welcome to Thorium 🎉🦀".to_owned();
        // build a body with our verification email
        let body = format!(
            "Please verify your Thorium account by clicking on the following link:\n\n{link}"
        );
        // send our verification email
        client.send(&self.email, subject, body).await
    }

    /// Verify an email for a user
    ///
    /// # Arguments
    ///
    /// * `verification_token` - The verification token to check
    pub async fn verify_email(
        &mut self,
        verification_token: &String,
        shared: &Shared,
    ) -> Result<(), ApiError> {
        // check if our verification token matches
        if Some(verification_token) == self.verification_token.as_ref() {
            // update our user object
            self.verified = true;
            self.verification_token = None;
            // clear this users verification token and set them as verified in redis
            db::users::clear_verification_token(&self.username, shared).await?;
            Ok(())
        } else {
            // this is the wrong verification token
            unauthorized!()
        }
    }

    /// Get a [`User`] by username
    ///
    /// This should only be used when absolutely neccasary as it bypasses all authentication checks
    ///
    /// # Arguments
    ///
    /// * `username` - The username of the user to get
    /// * `shared` - Shared Thorium objects
    pub async fn force_get(username: &str, shared: &Shared) -> Result<User, ApiError> {
        // get user
        db::users::get(username, shared).await
    }

    /// Lists usernames
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    pub async fn list(&self, shared: &Shared) -> Result<Vec<String>, ApiError> {
        // list all usernames
        db::users::list(shared).await
    }

    /// Lists users with details
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    pub async fn list_details(&self, shared: &Shared) -> Result<Vec<ScrubbedUser>, ApiError> {
        // only admins can list user details
        is_admin!(self);
        // just get a backup since we want all users anyways
        let users = db::users::backup(shared).await?;
        // cast these users to scrubbed users
        let scrubbed = users.into_iter().map(ScrubbedUser::from).collect();
        Ok(scrubbed)
    }

    /// Checks if a vector of users exist
    ///
    /// # Arguments
    ///
    /// * `username` - the names of the users to check
    /// * `shared` - Shared Thorium objects
    pub async fn exists_many(usernames: &HashSet<String>, shared: &Shared) -> Result<(), ApiError> {
        // check if a set of users exists
        db::users::exists_many(usernames, shared).await
    }

    /// Checks if a user exists
    ///
    /// Returns true if this user exists.
    ///
    /// # Arguments
    ///
    /// * `username` - the name of a user to check
    /// * `shared` - Shared Thorium objects
    pub async fn exists(username: &str, shared: &Shared) -> Result<bool, ApiError> {
        // check if this user exists
        db::users::exists(username, shared).await
    }

    /// Deletes a user from Thorium
    ///
    /// # Arguments
    ///
    /// * `user` - The user requesting this delete
    /// * `delete` - The username to delete
    /// * `shared` - Shared Thorium objects
    pub async fn delete(user: User, delete: &str, shared: &Shared) -> Result<(), ApiError> {
        // only admins or the user in question can delete their user
        // get the users data to delete
        let target = match (user.is_admin(), user.username == delete) {
            // were an admin deleting ourselves or we aren't and admin and were deleting ourselves
            (true | false, true) => user,
            // were an admin deleting another user
            (true, false) => Self::force_get(delete, shared).await?,
            // we aren't an admin and were aren't deleting ourselves
            _ => return unauthorized!(),
        };

        // delete user in the background
        db::users::delete(&target, shared).await
    }

    /// Checks if a user is an admin
    #[must_use]
    pub fn is_admin(&self) -> bool {
        self.role == UserRole::Admin
    }

    /// Check if this user is an admin or analyst
    #[must_use]
    pub fn is_admin_or_analyst(&self) -> bool {
        self.role == UserRole::Admin || self.role == UserRole::Analyst
    }

    /// Checks if a user is a developer
    ///
    /// # Arguments
    ///
    /// * `scaler` - The image scaler to check that we can develop for
    #[must_use]
    pub fn is_developer(&self, scaler: ImageScaler) -> bool {
        // make sure this user
        match self.role {
            // admins can develop for anything
            UserRole::Admin => true,
            // analysts can develop for anything
            UserRole::Analyst => true,
            // check if this developer can develop on all scalers
            UserRole::Developer {
                k8s,
                bare_metal,
                windows,
                external,
                kvm,
            } => {
                // check if this user can develop for this scaler
                match scaler {
                    ImageScaler::K8s => k8s,
                    ImageScaler::BareMetal => bare_metal,
                    ImageScaler::Windows => windows,
                    ImageScaler::External => external,
                    ImageScaler::Kvm => kvm,
                }
            }
            // this user cannot develop images/pipelines
            UserRole::User => false,
        }
    }

    /// Checks if a user is a developer
    ///
    /// # Arguments
    ///
    /// * `scaler` - The image scaler to check that we can develop for
    #[must_use]
    pub fn is_developer_many(&self, scalers: &[ImageScaler]) -> bool {
        // make sure this user
        match self.role {
            // admins can develop for anything
            UserRole::Admin => true,
            // this user can develop for anything
            UserRole::Analyst => true,
            // check if this developer can develop on all scalers
            UserRole::Developer {
                k8s,
                bare_metal,
                windows,
                external,
                kvm,
            } => {
                // check if this user can develop for this scaler
                for scaler in scalers {
                    // get the correct scaler permission
                    let permission = match scaler {
                        ImageScaler::K8s => k8s,
                        ImageScaler::BareMetal => bare_metal,
                        ImageScaler::Windows => windows,
                        ImageScaler::External => external,
                        ImageScaler::Kvm => kvm,
                    };
                    // if we don't have permission then short circuit
                    if !permission {
                        return false;
                    }
                }
                true
            }
            // this user cannot develop images/pipelines
            UserRole::User => false,
        }
    }

    /// generates a random token
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    fn gen_token(&mut self, shared: &Shared) {
        // update token and its expiration
        self.token = token!();
        self.token_expiration = token_expire!(shared);
    }

    /// Saves a users token into the backend
    ///
    /// # Arguments
    ///
    /// * `old` - The old token for this user
    /// * `shared` - Shared Thorium objects
    async fn save_token(&self, old: &str, shared: &Shared) -> Result<(), ApiError> {
        db::users::save_token(self, old, shared).await
    }

    /// Generate and save a new token for a user
    pub async fn regen_token(&mut self, shared: &Shared) -> Result<(), ApiError> {
        // get our old token
        let old = self.token.clone();
        // generate a new token
        self.gen_token(shared);
        // save our new token
        self.save_token(&old, shared).await?;
        Ok(())
    }

    /// Updates a user
    ///
    /// This will invalidate the user's current token if the
    /// password is updated.
    ///
    /// # Arguments
    ///
    /// * `update` - The update to apply to this user
    /// * `shared` - Shared objects in Thorium
    #[instrument(name = "User::update", skip_all, err(Debug))]
    pub async fn update(mut self, update: UserUpdate, shared: &Shared) -> Result<Self, ApiError> {
        // if we are updating our role make sure we are an admin
        if update.role.is_some() {
            // only admins can update roles
            is_admin!(self);
            // update our role
            crate::update!(self.role, update.role);
        }
        // check if we are updating their password
        if let Some(password) = &update.password {
            // disallow password updates for non local accounts
            if shared.config.thorium.auth.ldap.is_none() || self.password.is_some() {
                event!(Level::INFO, msg = "Updating password");
                // get our secret key
                let key = &shared.config.thorium.secret_key;
                // hash password and set a new token
                self.password = Some(hash_pw!(password, key));
                // get our old token
                let old_token = self.token.clone();
                // generate a new token
                self.gen_token(shared);
                // save this users token to the db
                db::users::save_token(&self, &old_token, shared).await?;
            } else {
                return unavailable!("Cannot update password when ldap is enabled".to_string());
            }
        }
        // apply any settings updates
        if let Some(settings) = update.settings {
            settings.apply(&mut self.settings)?;
        }
        // save update user to the backend
        db::users::save(&self, shared).await?;
        Ok(self)
    }

    /// Updates a logged-in user without further authentication
    ///
    /// This will invalidate the user's current token if the
    /// password is updated.
    ///
    /// # Arguments
    ///
    /// * `update` - The update to apply to this user
    /// * `shared` - Shared objects in Thorium
    #[instrument(name = "User::update_user", skip_all, err(Debug))]
    pub async fn update_user(
        &self,
        username: &str,
        update: UserUpdate,
        shared: &Shared,
    ) -> Result<(), ApiError> {
        // only admins can update other users
        is_admin!(self);
        // get info on the target user
        let mut target = User::force_get(username, shared).await?;
        // check if we are updating their password
        if let Some(password) = &update.password {
            // disallow password updates for non local accounts
            if shared.config.thorium.auth.ldap.is_none() || target.password.is_some() {
                event!(Level::INFO, msg = "Updating password");
                // clone key so it will live throughout async closure
                let key = &shared.config.thorium.secret_key;
                // hash password and set a new token
                target.password = Some(hash_pw!(password, key));
                // get our old token
                let old_token = target.token.clone();
                // generate a new token
                target.gen_token(shared);
                // save this users token to the db
                db::users::save_token(&target, &old_token, shared).await?;
            } else {
                return unavailable!("Cannot update password when ldap is enabled".to_string());
            }
        }
        // update our role
        crate::update!(target.role, update.role);
        // apply any settings updates
        if let Some(settings) = update.settings {
            settings.apply(&mut target.settings)?;
        }
        // save update user to the backend
        db::users::save(&target, shared).await?;
        Ok(())
    }

    /// Authenticate a user with the correct authentication method
    ///
    /// This gets the authorization data from the authorization header.
    ///
    /// # Arguments
    ///
    /// * `auth_header` - The auth header value to pull creds from
    /// * `verify_email` - Whether to require a verified email or not
    /// * `shared` - Shared objects in Thorium
    #[instrument(name = "User::auth", skip_all, err(Debug))]
    async fn auth(
        auth_header: &str,
        verify_email: bool,
        shared: &Shared,
    ) -> Result<Self, ApiError> {
        // get our auth method
        let method = check_unauth!(AuthMethods::from_str(auth_header));
        // try to authenticate our user
        match method.authenticate(verify_email, shared).await {
            Ok(user) => {
                event!(Level::INFO, user = &user.username);
                Ok(user)
            }
            Err(error) => {
                // we failed to auth this user due to an error
                event!(Level::ERROR, error = true, error_msg = error.to_string());
                Err(error)
            }
        }
    }

    /// Authorize this user can access some groups or gets the groups they can
    ///
    /// For admins this will get all groups in the cluster
    ///
    /// # Arguments
    ///
    /// * `groups` - The groups to restrict this authorization check too
    /// * `shared` - Shared Thorium objects
    #[instrument(name = "Users::authorize_groups", skip_all, err(Debug))]
    pub async fn authorize_groups(
        &self,
        groups: &mut Vec<String>,
        shared: &Shared,
    ) -> Result<(), ApiError> {
        // for users we can search their groups but for admins we need to search all groups
        // if this user is not an admin then check if any groups were requested otherwise return
        // all the groups they are in
        if self.role == UserRole::Admin {
            // this user is an admin so they can access all groups
            // if they provided groups we just need to make sure they exist
            if !groups.is_empty() {
                // TODO: make sure groups exist
                return Ok(());
            }
            // no groups were provided so get 10000 groups
            // this is a bit of a hack and means if theres more then 10000 groups we silently miss things
            // TODO: Fix this
            let mut list = db::groups::list(0, 10000, shared).await?;
            // extend our groups list
            groups.append(&mut list.names);
            Ok(())
        } else {
            // this user is not an admin so make sure they can access any requested groups
            if groups.is_empty() {
                // no groups were provided so just default to the groups this user is a part of
                Group::authorize_all(self, &self.groups, shared).await?;
                // add our users groups
                groups.append(&mut self.groups.clone());
                Ok(())
            } else {
                Group::authorize_all(self, groups, shared).await?;
                Ok(())
            }
        }
    }

    /// Sync all ldap users info
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    pub async fn sync_all_unix_info(&self, shared: &Shared) -> Result<(), ApiError> {
        // only admins can sync unix info
        is_admin!(self);
        // get our ldap conf
        if let Some(conf) = &shared.config.thorium.auth.ldap {
            //  build an ldap connection
            let (conn, mut ldap) = ldap!(conf).await?;
            // drive this connection to completion
            ldap3::drive!(conn);
            // bind to our system user
            bind_to_ldap_system_user(&mut ldap, conf).await?;
            // get a list of all users
            for user in self.list_details(shared).await? {
                // if we don't have a password set then assume ldap and update our unix info
                let unix = if user.local {
                    // this is a local user so just use our default unix ids
                    shared.config.thorium.auth.local_user_ids.clone()
                } else {
                    // this is an ldap based user so get updated info instead
                    get_unix_info(&user.username, conf, &mut ldap).await?
                };
                // update this users unix info
                db::users::update_unix_info(&user.username, &unix, shared).await?;
            }
            // unbind our ldap socket
            ldap.unbind().await?;
            Ok(())
        } else {
            bad!("LDAP is not configured!".to_owned())
        }
    }

    /// Send an account-link confirmation email to an existing user
    ///
    /// This is auth-provider agnostic and backs both OAuth and LDAP account linking.
    /// It saves a link token tied to the given alias and emails the existing account a
    /// confirmation link. Clicking that link (the `/oauth/{provider}/link` route)
    /// attaches the alias to their account.
    ///
    /// # Arguments
    ///
    /// * `provider` - The provider we are trying to link this user too
    /// * `username` - The username of the existing account to link
    /// * `alias` - The provider alias to link to this user on confirmation
    /// * `email` - The email address to send the confirmation link to
    /// * `shared` - Shared Thorium objects
    #[instrument(name = "User::send_alias_link_email", skip(shared), err(Debug))]
    pub async fn send_alias_link_email(
        provider: &str,
        username: &str,
        alias: &str,
        email: &str,
        shared: &Shared,
    ) -> Result<(), ApiError> {
        // get an email client or error since account linking requires email verification
        let client = match &shared.email {
            Some(client) => client,
            None => return unavailable!("Email is not configured!".to_owned()),
        };
        // resolve the base url to embed in our link, preferring the oauth redirect base
        let oauth_base = shared
            .config
            .thorium
            .auth
            .oauth
            .as_ref()
            .map(|oauth| oauth.redirect_base.as_str());
        let email_base = shared
            .config
            .thorium
            .auth
            .email
            .as_ref()
            .map(|email| email.base_url.as_str());
        // fall back to the email base url so ldap-only deployments still work
        let base = match resolve_link_base(oauth_base, email_base) {
            Some(base) => base,
            None => {
                return unavailable!(
                    "No base URL is configured for account link emails!".to_owned()
                );
            }
        };
        // build an account link token
        let link_token = token!();
        // save our link token and the alias for this user
        db::users::save_link_token(provider, username, &link_token, alias, shared).await?;
        // build our link to the new provider account link to embed in the email
        let link = alias_link_url(base, provider, username, &link_token)?;
        // build the subject for the account link email
        let subject = format!("Link Thorium account to new auth provider: {provider}");
        // get how long this account link is valid for in seconds
        let expire = shared
            .config
            .thorium
            .auth
            .oauth
            .as_ref()
            .map(|oauth| oauth.link_expire)
            .unwrap_or(crate::conf::default_oauth_link_expire());
        // build a human readable time to live for this link
        let ttl = humanize_ttl(expire);
        // build a body with our account link email
        let body = format!(
            "If you would like to link your Thorium account to the {provider} auth provider then click on the following link in the next {ttl}:\n\n{link}"
        );
        // send our account link email
        client.send(email, subject, body).await
    }

    /// Link a new provider alias to an existing account using a link token
    ///
    /// This is auth-provider agnostic and backs the `/oauth/{provider}/link` route for
    /// both OAuth and LDAP. The emailed link proves ownership of the account's email so
    /// the account is also marked verified on success.
    ///
    /// # Arguments
    ///
    /// * `provider` - The provider we are linking this account too
    /// * `username` - The username of the account to add the alias too
    /// * `token` - The link token from the confirmation email
    /// * `shared` - Shared Thorium objects
    #[instrument(name = "User::link_alias", skip(token, shared), err(Debug))]
    pub async fn link_alias(
        provider: &str,
        username: &str,
        token: &str,
        shared: &Shared,
    ) -> Result<(), ApiError> {
        // get this links alias if it exists
        let alias = db::users::consume_link_token(provider, username, token, shared).await?;
        // make sure this alias hasn't been claimed as a username since the link was sent
        if User::exists(&alias, shared).await? {
            // a user already exists with this alias as their username
            return conflict!(format!("{alias} is already a Thorium user"));
        }
        // make sure this alias isn't already linked to a different account
        if let Some(owner) = db::users::get_username_by_alias(provider, &alias, shared).await? {
            // only error if its linked to a different user
            if owner != username {
                return conflict!(format!(
                    "This {provider} identity is already linked to an account"
                ));
            }
        }
        // get the user we want to add an alias too
        let mut user = User::force_get(username, shared).await?;
        // add this alias to this user
        user.aliases.insert(provider.to_owned(), alias);
        // save this users info
        db::users::save(&user, shared).await?;
        // since we got this link through email we can also verify their email if its not yet verified
        if !user.verified {
            // clear this users verification token and set them as verified in redis
            db::users::clear_verification_token(username, shared).await?;
        }
        Ok(())
    }

    /// Revoke an active account-link attempt
    ///
    /// # Arguments
    ///
    /// * `provider` - The provider we are revoking an attempted account link for
    /// * `username` - The username the link attempt was for
    /// * `token` - The link token to revoke
    /// * `shared` - Shared Thorium objects
    #[instrument(name = "User::revoke_alias_link", skip(token, shared), err(Debug))]
    pub async fn revoke_alias_link(
        provider: &str,
        username: &str,
        token: &str,
        shared: &Shared,
    ) -> Result<(), ApiError> {
        // consume and forget this account link token
        db::users::consume_link_token(provider, username, token, shared).await?;
        Ok(())
    }
}

/// Resolve the base URL to embed in an account-link email
///
/// Prefers the OAuth redirect base (the UI base shared with the OAuth flow) and falls
/// back to the email verification base URL so LDAP-only deployments still work.
///
/// # Arguments
///
/// * `oauth_redirect_base` - The OAuth redirect base url if OAuth is configured
/// * `email_base_url` - The email verification base url if email is configured
fn resolve_link_base<'a>(
    oauth_redirect_base: Option<&'a str>,
    email_base_url: Option<&'a str>,
) -> Option<&'a str> {
    // prefer the oauth redirect base then fall back to the email base url
    oauth_redirect_base.or(email_base_url)
}

/// Build the account-link URL to embed in a confirmation email
///
/// # Arguments
///
/// * `base` - The base scheme + domain to build the link against
/// * `provider` - The provider this link is for
/// * `username` - The username of the account being linked
/// * `token` - The link verification token
fn alias_link_url(
    base: &str,
    provider: &str,
    username: &str,
    token: &str,
) -> Result<Url, ApiError> {
    // build the base url for linking accounts
    let endpoint = format!("{base}/oauth/{provider}/link");
    // build our link with the username and token as query params
    let link = Url::parse_with_params(&endpoint, &[("username", username), ("token", token)])?;
    Ok(link)
}

/// Format a number of seconds into a human readable, spaced duration
///
/// humantime renders e.g. "1day", so insert a space between each value and its unit so
/// it reads "1 day".
///
/// # Arguments
///
/// * `secs` - The number of seconds to format
fn humanize_ttl(secs: u64) -> String {
    // render the raw humantime duration
    let raw = humantime::format_duration(std::time::Duration::from_secs(secs)).to_string();
    // build a human time formatted string with spaces
    let mut ttl = String::with_capacity(raw.len() + 1);
    // keep track of the character before our current one
    let mut prev: Option<char> = None;
    // step over each character and add spaces when needed
    for chr in raw.chars() {
        // we will never add a space on the first digit
        if let Some(prev) = prev {
            // check if our last character was a digit and the current one is not
            if prev.is_ascii_digit() && chr.is_ascii_alphabetic() {
                // add a space to seperate the amount from the time visually
                ttl.push(' ');
            }
        }
        // add the next character
        ttl.push(chr);
        // keep track of our previous char
        prev = Some(chr);
    }
    ttl
}

/// Base64 decode a string
///
/// # Arguments
///
/// * `encoded` - A base64 encoded string to decode
fn b64_decode(encoded: &str) -> Result<String, ApiError> {
    // decode our base64'd bytes
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    // convert our decoded bytes to a string
    let decoded_string = str::from_utf8(&decoded[..])?.to_owned();
    Ok(decoded_string)
}

pub struct AuthReject;

impl IntoResponse for AuthReject {
    fn into_response(self) -> Response {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

impl<S> FromRequestParts<S> for User
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthReject;
    /// Gets an authenticated user from a request
    ///
    /// # Arguments
    ///
    /// * `parts` - The request parts to extract our secret key from
    /// * `state` - Shared Thorium objects
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // get the shared app state
        let state = AppState::from_ref(state);
        // extract the authorization headers for this user
        if let Some(header_val) = parts.headers.get("authorization") {
            // try to cast our authorization header value to a str
            if let Ok(header_str) = header_val.to_str() {
                // authenticate this user and make sure they have verified their email
                if let Ok(user) = User::auth(header_str, true, &state.shared).await {
                    return Ok(user);
                }
            }
        }
        // we failed to extract our auth info from our headers
        Err(AuthReject)
    }
}

impl From<User> for ScrubbedUser {
    fn from(user: User) -> Self {
        ScrubbedUser {
            username: user.username,
            email: user.email,
            role: user.role,
            groups: user.groups,
            token: user.token,
            token_expiration: user.token_expiration,
            unix: user.unix,
            settings: user.settings,
            local: user.password.is_some(),
            verified: user.verified,
        }
    }
}

impl From<User> for AuthResponse {
    /// Build an `AuthResponse` from a User
    ///
    /// # Arguments
    ///
    /// * `user` - The user to build an `AuthResponse` from
    fn from(user: User) -> Self {
        // check if this user has verified their email
        if user.verified {
            // this user has
            AuthResponse::Authed {
                token: user.token,
                expires: user.token_expiration,
            }
        } else {
            // This user has not yet verified their email
            AuthResponse::VerifyEmail(user.email)
        }
    }
}

impl<S> FromRequestParts<S> for AuthResponse
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthReject;
    /// Gets an authenticated user from a request
    ///
    /// # Arguments
    ///
    /// * `parts` - The request parts to extract our secret key from
    /// * `state` - Shared Thorium objects
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // get the shared app state
        let state = AppState::from_ref(state);
        // extract the authorization headers for this user
        if let Some(header_val) = parts.headers.get("authorization") {
            // try to cast our authorization header value to a str
            if let Ok(header_str) = header_val.to_str() {
                // authenticate this user but don't require a verified email
                if let Ok(user) = User::auth(header_str, false, &state.shared).await {
                    // return the correct auth response based on if we have a verified email or not
                    let resp = if user.verified {
                        // this user has a verified email and has authenticated so return their token info
                        AuthResponse::Authed {
                            token: user.token,
                            expires: user.token_expiration,
                        }
                    } else {
                        // this user still needs to verify their email
                        AuthResponse::VerifyEmail(user.email)
                    };
                    return Ok(resp);
                }
            }
        }
        // we failed to extract our auth info from our headers
        Err(AuthReject)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LdapRegOutcome, alias_link_url, humanize_ttl, ldap_registration_outcome, resolve_link_base,
    };

    #[test]
    fn ldap_outcome_creates_when_nothing_taken() {
        // no existing alias or email owner means we create a brand new pure-ldap user
        assert_eq!(
            ldap_registration_outcome(None, None),
            LdapRegOutcome::Create
        );
    }

    #[test]
    fn ldap_outcome_links_when_email_taken() {
        // an email already owned by an account forces linking the ldap alias to it
        assert_eq!(
            ldap_registration_outcome(None, Some("john".to_owned())),
            LdapRegOutcome::Link {
                existing: "john".to_owned()
            }
        );
    }

    #[test]
    fn ldap_outcome_alias_taken_short_circuits() {
        // an already linked ldap identity takes precedence even when an email matches
        assert_eq!(
            ldap_registration_outcome(Some("john".to_owned()), None),
            LdapRegOutcome::AliasTaken
        );
        assert_eq!(
            ldap_registration_outcome(Some("john".to_owned()), Some("jane".to_owned())),
            LdapRegOutcome::AliasTaken
        );
    }

    #[test]
    fn link_base_prefers_oauth_then_falls_back_to_email() {
        // prefer the oauth redirect base when it is configured
        assert_eq!(
            resolve_link_base(Some("https://oauth"), Some("https://email")),
            Some("https://oauth")
        );
        // fall back to the email base url when oauth is not configured
        assert_eq!(
            resolve_link_base(None, Some("https://email")),
            Some("https://email")
        );
        // nothing configured yields no base url
        assert_eq!(resolve_link_base(None, None), None);
    }

    #[test]
    fn alias_link_url_targets_the_link_route() {
        // build a link against an api base url for the ldap provider
        let url = alias_link_url("https://thorium.example.com/api", "ldap", "john", "tok123")
            .expect("failed to build alias link url");
        // the path should target the providers link route
        assert_eq!(url.path(), "/api/oauth/ldap/link");
        // the username and token should be present as query params
        let params: std::collections::HashMap<String, String> =
            url.query_pairs().into_owned().collect();
        assert_eq!(params.get("username").map(String::as_str), Some("john"));
        assert_eq!(params.get("token").map(String::as_str), Some("tok123"));
    }

    #[test]
    fn humanize_ttl_separates_amount_and_unit() {
        // a single day renders with a space between the amount and unit
        assert_eq!(humanize_ttl(86_400), "1 day");
        // no digit should be immediately followed by a letter in the output
        let formatted = humanize_ttl(90);
        for window in formatted.as_bytes().windows(2) {
            assert!(
                !(window[0].is_ascii_digit() && window[1].is_ascii_alphabetic()),
                "found unspaced amount/unit in {formatted:?}"
            );
        }
    }
}

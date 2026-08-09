//! Utilties for testing the Thorium API

#[cfg(feature = "ai")]
pub mod ai;
mod api;
pub mod generators;
mod helpers;
mod impls;

pub use api::{CONF, admin_client, admin_token};

// expose a blocking admin client for sync tests
#[cfg(all(feature = "sync", not(feature = "python")))]
pub use api::admin_client_blocking;

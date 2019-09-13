use axum::routing::get_service;
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use crate::utils::AppState;

/// Add the ui route to our router
///
/// # Arguments
///
// * `router` - The router to add routes too
pub fn mount(router: Router<AppState>) -> Router<AppState> {
    router
        .nest_service("/assets", get_service(ServeDir::new("./ui/assets")))
        .nest_service(
            "/ui/thorium.ico",
            get_service(ServeFile::new("./ui/thorium.ico")),
        )
        .nest_service(
            "/ui/ferris-scientist.png",
            get_service(ServeDir::new("./ui/ferris-scientist.png")),
        )
        .nest_service(
            "/ui/manifest.json",
            get_service(ServeFile::new("./ui/manifest.json")),
        )
        .nest_service(
            "/ui/",
            get_service(ServeFile::new("./ui/index.html")).fallback("./ui/index.html"),
        )
}

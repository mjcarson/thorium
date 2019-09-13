//! Utility functions for repos

/// Validate the repo URL
///
/// # Arguments
///
/// * `url` - The URL to validate
pub fn validate_repo_url(url: &str) -> Result<(), thorium::Error> {
    let mut split = url.split('/');
    let host = split.next();
    if host.is_none() {
        return Err(thorium::Error::new("the repo URL is empty"));
    }
    let user = split.next();
    if user.is_none() {
        return Err(thorium::Error::new(
            "the repo URL is missing a user and name",
        ));
    }
    let name = split.next();
    if name.is_none() {
        return Err(thorium::Error::new("the repo URL is missing a name"));
    }
    if split.next().is_some() {
        return Err(thorium::Error::new("the repo URL has too many components"));
    }
    Ok(())
}

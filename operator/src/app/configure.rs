use thorium::{Error, Thorium};

/// Initialize Thorium system settings
///
/// Note: initialization of settings does not override existing settings for clusters
/// that have already been provisioned.
///
/// # Arguments
///
/// * `thorium` - The Thorium client being used for API interactions
pub async fn init_settings(thorium: &Thorium) -> Result<(), Error> {
    println!("Initializing Thorium system settings");
    let result = thorium.system.init().await?;
    // return any non-success result codes
    if result.status() != 204 {
        Err(Error::new(format!(
            "Failed to init system settings: {}",
            &result.status()
        )))
    } else {
        Ok(())
    }
}

//! Tests the basic routes in Thorium

use thorium::{is, test_utilities, Error};

#[tokio::test]
async fn identify() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // send the identify query
    let resp = client.basic.identify().await?;
    // make sure we get the right string back
    is!(resp, "Thorium".to_owned());
    Ok(())
}

#[tokio::test]
async fn health() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // send the identify query
    let health = client.basic.health().await?;
    // make sure Thorium is healthy
    is!(health, true);
    Ok(())
}

//! Tests the users routes in Thorium

use thorium::test_utilities::{self, generators};
use thorium::Error;

#[tokio::test]
async fn delete() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // get a user client
    let client = generators::client(&client).await?;
    // get our users info
    let info = client.users.info().await?;
    // delete our user
    client.users.delete(&info.username).await?;
    Ok(())
}

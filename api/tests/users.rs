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

/// Write a temporary PNG file containing the given data and return its path
///
/// # Arguments
///
/// * `name` - A unique name to use for the temp file
/// * `data` - The image data to write
async fn write_temp_picture(name: &str, data: &[u8]) -> Result<std::path::PathBuf, Error> {
    // build a unique path for this temp picture
    let path = std::env::temp_dir().join(format!("{name}.png"));
    // write our image data to disk
    tokio::fs::write(&path, data)
        .await
        .map_err(|err| Error::new(format!("Failed to write temp picture: {err}")))?;
    Ok(path)
}

#[tokio::test]
async fn profile_picture() -> Result<(), Error> {
    // get admin client
    let admin = test_utilities::admin_client().await?;
    // get a user client
    let client = generators::client(&admin).await?;
    // get our username
    let username = client.users.info().await?.username;
    // build some fake image data
    let image_data = b"\x89PNG\r\n\x1a\n this is a fake profile picture".to_vec();
    // write our image to a temp file
    let path = write_temp_picture(&username, &image_data).await?;
    // upload our profile picture
    client.users.upload_profile_picture(&path).await?;
    // get our profile picture back and make sure it matches
    let retrieved = client.users.get_profile_picture(&username).await?;
    assert_eq!(retrieved.as_ref(), image_data.as_slice());
    // delete our profile picture
    client.users.delete_profile_picture().await?;
    // getting our profile picture now should return a 404
    let err = client
        .users
        .get_profile_picture(&username)
        .await
        .expect_err("Expected a 404 after deleting the profile picture");
    assert_eq!(err.status().map(|code| code.as_u16()), Some(404));
    // clean up our temp file
    let _ = tokio::fs::remove_file(&path).await;
    Ok(())
}

#[tokio::test]
async fn profile_picture_get_other_user() -> Result<(), Error> {
    // get admin client
    let admin = test_utilities::admin_client().await?;
    // get two separate user clients
    let owner = generators::client(&admin).await?;
    let other = generators::client(&admin).await?;
    // get the owner's username
    let owner_username = owner.users.info().await?.username;
    // build some fake image data
    let image_data = b"\x89PNG\r\n\x1a\n another fake profile picture".to_vec();
    // write the owner's image to a temp file
    let path = write_temp_picture(&owner_username, &image_data).await?;
    // upload the owner's profile picture
    owner.users.upload_profile_picture(&path).await?;
    // any authenticated user should be able to get another user's picture
    let retrieved = other.users.get_profile_picture(&owner_username).await?;
    assert_eq!(retrieved.as_ref(), image_data.as_slice());
    // clean up our temp file
    let _ = tokio::fs::remove_file(&path).await;
    Ok(())
}

#[tokio::test]
async fn profile_picture_not_found() -> Result<(), Error> {
    // get admin client
    let admin = test_utilities::admin_client().await?;
    // get a user client with no profile picture
    let client = generators::client(&admin).await?;
    // get our username
    let username = client.users.info().await?.username;
    // getting a profile picture that doesn't exist should return a 404
    let get_err = client
        .users
        .get_profile_picture(&username)
        .await
        .expect_err("Expected a 404 getting a missing profile picture");
    assert_eq!(get_err.status().map(|code| code.as_u16()), Some(404));
    // deleting a profile picture that doesn't exist should return a 404
    let delete_err = client
        .users
        .delete_profile_picture()
        .await
        .expect_err("Expected a 404 deleting a missing profile picture");
    assert_eq!(delete_err.status().map(|code| code.as_u16()), Some(404));
    Ok(())
}

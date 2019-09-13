//! Utility functions relating to images

use std::sync::{Arc, Mutex};

use futures::{stream, TryStreamExt};
use thorium::{models::Image, Cursor, Error, Thorium};

/// Search an image cursor for a given image
///
/// # Arguments
///
/// * `cursor` - The image cursor to search
/// * `group` - The group the image cursor is crawling
/// * `image_name` - The name of the image we are searching for
/// * `matching_groups` - A list of groups containing the matching image
async fn search_image_cursor(
    mut cursor: Cursor<Image>,
    group: String,
    image_name: &str,
    matching_groups: Arc<Mutex<Vec<String>>>,
) -> Result<(), Error> {
    while !cursor.exhausted {
        cursor.next().await?;
        if cursor.names.iter().any(|name| name == image_name) {
            // add the matching group to the list
            matching_groups.lock().unwrap().push(group.clone());
            // stop searching because the image can only appear once within a group
            return Ok(());
        }
    }
    Ok(())
}

/// Find the group that a given image belongs to among the current user's groups
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `image_name` - The name of the image
pub async fn find_image_group(thorium: &Thorium, image_name: &str) -> Result<String, Error> {
    // get all groups for the current user
    let groups = super::groups::get_all_groups(thorium).await?;
    // create a list to contain groups that have a image of the given name
    let matching_groups: Vec<String> = Vec::new();
    // wrap in an Arc<Mutex<>> to add to the list concurrently
    let matching_groups = Arc::new(Mutex::new(matching_groups));
    // create image cursors for each group
    stream::iter(
        groups
            .into_iter()
            .map(|group| Ok((thorium.images.list(&group).limit(1_000_000), group))),
    )
    // concurrently search for the image in each group and add matching groups to the list
    .try_for_each_concurrent(None, |(cursor, group)| {
        search_image_cursor(cursor, group, image_name, matching_groups.clone())
    })
    .await?;
    // unwrap the matching groups from the Arc and Mutex
    let matching_groups = Arc::into_inner(matching_groups)
        .ok_or(Error::new("Concurrency error retrieving image"))?
        .into_inner()
        .map_err(|_| Error::new("Poison mutex error retrieving image"))?;
    // ensure that only a single matching group was found
    match matching_groups.len() {
        len if len < 1 => Err(Error::new("Image not found")),
        len if len > 1 => Err(Error::new(format!(
            "Images with the given name exist in more than one group: {matching_groups:?}. Please specify a group"
        ))),
        _ => matching_groups
            .into_iter()
            .next()
            .ok_or(Error::new("Unable to retrieve image")),
    }
}

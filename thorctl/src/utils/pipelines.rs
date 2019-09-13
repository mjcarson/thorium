//! Utility functions relating to pipelines

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use futures::stream::{self, TryStreamExt};
use thorium::{
    client::{Cursor, Thorium},
    models::Pipeline,
    Error,
};

/// Search the pipeline cursor for a given pipeline
///
/// # Arguments
///
/// * `cursor` - The pipeline cursor to search
/// * `group` - The group the pipeline cursor is crawling
/// * `matching_groups` - A map of the pipelines we're searching for
///                       and the groups they've been found in
async fn search_pipeline_cursor(
    mut cursor: Cursor<Pipeline>,
    group: String,
    matching_groups: Arc<HashMap<String, Mutex<Vec<String>>>>,
) -> Result<(), Error> {
    let mut all_pipelines = matching_groups.keys().collect::<HashSet<_>>();
    while !cursor.exhausted {
        cursor.next().await?;
        for name in &cursor.names {
            if let Some(groups) = matching_groups.get(name) {
                // TODO: maybe handle error correctly here?
                groups.lock().unwrap().push(group.clone());
                // mark that we've seen this pipeline already
                all_pipelines.remove(name);
            }
        }
        // if we've already seen all the pipelines we're searching for, return early
        if all_pipelines.is_empty() {
            return Ok(());
        }
    }
    Ok(())
}

/// Find the groups that pipelines belong to among the current user's groups
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `pipelines` - The names of the pipelines to search for groups for
pub async fn find_pipelines_groups<'a, I>(
    thorium: &Thorium,
    pipelines: I,
) -> Result<HashMap<String, String>, Error>
where
    I: Iterator<Item = &'a String>,
{
    // get all groups for the current user
    let groups = super::groups::get_all_groups(thorium).await?;
    // create a list to contain groups that have a pipeline of the given name
    let mut matching_groups: HashMap<String, Mutex<Vec<String>>> = HashMap::new();
    // initialize our matching groups map
    for pipeline in pipelines {
        matching_groups.insert(pipeline.to_owned(), Mutex::new(Vec::new()));
    }
    // wrap in an Arc<Mutex<>> to add to the list concurrently
    let matching_groups = Arc::new(matching_groups);
    // create pipeline cursors for each group
    stream::iter(
        groups
            .into_iter()
            // TODO: maybe do something better than setting a limit of 1,000,000
            .map(|group| Ok((thorium.pipelines.list(&group).limit(1_000_000), group))),
    )
    // concurrently search for the pipeline in each group and add matching groups to the list
    .try_for_each_concurrent(None, |(cursor, group)| {
        search_pipeline_cursor(cursor, group, matching_groups.clone())
    })
    .await?;
    // unwrap the matching groups from the Arc and Mutex
    let matching_groups = Arc::into_inner(matching_groups).ok_or(Error::new(
        "Concurrency error searching for pipelines' groups",
    ))?;
    let mut matching_groups: HashMap<String, Vec<String>> = matching_groups
        .into_iter()
        .map(|(pipeline, groups)| {
            let groups = groups
                .into_inner()
                .map_err(|_| Error::new("Poison mutex error searching for pipelines' groups"))?;
            Ok((pipeline, groups))
        })
        // propagate any errors
        .collect::<Result<Vec<(String, Vec<String>)>, Error>>()?
        .into_iter()
        // collect into a hash map of pipeline to groups
        .collect();
    // find any pipelines found in multiple groups
    let multi_matches: HashMap<String, Vec<String>> = matching_groups
        .extract_if(|_, groups| groups.len() > 1)
        .collect();
    if !multi_matches.is_empty() {
        return Err(Error::new(format!(
            "The following pipelines were found in multiple groups! Please specify a group: {multi_matches:?}"
        )));
    }
    // find any pipelines found in no groups
    let no_matches: HashMap<String, Vec<String>> = matching_groups
        .extract_if(|_, groups| groups.is_empty())
        .collect();
    if !no_matches.is_empty() {
        return Err(Error::new(format!(
            "The following pipelines could not be found: '{:?}'",
            no_matches.keys()
        )));
    }
    // return the pipelines mapped to the single group we found
    Ok(matching_groups
        .into_iter()
        .map(|(pipeline, mut groups)| (pipeline, groups.remove(0)))
        .collect())
}

/// Find the group the pipeline belongs to among the current user's groups
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `pipeline` - The name of the pipeline to search for a group
pub async fn find_pipeline_group(thorium: &Thorium, pipeline: &String) -> Result<String, Error> {
    // use the above function but just give it a single pipeline
    find_pipelines_groups(thorium, std::iter::once(pipeline))
        .await?
        .into_values()
        .next()
        .ok_or(thorium::Error::new(format!(
            "Unable to find group for the pipeline '{pipeline}'",
        )))
}

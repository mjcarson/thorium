//! Integration tests for `SigmaRule` entity CRUD

mod common;

use test_utilities::generators;
use thorium::test_utilities;

/// Verify a sigma entity can be created and matches its request
#[tokio::test]
async fn create() -> Result<(), thorium::Error> {
    // set up an admin client and a group
    let (client, group) = common::setup().await?;
    // build a sigma rule entity request and verify creation
    let req = generators::gen_entity(&group, generators::gen_sigma_meta());
    common::check_create(&client, &group, req).await
}

/// Verify a sigma entity is updated as requested
#[tokio::test]
async fn update() -> Result<(), thorium::Error> {
    // set up an admin client and a group
    let (client, group) = common::setup().await?;
    // build a sigma rule entity request and verify updates apply
    let req = generators::gen_entity(&group, generators::gen_sigma_meta());
    common::check_update_meta(&client, req, generators::gen_sigma_update).await
}

/// Verify a sigma entity can target every kind of data a sigma rule applies too
///
/// `CompiledFunctions` and `DecompiledFunctions` had no arms in the old hand written
/// `SigmaRuleAppliesTo::FromStr`, so targeting either of them used to 400 on create.
#[tokio::test]
async fn create_all_applies_to() -> Result<(), thorium::Error> {
    // set up an admin client and a group
    let (client, group) = common::setup().await?;
    // build a sigma rule entity request targeting every kind and verify creation
    let req = generators::gen_entity(&group, generators::gen_sigma_meta_all_applies_to());
    common::check_create(&client, &group, req).await
}

/// Verify a sigma entity can be deleted
#[tokio::test]
async fn delete() -> Result<(), thorium::Error> {
    // set up an admin client and a group
    let (client, group) = common::setup().await?;
    // build a sigma rule entity request and verify deletion
    let req = generators::gen_entity(&group, generators::gen_sigma_meta());
    common::check_delete(&client, req).await
}

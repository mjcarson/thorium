//! Integration tests for `CompiledFunction` entity CRUD

mod common;

use test_utilities::generators;
use thorium::test_utilities;

#[tokio::test]
async fn create() -> Result<(), thorium::Error> {
    // set up an admin client and a group
    let (client, group) = common::setup().await?;
    // build a compiled function entity request and verify creation
    let req = generators::gen_entity(&group, generators::gen_compiled_function_meta());
    common::check_create(&client, &group, req).await
}

#[tokio::test]
async fn update() -> Result<(), thorium::Error> {
    // set up an admin client and a group
    let (client, group) = common::setup().await?;
    // build a compiled function entity request and verify updates apply
    let req = generators::gen_entity(&group, generators::gen_compiled_function_meta());
    common::check_update(&client, req).await
}

#[tokio::test]
async fn delete() -> Result<(), thorium::Error> {
    // set up an admin client and a group
    let (client, group) = common::setup().await?;
    // build a compiled function entity request and verify deletion
    let req = generators::gen_entity(&group, generators::gen_compiled_function_meta());
    common::check_delete(&client, req).await
}

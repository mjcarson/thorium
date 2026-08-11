//! Integration tests for `Json` entity CRUD

mod common;

use test_utilities::generators;
use thorium::fail;
use thorium::models::{EntityMetadataRequest, JsonEntity};
use thorium::test_utilities;

/// Verify a json entity can be created and matches its request
#[tokio::test]
async fn create() -> Result<(), thorium::Error> {
    // set up an admin client and a group
    let (client, group) = common::setup().await?;
    // build a json entity request and verify creation
    let req = generators::gen_entity(&group, generators::gen_json_meta());
    common::check_create(&client, &group, req).await
}

/// Verify a json entity is updated as requested
#[tokio::test]
async fn update() -> Result<(), thorium::Error> {
    // set up an admin client and a group
    let (client, group) = common::setup().await?;
    // build a json entity request and verify updates apply
    let req = generators::gen_entity(&group, generators::gen_json_meta());
    common::check_update_meta(&client, req, generators::gen_json_update).await
}

/// Verify a json entity can be deleted
#[tokio::test]
async fn delete() -> Result<(), thorium::Error> {
    // set up an admin client and a group
    let (client, group) = common::setup().await?;
    // build a json entity request and verify deletion
    let req = generators::gen_entity(&group, generators::gen_json_meta());
    common::check_delete(&client, req).await
}

/// Verify documents that sigma cannot scan or that are too large are rejected
#[tokio::test]
async fn reject_invalid_documents() -> Result<(), thorium::Error> {
    // set up an admin client and a group
    let (client, group) = common::setup().await?;
    // build the documents that our API should reject
    let rejected = vec![
        // a top level array is not a valid sigma event
        serde_json::json!([1, 2, 3]),
        // neither is a bare string
        serde_json::json!("just a string"),
        // nor a null document
        serde_json::Value::Null,
        // and this one is well over our default 1 MiB limit
        serde_json::json!({ "blob": "A".repeat(2 * 1024 * 1024) }),
    ];
    // make sure every one of those documents is rejected with a 400
    for data in rejected {
        // build a json entity request around this document
        let meta = EntityMetadataRequest::Json(JsonEntity::new(data));
        // try to create this entity and bind the response so `fail!` can reuse it
        let resp = client
            .entities
            .create(generators::gen_entity(&group, meta))
            .await;
        fail!(resp, 400);
    }
    Ok(())
}

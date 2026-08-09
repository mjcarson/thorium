//! Test the MCP tools Thorium exposes at `/api/mcp`
//!
//! Thorium's MCP server is an rmcp streamable http service mounted as a fallback service, not a
//! set of axum routes, so these tests drive it with a real MCP client instead of hitting URLs.
//! That client is Thorium's own [`thorium::ai::ThorChat`], backed by the scripted fake AI in
//! [`thorium::test_utilities::ai`], which means every request here travels the full production
//! path: MCP transport, tool router, handler, and the loopback Thorium client the handlers build
//! to re-enter the REST API.
//!
//! Note that `ClientSettings::disable_proxy` defaults to false, so an `HTTP_PROXY` in the
//! environment without `127.0.0.1` in `NO_PROXY` will route this loopback traffic through that
//! proxy and these tests will fail to connect. Every other integration test in this crate shares
//! that exposure.

use rand::RngCore;
use rmcp::model::ErrorCode;
use rmcp::object;
use thorium::ai::AiResponse;
use thorium::client::ResultsClient;
use thorium::test_utilities::ai::{
    call, call_one, call_without_args, content_text, mcp_fail, resource_text, structured, thorchat,
    unauthed_mcp,
};
use thorium::test_utilities::{self, generators};
use thorium::{Thorium, contains, is, is_in, is_not, vec_in_vec};
use uuid::Uuid;

use thorium::models::{
    Buffer, OriginRequest, OutputDisplayType, OutputRequest, SampleRequest, Tree,
};

/// The name of the tool these tests attach results to
const TOOL: &str = "McpTool";

/// The result these tests attach to their samples
const RESULT: &str = "I am an mcp test result";

/// Every tool the Thorium MCP server is expected to advertise
const EXPECTED_TOOLS: [&str; 7] = [
    "get_sample",
    "get_sample_results",
    "list_sample_result_file_paths",
    "get_sample_result_file",
    "list_images",
    "list_pipelines",
    "start_tree",
];

/// Make sure an advertised tool's schema declares every param its handler destructures
///
/// # Arguments
///
/// * `tool` - The advertised tool to check the schema of
/// * `expected` - The names of the params this tool's schema must declare
fn has_params(tool: &rmcp::model::Tool, expected: &[&str]) -> Result<(), thorium::Error> {
    // get the properties this tool's schema declares
    let properties = match tool.input_schema.get("properties") {
        Some(properties) => properties,
        None => {
            return Err(thorium::Error::new(format!(
                "The '{}' mcp tool's schema has no properties",
                tool.name
            )));
        }
    };
    // make sure each param we expect is declared
    for param in expected {
        if properties.get(param).is_none() {
            return Err(thorium::Error::new(format!(
                "The '{}' mcp tool's schema is missing the '{param}' param",
                tool.name
            )));
        }
    }
    Ok(())
}

/// Generate a buffer of random bytes so every test run uploads unique samples
fn random_buffer() -> Vec<u8> {
    // build a buffer to fill with random data
    let mut data = vec![0u8; 64];
    // fill our buffer with random data
    rand::rng().fill_bytes(&mut data);
    data
}

/// Upload a random sample to a fresh group
///
/// # Arguments
///
/// * `client` - The client to upload this sample with
async fn sample(client: &Thorium) -> Result<(String, String), thorium::Error> {
    // create a group to hold this sample
    let group = generators::groups(1, client).await?.remove(0).name;
    // build a sample request with random data so reruns don't collide
    let req = SampleRequest::new_buffer(Buffer::new(random_buffer()), vec![group.clone()])
        .description("mcp test file")
        .origin(OriginRequest::downloaded(
            "https://mcp-tests.thorium",
            Some("mcp-tests".to_owned()),
        ));
    // upload this sample
    let hashes = client.files.create(req).await?;
    Ok((group, hashes.sha256))
}

/// Upload a random sample and attach a result with two result files to it
///
/// The returned result id is what the `get_sample_result_file` tool embeds in the uri of the
/// resource it hands back.
///
/// # Arguments
///
/// * `client` - The client to upload this sample and its results with
async fn sample_with_results(client: &Thorium) -> Result<(String, String, Uuid), thorium::Error> {
    // upload a sample to attach results too
    let (group, sha256) = sample(client).await?;
    // build a result with two result files at different depths
    let req = OutputRequest::new(sha256.clone(), TOOL, RESULT, OutputDisplayType::String).buffers(
        vec![
            Buffer::new("mcp-file-one").name("one.txt"),
            Buffer::new("mcp-file-two").name("nested/two.txt"),
        ],
    );
    // send this result to the API
    let resp = client.files.create_result(req).await?;
    Ok((group, sha256, resp.id))
}

#[tokio::test]
async fn list_tools_advertises_all_tools() -> Result<(), thorium::Error> {
    // get an admin token
    let token = test_utilities::admin_token().await?;
    // building a chat runs the real initialize + tools/list handshake
    let chat = thorchat(&token).await?;
    // get the names of every tool the server advertised
    let names = chat
        .ai
        .advertised
        .iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<String>>();
    // make sure every tool we expect was advertised and nothing extra was
    let expected = EXPECTED_TOOLS.map(ToOwned::to_owned);
    vec_in_vec!(expected, names);
    is!(names.len(), EXPECTED_TOOLS.len());
    // every tool needs a description and a schema for an AI to be able to call it
    for tool in &chat.ai.advertised {
        // make sure this tool has a non empty description
        match &tool.description {
            Some(description) => is_not!(description.trim(), ""),
            None => {
                return Err(thorium::Error::new(format!(
                    "The '{}' mcp tool has no description",
                    tool.name
                )));
            }
        }
        // make sure each tool asks for the params its handler destructures
        match tool.name.as_ref() {
            "get_sample" | "get_sample_results" => has_params(tool, &["sha256"])?,
            "list_sample_result_file_paths" => has_params(tool, &["sha256", "tool"])?,
            "get_sample_result_file" => has_params(tool, &["sha256", "tool", "path"])?,
            "list_images" | "list_pipelines" => has_params(tool, &["group"])?,
            "start_tree" => has_params(tool, &["samples", "repos", "entities", "tags"])?,
            other => {
                return Err(thorium::Error::new(format!(
                    "The mcp server advertised an unexpected tool '{other}'"
                )));
            }
        }
    }
    // the chat's context should know about the same tools the server advertised
    is!(chat.context.tools().len(), EXPECTED_TOOLS.len());
    Ok(())
}

#[tokio::test]
async fn get_sample() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // upload a sample to get info on
    let (_, sha256) = sample(&client).await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // get info on our sample
    let result = call_one(&chat, "get_sample", object!({"sha256": sha256.clone()})).await?;
    // this call should have succeeded
    is!(result.is_error, Some(false));
    // the structured content should describe the sample we uploaded
    let structured = structured(&result)?;
    is!(structured.get("sha256").and_then(|v| v.as_str()), Some(sha256.as_str()));
    // the text content should be the same json
    let parsed: serde_json::Value = serde_json::from_str(content_text(&result, 0)?)?;
    is!(&parsed, structured);
    Ok(())
}

#[tokio::test]
async fn get_sample_results() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // upload a sample with results to read back
    let (_, sha256, _) = sample_with_results(&client).await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // get the results for our sample
    let result = call_one(&chat, "get_sample_results", object!({"sha256": sha256})).await?;
    // this call should have succeeded
    is!(result.is_error, Some(false));
    // the handler flattens each tool's results down to just the newest result's value
    let structured = structured(&result)?;
    is!(structured.get(TOOL).and_then(|v| v.as_str()), Some(RESULT));
    Ok(())
}

#[tokio::test]
async fn list_sample_result_file_paths() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // upload a sample with two result files
    let (_, sha256, _) = sample_with_results(&client).await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // list the result files for our tool
    let result = call_one(
        &chat,
        "list_sample_result_file_paths",
        object!({"sha256": sha256, "tool": TOOL}),
    )
    .await?;
    // this call should have succeeded
    is!(result.is_error, Some(false));
    // pull the paths out of our structured content
    let paths = match structured(&result)?.as_array() {
        Some(paths) => paths
            .iter()
            .filter_map(|path| path.as_str().map(ToOwned::to_owned))
            .collect::<Vec<String>>(),
        None => {
            return Err(thorium::Error::new(
                "Expected list_sample_result_file_paths to return an array of paths",
            ));
        }
    };
    // both of the files we uploaded should be listed
    is!(paths.len(), 2);
    is_in!(paths, "one.txt".to_owned());
    is_in!(paths, "nested/two.txt".to_owned());
    // there should be one content block per path
    is!(result.content.len(), 2);
    Ok(())
}

#[tokio::test]
async fn get_sample_result_file() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // upload a sample with two result files
    let (_, sha256, result_id) = sample_with_results(&client).await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // download the first of our result files
    let result = call_one(
        &chat,
        "get_sample_result_file",
        object!({"sha256": sha256.clone(), "tool": TOOL, "path": "one.txt"}),
    )
    .await?;
    // this call should have succeeded
    is!(result.is_error, Some(false));
    // this is the only tool that returns a resource instead of structured content
    is!(result.structured_content.is_none(), true);
    is!(result.content.len(), 1);
    // get the resource this tool handed back
    let (uri, text) = resource_text(&result, 0)?;
    // the resource should hold the contents of the file we uploaded
    is!(text, "mcp-file-one");
    // and its uri should point back at the result this file came from
    is!(uri, format!("{sha256}/{TOOL}/{result_id}/one.txt"));
    Ok(())
}

#[tokio::test]
async fn get_sample_result_file_binary_is_lossy() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // upload a sample to attach a binary result file too
    let (_, sha256) = sample(&client).await?;
    // build a result with a result file that is not valid utf8
    let req = OutputRequest::new(sha256.clone(), TOOL, RESULT, OutputDisplayType::String)
        .buffer(Buffer::new(vec![0xff, 0xfe, 0x00, 0x01]).name("binary.bin"));
    // send this result to the API
    client.files.create_result(req).await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // download our binary result file
    let result = call_one(
        &chat,
        "get_sample_result_file",
        object!({"sha256": sha256, "tool": TOOL, "path": "binary.bin"}),
    )
    .await?;
    // get the text of the resource this tool handed back
    let (_, text) = resource_text(&result, 0)?;
    // this tool decodes result files with from_utf8_lossy, so the invalid bytes became
    // replacement characters and the original bytes are gone. this test pins that behavior so a
    // future switch to a base64 blob resource fails loudly instead of silently changing what
    // clients receive
    contains!(text, "\u{FFFD}");
    is_not!(text.as_bytes(), [0xff, 0xfe, 0x00, 0x01]);
    Ok(())
}

#[tokio::test]
async fn list_images() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // create a group with two images in it
    let group = generators::groups(1, &client).await?.remove(0).name;
    let images = generators::images(&group, 2, false, &client).await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // list the images in our group
    let result = call_one(&chat, "list_images", object!({"group": group})).await?;
    // this call should have succeeded
    is!(result.is_error, Some(false));
    // pull the image names out of our structured content
    let names = match structured(&result)?.get("data").and_then(|data| data.as_array()) {
        Some(data) => data
            .iter()
            .filter_map(|image| image.get("name").and_then(|name| name.as_str()))
            .map(ToOwned::to_owned)
            .collect::<Vec<String>>(),
        None => {
            return Err(thorium::Error::new(
                "Expected list_images to return a data array",
            ));
        }
    };
    // both of the images we created should be listed
    for image in &images {
        is_in!(names, image.name);
    }
    // and there should be one content block per image
    is!(result.content.len(), names.len());
    Ok(())
}

#[tokio::test]
async fn list_pipelines() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // create a group with a pipeline in it
    let group = generators::groups(1, &client).await?.remove(0).name;
    let pipeline = generators::gen_pipe(&group, 2, false, &client).await?;
    client.pipelines.create(&pipeline).await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // list the pipelines in our group
    let result = call_one(&chat, "list_pipelines", object!({"group": group})).await?;
    // this call should have succeeded
    is!(result.is_error, Some(false));
    // pull the pipeline names out of our structured content
    let names = match structured(&result)?.get("data").and_then(|data| data.as_array()) {
        Some(data) => data
            .iter()
            .filter_map(|pipe| pipe.get("name").and_then(|name| name.as_str()))
            .map(ToOwned::to_owned)
            .collect::<Vec<String>>(),
        None => {
            return Err(thorium::Error::new(
                "Expected list_pipelines to return a data array",
            ));
        }
    };
    // the pipeline we created should be listed
    is_in!(names, pipeline.name);
    Ok(())
}

#[tokio::test]
async fn start_tree() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // upload a root sample
    let (group, root) = sample(&client).await?;
    // upload a child that was unpacked out of our root
    let child_req = SampleRequest::new_buffer(Buffer::new(random_buffer()), vec![group])
        .description("mcp test child")
        .origin(OriginRequest::unpacked(&root, Some("mcp-tests".to_owned())));
    let child = client.files.create(child_req).await?.sha256;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // grow a tree starting from our root, passing samples as a list
    let result = call_one(&chat, "start_tree", object!({"samples": [root.clone()]})).await?;
    // this call should have succeeded
    is!(result.is_error, Some(false));
    // deserialize the tree we got back
    let tree: Tree = serde_json::from_value(structured(&result)?.clone())?;
    // the tree should have started from our root
    is!(tree.initial.len(), 1);
    // render the tree so we can check that both of our samples turned up in it
    let rendered = serde_json::to_string(&tree)?;
    contains!(rendered, root.as_str());
    contains!(rendered, child.as_str());
    // the samples field is annotated with OneOrMany so a bare string should work the same way
    let bare = call_one(&chat, "start_tree", object!({"samples": root.clone()})).await?;
    is!(bare.is_error, Some(false));
    // and it should have started from the same single sample
    let bare_tree: Tree = serde_json::from_value(structured(&bare)?.clone())?;
    is!(bare_tree.initial, tree.initial);
    Ok(())
}

#[tokio::test]
async fn get_sample_unknown_sha256() -> Result<(), thorium::Error> {
    // get an admin token
    let token = test_utilities::admin_token().await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // ask for a sample that was never uploaded
    let result = call_one(
        &chat,
        "get_sample",
        object!({"sha256": "0".repeat(64)}),
    )
    .await;
    // a missing sample should map to a resource not found error
    mcp_fail(result, ErrorCode::RESOURCE_NOT_FOUND, Some("not found"))
}

#[tokio::test]
async fn list_sample_result_file_paths_unknown_tool() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // upload a sample with results for a different tool
    let (_, sha256, _) = sample_with_results(&client).await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // ask for result files from a tool that never ran
    let result = call_one(
        &chat,
        "list_sample_result_file_paths",
        object!({"sha256": sha256.clone(), "tool": "NotATool"}),
    )
    .await;
    // a tool with no results should map to a resource not found error
    mcp_fail(
        result,
        ErrorCode::RESOURCE_NOT_FOUND,
        Some(&format!(
            "NotATool doesn't exist or doesn't have results for {sha256}"
        )),
    )
}

#[tokio::test]
async fn get_sample_result_file_unknown_tool() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // upload a sample with results for a different tool
    let (_, sha256, _) = sample_with_results(&client).await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // ask for a result file from a tool that never ran
    let result = call_one(
        &chat,
        "get_sample_result_file",
        object!({"sha256": sha256.clone(), "tool": "NotATool", "path": "one.txt"}),
    )
    .await;
    // a tool with no results should map to a resource not found error
    mcp_fail(
        result,
        ErrorCode::RESOURCE_NOT_FOUND,
        Some(&format!(
            "NotATool doesn't exist or doesn't have results for {sha256}"
        )),
    )
}

#[tokio::test]
async fn get_sample_result_file_unknown_path() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // upload a sample with two result files
    let (_, sha256, _) = sample_with_results(&client).await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // ask for a result file that this result doesn't have
    let result = call_one(
        &chat,
        "get_sample_result_file",
        object!({"sha256": sha256, "tool": TOOL, "path": "not-a-file.txt"}),
    )
    .await;
    // a missing result file should be a resource not found error, but every s3 GetObject failure
    // is converted with bad_internal! in api/src/utils/errors.rs:426, so a missing key comes back
    // as a 400 that mcp maps to an invalid request. worse, that 400 carries the whole `{:#?}` of
    // the aws sdk error, so bucket names and request ids leak to the caller. pin both here so a
    // fix to that conversion fails this test instead of silently changing the mcp contract
    mcp_fail(result, ErrorCode::INVALID_REQUEST, Some("NoSuchKey"))
}

#[tokio::test]
async fn start_tree_empty_query() -> Result<(), thorium::Error> {
    // get an admin token
    let token = test_utilities::admin_token().await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // every field on this tool defaults to empty, so an empty object is a valid tool call that
    // produces a query with nothing to start growing from
    let result = call_one(&chat, "start_tree", object!({})).await;
    // an empty tree query should map to an invalid request error
    mcp_fail(
        result,
        ErrorCode::INVALID_REQUEST,
        Some("Initial starting data must be set!"),
    )
}

#[tokio::test]
async fn missing_required_arguments() -> Result<(), thorium::Error> {
    // get an admin token
    let token = test_utilities::admin_token().await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // call a tool with no arguments at all so rmcp's extractor has nothing to deserialize
    let result = chat.call_tools(vec![call_without_args("get_sample")]).await;
    // missing params should be rejected before the handler ever runs
    mcp_fail(
        result,
        ErrorCode::INVALID_PARAMS,
        Some("failed to deserialize parameters"),
    )
}

#[tokio::test]
async fn unknown_tool_name() -> Result<(), thorium::Error> {
    // get an admin token
    let token = test_utilities::admin_token().await?;
    // build a chat to call mcp tools with
    let chat = thorchat(&token).await?;
    // call a tool the server never advertised
    let result = call_one(&chat, "not_a_real_tool", object!({})).await;
    // rmcp rejects unknown tools with invalid params rather than method not found
    mcp_fail(result, ErrorCode::INVALID_PARAMS, Some("tool not found"))
}

#[tokio::test]
async fn missing_authorization_header() -> Result<(), thorium::Error> {
    // make sure the API is up before we try to connect to it
    test_utilities::admin_token().await?;
    // connect to the mcp server without sending an authorization header
    let mcp = unauthed_mcp().await?;
    // there is no auth middleware in front of /api/mcp, so listing tools still works
    let tools = mcp.list_tools(None).await?;
    is!(tools.tools.len(), EXPECTED_TOOLS.len());
    // but every tool needs a token to build a client, so calling one fails
    let result = mcp
        .call_tool(rmcp::model::CallToolRequestParam {
            name: "get_sample".into(),
            arguments: Some(object!({"sha256": "0".repeat(64)})),
        })
        .await
        .map_err(thorium::Error::from);
    // a missing token should be rejected before we ever talk to the REST API
    mcp_fail(
        result,
        ErrorCode::INVALID_PARAMS,
        Some("Missing authorization header"),
    )?;
    // shut our client down cleanly, ignoring how its background task exited
    let _ = mcp.cancel().await;
    Ok(())
}

#[tokio::test]
async fn non_admin_cannot_see_other_groups_data() -> Result<(), thorium::Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // upload a sample the admin can see and a non member cannot
    let (group, sha256, _) = sample_with_results(&client).await?;
    // create an image and a pipeline in that same group
    let pipeline = generators::gen_pipe(&group, 1, false, &client).await?;
    client.pipelines.create(&pipeline).await?;
    // create a user who is not in that group
    let (_, other_token) = generators::client_with_token(&client).await?;
    // build a chat for our non member
    let chat = thorchat(&other_token).await?;
    // a non member shouldn't be able to see this sample at all. because samples are filtered by
    // group before they are looked up, this is a not found rather than an unauthorized
    let result = call_one(&chat, "get_sample", object!({"sha256": sha256.clone()})).await;
    mcp_fail(result, ErrorCode::RESOURCE_NOT_FOUND, None)?;
    // listing images in a group they aren't in is an unauthorized error in the REST API, but the
    // mcp error conversion only maps 404 and 400 to distinct codes so everything else collapses
    // into an internal error. assert the code only, since an unauthorized error carries no message
    let result = call_one(&chat, "list_images", object!({"group": group.clone()})).await;
    mcp_fail(result, ErrorCode::INTERNAL_ERROR, None)?;
    // listing pipelines in that group collapses the same way
    let result = call_one(&chat, "list_pipelines", object!({"group": group})).await;
    mcp_fail(result, ErrorCode::INTERNAL_ERROR, None)?;
    // growing a tree from a sample they can't see fails rather than returning an empty tree, since
    // the tree seeds itself with Sample::get and that is group filtered before the lookup
    let result = call_one(&chat, "start_tree", object!({"samples": [sha256]})).await;
    mcp_fail(result, ErrorCode::RESOURCE_NOT_FOUND, None)
}

#[tokio::test]
async fn ask_runs_the_tool_loop() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // upload a sample for our ai to ask about
    let (_, sha256) = sample(&client).await?;
    // build a chat to ask questions of
    let mut chat = thorchat(&token).await?;
    // script our ai to call a tool and then answer once it has the result
    chat.ai
        .push(AiResponse::CallTool(vec![call(
            "get_sample",
            object!({"sha256": sha256.clone()}),
        )]));
    chat.ai
        .push(AiResponse::Response(Some("all done".to_owned())));
    // ask our question
    let answer = chat.ask("What do you know about this sample?").await?;
    // we should have gotten our scripted answer back
    is!(answer, Some("all done".to_owned()));
    // our ai should have been told about exactly one batch of one tool result
    is!(chat.ai.observed.len(), 1);
    is!(chat.ai.observed[0].len(), 1);
    // and that tool call should have succeeded against the real API
    let (_, name, result) = &chat.ai.observed[0][0];
    is!(name.as_str(), "get_sample");
    is!(result.is_error, Some(false));
    // the context should hold both our question and the tool result
    let history = chat.context.history();
    is!(history.len(), 2);
    contains!(history[0], "What do you know about this sample?");
    contains!(history[1], sha256.as_str());
    Ok(())
}

#[tokio::test]
async fn ask_loops_until_response() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // upload a sample with results so both of our tool calls have something to read
    let (_, sha256, _) = sample_with_results(&client).await?;
    // build a chat to ask questions of
    let mut chat = thorchat(&token).await?;
    // script two rounds of tool calls before our ai finally answers
    chat.ai
        .push(AiResponse::CallTool(vec![call(
            "get_sample",
            object!({"sha256": sha256.clone()}),
        )]));
    chat.ai
        .push(AiResponse::CallTool(vec![call(
            "get_sample_results",
            object!({"sha256": sha256.clone()}),
        )]));
    chat.ai
        .push(AiResponse::Response(Some("two rounds".to_owned())));
    // ask our question
    let answer = chat.ask("Tell me everything").await?;
    // we should have gotten our scripted answer back
    is!(answer, Some("two rounds".to_owned()));
    // the tool loop should have re-entered, giving us two separate batches of results
    is!(chat.ai.observed.len(), 2);
    is!(chat.ai.observed[0][0].1, "get_sample");
    is!(chat.ai.observed[1][0].1, "get_sample_results");
    Ok(())
}

#[tokio::test]
async fn ask_calls_tools_in_parallel() -> Result<(), thorium::Error> {
    // get admin client and token
    let client = test_utilities::admin_client().await?;
    let token = test_utilities::admin_token().await?;
    // upload a sample with results and give it a group with an image in it
    let (group, sha256, _) = sample_with_results(&client).await?;
    generators::images(&group, 1, false, &client).await?;
    // build a chat to ask questions of
    let mut chat = thorchat(&token).await?;
    // build three tool calls that should all be issued at once
    let calls = vec![
        call("get_sample", object!({"sha256": sha256.clone()})),
        call("get_sample_results", object!({"sha256": sha256.clone()})),
        call("list_images", object!({"group": group})),
    ];
    // remember the ids we asked for so we can check they all came back
    let ids = calls.iter().map(|(id, _)| *id).collect::<Vec<Uuid>>();
    // script our ai to make all three calls at once and then answer
    chat.ai.push(AiResponse::CallTool(calls));
    chat.ai
        .push(AiResponse::Response(Some("fanned out".to_owned())));
    // ask our question
    let answer = chat.ask("Summarize this sample").await?;
    // we should have gotten our scripted answer back
    is!(answer, Some("fanned out".to_owned()));
    // all three calls should have come back in a single batch
    is!(chat.ai.observed.len(), 1);
    is!(chat.ai.observed[0].len(), 3);
    // every id we asked for should be accounted for and every call should have succeeded
    for (id, _, result) in &chat.ai.observed[0] {
        is_in!(ids, *id);
        is!(result.is_error, Some(false));
    }
    Ok(())
}

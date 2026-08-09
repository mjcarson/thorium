//! Utilities for testing Thorium's MCP tools through [`ThorChat`]
//!
//! Thorium's MCP server is not a set of axum routes, it is an rmcp
//! [`StreamableHttpService`](rmcp::transport::streamable_http_server::StreamableHttpService)
//! mounted as a fallback service at `/api/mcp`. That means the only honest way to test it is to
//! drive it with a real MCP client, and the client Thorium ships is [`ThorChat`].
//!
//! [`ThorChat`] is generic over [`AiSupport`], and the only real implementation talks to a live
//! LLM endpoint. So this module provides [`ScriptedAi`], an [`AiSupport`] implementation whose
//! "reasoning" is a queue of canned responses the test pushes in ahead of time. That gives tests
//! the entire real path -- [`ThorChat::new`] -> mcp `initialize` -> `tools/list` ->
//! [`AiSupport::load_tools`] -> [`ThorChat::ask`] -> [`ThorChat::call_tools`] -> real MCP
//! `tools/call` requests against the running API -- with no AI infrastructure at all.
//!
//! Note that [`ClientSettings::disable_proxy`](crate::client::ClientSettings::disable_proxy)
//! defaults to false, so an `HTTP_PROXY` in the environment without `127.0.0.1` in `NO_PROXY`
//! will route this loopback traffic through that proxy. Every other integration test in this
//! crate shares that exposure.

use rmcp::model::{
    CallToolRequestParam, CallToolResult, ClientCapabilities, ClientInfo, ErrorCode, Implementation,
    InitializeRequestParam, JsonObject, ListToolsResult, ProtocolVersion, ResourceContents, Tool,
};
use rmcp::service::{RunningService, ServiceError};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{RoleClient, ServiceExt};
use std::collections::VecDeque;
use uuid::Uuid;

use crate::ai::{AiMsgRole, AiResponse, AiSupport, SharedThorChatContext, ThorChat};
use crate::client::Keys;
use crate::test_utilities::CONF;
use crate::{CtlConf, Error};

/// A tool that [`ScriptedAi`] knows about
///
/// This exists instead of reusing [`rmcp::model::Tool`] because [`AiSupport::tool_name`] must
/// return a `&String` and rmcp stores tool names as a [`std::borrow::Cow`], which cannot be
/// borrowed as a `&String`.
#[derive(Debug, Clone)]
pub struct ScriptedTool {
    /// The name of this tool
    pub name: String,
    /// The description the MCP server advertised for this tool
    pub description: Option<String>,
    /// The JSON schema describing this tool's parameters
    pub input_schema: serde_json::Value,
}

/// An [`AiSupport`] implementation that replays a canned script instead of doing inference
///
/// Tests build a [`ThorChat`] with [`thorchat`], push the responses they want the "AI" to give
/// with [`ScriptedAi::push`], and then call [`ThorChat::ask`]. Everything downstream of the AI --
/// the MCP transport, the tool router, the handlers, and the loopback Thorium client they build --
/// is the real thing.
pub struct ScriptedAi {
    /// The tools the MCP server advertised, exactly as they came off the wire
    ///
    /// [`AiSupport::load_tools`] loses information when it converts into [`ScriptedTool`], so the
    /// raw tools are kept here for tests that assert on the advertised tool set.
    pub advertised: Vec<Tool>,
    /// The responses this AI will hand back, in order
    pub script: VecDeque<AiResponse>,
    /// Every batch of tool results this AI was told about, in the order they arrived
    pub observed: Vec<Vec<(Uuid, String, CallToolResult)>>,
    /// Whether debug mode was enabled
    pub debug: bool,
    /// The shared chat context this AI writes its history into
    pub context: SharedThorChatContext<Self>,
}

impl ScriptedAi {
    /// Queue up a response for this AI to give
    ///
    /// Responses are handed back in the order they were pushed.
    ///
    /// # Arguments
    ///
    /// * `response` - The response to queue
    pub fn push(&mut self, response: AiResponse) {
        self.script.push_back(response);
    }

    /// Pop the next scripted response off the queue
    ///
    /// An empty queue means the test under-scripted this AI, which would otherwise show up as an
    /// infinite loop in [`ThorChat::ask`], so this errors loudly instead.
    fn pop(&mut self) -> Result<AiResponse, Error> {
        match self.script.pop_front() {
            Some(response) => Ok(response),
            None => Err(Error::new(
                "ScriptedAi ran out of scripted responses; push another AiResponse before asking",
            )),
        }
    }
}

#[async_trait::async_trait]
impl AiSupport for ScriptedAi {
    /// The tools this AI can call
    type Tool = ScriptedTool;

    /// A single chat message, kept as a plain rendered string
    type ChatMsg = String;

    /// Setup this scripted AI
    ///
    /// The config is ignored since this AI never talks to an inference endpoint.
    ///
    /// # Arguments
    ///
    /// * `conf` - A Thorctl config, unused by this implementation
    /// * `context` - The shared context for ai chat
    async fn setup(
        _conf: &CtlConf,
        context: &SharedThorChatContext<Self>,
    ) -> Result<Self, Error> {
        Ok(ScriptedAi {
            advertised: Vec::default(),
            script: VecDeque::default(),
            observed: Vec::default(),
            debug: false,
            context: context.clone(),
        })
    }

    /// Configure the debug mode for this ai
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether or not debug mode is enabled
    fn debug_mode(&mut self, enabled: bool) {
        self.debug = enabled;
    }

    /// Get a tools name
    ///
    /// # Arguments
    ///
    /// * `tool` - The tool to get a name for
    fn tool_name(tool: &Self::Tool) -> &String {
        &tool.name
    }

    /// Tell our AI about our tools
    ///
    /// # Arguments
    ///
    /// * `mcp_tools` - The mcp tools to tell our ai about
    fn load_tools(&mut self, mcp_tools: ListToolsResult) -> Result<(), Error> {
        // convert each advertised mcp tool into a tool this ai can track
        for mcp_tool in &mcp_tools.tools {
            // build our scripted tool
            let tool = ScriptedTool {
                name: mcp_tool.name.to_string(),
                description: mcp_tool.description.as_ref().map(ToString::to_string),
                // the schema is behind an Arc so clone the map out from under it
                input_schema: serde_json::Value::Object((*mcp_tool.input_schema).clone()),
            };
            // add this tool to our shared context
            self.context.add_tool(tool);
        }
        // keep the raw advertised tools around for tests to assert on
        self.advertised = mcp_tools.tools;
        Ok(())
    }

    /// Build a chat completion message
    ///
    /// # Arguments
    ///
    /// * `role` - The role for this message
    /// * `name` - The name to use for this message (primarily for tool names)
    /// * `msg` - The message to convert
    fn build_chat_msg(
        role: AiMsgRole,
        name: Option<String>,
        msg: impl Into<String>,
    ) -> Self::ChatMsg {
        // render this message with its name if it has one
        match name {
            Some(name) => format!("{role:?}/{name}: {}", msg.into()),
            None => format!("{role:?}: {}", msg.into()),
        }
    }

    /// Add the result from a tool
    ///
    /// # Arguments
    ///
    /// * `tool_results` - The tool results to tell our AI about
    async fn add_tool_results(
        &mut self,
        tool_results: Vec<(Uuid, String, CallToolResult)>,
    ) -> Result<AiResponse, Error> {
        // add each tool result to our shared context
        for (id, name, result) in &tool_results {
            // serialize this result the way a real ai backend would before sending it upstream
            let serialized = serde_json::to_string(result)?;
            // add this result to our context
            self.context.add_tool_result(*id, name, serialized);
        }
        // record this batch so tests can assert on what the tools actually returned
        self.observed.push(tool_results);
        // hand back the next scripted response
        self.pop()
    }

    /// Ask this agent a question
    ///
    /// # Arguments
    ///
    /// * `question` - The question to ask our ai
    async fn ask<T: Into<String> + Send + Sync>(
        &mut self,
        question: T,
    ) -> Result<AiResponse, Error> {
        // record the question in our shared context
        self.context.add_chat(AiMsgRole::User, question.into())?;
        // hand back the next scripted response
        self.pop()
    }
}

/// Build a Thorctl config pointed at the in process test API
///
/// The token must be the raw token, not the base64 encoded auth string a
/// [`Thorium`](crate::Thorium) client holds. MCP clients send `Authorization: Bearer <raw token>`
/// and the MCP handlers hand whatever follows the first space to `Thorium::build(..).token(..)`,
/// which does its own encoding.
///
/// # Arguments
///
/// * `token` - The raw Thorium token to authenticate with
#[must_use]
pub fn ctl_conf(token: &str) -> CtlConf {
    // build the url to the test api
    let api = format!("http://{}:{}", CONF.thorium.interface, CONF.thorium.port);
    // build a config pointed at that api
    CtlConf::new(Keys::new_token(api, token))
}

/// Build a [`ThorChat`] backed by a [`ScriptedAi`] and connected to the test API's MCP server
///
/// This runs the real `initialize` and `tools/list` handshake, so the returned chat already knows
/// about every tool the server advertises.
///
/// # Arguments
///
/// * `token` - The raw Thorium token to authenticate with
pub async fn thorchat(token: &str) -> Result<ThorChat<ScriptedAi>, Error> {
    ThorChat::<ScriptedAi>::new(&ctl_conf(token)).await
}

/// Connect to the test API's MCP server without sending an `Authorization` header
///
/// `crate::ai::utils::setup_mcp` refuses to build a client without a token and lives in a private
/// module, so the transport is built here directly. This mirrors that function exactly apart from
/// omitting the auth header, which is what lets tests reach the missing-header branch in
/// `McpConfig::grab_token`.
pub async fn unauthed_mcp() -> Result<RunningService<RoleClient, InitializeRequestParam>, Error> {
    // build the url to the test api's mcp routes
    let mcp_uri = format!(
        "http://{}:{}/api/mcp",
        CONF.thorium.interface, CONF.thorium.port
    );
    // build the config to use with this transport, deliberately without an auth header
    let mut config = StreamableHttpClientTransportConfig::with_uri(mcp_uri);
    // make our mcp client stateless
    config.allow_stateless = true;
    // setup our transport
    let transport = StreamableHttpClientTransport::from_config(config);
    // build our client info
    let client_info = ClientInfo {
        protocol_version: ProtocolVersion::default(),
        capabilities: ClientCapabilities::default(),
        client_info: Implementation {
            name: "ThoriumTest".to_owned(),
            title: Some("ThoriumTest".to_owned()),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            icons: None,
            website_url: None,
        },
    };
    // start our mcp client
    let mcp_client = client_info
        .serve(transport)
        .await
        .map_err(|err| Error::new(format!("Failed to start an unauthed mcp client: {err}")))?;
    Ok(mcp_client)
}

/// Build a single tool call request with a fresh id
///
/// # Arguments
///
/// * `name` - The name of the mcp tool to call
/// * `args` - The arguments to call this tool with
#[must_use]
pub fn call(name: &'static str, args: JsonObject) -> (Uuid, CallToolRequestParam) {
    (
        Uuid::new_v4(),
        CallToolRequestParam {
            name: name.into(),
            arguments: Some(args),
        },
    )
}

/// Build a tool call request that passes no arguments at all
///
/// This is how tests reach rmcp's parameter extractor with nothing to deserialize.
///
/// # Arguments
///
/// * `name` - The name of the mcp tool to call
#[must_use]
pub fn call_without_args(name: &'static str) -> (Uuid, CallToolRequestParam) {
    (
        Uuid::new_v4(),
        CallToolRequestParam {
            name: name.into(),
            arguments: None,
        },
    )
}

/// Call a single mcp tool and get its result
///
/// # Arguments
///
/// * `chat` - The chat to call this tool with
/// * `name` - The name of the mcp tool to call
/// * `args` - The arguments to call this tool with
pub async fn call_one(
    chat: &ThorChat<ScriptedAi>,
    name: &'static str,
    args: JsonObject,
) -> Result<CallToolResult, Error> {
    // call this single tool
    let mut results = chat.call_tools(vec![call(name, args)]).await?;
    // we asked for exactly one tool call so we should have gotten exactly one result
    if results.len() != 1 {
        return Err(Error::new(format!(
            "Expected exactly 1 result from '{name}' but got {}",
            results.len()
        )));
    }
    // pull our only result out
    let (_, _, result) = results.remove(0);
    Ok(result)
}

/// Assert that an mcp call failed with a specific error code and message
///
/// The crate's `fail!` macro cannot be used here because it compares `Error::status`, which
/// returns `None` for [`Error::RmcpServiceError`], so every mcp error would look identical to it.
///
/// # Arguments
///
/// * `result` - The result of the mcp call that should have failed
/// * `expected` - The rmcp error code the call should have failed with
/// * `contains` - A substring the error message must contain, if any
///
/// # Errors
///
/// Errors if the call succeeded, failed with a different error type, failed with a different
/// code, or failed with a message that doesn't contain `contains`.
pub fn mcp_fail<T: std::fmt::Debug>(
    result: Result<T, Error>,
    expected: ErrorCode,
    contains: Option<&str>,
) -> Result<(), Error> {
    // get the error this call failed with
    let error = match result {
        Ok(value) => {
            return Err(Error::new(format!(
                "Expected an mcp error with code {expected:?} but the call succeeded with {value:?}"
            )));
        }
        Err(error) => error,
    };
    // make sure this is an mcp protocol error and not some other failure
    let data = match &error {
        Error::RmcpServiceError(ServiceError::McpError(data)) => data,
        other => {
            return Err(Error::new(format!(
                "Expected an mcp error with code {expected:?} but got a different error: {other:?}"
            )));
        }
    };
    // make sure we got the code we expected
    if data.code != expected {
        return Err(Error::new(format!(
            "Expected mcp error code {expected:?} but got {:?} with message '{}'",
            data.code, data.message
        )));
    }
    // make sure our message contains what we expected it too
    if let Some(contains) = contains
        && !data.message.contains(contains)
    {
        return Err(Error::new(format!(
            "Expected mcp error message to contain '{contains}' but got '{}'",
            data.message
        )));
    }
    Ok(())
}

/// Get the text of a specific content block in an mcp tool result
///
/// # Arguments
///
/// * `result` - The tool result to get content from
/// * `index` - The index of the content block to get text for
///
/// # Errors
///
/// Errors if there is no content block at `index` or that block isn't text.
pub fn content_text(result: &CallToolResult, index: usize) -> Result<&str, Error> {
    // get the content block at this index
    let content = match result.content.get(index) {
        Some(content) => content,
        None => {
            return Err(Error::new(format!(
                "Expected a content block at index {index} but this result only has {}",
                result.content.len()
            )));
        }
    };
    // get this block's text
    match content.as_text() {
        Some(text) => Ok(&text.text),
        None => Err(Error::new(format!(
            "Expected the content block at index {index} to be text but it was {content:?}"
        ))),
    }
}

/// Get the uri and text of an embedded resource in an mcp tool result
///
/// # Arguments
///
/// * `result` - The tool result to get a resource from
/// * `index` - The index of the content block to get a resource for
///
/// # Errors
///
/// Errors if there is no content block at `index`, that block isn't an embedded resource, or
/// that resource is a blob instead of text.
pub fn resource_text(result: &CallToolResult, index: usize) -> Result<(&str, &str), Error> {
    // get the content block at this index
    let content = match result.content.get(index) {
        Some(content) => content,
        None => {
            return Err(Error::new(format!(
                "Expected a content block at index {index} but this result only has {}",
                result.content.len()
            )));
        }
    };
    // make sure this block is an embedded resource
    let resource = match content.as_resource() {
        Some(resource) => resource,
        None => {
            return Err(Error::new(format!(
                "Expected the content block at index {index} to be a resource but it was {content:?}"
            )));
        }
    };
    // pull the uri and text out of this resource
    match &resource.resource {
        ResourceContents::TextResourceContents { uri, text, .. } => Ok((uri, text)),
        ResourceContents::BlobResourceContents { uri, .. } => Err(Error::new(format!(
            "Expected the resource at index {index} to be text but '{uri}' was a blob"
        ))),
    }
}

/// Get the structured content of an mcp tool result
///
/// # Arguments
///
/// * `result` - The tool result to get structured content from
///
/// # Errors
///
/// Errors if this result has no structured content.
pub fn structured(result: &CallToolResult) -> Result<&serde_json::Value, Error> {
    match &result.structured_content {
        Some(structured) => Ok(structured),
        None => Err(Error::new(
            "Expected this tool result to have structured content but it had none",
        )),
    }
}

//! An async chat worker for Thorctl

use crate::Error;
use kanal::{AsyncReceiver, AsyncSender};
use thorium::ai::{AiResponse, AiSupport, SharedThorChatContext, ThorChat};
use tokio::task::JoinHandle;

use crate::handlers::ai::chat::AppEvent;
use crate::handlers::ai::chat::components::SharedChatStatus;

/// A Thorium AI chat client for a detached ThorChat
pub struct ThorChatClient<A: AiSupport + 'static> {
    /// A channel to send messages to the AI
    pub to_chat: AsyncSender<String>,
    /// The shared context with our detached client
    pub context: SharedThorChatContext<A>,
    /// What our detached client is currently doing
    pub status: SharedChatStatus,
    /// The join handle for our detached chat bot
    join_handle: JoinHandle<Result<(), Error>>,
}

impl<A: AiSupport + 'static> ThorChatClient<A> {
    /// Create a new [`ThorChatClient`]
    ///
    /// # Arguments
    ///
    /// * `thor_chat` - The chat bot to detach
    /// * `to_app` - A channel to send app events over
    pub fn new(thor_chat: ThorChat<A>, to_app: &AsyncSender<AppEvent>) -> Self {
        // build a channel to receive mesages from the user over
        let (to_chat, from_user) = kanal::unbounded_async();
        // get a copy of our shared context
        let context = thor_chat.context.clone();
        // build the status our detached client will report its progress with
        let status = SharedChatStatus::default();
        // build our detached chat bot
        let detachable = DetachedThorChat {
            from_user,
            to_app: to_app.clone(),
            status: status.clone(),
            thor_chat,
        };
        // detach our chat bot and let it run in the background
        let join_handle = tokio::task::spawn(detachable.start());
        // build our chat client
        ThorChatClient {
            to_chat,
            context,
            status,
            join_handle,
        }
    }
}

/// A detached Thorium chat bot that runs in the background and communicates over channels
pub struct DetachedThorChat<A: AiSupport> {
    /// The channel to receive messages from the user over
    from_user: AsyncReceiver<String>,
    /// The channel to send response over
    to_app: AsyncSender<AppEvent>,
    /// What we are currently doing
    status: SharedChatStatus,
    /// The detached chat bot
    thor_chat: ThorChat<A>,
}

impl<A: AiSupport> DetachedThorChat<A> {
    /// Ask this agent a question
    ///
    /// This mirrors [`ThorChat::ask`] but reports what phase we are in as we go
    /// so the tui can show a spinner. [`ThorChat`] is shared with non-tui
    /// callers so we compose its public parts here instead of teaching it about
    /// our progress reporting. If the loop in [`ThorChat::ask`] changes then
    /// this copy needs to change with it.
    ///
    /// # Arguments
    ///
    /// * `question` - The question to ask our ai
    async fn ask<T: Into<String> + Send + Sync>(
        &mut self,
        question: T,
    ) -> Result<Option<String>, Error> {
        // ask our ai this question
        // if we need to run tools then run them and query the ai again otherwise return our answer
        match self.thor_chat.ai.ask(question).await? {
            // we need to run tools so do that
            AiResponse::CallTool(mut tool_calls) => {
                // loop and execute tools until we get a response
                loop {
                    // get the names of the tools we are about to call
                    let names = tool_calls
                        .iter()
                        .map(|(_, params)| params.name.to_string())
                        .collect();
                    // tell the tui we are waiting on these tools
                    self.status.calling_tools(names);
                    // rerender our app and ignore any errors since the app should rerender later
                    let _ = self.to_app.send(AppEvent::Redraw).await;
                    // call our tools in parallel
                    let tool_results = self.thor_chat.call_tools(tool_calls).await?;
                    // adding our tool results queries the ai again so we are back to thinking
                    self.status.thinking();
                    // rerender our app and ignore any errors since the app should rerender later
                    let _ = self.to_app.send(AppEvent::Redraw).await;
                    // tell our ai about these results
                    match self.thor_chat.ai.add_tool_results(tool_results).await? {
                        AiResponse::CallTool(new_calls) => tool_calls = new_calls,
                        AiResponse::Response(response) => {
                            return Ok(response);
                        }
                    }
                    // rerender our app and ignore any errors since the app should rerender later
                    let _ = self.to_app.send(AppEvent::Redraw).await;
                }
            }
            AiResponse::Response(response) => Ok(response),
        }
    }

    /// Answer questions from the user until our channel closes
    pub async fn start(mut self) -> Result<(), Error> {
        // handle messages forever
        loop {
            // handle messages from the user until told to stop
            let msg = self.from_user.recv().await?;
            // tell the tui we are waiting on the ai before we send anything
            self.status.thinking();
            // rerender our app so our spinner shows up right away
            let _ = self.to_app.send(AppEvent::Redraw).await;
            // ask our question
            let event = match self.ask(msg).await {
                // our detached client actually just accesses the shared chat context
                // but we still need to tell it to redraw the current screen
                Ok(_) => AppEvent::Redraw,
                // build a tool call failure event
                Err(error) => AppEvent::ToolCallFailure { error },
            };
            // we are done with this turn so stop our spinner
            self.status.idle();
            // send this response to the app
            self.to_app.send(event).await?;
        }
    }
}

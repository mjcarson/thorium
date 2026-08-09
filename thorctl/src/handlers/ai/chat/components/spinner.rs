//! An activity spinner for thorctl ai chat
//!
//! This module tracks what the detached chat worker is currently doing and
//! renders an animated indicator for it. The indicator is only shown while the
//! worker is busy so the user can tell the difference between the AI working
//! and the tui being wedged.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// The frames our spinner cycles through
///
/// Each frame is 5 slots wide and each slot is 2 columns, so every frame is
/// exactly 10 columns. Our crab carries an ear of corn to the brain and then
/// heads back empty handed to fetch another one. Empty slots are two spaces so
/// that they are the same width as an emoji and our label never shifts as the
/// crab moves.
const FRAMES: [&str; 6] = [
    "🦀🌽    🧠",
    "  🦀🌽  🧠",
    "    🦀🌽🧠",
    "    🦀  🧠",
    "  🦀    🧠",
    "🦀      🧠",
];

/// How long each spinner frame is displayed for in milliseconds
const FRAME_INTERVAL_MILLIS: u128 = 150;

/// What our detached chat worker is currently doing
#[derive(Debug, Clone)]
pub enum ChatActivity {
    /// The worker is waiting on the user for a new prompt
    Idle,
    /// The worker is waiting on a response from the LLM
    Thinking,
    /// The worker is waiting on these MCP tools to return
    CallingTools(Vec<String>),
}

impl ChatActivity {
    /// Whether our worker is currently doing something the user is waiting on
    pub fn is_busy(&self) -> bool {
        !matches!(self, ChatActivity::Idle)
    }

    /// Build the label describing this activity
    fn label(&self) -> String {
        match self {
            // we have nothing to say when we are idle
            ChatActivity::Idle => String::new(),
            // we are waiting on the LLM to respond
            ChatActivity::Thinking => "Thinking… ".to_owned(),
            // we are waiting on a single tool so name it directly
            ChatActivity::CallingTools(names) if names.len() == 1 => {
                format!("Calling tool `{}`… ", names[0])
            }
            // we are waiting on several tools so list them all
            ChatActivity::CallingTools(names) => {
                format!("Calling tools: {}… ", names.join(", "))
            }
        }
    }
}

/// The status of our chat worker at a single point in time
#[derive(Debug, Clone)]
pub struct ChatStatus {
    /// What our worker is currently doing
    pub activity: ChatActivity,
    /// When our worker started working on the current turn
    pub started: Instant,
}

/// Our chat workers status shared between that worker and the tui
pub struct SharedChatStatus(Arc<Mutex<ChatStatus>>);

impl Default for SharedChatStatus {
    /// Setup a default idle chat status
    fn default() -> Self {
        // build a status for a worker that hasn't done anything yet
        let status = ChatStatus {
            activity: ChatActivity::Idle,
            started: Instant::now(),
        };
        // wrap this status in an arc and a mutex
        SharedChatStatus(Arc::new(Mutex::new(status)))
    }
}

impl std::clone::Clone for SharedChatStatus {
    /// Clone a reference
    fn clone(&self) -> Self {
        // clone our inner value and rewrap
        SharedChatStatus(self.0.clone())
    }
}

impl SharedChatStatus {
    /// Set the current activity for our worker
    ///
    /// The start time is only reset when we transition out of being idle so
    /// that the elapsed time we display covers the entire turn instead of just
    /// the current phase of it.
    ///
    /// # Arguments
    ///
    /// * `activity` - The new activity for our worker
    fn set(&self, activity: ChatActivity) {
        // get a guard to our shared status
        let mut guard = self.0.lock().unwrap();
        // reset our timer if we are just starting a new turn
        if !guard.activity.is_busy() {
            guard.started = Instant::now();
        }
        // update our current activity
        guard.activity = activity;
    }

    /// Mark our worker as waiting on a response from the LLM
    pub fn thinking(&self) {
        self.set(ChatActivity::Thinking);
    }

    /// Mark our worker as waiting on some MCP tools to return
    ///
    /// # Arguments
    ///
    /// * `names` - The names of the tools we are waiting on
    pub fn calling_tools(&self, names: Vec<String>) {
        self.set(ChatActivity::CallingTools(names));
    }

    /// Mark our worker as done and waiting on the user for a new prompt
    pub fn idle(&self) {
        // get a guard to our shared status
        let mut guard = self.0.lock().unwrap();
        // our worker is no longer doing anything
        guard.activity = ChatActivity::Idle;
    }

    /// Get an owned copy of our current status
    ///
    /// This clones instead of handing out a guard so that our render path
    /// doesn't hold this lock while it draws.
    pub fn snapshot(&self) -> ChatStatus {
        // get a guard to our shared status
        let guard = self.0.lock().unwrap();
        // clone our current status
        guard.clone()
    }
}

/// Render our activity spinner
///
/// This renders nothing when our worker is idle.
///
/// # Arguments
///
/// * `frame` - The frame to render to
/// * `area` - The area to render the spinner in
/// * `status` - The status of the worker to render a spinner for
pub fn render(frame: &mut Frame, area: Rect, status: &ChatStatus) {
    // don't render anything if our worker isn't doing anything
    if !status.activity.is_busy() {
        return;
    }
    // get how long our worker has been working on this turn
    let elapsed = status.started.elapsed();
    // determine which frame of our animation to show
    let index = (elapsed.as_millis() / FRAME_INTERVAL_MILLIS) as usize % FRAMES.len();
    // build our spinner line
    // the animation itself is left unstyled since emoji bring their own colors
    let line = Line::from(vec![
        Span::raw(" "),
        Span::raw(FRAMES[index]),
        Span::raw(" "),
        Span::styled(status.activity.label(), Style::default().fg(Color::Gray)),
        Span::styled(
            format!("{:.1}s", elapsed.as_secs_f32()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    // render our spinner
    frame.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
    use super::FRAMES;
    use unicode_width::UnicodeWidthStr;

    /// Every frame must be the same width or our label will jitter as the crab moves
    #[test]
    fn frames_are_a_constant_width() {
        // our frames are 5 slots wide and each slot is 2 columns
        for frame in FRAMES {
            assert_eq!(frame.width(), 10, "frame {frame:?} is not 10 columns wide");
        }
    }
}

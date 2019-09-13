//! Traits allowing for backends to advertize support for various functionality in Thorium

mod notifications;
mod outputs;
mod tags;

pub use notifications::NotificationSupport;
pub use outputs::OutputSupport;
pub use tags::TagSupport;

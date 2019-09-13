//! The different search stores to stream data too

use serde_json::Value;
use thorium::{Conf, Error};

mod elastic;

pub use elastic::Elastic;

#[async_trait::async_trait]
pub trait SearchStore: Sync + Send + 'static + Sized {
    /// Create a new search store client
    ///
    /// # Arguments
    ///
    /// * `conf` - A Thorium config
    /// * `index` - The index to send docs too
    fn new(conf: &Conf, index: &str) -> Result<Self, Error>;

    /// Send some documents to our search store to be indexed
    ///
    /// # Arguments
    ///
    /// * `docs` - The docs to send
    async fn send(&self, docs: &mut Vec<Value>) -> Result<(), Error>;
}

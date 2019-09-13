//! Support streaming data into elastic

use elasticsearch::auth::Credentials;
use elasticsearch::cert::CertificateValidation;
use elasticsearch::http::request::JsonBody;
use elasticsearch::http::transport::{SingleNodeConnectionPool, TransportBuilder};
use elasticsearch::{BulkParts, Elasticsearch};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thorium::{Conf, Error};
use tracing::instrument;
use url::Url;

use super::SearchStore;

pub struct Elastic {
    /// The elastic client to use when streaming data
    elastic: Elasticsearch,
    /// The index to store data in
    index: String,
}

impl Elastic {
    /// Create a new Elastic streamer
    ///
    /// # Arguments
    ///
    /// * `conf` - A Thorium config
    /// * `index` - The index to send docs too
    pub fn new(conf: &Conf, index: &str) -> Result<Self, Error> {
        // Until https://github.com/elastic/elasticsearch-rs/pull/189 is merged
        // we can only support a single node connection pool
        // try to cast our node to a url
        let url = Url::parse(&conf.elastic.node)?;
        // build our connection pool
        let pool = SingleNodeConnectionPool::new(url);
        // get our username and password
        let username = conf.elastic.username.clone();
        let password = conf.elastic.password.clone();
        // build our transport object for elastic
        let transport = TransportBuilder::new(pool)
            .auth(Credentials::Basic(username, password))
            .cert_validation(CertificateValidation::None)
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        // build our elastic client
        let elastic = Elasticsearch::new(transport);
        // create our elastic struct
        Ok(Elastic {
            elastic,
            index: index.to_owned(),
        })
    }
}

#[async_trait::async_trait]
impl SearchStore for Elastic {
    /// Create a new search store client
    ///
    /// # Arguments
    ///
    /// * `conf` - A Thorium config
    /// * `index` - The index to send docs too
    fn new(conf: &Conf, index: &str) -> Result<Self, Error> {
        Elastic::new(conf, index)
    }

    /// Send some documents to our search store to be indexed
    ///
    /// # Arguments
    ///
    /// * `docs` - The docs to send
    #[instrument(name = "SearchStore<Elastic>::send", skip_all, fields(docs = docs.len()), err(Debug))]
    async fn send(&self, docs: &mut Vec<Value>) -> Result<(), Error> {
        // convert our values to json bodies
        let body = docs
            .drain(..)
            .map(JsonBody::from)
            .collect::<Vec<JsonBody<Value>>>();
        // send these documents
        let resp = self
            .elastic
            .bulk(BulkParts::Index(&self.index))
            .body(body)
            .send()
            .await?;
        // check if we ran into an error or not
        if resp.status_code().is_success() {
            // check if any errors occured
            let resp: ElasticResp = resp.json().await?;
            if resp.errors {
                Err(Error::new("Failed to stream documents to elastic"))
            } else {
                Ok(())
            }
        } else {
            // get the error message to return
            let msg = resp.text().await?;
            Err(Error::new(msg))
        }
    }
}

/// A response from elastic from submitting results
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ElasticResp {
    /// How long these results took to be ingested
    took: u64,
    /// Whether any errors were encountered
    errors: bool,
    /// Any errors
    items: Value,
}

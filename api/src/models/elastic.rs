//! Structures related to searching in Elastic Search

use chrono::prelude::*;
use uuid::Uuid;

/// The different elastic indexes
#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
pub enum ElasticIndexes {
    /// The results for jobs on samples
    SamplesResults,
}

impl std::fmt::Display for ElasticIndexes {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ElasticIndexes::SamplesResults => write!(f, "SamplesResults"),
        }
    }
}

impl Default for ElasticIndexes {
    /// Set our default elastic index to samples results
    fn default() -> Self {
        ElasticIndexes::SamplesResults
    }
}

impl ElasticIndexes {
    /// Get the full index name for a specific index
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    #[cfg(feature = "api")]
    pub fn full_name<'a>(&self, shared: &'a crate::utils::Shared) -> &'a str {
        match self {
            ElasticIndexes::SamplesResults => &shared.config.elastic.results,
        }
    }
}
/// Default the Result list limit to 50
fn default_search_limit() -> u32 {
    50
}

/// The query params for searching results
#[derive(Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
pub struct ElasticSearchParams {
    /// The indexes to search
    #[serde(default)]
    pub index: ElasticIndexes,
    /// The query to use when searching
    #[serde(default)]
    pub query: String,
    /// The groups to search data from
    #[serde(default)]
    pub groups: Vec<String>,
    /// When to start searching data at
    #[serde(default = "Utc::now")]
    pub start: DateTime<Utc>,
    /// When to stop searching data at
    pub end: Option<DateTime<Utc>>,
    /// The cursor id to use if one exists
    pub cursor: Option<Uuid>,
    /// The max number of items to return in this response
    #[serde(default = "default_search_limit")]
    pub limit: u32,
}

/// The query params for searching results
#[derive(Serialize, Deserialize, Debug)]
pub struct ElasticSearchOpts {
    /// The indexes to search
    pub index: ElasticIndexes,
    /// The query to use when searching
    pub query: String,
    /// The groups to search data from
    pub groups: Vec<String>,
    /// When to start searching data at
    pub start: Option<DateTime<Utc>>,
    /// When to stop searching data at
    pub end: Option<DateTime<Utc>>,
    /// The cursor id to use if one exists
    pub cursor: Option<Uuid>,
    /// The max number of objects to retrieve on a single page
    pub page_size: usize,
    /// The max number of items to return with this cursor
    pub limit: Option<usize>,
}

impl ElasticSearchOpts {
    /// Create a new search query
    ///
    /// # Arguments
    ///
    /// * `query` - The search query to use
    pub fn new<T: Into<String>>(query: T) -> Self {
        // build a base search options struct
        ElasticSearchOpts {
            index: ElasticIndexes::SamplesResults,
            query: query.into(),
            groups: Vec::default(),
            start: None,
            end: None,
            cursor: None,
            page_size: 50,
            limit: None,
        }
    }
}

// A specific document in elastic
#[derive(Deserialize, Serialize, Default, Debug)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
pub struct ElasticDoc {
    /// The id for this document
    #[serde(alias = "_id")]
    pub id: String,
    /// The index this doc cames from
    #[serde(alias = "_index")]
    pub index: String,
    /// The score for this doc
    #[serde(alias = "_score")]
    pub score: Option<f64>,
    /// The actual document in elastic
    #[serde(alias = "_source")]
    pub source: Option<serde_json::Value>,
    /// The actual document in elastic
    pub highlight: Option<serde_json::Value>,
    /// The sort values for this document
    pub sort: Vec<i64>,
}

//! Add support for streaming results to a search store

use std::collections::HashMap;

use chrono::prelude::*;
use serde::Serialize;
use serde_json::{json, Value};
use thorium::client::ResultsClient;
use thorium::models::{Cursor, OutputChunk};
use thorium::models::{OutputBundle, ResultListOpts};
use thorium::{Conf, Error, Thorium};

use super::DataSource;

pub struct SamplesOutput;

#[async_trait::async_trait]
impl DataSource for SamplesOutput {
    /// The data this cursor will be pulling
    type DataType = OutputBundle;

    /// Get the timestamp for this datatype
    ///
    /// # Arguments
    ///
    /// * `data` - The data to get the timestamp from
    fn timestamp(data: &Self::DataType) -> DateTime<Utc> {
        data.latest
    }

    /// Get the name of the index to write these documents too
    fn index(conf: &Conf) -> &str {
        &conf.elastic.results
    }

    /// Get the earliest data might exist at
    fn earliest(conf: &Conf) -> DateTime<Utc> {
        DateTime::from_timestamp(conf.thorium.files.earliest, 0).unwrap()
    }

    /// Pull data from a section of time to stream to our search store
    async fn build_cursor(
        thorium: &Thorium,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Cursor<Self::DataType>, Error> {
        // build our cursor options
        let opts = ResultListOpts::default().start(start).end(end);
        // get our new cursor
        thorium.files.list_results_bundle(opts).await
    }

    /// Cast this data to a serialized Json Value
    fn to_value(
        data: &Self::DataType,
        values: &mut Vec<Value>,
        now: DateTime<Utc>,
    ) -> Result<(), Error> {
        // serialize all of our results
        let mut result_map = HashMap::with_capacity(data.results.len());
        // crawl the results in this output bundle
        for (id, chunk) in &data.results {
            // serialize this result
            let serialized = serde_json::to_string(&chunk)?;
            // add this serialized result
            result_map.insert(*id, (serialized, chunk));
        }
        // track our search safe serialized results
        let mut safe_map = HashMap::with_capacity(result_map.len());
        // crawl over our serialized results and break them up
        for (id, (serialized, chunk)) in &result_map {
            // track our current position in our serialized string
            let mut current = 0;
            // track the sub strings we find
            let mut chunks = Vec::with_capacity(std::cmp::max(serialized.len() / 32_766, 1));
            // break this string up into search safe chunks
            while current != serialized.len() {
                // get a ref to our current window of the results string
                let window = &serialized[current..];
                // detect the boundary for this safe chunk
                let boundary = window.floor_char_boundary(32_766);
                // track this substring
                chunks.push(&window[..boundary]);
                // update our current position
                current += boundary;
            }
            // serialize the children
            let children = chunk
                .children
                .iter()
                .map(|(sha, sub)| format!("{sha} {sub}"))
                .collect::<Vec<String>>();
            // add this safe result
            safe_map.insert(*id, (chunks, children, chunk));
        }
        // track the result entries for each group
        let mut entries = HashMap::with_capacity(safe_map.len());
        // crawl over the tool results in each group
        for (group, tools) in &data.map {
            // Crawl over the tools for this group and add their values
            for (name, id) in tools {
                // get this tools serialized results
                if let Some((result, children, chunk)) = safe_map.get(id) {
                    // build this output entry
                    let entry = SamplesOutputEntry::new(result, children, chunk);
                    // add this entry
                    entries.insert(name.clone(), entry);
                }
            }
            // set the id for this document
            values.push(json!({"index": {"_id": format!("{}-{}", data.sha256, group)}}));
            // build the final document to add and add it to our vec of docs
            values.push(json!({
                "sha256": &data.sha256,
                "streamed": &now,
                "group": group,
                "results": entries,
            }));
            // clear our entries map
            entries.clear();
        }
        Ok(())
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct SamplesOutputEntry<'a> {
    /// The result
    pub result: &'a Vec<&'a str>,
    /// Any files tied to this result
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: &'a Vec<String>,
    /// The children found by this tool
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: &'a Vec<String>,
}

impl<'a> SamplesOutputEntry<'a> {
    /// Build a `SamplesOutputEntry`
    ///
    /// # Arguments
    ///
    /// * `result` - This tools serialized and search safe results
    /// * `children` - The children for this entry
    /// * `bundle` - The bundle to build from
    fn new(result: &'a Vec<&'a str>, children: &'a Vec<String>, chunk: &'a OutputChunk) -> Self {
        SamplesOutputEntry {
            result,
            files: &chunk.files,
            children,
        }
    }
}

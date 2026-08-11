//! A blob of arbitrary json that can be scanned by sigma rules

use std::hash::{Hash, Hasher};

/// A single line/document of arbitrary json
///
/// The data is always a top level json object so that it can be handed
/// directly to sigma-rust as an event. This lets sigma rules address the
/// documents own keys (`process.name`) instead of a wrapper field.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "api", derive(utoipa::ToSchema))]
pub struct JsonEntity {
    /// The parsed json document for this entity
    #[cfg_attr(feature = "api", schema(value_type = Object))]
    pub data: serde_json::Value,
}

/// Recursively hash a [`serde_json::Value`]
///
/// [`serde_json::Value`] does not implement [`Hash`] so we have to walk it
/// ourselves. Each value kind writes a distinct discriminant first so values of
/// different kinds can never produce the same hash stream.
///
/// # Arguments
///
/// * `value` - The json value to hash
/// * `state` - The hasher to write our hash into
fn hash_value<H: Hasher>(value: &serde_json::Value, state: &mut H) {
    // hash this value based on what kind of json value it is
    match value {
        serde_json::Value::Null => state.write_u8(0),
        serde_json::Value::Bool(flag) => {
            // tag this as a bool and then hash it
            state.write_u8(1);
            flag.hash(state);
        }
        serde_json::Value::Number(number) => {
            // tag this as a number
            state.write_u8(2);
            // serde_json::Number has no Hash impl so hash its canonical string form
            number.to_string().hash(state);
        }
        serde_json::Value::String(text) => {
            // tag this as a string and then hash it
            state.write_u8(3);
            text.hash(state);
        }
        serde_json::Value::Array(items) => {
            // tag this as an array and mix in its length so [[1], [2]] != [[1, 2]]
            state.write_u8(4);
            state.write_usize(items.len());
            // hash each of our items in order
            for item in items {
                hash_value(item, state);
            }
        }
        serde_json::Value::Object(map) => {
            // tag this as an object and mix in its length
            state.write_u8(5);
            state.write_usize(map.len());
            // serde_json's `preserve_order` feature is off so Map is a BTreeMap and this
            // iterates in sorted key order, making our hash deterministic across processes
            for (key, item) in map {
                key.hash(state);
                hash_value(item, state);
            }
        }
    }
}

impl Hash for JsonEntity {
    /// Hash this json entities document
    ///
    /// # Arguments
    ///
    /// * `state` - The hasher to write our hash into
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_value(&self.data, state);
    }
}

impl JsonEntity {
    /// Create a new json entity from an already parsed document
    ///
    /// # Arguments
    ///
    /// * `data` - The json document for this entity
    ///
    /// # Examples
    ///
    /// ```
    /// use thorium::models::JsonEntity;
    ///
    /// JsonEntity::new(serde_json::json!({"process": {"name": "powershell.exe"}}));
    /// ```
    #[must_use]
    pub fn new(data: serde_json::Value) -> Self {
        JsonEntity { data }
    }

    /// Add this json entities metadata to a form
    ///
    /// # Arguments
    ///
    /// * `form` - The form to add too
    #[cfg(feature = "client")]
    pub fn add_to_form(
        self,
        form: reqwest::multipart::Form,
    ) -> Result<reqwest::multipart::Form, crate::Error> {
        // serialize our document back to its compact string form
        let raw = serde_json::to_string(&self.data)?;
        // always set our entity kind
        Ok(form
            .text("kind", super::EntityKinds::Json.as_str())
            .text("metadata[json_data]", raw))
    }

    /// Create a new json entity with the info in the form
    ///
    /// # Arguments
    ///
    /// * `form` - The entity metadata form
    /// * `shared` - Shared Thorium objects
    #[cfg(feature = "api")]
    pub fn from_form(
        form: super::EntityMetadataForm,
        shared: &crate::utils::Shared,
    ) -> Result<Self, crate::utils::ApiError> {
        // if we don't have any json data set then return an error
        let raw = match form.json_data {
            Some(raw) => raw,
            None => return crate::bad!("Json entities must have json data!".to_owned()),
        };
        Self::parse_and_validate(&raw, shared)
    }

    /// Parse a raw json document and make sure it can be scanned by sigma
    ///
    /// This is shared by the create and update paths so both enforce the same limits.
    ///
    /// # Errors
    ///
    /// Returns an error if the document is larger than our configured limit, is not
    /// valid json, or is not a top level json object.
    ///
    /// # Arguments
    ///
    /// * `raw` - The raw json document to parse
    /// * `shared` - Shared Thorium objects
    #[cfg(feature = "api")]
    pub fn parse_and_validate(
        raw: &str,
        shared: &crate::utils::Shared,
    ) -> Result<Self, crate::utils::ApiError> {
        // get the max json document size from our config
        let max = shared.config.thorium.entities.max_json_size;
        // reject documents that are too large before we build a parse tree for them
        if raw.len() as u64 > max.as_u64() {
            return crate::bad!(format!(
                "Json entity data is too large: {} > {max}",
                bytesize::ByteSize::b(raw.len() as u64),
            ));
        }
        // parse our raw json document
        let data: serde_json::Value = match serde_json::from_str(raw) {
            Ok(data) => data,
            Err(error) => {
                return crate::bad!(format!("Failed to parse json entity data: {error}"));
            }
        };
        // sigma can only scan top level json objects
        if !data.is_object() {
            return crate::bad!("Json entity data must be a top level json object!".to_owned());
        }
        Ok(JsonEntity { data })
    }
}

//! The errors that can be returned by some scylla traits

#[derive(Debug)]
pub enum DeserializationError {
    /// We expected text and got a different row data kind
    ExpectedText,
    /// We expected this column to contain data
    ExpectedNotNull,
    /// The column contained an unknown value
    UnknownValue,
}

impl std::fmt::Display for DeserializationError {
    /// Allow our deserialization errors to be displayed
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

// impl the std error trait for our error
impl std::error::Error for DeserializationError {}

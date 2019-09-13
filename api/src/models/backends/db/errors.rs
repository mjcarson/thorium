use crate::bad_internal;
use crate::utils::ApiError;

impl From<bb8_redis::redis::RedisError> for ApiError {
    /// Cast a bb8_redis error into an ApiError
    ///
    /// # Arguments
    ///
    /// * `error` - The bb8_redis error to convert to an ApiError
    fn from(error: bb8_redis::redis::RedisError) -> Self {
        bad_internal!(format!("Redis backend failure: {:#?}", error))
    }
}

impl From<scylla::transport::errors::QueryError> for ApiError {
    /// Cast a scylla error into an ApiError
    ///
    /// # Arguments
    ///
    /// * `error` - The bb8_redis error to convert to an ApiError
    fn from(error: scylla::transport::errors::QueryError) -> Self {
        bad_internal!(format!("Scylla query error: {:#?}", error))
    }
}

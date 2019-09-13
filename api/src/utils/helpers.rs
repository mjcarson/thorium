//! Helper functions for the Thorium API and friends
use chrono::prelude::*;
use futures::Stream;

/// Get the number of hours so far in this year
///
/// # Arguments
///
/// * `timestamp` - The timestamp to determine our partition for
/// * `year` - The year to determine our partition for
/// * `chunk` - The size of our partition
///
/// # Panics
///
/// This should never actually panic as its just unwraps on known good values.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_lossless)]
pub fn partition(timestamp: DateTime<Utc>, year: i32, chunk: u16) -> i32 {
    let duration = timestamp.naive_utc()
        - NaiveDate::from_ymd_opt(year, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 1)
            .unwrap();
    // get the correct chunk
    duration.num_seconds() as i32 / chunk as i32
}

/// Get the number of hours so far in this year
///
/// # Arguments
///
/// * `timestamp` - The timestamp to determine our partition for
/// * `year` - The year to determine our partition for
/// * `chunk` - The size of our partition
///
/// # Panics
///
/// This should never actually panic as its just unwraps on known good values.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_lossless)]
pub fn partition_i64(timestamp: i64, year: i32, chunk: u16) -> i32 {
    // build a naive timestamp
    let datetime = DateTime::from_timestamp_millis(timestamp).unwrap();
    // get a duration
    let duration = datetime.naive_utc()
        - NaiveDate::from_ymd_opt(year, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 1)
            .unwrap();
    // get the correct chunk
    duration.num_seconds() as i32 / chunk as i32
}

/// Resolves `FnOnce` errors by asserting an iterator is Send
///
/// See <https://users.rust-lang.org/t/implementation-of-fnonce-is-not-general-enough-with-async-block/83427/3>
///
/// # Arguments
///
/// * `it` - The iterator to assert
pub fn assert_send_stream<R>(it: impl Send + Stream<Item = R>) -> impl Send + Stream<Item = R> {
    it
}

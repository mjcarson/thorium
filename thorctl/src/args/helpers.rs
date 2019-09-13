//! Helper functions for Thorctl arguments

use std::str::FromStr;

/// Provide a possible number range for an argument, producing an error if a user
/// provides a value not within the range
///
/// Inspired by <https://docs.rs/clap-num/latest/clap_num/fn.number_range.html>
///
/// # Arguments
///
/// * `s` - The raw argument str to parse
/// * `min` - The minimum the argument value can be
/// * `max` - The maximum the argument value can be
pub fn number_range<T>(s: &str, min: T, max: T) -> Result<T, String>
where
    T: FromStr + Copy + Ord + PartialOrd + std::fmt::Display,
    <T as FromStr>::Err: std::fmt::Display,
{
    debug_assert!(min <= max, "minimum of {min} exceeds maximum of {max}");
    let val = s.parse::<T>().map_err(|err| err.to_string())?;
    if val > max {
        Err(format!("exceeds maximum of {max}"))
    } else if val < min {
        Err(format!("less than minimum of {min}"))
    } else {
        Ok(val)
    }
}

//! Helpers for the backends level of the Thorium API

use uuid::Uuid;

use crate::bad;
use crate::utils::ApiError;

/// The max length of most names in K8's
const K8_NAME_MAX_CHARS: usize = 63;

/// Convert a UTF-8 encoded Rust str with possible special characters to a valid
/// name in K8's (<253 characters, lowercase alphanumeric or "-", starts and ends
/// with alphanumeric); see <https://kubernetes.io/docs/concepts/overview/working-with-objects/names/>
///
/// Appends the given UUID to ensure the output name is unique even if another
/// UTF-8 encoding str has the same pattern of valid/invalid characters
/// (i.e. `my-name😄` and `my-name😎` both yield `my-name-`, so a UUID is required
/// to distinguish them)
///
/// # Errors
///
/// Returns an Error if the str value is empty
///
/// # Arguments
///
/// * `value` - The str value to convert to a K8's name
/// * `id` - The UUID to append to the K8's name
pub fn to_k8s_name<T: AsRef<str>>(value: T, id: Uuid) -> Result<String, ApiError> {
    // cast the value to a str
    let value = value.as_ref();
    // check that the value isn't empty
    if value.is_empty() {
        return bad!("Name cannot be empty!".to_string());
    }
    let id = id.to_string();
    // truncate to the maximum number of K8's name characters
    // with space left over for the UUID and a hyphen separator
    let truncated = value
        .chars()
        .take(K8_NAME_MAX_CHARS - id.len() - 1)
        .collect::<String>();
    // convert to lowercase
    let lower = truncated.to_lowercase();
    // replace non-ascii and non-alphanumeric/non-"-" to "-"
    let replaced = lower.replace(
        |c: char| !c.is_ascii() || (!c.is_alphanumeric() && c != '-'),
        "-",
    );
    let mut chars = replaced.chars();
    let k8s_name = if let Some(first_char) = chars.next() {
        if first_char == '-' {
            // replace the first "-" with a "z"; k8's names must start with alphanumeric
            format!("z{}-{}", chars.collect::<String>(), id)
        } else {
            format!("{replaced}-{id}")
        }
    } else {
        format!("{replaced}-{id}")
    };
    Ok(k8s_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_valid_k8s_name(name: &str) -> bool {
        return !name.is_empty()
            && name.is_ascii()
            && name.len() <= K8_NAME_MAX_CHARS
            && name.to_ascii_lowercase() == name
            && name.chars().next().unwrap().is_alphanumeric();
    }

    #[test]
    fn test_k8s_name_already_valid() {
        let name = "already-valid-name123";
        let id = Uuid::new_v4();
        let k8s_name = to_k8s_name(name, id).unwrap();
        assert!(k8s_name.starts_with("already-valid-name123"));
        assert!(is_valid_k8s_name(&k8s_name));
        assert!(k8s_name.ends_with(&format!("-{id}")));
    }

    #[test]
    fn test_k8s_name_too_long() {
        let long_name = "a".repeat(300);
        let k8s_name = to_k8s_name(long_name, Uuid::new_v4()).unwrap();
        assert_eq!(k8s_name.len(), K8_NAME_MAX_CHARS);
        assert!(is_valid_k8s_name(&k8s_name));
    }

    #[test]
    fn test_k8s_name_invalid_chars() {
        let name = "invalid!@#name";
        let k8s_name = to_k8s_name(name, Uuid::new_v4()).unwrap();
        assert!(k8s_name.starts_with("invalid---name"));
        assert!(is_valid_k8s_name(&k8s_name));
    }

    #[test]
    fn test_k8s_name_start_end_chars() {
        let name = "-start-and-end-";
        let k8s_name = to_k8s_name(name, Uuid::new_v4()).unwrap();
        assert!(k8s_name.starts_with("zstart-and-end"));
        assert!(is_valid_k8s_name(&k8s_name));
    }

    #[test]
    fn test_k8s_name_empty() {
        let name = "";
        assert!(to_k8s_name(name, Uuid::new_v4()).is_err());
    }

    #[test]
    fn test_k8s_name_utf8() {
        let name = "naмe-with-нон-ascii";
        let k8s_name = to_k8s_name(name, Uuid::new_v4()).unwrap();
        assert!(k8s_name.starts_with("na-e-with-----ascii"));
        assert!(is_valid_k8s_name(&k8s_name));
    }

    #[test]
    fn test_k8s_name_cap() {
        let name = "name-wItH-CAPS";
        let k8s_name = to_k8s_name(name, Uuid::new_v4()).unwrap();
        assert!(k8s_name.starts_with("name-with-caps"));
        assert!(is_valid_k8s_name(&k8s_name));
    }
}

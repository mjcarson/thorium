//! Build the keys to cursors in redis

use uuid::Uuid;

use crate::models::backends::db::cursors::CursorKind;
use crate::utils::Shared;

/// Build the key to this cursors data
pub fn data(kind: CursorKind, id: &Uuid, shared: &Shared) -> String {
    format!(
        "{ns}:cursors:{kind}:{id}",
        kind = kind.as_str(),
        ns = &shared.config.thorium.namespace
    )
}

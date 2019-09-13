//! Setup the exports tables/prepared statements in Scylla

use scylla::prepared_statement::PreparedStatement;
use scylla::Session;

use crate::Conf;

/// The prepared statements for exports
pub struct ExportsPreparedStatements {
    /// Insert an export
    pub insert: PreparedStatement,
    /// Get an export
    pub get: PreparedStatement,
    /// Check if an export exists
    pub exists: PreparedStatement,
    /// Update an export
    pub update: PreparedStatement,
    /// Insert an export error
    pub insert_error: PreparedStatement,
    /// Delete an export error
    pub delete_error: PreparedStatement,
    /// List the ties when listing export errors
    pub list_ties_error: PreparedStatement,
    /// Get the export errors for a cursor
    pub list_pull_error: PreparedStatement,
    // Get an export error by id
    pub get_error_by_id: PreparedStatement,
}

impl ExportsPreparedStatements {
    /// Build a new commitishes prepared statement struct
    ///
    /// # Arguments
    ///
    /// * `sessions` - The scylla session to use
    /// * `config` - The Thorium config
    pub async fn new(session: &Session, config: &Conf) -> Self {
        // setup the exports table
        setup_exports(session, config).await;
        setup_export_errors(session, config).await;
        // setup our materialized views
        setup_export_error_ids_mat_view(session, config).await;
        // setup our prepared statements
        let insert = insert(session, config).await;
        let get = get(session, config).await;
        let exists = exists(session, config).await;
        let update = update(session, config).await;
        let insert_error = insert_error(session, config).await;
        let delete_error = delete_error(session, config).await;
        let list_ties_error = list_ties_error(session, config).await;
        let list_pull_error = list_pull_error(session, config).await;
        let get_error_by_id = get_error_by_id(session, config).await;
        // build our prepared statement object
        ExportsPreparedStatements {
            insert,
            get,
            exists,
            update,
            insert_error,
            delete_error,
            list_ties_error,
            list_pull_error,
            get_error_by_id,
        }
    }
}

/// Setup the export status table
///
/// This table tracks the status and owner of an export.
///
/// # Arguments
///
/// * `sessions` - The scylla session to use
/// * `config` - The Thorium config
async fn setup_exports(session: &Session, config: &Conf) {
    // build cmd for exports table
    let table_create = format!(
        "CREATE TABLE IF NOT EXISTS {ns}.exports (\
            type TEXT,
            name TEXT,
            user TEXT,
            start TIMESTAMP,
            current TIMESTAMP,
            end TIMESTAMP,
            PRIMARY KEY ((type, name)))",
        ns = &config.thorium.namespace,
    );
    session
        .query_unpaged(table_create, &[])
        .await
        .expect("failed to add the exports table");
}

/// Setup the export errors table
///
/// This table tracks any failed exports
///
/// # Arguments
///
/// * `sessions` - The scylla session to use
/// * `config` - The Thorium config
async fn setup_export_errors(session: &Session, config: &Conf) {
    // build cmd for exports table
    let table_create = format!(
        "CREATE TABLE IF NOT EXISTS {ns}.export_errors (\
            export_name TEXT,
            type TEXT,
            year INT,
            bucket INT,
            start TIMESTAMP,
            id UUID,
            end TIMESTAMP,
            code INT,
            msg TEXT,
            PRIMARY KEY ((export_name, type, year, bucket), start, id))
            WITH default_time_to_live = 2628000",
        ns = &config.thorium.namespace,
    );
    session
        .query_unpaged(table_create, &[])
        .await
        .expect("failed to add the export errors table");
}

/// Setup the repo materialized view for Thorium
///
/// # Arguments
///
/// * `sessions` - The scylla session to use
/// * `config` - The Thorium config
async fn setup_export_error_ids_mat_view(session: &Session, config: &Conf) {
    // build cmd for table insert
    let table_create = format!(
            "CREATE MATERIALIZED VIEW IF NOT EXISTS {ns}.export_error_ids AS \
            SELECT export_name, type, year, bucket, start, end, code, msg, id FROM {ns}.export_errors \
            WHERE id IS NOT NULL \
            AND export_name IS NOT NULL \
            AND type IS NOT NULL \
            AND year IS NOT NULL \
            AND bucket IS NOT NULL \
            AND start IS NOT NULL \
            PRIMARY KEY (id, export_name, type, year, bucket, start)",
            ns = &config.thorium.namespace,
        );
    session
        .query_unpaged(table_create, &[])
        .await
        .expect("failed to add export error ids materialized view");
}

/// build the exports insert prepared statement
///
/// # Arguments
///
/// * `sessions` - The scylla session to use
/// * `conf` - The Thorium config
async fn insert(session: &Session, config: &Conf) -> PreparedStatement {
    // build exports insert prepared statement
    session
        .prepare(format!(
            "INSERT INTO {}.exports \
                (type, name, user, start, current, end) \
                VALUES (?, ?, ?, ?, ?, ?)",
            &config.thorium.namespace
        ))
        .await
        .expect("Failed to prepare scylla exports insert statement")
}

/// build the exports get prepared statement
///
/// # Arguments
///
/// * `sessions` - The scylla session to use
/// * `conf` - The Thorium config
async fn get(session: &Session, config: &Conf) -> PreparedStatement {
    // build exports get prepared statement
    session
        .prepare(format!(
            "select name, user, start, current, end \
                 FROM {}.exports \
                 WHERE type = ? AND name = ?",
            &config.thorium.namespace
        ))
        .await
        .expect("Failed to prepare scylla exports get statement")
}

/// build the exports exists prepared statement
///
/// # Arguments
///
/// * `sessions` - The scylla session to use
/// * `conf` - The Thorium config
async fn exists(session: &Session, config: &Conf) -> PreparedStatement {
    // build exports exists prepared statement
    session
        .prepare(format!(
            "SELECT name FROM {}.exports \
                WHERE type = ? \
                AND name = ? \
                LIMIT 1",
            &config.thorium.namespace
        ))
        .await
        .expect("Failed to prepare scylla exports exists statement")
}

/// build the export update prepared statement
///
/// # Arguments
///
/// * `sessions` - The scylla session to use
/// * `conf` - The Thorium config
async fn update(session: &Session, config: &Conf) -> PreparedStatement {
    // build export update prepared statement
    session
        .prepare(format!(
            "UPDATE {}.exports \
                SET current = ? \
                WHERE type = ? \
                AND name = ?",
            &config.thorium.namespace
        ))
        .await
        .expect("Failed to prepare scylla exports update statement")
}

/// build the export errors insert prepared statement
///
/// # Arguments
///
/// * `sessions` - The scylla session to use
/// * `conf` - The Thorium config
async fn insert_error(session: &Session, config: &Conf) -> PreparedStatement {
    // build export errors insert prepared statement
    session
        .prepare(format!(
            "INSERT INTO {}.export_errors \
                (export_name, type, year, bucket, start, end, id, code, msg) \
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            &config.thorium.namespace
        ))
        .await
        .expect("Failed to prepare scylla export errors insert statement")
}

/// build the export errors delete prepared statement
///
/// # Arguments
///
/// * `sessions` - The scylla session to use
/// * `conf` - The Thorium config
async fn delete_error(session: &Session, config: &Conf) -> PreparedStatement {
    // build export errors delete prepared statement
    session
        .prepare(format!(
            "DELETE FROM {}.export_errors \
                WHERE export_name = ? \
                AND type = ? \
                AND year = ? \
                AND bucket = ? \
                AND start = ? \
                AND id = ?",
            &config.thorium.namespace
        ))
        .await
        .expect("Failed to prepare scylla export errors delete statement")
}

/// Gets any remaining rows from past ties in listing export errors
///
/// # Arguments
///
/// * `sessions` - The scylla session to use
/// * `conf` - The Thorium config
async fn list_ties_error(session: &Session, config: &Conf) -> PreparedStatement {
    // build export errors list ties prepared statement
    session
        .prepare(format!(
            "SELECT id, start, end, code, msg \
                FROM {}.export_errors \
                WHERE year = ? \
                AND bucket = ? \
                AND export_name = ? \
                AND type = ?
                AND start = ? \
                AND id <= ? \
                LIMIT ?",
            &config.thorium.namespace
        ))
        .await
        .expect("Failed to prepare scylla export errors list ties statement")
}

/// Pulls the data for listing export errors in Thorium
///
/// # Arguments
///
/// * `sessions` - The scylla session to use
/// * `conf` - The Thorium config
async fn list_pull_error(session: &Session, config: &Conf) -> PreparedStatement {
    // build export errors list pull prepared statement
    session
        .prepare(format!(
            "SELECT id, start, end, code, msg \
                FROM {}.export_errors \
                WHERE year = ? \
                AND bucket in ? \
                AND export_name = ? \
                AND type = ?
                AND start < ? \
                AND start > ? \
                PER PARTITION LIMIT ?",
            &config.thorium.namespace
        ))
        .await
        .expect("Failed to prepare scylla export errors list pull statement")
}

/// Pulls the data for export errors by id in Thorium
///
/// # Arguments
///
/// * `sessions` - The scylla session to use
/// * `conf` - The Thorium config
async fn get_error_by_id(session: &Session, config: &Conf) -> PreparedStatement {
    // build the export errors by id get prepared statement
    session
        .prepare(format!(
            "SELECT id, start, end, code, msg \
                FROM {}.export_error_ids \
                WHERE id = ? AND export_name = ? AND type = ?",
            &config.thorium.namespace
        ))
        .await
        .expect("Failed to prepare scylla export errors ids statement")
}

//! SQLite connection pool and schema migrations.

use actix_web::web;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

use crate::error::AppError;

/// A pooled SQLite connection pool.
pub type Pool = r2d2::Pool<SqliteConnectionManager>;
/// A connection checked out from the [`Pool`].
pub type Conn = r2d2::PooledConnection<SqliteConnectionManager>;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    username        TEXT    NOT NULL UNIQUE,
    email           TEXT    UNIQUE,
    password_hash   TEXT    NOT NULL,
    display_name    TEXT,
    is_active       INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT    NOT NULL,
    updated_at      TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS pastes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id         INTEGER REFERENCES users(id) ON DELETE SET NULL,
    short_id        TEXT    NOT NULL UNIQUE,
    title           TEXT,
    content         TEXT    NOT NULL,
    language        TEXT,
    visibility      TEXT    NOT NULL DEFAULT 'public',
    expires_at      TEXT,
    is_deleted      INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT    NOT NULL,
    updated_at      TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pastes_user_id    ON pastes(user_id);
CREATE INDEX IF NOT EXISTS idx_pastes_created_at ON pastes(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_pastes_expires_at ON pastes(expires_at);
"#;

/// Build a connection pool for the SQLite database at `path`.
///
/// Each new connection enables WAL mode (better read concurrency), a busy
/// timeout (avoid spurious "database is locked" errors), and foreign-key
/// enforcement (so `ON DELETE SET NULL` works).
pub fn init_pool(path: &str) -> Result<Pool, AppError> {
    // Enable WAL once, up front, on a single connection. WAL is a persistent,
    // database-level setting stored in the file header, so pooled connections
    // inherit it. Doing it here avoids many connections racing to switch the
    // journal mode at pool warm-up (which logs "database is locked").
    {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA busy_timeout = 5000; PRAGMA journal_mode = WAL;")?;
    }

    // Per-connection PRAGMAs only: these don't take a write lock, so they're
    // safe to run concurrently as the pool fills.
    let manager = SqliteConnectionManager::file(path).with_init(|conn| {
        conn.execute_batch("PRAGMA busy_timeout = 5000; PRAGMA foreign_keys = ON;")
    });
    r2d2::Pool::builder()
        .build(manager)
        .map_err(|e| AppError::Internal(format!("failed to build connection pool: {e}")))
}

/// Create tables and indexes if they do not already exist. Idempotent.
pub fn run_migrations(conn: &Conn) -> Result<(), AppError> {
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

/// Run a blocking database closure on the Actix blocking thread pool with a
/// pooled connection, mapping the pool/join errors to [`AppError`].
///
/// This centralizes the `pool.clone()` + `web::block` + connection-checkout +
/// error-mapping boilerplate that every database-touching handler needs.
pub async fn db_task<F, T>(pool: &Pool, f: F) -> Result<T, AppError>
where
    F: FnOnce(&Conn) -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    let pool = pool.clone();
    web::block(move || {
        let conn = pool.get()?;
        f(&conn)
    })
    .await
    .map_err(|e| AppError::Internal(format!("blocking task failed: {e}")))?
}

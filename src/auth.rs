//! Password hashing, session helpers, and auth HTTP handlers.

use actix_session::Session;
use actix_web::{HttpResponse, web};
use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use serde_json::json;

use crate::db::{Conn, Pool};
use crate::error::AppError;
use crate::models::{Login, Register, User, row_to_user};
use crate::validation;

const SESSION_KEY: &str = "uid";

/// Hash a plaintext password with Argon2 (random per-password salt).
pub fn hash_password(plaintext: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(plaintext.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("password hashing failed: {e}")))
}

/// Verify a plaintext password against a stored Argon2 hash.
pub fn verify_password(plaintext: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

fn fetch_user_by_id(conn: &Conn, id: i64) -> Result<Option<User>, AppError> {
    conn.query_row("SELECT * FROM users WHERE id = ?1", params![id], |r| {
        row_to_user(r)
    })
    .optional()
    .map_err(AppError::from)
}

/// Look up a user by username. Public so the HTML UI can reuse it.
pub fn user_by_username(conn: &Conn, username: &str) -> Result<Option<User>, AppError> {
    conn.query_row(
        "SELECT * FROM users WHERE username = ?1",
        params![username],
        row_to_user,
    )
    .optional()
    .map_err(AppError::from)
}

/// Insert a new user and return its id. Maps UNIQUE violations to `Conflict`.
/// Public so the HTML UI can reuse it.
pub fn insert_user(
    conn: &Conn,
    username: &str,
    email: Option<&str>,
    password_hash: &str,
    display_name: Option<&str>,
) -> Result<i64, AppError> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO users (username, email, password_hash, display_name, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![username, email, password_hash, display_name, now],
    )
    .map_err(|e| map_insert_error(e, "username or email already taken"))?;
    Ok(conn.last_insert_rowid())
}

/// Load the currently logged-in user, if any, from the session cookie.
pub async fn optional_user(session: &Session, pool: &Pool) -> Result<Option<User>, AppError> {
    let uid: Option<i64> = session
        .get(SESSION_KEY)
        .map_err(|e| AppError::Internal(format!("session read failed: {e}")))?;
    let Some(id) = uid else {
        return Ok(None);
    };
    let pool = pool.clone();
    web::block(move || -> Result<Option<User>, AppError> {
        let conn = pool.get()?;
        fetch_user_by_id(&conn, id)
    })
    .await
    .map_err(|e| AppError::Internal(format!("blocking task failed: {e}")))?
}

/// Like [`optional_user`] but errors with `Unauthorized` when not logged in.
pub async fn require_user(session: &Session, pool: &Pool) -> Result<User, AppError> {
    optional_user(session, pool)
        .await?
        .ok_or(AppError::Unauthorized)
}

/// Translate a UNIQUE-constraint violation into a friendly `Conflict`.
fn map_insert_error(err: rusqlite::Error, conflict_msg: &str) -> AppError {
    if let rusqlite::Error::SqliteFailure(e, _) = &err
        && e.code == rusqlite::ErrorCode::ConstraintViolation
    {
        return AppError::Conflict(conflict_msg.to_string());
    }
    AppError::from(err)
}

/// `POST /api/auth/register`
pub async fn register(
    pool: web::Data<Pool>,
    session: Session,
    body: web::Json<Register>,
) -> Result<HttpResponse, AppError> {
    let r = body.into_inner();
    validation::username(&r.username)?;
    validation::password(&r.password)?;
    if let Some(email) = &r.email {
        validation::email(email)?;
    }
    let password_hash = hash_password(&r.password)?;

    let pool = pool.get_ref().clone();
    let user = web::block(move || -> Result<User, AppError> {
        let conn = pool.get()?;
        let id = insert_user(
            &conn,
            &r.username,
            r.email.as_deref(),
            &password_hash,
            r.display_name.as_deref(),
        )?;
        fetch_user_by_id(&conn, id)?.ok_or_else(|| AppError::Internal("user disappeared".into()))
    })
    .await
    .map_err(|e| AppError::Internal(format!("blocking task failed: {e}")))??;

    session
        .insert(SESSION_KEY, user.id)
        .map_err(|e| AppError::Internal(format!("session write failed: {e}")))?;
    Ok(HttpResponse::Created().json(&user))
}

/// `POST /api/auth/login`
pub async fn login(
    pool: web::Data<Pool>,
    session: Session,
    body: web::Json<Login>,
) -> Result<HttpResponse, AppError> {
    let creds = body.into_inner();
    let pool2 = pool.get_ref().clone();
    let username = creds.username.clone();
    let user = web::block(move || -> Result<Option<User>, AppError> {
        let conn = pool2.get()?;
        user_by_username(&conn, &username)
    })
    .await
    .map_err(|e| AppError::Internal(format!("blocking task failed: {e}")))??;

    let user = match user {
        Some(u) if u.is_active && verify_password(&creds.password, &u.password_hash) => u,
        // Same error whether the user is missing or the password is wrong.
        _ => return Err(AppError::Unauthorized),
    };

    session
        .insert(SESSION_KEY, user.id)
        .map_err(|e| AppError::Internal(format!("session write failed: {e}")))?;
    Ok(HttpResponse::Ok().json(&user))
}

/// `POST /api/auth/logout`
pub async fn logout(session: Session) -> Result<HttpResponse, AppError> {
    session.purge();
    Ok(HttpResponse::Ok().json(json!({ "ok": true })))
}

/// `GET /api/auth/me`
pub async fn me(pool: web::Data<Pool>, session: Session) -> Result<HttpResponse, AppError> {
    let user = optional_user(&session, pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(json!({ "user": user })))
}

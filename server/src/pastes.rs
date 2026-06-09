//! Paste CRUD: storage helpers, shared create/update logic, and API handlers.

use actix_session::Session;
use actix_web::{HttpResponse, web};
use chrono::{Duration, Utc};
use rusqlite::{OptionalExtension, params};

use crate::auth::{optional_user, require_user};
use crate::config::Config;
use crate::db::{Conn, Pool, db_task};
use crate::error::AppError;
use crate::ids;
use crate::models::{CreatePaste, Paste, PasteResponse, UpdatePaste, User, row_to_paste};
use crate::validation;

/// How many short-id candidates to try before giving up on a collision.
const MAX_ID_ATTEMPTS: usize = 5;

// ----- Storage helpers (run on the blocking pool via `db_task`) -----

fn fetch_by_short(conn: &Conn, short_id: &str) -> Result<Option<Paste>, AppError> {
    conn.query_row(
        "SELECT * FROM pastes WHERE short_id = ?1",
        params![short_id],
        row_to_paste,
    )
    .optional()
    .map_err(AppError::from)
}

fn fetch_by_id(conn: &Conn, id: i64) -> Result<Paste, AppError> {
    conn.query_row("SELECT * FROM pastes WHERE id = ?1", params![id], |r| {
        row_to_paste(r)
    })
    .map_err(AppError::from)
}

fn insert_paste(
    conn: &Conn,
    user_id: Option<i64>,
    title: Option<String>,
    content: String,
    language: Option<String>,
    visibility: String,
    expires_at: Option<String>,
) -> Result<Paste, AppError> {
    let now = Utc::now().to_rfc3339();
    for _ in 0..MAX_ID_ATTEMPTS {
        let short_id = ids::generate(ids::DEFAULT_LEN);
        let result = conn.execute(
            "INSERT INTO pastes
                (user_id, short_id, title, content, language, visibility, expires_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![user_id, short_id, title, content, language, visibility, expires_at, now],
        );
        match result {
            Ok(_) => return fetch_by_id(conn, conn.last_insert_rowid()),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                continue; // short_id collision: try another
            }
            Err(e) => return Err(AppError::from(e)),
        }
    }
    Err(AppError::Internal(
        "could not allocate a unique short id".into(),
    ))
}

// ----- Shared business logic (reused by API and HTML handlers) -----

/// Compute an `expires_at` timestamp from a requested lifetime in seconds.
fn resolve_expiry(seconds: Option<i64>) -> Result<Option<String>, AppError> {
    match seconds {
        None => Ok(None),
        Some(s) if s > 0 => Ok(Some((Utc::now() + Duration::seconds(s)).to_rfc3339())),
        Some(_) => Err(AppError::Validation(
            "expires_in_seconds must be positive".into(),
        )),
    }
}

/// Validate and store a new paste. Shared by the JSON API and the HTML form.
pub async fn create_core(
    pool: &Pool,
    config: &Config,
    user: Option<&User>,
    input: CreatePaste,
) -> Result<Paste, AppError> {
    validation::content(&input.content, config.max_paste_bytes)?;
    if let Some(t) = &input.title {
        validation::title(t)?;
    }
    if let Some(l) = &input.language {
        validation::language(l)?;
    }
    let visibility = input.visibility.unwrap_or_else(|| "public".to_string());
    validation::visibility(&visibility)?;

    let user_id = user.map(|u| u.id);
    if visibility == "private" && user_id.is_none() {
        return Err(AppError::Validation(
            "private pastes require an account".into(),
        ));
    }
    let expires_at = resolve_expiry(input.expires_in_seconds)?;

    let CreatePaste {
        title,
        content,
        language,
        ..
    } = input;
    db_task(pool, move |conn| {
        insert_paste(
            conn, user_id, title, content, language, visibility, expires_at,
        )
    })
    .await
}

/// Apply an update to an owned paste. Shared by the JSON API.
async fn update_core(
    pool: &Pool,
    config: &Config,
    user: &User,
    short_id: String,
    input: UpdatePaste,
) -> Result<Paste, AppError> {
    if let Some(t) = &input.title {
        validation::title(t)?;
    }
    if let Some(c) = &input.content {
        validation::content(c, config.max_paste_bytes)?;
    }
    if let Some(l) = &input.language {
        validation::language(l)?;
    }
    // Owner is guaranteed logged-in here, so any valid visibility is allowed.
    if let Some(v) = &input.visibility {
        validation::visibility(v)?;
    }
    // Whether to touch expiry at all, and the new value when we do. Omitting the
    // field leaves expiry unchanged; sending it sets (or, with a non-positive
    // value, is rejected by `resolve_expiry`).
    let change_expiry = input.expires_in_seconds.is_some();
    let new_expiry = resolve_expiry(input.expires_in_seconds)?;

    let user_id = user.id;
    db_task(pool, move |conn| {
        let existing = fetch_by_short(conn, &short_id)?.ok_or(AppError::NotFound)?;
        if existing.is_deleted {
            return Err(AppError::NotFound);
        }
        if !existing.owned_by(Some(user_id)) {
            return Err(AppError::Forbidden);
        }
        conn.execute(
            "UPDATE pastes SET
                title      = COALESCE(?1, title),
                content    = COALESCE(?2, content),
                language   = COALESCE(?3, language),
                visibility = COALESCE(?4, visibility),
                expires_at = CASE WHEN ?5 = 1 THEN ?6 ELSE expires_at END,
                updated_at = ?7
             WHERE id = ?8",
            params![
                input.title,
                input.content,
                input.language,
                input.visibility,
                change_expiry as i64,
                new_expiry,
                Utc::now().to_rfc3339(),
                existing.id,
            ],
        )?;
        fetch_by_id(conn, existing.id)
    })
    .await
}

// ----- API handlers -----

/// `POST /api/pastes`
pub async fn create(
    pool: web::Data<Pool>,
    config: web::Data<Config>,
    session: Session,
    body: web::Json<CreatePaste>,
) -> Result<HttpResponse, AppError> {
    let user = optional_user(&session, pool.get_ref()).await?;
    let paste = create_core(
        pool.get_ref(),
        config.get_ref(),
        user.as_ref(),
        body.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Created().json(PasteResponse::new(&paste, config.get_ref())))
}

/// `GET /api/pastes/{short_id}`
pub async fn read(
    pool: web::Data<Pool>,
    config: web::Data<Config>,
    session: Session,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let short_id = path.into_inner();
    let viewer = optional_user(&session, pool.get_ref()).await?;

    let paste = db_task(pool.get_ref(), move |conn| fetch_by_short(conn, &short_id))
        .await?
        .ok_or(AppError::NotFound)?;
    // Hide existence of gone or non-viewable pastes behind a 404.
    if paste.is_gone() || !paste.viewable_by(viewer.as_ref()) {
        return Err(AppError::NotFound);
    }
    Ok(HttpResponse::Ok().json(PasteResponse::new(&paste, config.get_ref())))
}

/// `PUT /api/pastes/{short_id}`
pub async fn update(
    pool: web::Data<Pool>,
    config: web::Data<Config>,
    session: Session,
    path: web::Path<String>,
    body: web::Json<UpdatePaste>,
) -> Result<HttpResponse, AppError> {
    let user = require_user(&session, pool.get_ref()).await?;
    let paste = update_core(
        pool.get_ref(),
        config.get_ref(),
        &user,
        path.into_inner(),
        body.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(PasteResponse::new(&paste, config.get_ref())))
}

/// `DELETE /api/pastes/{short_id}`
pub async fn delete(
    pool: web::Data<Pool>,
    session: Session,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let user = require_user(&session, pool.get_ref()).await?;
    let short_id = path.into_inner();
    db_task(pool.get_ref(), move |conn| {
        let existing = fetch_by_short(conn, &short_id)?.ok_or(AppError::NotFound)?;
        if existing.is_deleted {
            return Err(AppError::NotFound);
        }
        if !existing.owned_by(Some(user.id)) {
            return Err(AppError::Forbidden);
        }
        conn.execute(
            "UPDATE pastes SET is_deleted = 1, updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), existing.id],
        )?;
        Ok(())
    })
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

/// `GET /api/pastes` — list the current user's (non-deleted) pastes.
pub async fn list_mine(
    pool: web::Data<Pool>,
    config: web::Data<Config>,
    session: Session,
) -> Result<HttpResponse, AppError> {
    let user = require_user(&session, pool.get_ref()).await?;
    let pastes = list_for_user(pool.get_ref(), user.id).await?;
    let body: Vec<PasteResponse<'_>> = pastes
        .iter()
        .map(|p| PasteResponse::new(p, config.get_ref()))
        .collect();
    Ok(HttpResponse::Ok().json(body))
}

/// Fetch all non-deleted pastes for a user, newest first. Public to the HTML UI.
pub async fn list_for_user(pool: &Pool, user_id: i64) -> Result<Vec<Paste>, AppError> {
    db_task(pool, move |conn| {
        let mut stmt = conn.prepare(
            "SELECT * FROM pastes
             WHERE user_id = ?1 AND is_deleted = 0
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![user_id], row_to_paste)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    })
    .await
}

/// Fetch a single viewable paste by short id for the HTML UI.
pub async fn get_viewable(
    pool: &Pool,
    short_id: String,
    viewer: Option<&User>,
) -> Result<Paste, AppError> {
    let paste = db_task(pool, move |conn| fetch_by_short(conn, &short_id))
        .await?
        .ok_or(AppError::NotFound)?;

    // Same visibility rule as the JSON API; hide gone/non-viewable behind 404.
    if paste.is_gone() || !paste.viewable_by(viewer) {
        return Err(AppError::NotFound);
    }
    Ok(paste)
}

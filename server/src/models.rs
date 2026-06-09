//! Domain entities, request/response DTOs, and row mappers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Config;

/// A registered account.
#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub email: Option<String>,
    /// Never serialized to clients.
    #[serde(skip)]
    pub password_hash: String,
    pub display_name: Option<String>,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// A stored snippet.
#[derive(Debug, Clone, Serialize)]
pub struct Paste {
    pub id: i64,
    pub user_id: Option<i64>,
    pub short_id: String,
    pub title: Option<String>,
    pub content: String,
    pub language: Option<String>,
    pub visibility: String,
    pub expires_at: Option<String>,
    pub is_deleted: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Paste {
    /// True if the paste has an `expires_at` in the past.
    pub fn is_expired(&self) -> bool {
        match &self.expires_at {
            Some(ts) => DateTime::parse_from_rfc3339(ts)
                .map(|t| t.with_timezone(&Utc) <= Utc::now())
                .unwrap_or(false),
            None => false,
        }
    }

    /// True if the paste should be hidden from readers (deleted or expired).
    pub fn is_gone(&self) -> bool {
        self.is_deleted || self.is_expired()
    }

    /// Whether `viewer` may read this paste. Public/unlisted are readable by
    /// anyone who has the id; private requires the owner.
    pub fn viewable_by(&self, viewer: Option<&User>) -> bool {
        match self.visibility.as_str() {
            "private" => self.owned_by(viewer.map(|u| u.id)),
            _ => true,
        }
    }

    /// Whether the paste is owned by the user with id `user_id`. Anonymous
    /// pastes (`user_id == None`) are owned by nobody.
    pub fn owned_by(&self, user_id: Option<i64>) -> bool {
        self.user_id.is_some() && self.user_id == user_id
    }
}

// ----- Request DTOs -----

#[derive(Debug, Deserialize)]
pub struct CreatePaste {
    pub title: Option<String>,
    pub content: String,
    pub language: Option<String>,
    pub visibility: Option<String>,
    /// Lifetime in seconds from now; `None` means no expiry.
    pub expires_in_seconds: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePaste {
    pub title: Option<String>,
    pub content: Option<String>,
    pub language: Option<String>,
    pub visibility: Option<String>,
    pub expires_in_seconds: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct Register {
    pub username: String,
    pub password: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Login {
    pub username: String,
    pub password: String,
}

// ----- Response DTOs -----

/// A paste plus its shareable URL, for API responses.
#[derive(Debug, Serialize)]
pub struct PasteResponse<'a> {
    #[serde(flatten)]
    pub paste: &'a Paste,
    pub url: String,
}

impl<'a> PasteResponse<'a> {
    pub fn new(paste: &'a Paste, config: &Config) -> Self {
        Self {
            url: format!(
                "{}/p/{}",
                config.public_base_url.trim_end_matches('/'),
                paste.short_id
            ),
            paste,
        }
    }
}

// ----- Row mappers -----

/// Map a `users` row (`SELECT *`) into a [`User`].
pub fn row_to_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get("id")?,
        username: row.get("username")?,
        email: row.get("email")?,
        password_hash: row.get("password_hash")?,
        display_name: row.get("display_name")?,
        is_active: row.get("is_active")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

/// Map a `pastes` row (`SELECT *`) into a [`Paste`].
pub fn row_to_paste(row: &rusqlite::Row<'_>) -> rusqlite::Result<Paste> {
    Ok(Paste {
        id: row.get("id")?,
        user_id: row.get("user_id")?,
        short_id: row.get("short_id")?,
        title: row.get("title")?,
        content: row.get("content")?,
        language: row.get("language")?,
        visibility: row.get("visibility")?,
        expires_at: row.get("expires_at")?,
        is_deleted: row.get("is_deleted")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

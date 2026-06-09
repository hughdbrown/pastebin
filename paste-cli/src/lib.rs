//! Library half of the `paste` CLI: the request/response types, the path-based
//! defaulting helpers, and the upload routine. Kept separate from `main.rs` so
//! the pure pieces can be unit-tested without spawning a process.

use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Request body for `POST /api/pastes`, mirroring the server's `CreatePaste`.
/// `None` fields are omitted so the server applies its own defaults.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct CreatePaste {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<i64>,
}

/// The subset of the server's paste response we care about. Unknown fields are
/// ignored by serde, so this stays valid as the server grows its response.
#[derive(Debug, Deserialize)]
pub struct PasteResponse {
    pub short_id: String,
    pub url: String,
}

/// The server's structured error body (`{ "error": ..., "message": ... }`).
#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: String,
}

/// Default title: the file's name, e.g. `main.rs`.
pub fn derive_title(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// Default language tag: the file's extension, e.g. `rs`.
pub fn derive_language(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().into_owned())
}

/// Assemble a [`CreatePaste`] from the file path, its contents, and any explicit
/// CLI overrides. Title and language fall back to values derived from the path.
pub fn build_paste(
    path: &Path,
    content: String,
    title: Option<String>,
    language: Option<String>,
    visibility: Option<String>,
    expires_in_seconds: Option<i64>,
) -> CreatePaste {
    CreatePaste {
        title: title.or_else(|| derive_title(path)),
        language: language.or_else(|| derive_language(path)),
        content,
        visibility,
        expires_in_seconds,
    }
}

/// POST a paste to `{base_url}/api/pastes` and return the parsed response.
///
/// On a non-success status, the server's JSON `message` is surfaced in the
/// error (falling back to the raw body if it isn't the expected shape).
pub async fn upload(
    client: &reqwest::Client,
    base_url: &str,
    paste: &CreatePaste,
) -> anyhow::Result<PasteResponse> {
    let url = format!("{}/api/pastes", base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(paste)
        .send()
        .await
        .with_context(|| format!("could not reach pastebin server at {url}"))?;

    let status = resp.status();
    if status.is_success() {
        return resp
            .json()
            .await
            .context("server returned an unexpected response body");
    }

    // Surface the server's error message when the body is the expected shape.
    let body = resp.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<ErrorBody>(&body)
        .map(|e| e.message)
        .unwrap_or(body);
    anyhow::bail!("server returned {status}: {detail}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_and_language_default_from_path() {
        let path = Path::new("/tmp/notes/main.rs");
        assert_eq!(derive_title(path).as_deref(), Some("main.rs"));
        assert_eq!(derive_language(path).as_deref(), Some("rs"));
    }

    #[test]
    fn extensionless_path_has_no_language() {
        let path = Path::new("README");
        assert_eq!(derive_title(path).as_deref(), Some("README"));
        assert_eq!(derive_language(path), None);
    }

    #[test]
    fn build_paste_uses_path_defaults() {
        let paste = build_paste(
            Path::new("hello.py"),
            "print('hi')".to_string(),
            None,
            None,
            Some("public".to_string()),
            None,
        );
        assert_eq!(paste.title.as_deref(), Some("hello.py"));
        assert_eq!(paste.language.as_deref(), Some("py"));
        assert_eq!(paste.content, "print('hi')");
        assert_eq!(paste.visibility.as_deref(), Some("public"));
        assert_eq!(paste.expires_in_seconds, None);
    }

    #[test]
    fn explicit_overrides_win_over_path_defaults() {
        let paste = build_paste(
            Path::new("hello.py"),
            "x".to_string(),
            Some("My Title".to_string()),
            Some("rust".to_string()),
            Some("unlisted".to_string()),
            Some(3600),
        );
        assert_eq!(paste.title.as_deref(), Some("My Title"));
        assert_eq!(paste.language.as_deref(), Some("rust"));
        assert_eq!(paste.expires_in_seconds, Some(3600));
    }

    #[test]
    fn none_fields_are_omitted_from_json() {
        let paste = build_paste(
            Path::new("noext"),
            "body".to_string(),
            None,
            None,
            None,
            None,
        );
        let json = serde_json::to_value(&paste).unwrap();
        assert_eq!(json["content"], "body");
        assert_eq!(json["title"], "noext");
        // No extension, no visibility/expiry → those keys are absent entirely.
        assert!(json.get("language").is_none());
        assert!(json.get("visibility").is_none());
        assert!(json.get("expires_in_seconds").is_none());
    }
}

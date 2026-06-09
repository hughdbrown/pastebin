//! Input validation helpers. Each returns `AppError::Validation` on failure.

use crate::error::AppError;

/// Allowed paste visibility values.
pub const VISIBILITIES: [&str; 3] = ["public", "unlisted", "private"];

const MAX_TITLE: usize = 200;
const MAX_LANGUAGE: usize = 50;
const MIN_USERNAME: usize = 3;
const MAX_USERNAME: usize = 50;
const MIN_PASSWORD: usize = 8;
const MAX_EMAIL: usize = 255;

fn invalid(msg: impl Into<String>) -> AppError {
    AppError::Validation(msg.into())
}

/// Content must be non-empty and within the configured byte limit.
pub fn content(value: &str, max_bytes: usize) -> Result<(), AppError> {
    if value.is_empty() {
        return Err(invalid("content must not be empty"));
    }
    if value.len() > max_bytes {
        return Err(invalid(format!(
            "content exceeds the {max_bytes} byte limit"
        )));
    }
    Ok(())
}

pub fn title(value: &str) -> Result<(), AppError> {
    if value.chars().count() > MAX_TITLE {
        return Err(invalid(format!(
            "title must be at most {MAX_TITLE} characters"
        )));
    }
    Ok(())
}

pub fn language(value: &str) -> Result<(), AppError> {
    if value.chars().count() > MAX_LANGUAGE {
        return Err(invalid(format!(
            "language must be at most {MAX_LANGUAGE} characters"
        )));
    }
    Ok(())
}

pub fn visibility(value: &str) -> Result<(), AppError> {
    if VISIBILITIES.contains(&value) {
        Ok(())
    } else {
        Err(invalid(format!(
            "visibility must be one of {}",
            VISIBILITIES.join(", ")
        )))
    }
}

pub fn username(value: &str) -> Result<(), AppError> {
    let len = value.chars().count();
    if !(MIN_USERNAME..=MAX_USERNAME).contains(&len) {
        return Err(invalid(format!(
            "username must be {MIN_USERNAME}-{MAX_USERNAME} characters"
        )));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(invalid(
            "username may only contain letters, digits, '_' or '-'",
        ));
    }
    Ok(())
}

pub fn password(value: &str) -> Result<(), AppError> {
    if value.len() < MIN_PASSWORD {
        return Err(invalid(format!(
            "password must be at least {MIN_PASSWORD} characters"
        )));
    }
    Ok(())
}

/// Very small structural email check: one `@`, with text on both sides and a
/// dot in the domain. We deliberately avoid pretending to fully validate email.
pub fn email(value: &str) -> Result<(), AppError> {
    if value.len() > MAX_EMAIL {
        return Err(invalid(format!(
            "email must be at most {MAX_EMAIL} characters"
        )));
    }
    let mut parts = value.split('@');
    let (local, domain) = (parts.next(), parts.next());
    let extra = parts.next();
    match (local, domain, extra) {
        (Some(l), Some(d), None) if !l.is_empty() && d.contains('.') && !d.starts_with('.') => {
            Ok(())
        }
        _ => Err(invalid("email is not a valid address")),
    }
}

/// Validate a registration payload: username, password, and optional email.
/// Shared by the JSON API and the HTML form so both enforce the same rules.
pub fn registration(name: &str, pass: &str, mail: Option<&str>) -> Result<(), AppError> {
    username(name)?;
    password(pass)?;
    if let Some(m) = mail {
        email(m)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_rejects_empty_and_oversized() {
        assert!(content("", 100).is_err());
        assert!(content("ok", 100).is_ok());
        assert!(content("toolong", 3).is_err());
    }

    #[test]
    fn visibility_only_allows_known_values() {
        for v in VISIBILITIES {
            assert!(visibility(v).is_ok());
        }
        assert!(visibility("secret").is_err());
    }

    #[test]
    fn username_rules() {
        assert!(username("ab").is_err());
        assert!(username("alice_99").is_ok());
        assert!(username("bad name").is_err());
    }

    #[test]
    fn email_rules() {
        assert!(email("a@b.com").is_ok());
        assert!(email("nope").is_err());
        assert!(email("a@b").is_err());
        assert!(email("@b.com").is_err());
    }
}

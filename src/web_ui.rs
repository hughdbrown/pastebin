//! Minimal server-rendered HTML interface.
//!
//! Pages are rendered with small `format!`-based templates plus HTML escaping;
//! this keeps the dependency surface tiny for a deliberately minimal UI.

use actix_session::Session;
use actix_web::{HttpResponse, http::header, web};
use serde::Deserialize;

use crate::auth::{hash_password, optional_user, verify_password};
use crate::config::Config;
use crate::db::Pool;
use crate::error::AppError;
use crate::models::{CreatePaste, Paste, User};
use crate::pastes::{create_core, get_viewable, list_for_user};
use crate::validation;

// ----- HTML helpers -----

/// Escape text for safe inclusion in HTML element content / attributes.
fn escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Wrap page `body` in a shared layout with a nav bar reflecting auth state.
fn layout(title: &str, user: Option<&User>, body: &str) -> String {
    let nav = match user {
        Some(u) => format!(
            r#"<a href="/">new</a> <a href="/mine">my pastes</a>
               <span class="who">{}</span>
               <form class="inline" method="post" action="/logout"><button>logout</button></form>"#,
            escape(&u.username)
        ),
        None => {
            r#"<a href="/">new</a> <a href="/login">login</a> <a href="/register">register</a>"#
                .to_string()
        }
    };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  body {{ font-family: system-ui, sans-serif; max-width: 52rem; margin: 2rem auto; padding: 0 1rem; }}
  nav {{ display: flex; gap: 1rem; align-items: center; border-bottom: 1px solid #ddd; padding-bottom: .5rem; margin-bottom: 1rem; }}
  nav .who {{ margin-left: auto; color: #555; }}
  form.inline {{ display: inline; }}
  textarea {{ width: 100%; min-height: 14rem; font-family: ui-monospace, monospace; }}
  label {{ display: block; margin: .5rem 0 .2rem; }}
  input, select {{ padding: .3rem; }}
  pre {{ background: #f6f6f6; padding: 1rem; overflow: auto; border-radius: 4px; }}
  .err {{ color: #b00020; }}
  .meta {{ color: #555; font-size: .9rem; }}
  ul {{ padding-left: 1.2rem; }}
</style>
</head>
<body>
<nav>{nav}</nav>
{body}
</body>
</html>"#,
        title = escape(title),
        nav = nav,
        body = body,
    )
}

fn html(body: String) -> HttpResponse {
    HttpResponse::Ok()
        .content_type(header::ContentType::html())
        .body(body)
}

fn redirect(location: &str) -> HttpResponse {
    HttpResponse::Found()
        .append_header((header::LOCATION, location))
        .finish()
}

fn empty_to_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Map a human-friendly expiry choice to a lifetime in seconds.
fn parse_expiry(choice: &str) -> Option<i64> {
    match choice {
        "10m" => Some(600),
        "1h" => Some(3600),
        "1d" => Some(86_400),
        "1w" => Some(604_800),
        _ => None, // "never" or anything unknown
    }
}

// ----- Forms -----

#[derive(Debug, Deserialize)]
pub struct CreateForm {
    pub title: String,
    pub content: String,
    pub language: String,
    pub visibility: String,
    pub expires: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub password: String,
    pub email: String,
    pub display_name: String,
}

// ----- Page fragments -----

fn create_form_html(error: Option<&str>) -> String {
    let err = error
        .map(|e| format!(r#"<p class="err">{}</p>"#, escape(e)))
        .unwrap_or_default();
    format!(
        r#"<h1>New paste</h1>
{err}
<form method="post" action="/">
  <label for="title">Title (optional)</label>
  <input id="title" name="title" type="text" maxlength="200">
  <label for="content">Content</label>
  <textarea id="content" name="content" required></textarea>
  <label for="language">Language (optional)</label>
  <input id="language" name="language" type="text" maxlength="50">
  <label for="visibility">Visibility</label>
  <select id="visibility" name="visibility">
    <option value="public">public</option>
    <option value="unlisted">unlisted</option>
    <option value="private">private (requires login)</option>
  </select>
  <label for="expires">Expires</label>
  <select id="expires" name="expires">
    <option value="never">never</option>
    <option value="10m">10 minutes</option>
    <option value="1h">1 hour</option>
    <option value="1d">1 day</option>
    <option value="1w">1 week</option>
  </select>
  <p><button type="submit">Create paste</button></p>
</form>"#,
        err = err,
    )
}

// ----- Handlers -----

/// `GET /`
pub async fn index(pool: web::Data<Pool>, session: Session) -> Result<HttpResponse, AppError> {
    let user = optional_user(&session, pool.get_ref()).await?;
    Ok(html(layout(
        "New paste",
        user.as_ref(),
        &create_form_html(None),
    )))
}

/// `POST /`
pub async fn create_form(
    pool: web::Data<Pool>,
    config: web::Data<Config>,
    session: Session,
    form: web::Form<CreateForm>,
) -> Result<HttpResponse, AppError> {
    let user = optional_user(&session, pool.get_ref()).await?;
    let f = form.into_inner();
    let input = CreatePaste {
        title: empty_to_none(f.title),
        content: f.content,
        language: empty_to_none(f.language),
        visibility: Some(f.visibility),
        expires_in_seconds: parse_expiry(&f.expires),
    };
    match create_core(pool.get_ref(), config.get_ref(), user.as_ref(), input).await {
        Ok(paste) => Ok(redirect(&format!("/p/{}", paste.short_id))),
        // Re-render the form with the validation message.
        Err(AppError::Validation(msg)) => Ok(HttpResponse::BadRequest()
            .content_type(header::ContentType::html())
            .body(layout(
                "New paste",
                user.as_ref(),
                &create_form_html(Some(&msg)),
            ))),
        Err(other) => Err(other),
    }
}

/// `GET /p/{short_id}`
pub async fn view(
    pool: web::Data<Pool>,
    session: Session,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let user = optional_user(&session, pool.get_ref()).await?;
    let paste = get_viewable(pool.get_ref(), path.into_inner(), user.as_ref()).await?;
    Ok(html(layout(
        paste.title.as_deref().unwrap_or("Paste"),
        user.as_ref(),
        &paste_html(&paste),
    )))
}

fn paste_html(paste: &Paste) -> String {
    let title = paste
        .title
        .as_deref()
        .map(|t| format!("<h1>{}</h1>", escape(t)))
        .unwrap_or_else(|| "<h1>Paste</h1>".to_string());
    let lang = paste
        .language
        .as_deref()
        .map(|l| format!(" · {}", escape(l)))
        .unwrap_or_default();
    let expires = paste
        .expires_at
        .as_deref()
        .map(|e| format!(" · expires {}", escape(e)))
        .unwrap_or_default();
    format!(
        r#"{title}
<p class="meta">{visibility}{lang} · created {created}{expires}</p>
<pre>{content}</pre>"#,
        title = title,
        visibility = escape(&paste.visibility),
        lang = lang,
        created = escape(&paste.created_at),
        expires = expires,
        content = escape(&paste.content),
    )
}

/// `GET /mine`
pub async fn mine_page(pool: web::Data<Pool>, session: Session) -> Result<HttpResponse, AppError> {
    let Some(user) = optional_user(&session, pool.get_ref()).await? else {
        return Ok(redirect("/login"));
    };
    let pastes = list_for_user(pool.get_ref(), user.id).await?;
    let items = if pastes.is_empty() {
        "<p>No pastes yet.</p>".to_string()
    } else {
        let rows: String = pastes
            .iter()
            .map(|p| {
                let label = p.title.as_deref().unwrap_or(&p.short_id);
                format!(
                    r#"<li><a href="/p/{id}">{label}</a> <span class="meta">({vis})</span></li>"#,
                    id = escape(&p.short_id),
                    label = escape(label),
                    vis = escape(&p.visibility),
                )
            })
            .collect();
        format!("<ul>{rows}</ul>")
    };
    let body = format!("<h1>My pastes</h1>{items}");
    Ok(html(layout("My pastes", Some(&user), &body)))
}

/// `GET /login`
pub async fn login_page() -> HttpResponse {
    let body = r#"<h1>Login</h1>
<form method="post" action="/login">
  <label for="u">Username</label>
  <input id="u" name="username" required>
  <label for="p">Password</label>
  <input id="p" name="password" type="password" required>
  <p><button type="submit">Login</button></p>
</form>"#;
    html(layout("Login", None, body))
}

/// `POST /login`
pub async fn login_post(
    pool: web::Data<Pool>,
    session: Session,
    form: web::Form<LoginForm>,
) -> Result<HttpResponse, AppError> {
    let f = form.into_inner();
    let username = f.username.clone();
    let pool2 = pool.get_ref().clone();
    let user = web::block(move || -> Result<Option<User>, AppError> {
        let conn = pool2.get()?;
        crate::auth::user_by_username(&conn, &username)
    })
    .await
    .map_err(|e| AppError::Internal(format!("blocking task failed: {e}")))??;

    match user {
        Some(u) if u.is_active && verify_password(&f.password, &u.password_hash) => {
            session
                .insert("uid", u.id)
                .map_err(|e| AppError::Internal(format!("session write failed: {e}")))?;
            Ok(redirect("/"))
        }
        _ => {
            let body = r#"<h1>Login</h1>
<p class="err">Invalid username or password.</p>
<form method="post" action="/login">
  <label for="u">Username</label>
  <input id="u" name="username" required>
  <label for="p">Password</label>
  <input id="p" name="password" type="password" required>
  <p><button type="submit">Login</button></p>
</form>"#;
            Ok(HttpResponse::Unauthorized()
                .content_type(header::ContentType::html())
                .body(layout("Login", None, body)))
        }
    }
}

/// `GET /register`
pub async fn register_page() -> HttpResponse {
    let body = r#"<h1>Register</h1>
<form method="post" action="/register">
  <label for="u">Username</label>
  <input id="u" name="username" required>
  <label for="e">Email (optional)</label>
  <input id="e" name="email" type="email">
  <label for="d">Display name (optional)</label>
  <input id="d" name="display_name">
  <label for="p">Password</label>
  <input id="p" name="password" type="password" required>
  <p><button type="submit">Register</button></p>
</form>"#;
    html(layout("Register", None, body))
}

/// `POST /register`
pub async fn register_post(
    pool: web::Data<Pool>,
    session: Session,
    form: web::Form<RegisterForm>,
) -> Result<HttpResponse, AppError> {
    let f = form.into_inner();
    let email = empty_to_none(f.email);
    let display_name = empty_to_none(f.display_name);

    let render_error = |msg: &str, user: Option<&User>| {
        HttpResponse::BadRequest()
            .content_type(header::ContentType::html())
            .body(layout(
                "Register",
                user,
                &format!(
                    r#"<h1>Register</h1><p class="err">{}</p><p><a href="/register">try again</a></p>"#,
                    escape(msg)
                ),
            ))
    };

    if let Err(AppError::Validation(msg)) = validation::username(&f.username) {
        return Ok(render_error(&msg, None));
    }
    if let Err(AppError::Validation(msg)) = validation::password(&f.password) {
        return Ok(render_error(&msg, None));
    }
    if let Some(e) = &email
        && let Err(AppError::Validation(msg)) = validation::email(e)
    {
        return Ok(render_error(&msg, None));
    }

    let password_hash = hash_password(&f.password)?;
    let pool2 = pool.get_ref().clone();
    let username = f.username.clone();
    let created = web::block(move || -> Result<i64, AppError> {
        let conn = pool2.get()?;
        crate::auth::insert_user(
            &conn,
            &username,
            email.as_deref(),
            &password_hash,
            display_name.as_deref(),
        )
    })
    .await
    .map_err(|e| AppError::Internal(format!("blocking task failed: {e}")))?;

    match created {
        Ok(id) => {
            session
                .insert("uid", id)
                .map_err(|e| AppError::Internal(format!("session write failed: {e}")))?;
            Ok(redirect("/"))
        }
        Err(AppError::Conflict(msg)) => Ok(render_error(&msg, None)),
        Err(other) => Err(other),
    }
}

/// `POST /logout`
pub async fn logout(session: Session) -> HttpResponse {
    session.purge();
    redirect("/")
}

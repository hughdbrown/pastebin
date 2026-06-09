# Pastebin web app — design

Date: 2026-06-08

## Goal
A text-sharing service: create a paste, get a shareable link, retrieve it
reliably. Accounts are supported but anonymous pastes are allowed. Exposes a
JSON REST API plus a minimal server-rendered HTML UI.

## Decisions (from brainstorming)
- **Database**: SQLite via `rusqlite` (the prompt's PostgreSQL schema adapted to
  SQLite types), behind an `r2d2` pool. Blocking DB calls run in `web::block`.
- **Auth**: accounts + anonymous. Signed cookie sessions (`actix-session`),
  passwords hashed with `argon2`.
- **Interface**: JSON API + minimal HTML pages.
- **Features**: visibility (public/unlisted/private), expiry, language tag,
  edit & delete (owner-only; anonymous pastes are immutable).
- "code formatting crate" in the prompt is read as **rustfmt** (project
  formatting), not a runtime syntax-highlighter. Highlighting is out of scope.
- Rate limiting is noted as a future add, not in the MVP.

## Stack
Rust 2024 edition, Actix-web 4, rusqlite (bundled) + r2d2/r2d2_sqlite,
actix-session (cookie store), argon2, rand, chrono, thiserror, serde.

## Layout (lib + bin so integration tests can link the crate)
`config`, `error`, `db` (pool + migrations), `models`, `ids`, `validation`,
`auth`, `pastes`, `web_ui`. `lib.rs` exposes `configure(ServiceConfig)`;
`main.rs` wires config/pool/session and serves.

## Data model (SQLite)
- `users(id INTEGER PK, username UNIQUE, email UNIQUE NULL, password_hash,
  display_name NULL, is_active INTEGER DEFAULT 1, created_at, updated_at)`
- `pastes(id INTEGER PK, user_id NULL -> users ON DELETE SET NULL,
  short_id UNIQUE, title NULL, content NOT NULL, language NULL,
  visibility DEFAULT 'public', expires_at NULL, is_deleted DEFAULT 0,
  created_at, updated_at)` + indexes on user_id, created_at, expires_at.

## Behavior
- **short_id**: 8-char base62, regenerate on UNIQUE collision (bounded retry).
- **Visibility**: public (listed+readable), unlisted (readable with id only),
  private (owner-only; anonymous may not create private).
- **Expiry**: lazy — expired or soft-deleted pastes return 404.
- **Edit/delete**: owner-only; delete is soft (`is_deleted=1`).
- **Validation**: content non-empty and <= 1 MiB (configurable), title <= 200,
  language <= 50, visibility in allowed set, username 3-50, password >= 8,
  basic email shape.

## API
`POST /api/pastes`, `GET/PUT/DELETE /api/pastes/{short_id}`,
`GET /api/pastes` (list mine), `POST /api/auth/register|login|logout`,
`GET /api/auth/me`. HTML: `/`, `POST /`, `/p/{short_id}`, `/login`,
`/register`, `/mine`, `/logout`.

## Errors / security / testing
- `AppError` -> `ResponseError` mapping to status + JSON `{error, message}`.
- argon2 hashing; signed session cookie (secret from env, random dev fallback);
  owner checks for private/edit/delete; private existence hidden as 404.
- Tests use a fresh temp-file SQLite DB per test (real DB, no mocks): unit tests
  for ids/validation/argon2; integration tests via `actix_web::test`.

## Ops
- **Dockerfile**: multi-stage (cargo build --release, slim runtime image).
- **justfile**: test, build, run, fmt, lint, docker build/deploy tasks.

# pastebin

A small pastebin web service in Rust: create, share, and retrieve text snippets.
It exposes both a JSON API and a minimal server-rendered HTML UI, backed by an
embedded SQLite database. The repo is a Cargo workspace with two crates:

- **`server/`** — the pastebin web service (`pastebin` binary).
- **`paste-cli/`** — a command-line client that sends a file to the server (`paste` binary).

## Features

- Create, read, update, and delete pastes (soft delete).
- Short, URL-safe IDs with collision retry.
- Visibility levels: `public`, `unlisted`, and `private` (private requires an account).
- Optional expiry (`expires_in_seconds`) — expired pastes are hidden behind a 404.
- Optional title and language tag, with input validation and a configurable size cap.
- User accounts with Argon2 password hashing and cookie-based sessions.
- Anonymous pastes (no account needed) for public/unlisted snippets.
- Embedded SQLite via `rusqlite` (`bundled` feature) over an `r2d2` connection pool.

## Tech stack

- [actix-web](https://actix.rs/) 4 — HTTP server and routing
- [actix-session](https://docs.rs/actix-session) — cookie-backed sessions
- [rusqlite](https://docs.rs/rusqlite) + [r2d2](https://docs.rs/r2d2) — SQLite with pooling
- [argon2](https://docs.rs/argon2) — password hashing
- Rust 2024 edition (requires Rust ≥ 1.96)

## Quick start

```sh
# Run the server (debug build) on http://127.0.0.1:8080
cargo run -p pastebin

# or, via the task runner
just run
```

Then open <http://127.0.0.1:8080> in a browser, use the JSON API below, or send a
file with the [`paste` CLI](#paste-cli).

This project uses [`just`](https://github.com/casey/just) as a task runner. Run
`just` to list all recipes. Common ones:

| Recipe                | What it does                                            |
| --------------------- | ------------------------------------------------------- |
| `just run`            | Run the server locally (debug build)                    |
| `just paste <file>`   | Send a file to a server with the CLI                    |
| `just test`           | Format check + clippy (warnings as errors) + `cargo test` |
| `just fmt`            | Auto-format the code                                     |
| `just lint`           | Clippy with warnings as errors                          |
| `just check`          | Fast type-check                                          |
| `just build`          | Optimized release build                                 |

## Configuration

All configuration is read from environment variables at startup (with defaults):

| Variable          | Default                  | Description                                            |
| ----------------- | ------------------------ | ------------------------------------------------------ |
| `BIND_ADDR`       | `127.0.0.1:8080`         | Address the HTTP server binds to                       |
| `DATABASE_PATH`   | `pastebin.db`            | Path to the SQLite database file                       |
| `MAX_PASTE_BYTES` | `1048576` (1 MiB)        | Maximum allowed paste content size, in bytes           |
| `PUBLIC_BASE_URL` | `http://127.0.0.1:8080`  | Base URL used to build shareable links                 |
| `SESSION_SECRET`  | _(unset)_                | Session signing secret. If unset, a random key is generated at startup (sessions won't survive a restart or work across instances). |

Example:

```sh
BIND_ADDR=0.0.0.0:9000 \
PUBLIC_BASE_URL=https://paste.example.com \
SESSION_SECRET="$(openssl rand -hex 32)" \
cargo run
```

## HTTP API

All API routes are under `/api`. Request and response bodies are JSON. Auth uses
a session cookie set by `/api/auth/login`.

### Pastes

| Method   | Path                  | Auth      | Description                              |
| -------- | --------------------- | --------- | ---------------------------------------- |
| `POST`   | `/api/pastes`         | optional  | Create a paste                           |
| `GET`    | `/api/pastes`         | required  | List the current user's pastes           |
| `GET`    | `/api/pastes/{id}`    | optional  | Read a paste by short id                 |
| `PUT`    | `/api/pastes/{id}`    | required  | Update an owned paste                    |
| `DELETE` | `/api/pastes/{id}`    | required  | Soft-delete an owned paste               |

### Auth

| Method | Path                  | Description              |
| ------ | --------------------- | ------------------------ |
| `POST` | `/api/auth/register`  | Create an account        |
| `POST` | `/api/auth/login`     | Log in (sets session)    |
| `POST` | `/api/auth/logout`    | Log out                  |
| `GET`  | `/api/auth/me`        | Current authenticated user |

### Create a paste

```sh
curl -X POST http://127.0.0.1:8080/api/pastes \
  -H 'Content-Type: application/json' \
  -d '{
        "title": "hello",
        "content": "fn main() { println!(\"hi\"); }",
        "language": "rust",
        "visibility": "public",
        "expires_in_seconds": 3600
      }'
```

`content` is required; `title`, `language`, `visibility` (`public` by default),
and `expires_in_seconds` are optional. A successful response is `201 Created`
with the stored paste plus a shareable `url`:

```json
{
  "id": 1,
  "short_id": "Ab3kQ9",
  "title": "hello",
  "content": "fn main() { println!(\"hi\"); }",
  "language": "rust",
  "visibility": "public",
  "expires_at": "2026-06-08T12:00:00+00:00",
  "is_deleted": false,
  "created_at": "...",
  "updated_at": "...",
  "url": "http://127.0.0.1:8080/p/Ab3kQ9"
}
```

`PUT` accepts the same fields, all optional — omitted fields are left unchanged.

## HTML UI

Server-rendered pages for browser use:

| Path            | Description                          |
| --------------- | ------------------------------------ |
| `/`             | Home / new-paste form                |
| `/p/{id}`       | View a paste                         |
| `/mine`         | The current user's pastes            |
| `/login`        | Log in                               |
| `/register`     | Register                             |

## `paste` CLI

`paste` is a small command-line client (the `paste-cli` crate) that reads a file
and posts its contents to a pastebin server's `POST /api/pastes` endpoint, then
prints the shareable URL.

```sh
# Build and run via cargo…
cargo run -p paste-cli -- notes.txt

# …or via just (extra flags pass through)
just paste notes.txt --visibility unlisted

# …or the built binary directly
paste snippet.rs --server https://paste.example.com --expires-in 3600
```

```
paste <FILE> [--server URL] [--title T] [--language L]
             [--visibility public|unlisted] [--expires-in SECONDS]
```

| Flag           | Default                           | Description                                  |
| -------------- | --------------------------------- | -------------------------------------------- |
| `--server`     | `http://127.0.0.1:8080` (or `$PASTEBIN_URL`) | Base URL of the pastebin server   |
| `--title`      | the file name                     | Paste title                                  |
| `--language`   | the file extension                | Language tag                                 |
| `--visibility` | `public`                          | `public` or `unlisted` (anonymous; no `private`) |
| `--expires-in` | _(none)_                          | Expire the paste after this many seconds     |

On success it prints the paste URL to stdout (handy for piping). On failure —
unreadable file, connection refused, or a rejection from the server — it prints
the error to stderr and exits non-zero, surfacing the server's own message.

## Development

```sh
just test     # fmt --check + clippy -D warnings + cargo test (whole workspace)
```

The server's integration tests live in `server/tests/integration.rs` and drive
the app through the same `configure` route table that `main` uses. The CLI's
HTTP round-trip is tested against a mock server in `paste-cli/tests/upload.rs`.

## Docker

```sh
just docker-build          # build the production image
just docker-run            # run locally on 127.0.0.1:8080 with a persistent volume
just docker-push image=ghcr.io/you/pastebin
just deploy  image=ghcr.io/you/pastebin tag=v0.1.0
```

The image is a multi-stage build: a `rust:1.96` builder stage produces the
`pastebin` release binary (only the server package is built, so the CLI's
dependencies never compile) that is copied into a slim Debian runtime running as
a non-root user. SQLite is statically linked (rusqlite `bundled`), and the
database is persisted to a `/data` volume.

## Project layout

```
Cargo.toml           Workspace manifest (members: server, paste-cli)
server/              The pastebin web service
  src/
    main.rs          Binary entry point: config, DB pool, server setup
    lib.rs           Route table (configure) shared by main and tests
    config.rs        Environment-driven configuration
    db.rs            Connection pool, migrations, blocking DB helpers
    models.rs        Entities, request/response DTOs, row mappers
    pastes.rs        Paste CRUD: storage, business logic, API handlers
    auth.rs          Registration, login, sessions, user lookup
    web_ui.rs        Server-rendered HTML handlers
    validation.rs    Input validation
    ids.rs           Short-id generation
    error.rs         Error type and HTTP response mapping
  tests/
    integration.rs   End-to-end API tests
paste-cli/           The `paste` command-line client
  src/
    main.rs          CLI entry point: arg parsing and orchestration
    lib.rs           Request/response types, path defaults, upload routine
  tests/
    upload.rs        HTTP round-trip tests against a mock server
```

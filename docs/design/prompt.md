# Purpose
A pastebin app is a text-sharing service where users upload snippets and get a unique URL to view them later. The most important functionality is: create a paste, generate a shareable link, and retrieve the paste reliably; common extensions are expiration, privacy, editing, deletion, and search.

# Stack
- Rust 1.96 (2024 edition)
- rusqlite
- Actix latest
- code formatting crate

# Core functionality
For a practical pastebin, the minimum feature set is:

- Create a paste from raw text.
- Return a unique ID or URL for that paste.
- Read a paste by ID.
- Optionally support expiry, public/private visibility, and editing/deletion.

The database usually stores paste metadata separately from the paste content if the content is large or you want simpler scaling. A typical relational design centers on users and pastes, with pastes referencing users as the owner.

# Database schema
Here is a solid starting schema for the two tables you asked for.

## Users table
``` sql
CREATE TABLE users (
    id              BIGSERIAL PRIMARY KEY,
    username        VARCHAR(50)  NOT NULL UNIQUE,
    email           VARCHAR(255) UNIQUE,
    password_hash   TEXT         NOT NULL,
    display_name    VARCHAR(100),
    is_active       BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);
```

## Pastes table
``` sql
CREATE TABLE pastes (
    id              BIGSERIAL PRIMARY KEY,
    user_id         BIGINT REFERENCES users(id) ON DELETE SET NULL,
    short_id        VARCHAR(32)  NOT NULL UNIQUE,
    title           VARCHAR(200),
    content         TEXT         NOT NULL,
    language        VARCHAR(50),
    visibility      VARCHAR(20)  NOT NULL DEFAULT 'public',
    expires_at      TIMESTAMPTZ,
    is_deleted      BOOLEAN       NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pastes_user_id ON pastes(user_id);
CREATE INDEX idx_pastes_created_at ON pastes(created_at DESC);
CREATE INDEX idx_pastes_expires_at ON pastes(expires_at);
```

## Table roles
users stores account identity and authentication data. pastes stores the actual snippet plus the shareable identifier, ownership, visibility, and lifecycle fields such as expiry or deletion state.

## Practical notes
If you want anonymous pastes, keep user_id nullable. If you want private pastes or access control later, add a separate permissions table; that pattern is commonly used when multiple users can access one paste.

# Product rules
The agent should know whether pastes are anonymous or account-based, whether users can edit or delete after creation, and whether pastes expire automatically. It should also know if paste URLs must be unguessable, whether private/password-protected pastes exist, and whether syntax highlighting or formatting is required.

# API surface
At minimum, the agent needs the expected endpoints and request/response shapes. Typical operations are create paste, fetch paste by short ID, update paste, delete paste, and optionally list pastes for a user or API client.

# Data and storage
The agent should know paste size limits, expected read/write volume, and whether paste content lives in SQL, blob storage, or both. For larger or high-volume systems, the usual pattern is to keep metadata in the database and store content in object storage, with expiry metadata used for cleanup.

# Security and abuse controls
It should know the authentication model, rate limits, spam controls, and any moderation or logging requirements. Pastebin-style services often need abuse prevention because they can be used for public text sharing or, unfortunately, data dumping.

# Minimal build brief
A good “agent brief” for implementation would include:

- Paste creation flow and required fields.
- Visibility model: public, unlisted, private, password-protected.
- Expiration rules and deletion behavior.
- ID generation rules and collision expectations.
- Storage choice for content and metadata.
- Authentication and authorization rules.
- Rate limits and abuse handling.
- Whether syntax highlighting, markdown, or versioning is in scope.

//! Runtime configuration, loaded from environment variables.

/// Application configuration. Cheap to clone (a few owned strings).
#[derive(Debug, Clone)]
pub struct Config {
    /// Address the HTTP server binds to, e.g. `127.0.0.1:8080`.
    pub bind_addr: String,
    /// Path to the SQLite database file.
    pub database_path: String,
    /// Maximum allowed paste content size, in bytes.
    pub max_paste_bytes: usize,
    /// Public base URL used to build shareable links, e.g. `http://localhost:8080`.
    pub public_base_url: String,
    /// Optional session signing secret. When absent a random key is generated
    /// at startup (fine for development, not for multi-instance production).
    pub session_secret: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8080".to_string(),
            database_path: "pastebin.db".to_string(),
            max_paste_bytes: 1 << 20, // 1 MiB
            public_base_url: "http://127.0.0.1:8080".to_string(),
            session_secret: None,
        }
    }
}

impl Config {
    /// Build configuration from environment variables, falling back to defaults.
    pub fn from_env() -> Self {
        let defaults = Self::default();
        Self {
            bind_addr: env_or("BIND_ADDR", defaults.bind_addr),
            database_path: env_or("DATABASE_PATH", defaults.database_path),
            max_paste_bytes: std::env::var("MAX_PASTE_BYTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(defaults.max_paste_bytes),
            public_base_url: env_or("PUBLIC_BASE_URL", defaults.public_base_url),
            session_secret: std::env::var("SESSION_SECRET")
                .ok()
                .filter(|s| !s.is_empty()),
        }
    }
}

fn env_or(key: &str, fallback: String) -> String {
    std::env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
}

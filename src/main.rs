//! Binary entry point: load config, open the database, and serve.

use actix_session::{SessionMiddleware, storage::CookieSessionStore};
use actix_web::cookie::Key;
use actix_web::middleware::Logger;
use actix_web::{App, HttpServer, web};

use pastebin::config::Config;
use pastebin::db;

/// Build a session signing key from the configured secret, or generate a random
/// one (logged as a warning, since sessions won't survive a restart).
fn session_key(config: &Config) -> Key {
    match &config.session_secret {
        Some(secret) => Key::derive_from(secret.as_bytes()),
        None => {
            log::warn!(
                "SESSION_SECRET not set; generating a random key. Sessions will not \
                 survive a restart and won't work across multiple instances."
            );
            Key::generate()
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = Config::from_env();
    let pool = db::init_pool(&config.database_path)
        .unwrap_or_else(|e| panic!("failed to open database '{}': {e}", config.database_path));
    {
        let conn = pool
            .get()
            .unwrap_or_else(|e| panic!("failed to get a database connection: {e}"));
        db::run_migrations(&conn).unwrap_or_else(|e| panic!("failed to run migrations: {e}"));
    }

    let key = session_key(&config);
    let bind_addr = config.bind_addr.clone();
    log::info!("listening on http://{bind_addr}");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), key.clone())
                    // Allow http during local development; set true behind HTTPS.
                    .cookie_secure(false)
                    .build(),
            )
            .wrap(Logger::default())
            .configure(pastebin::configure)
    })
    .bind(bind_addr)?
    .run()
    .await
}

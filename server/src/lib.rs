//! Pastebin: a text-sharing web service (JSON API + minimal HTML UI).

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod ids;
pub mod models;
pub mod pastes;
pub mod validation;
pub mod web_ui;

use actix_web::web;

/// Register all routes (API + HTML) on an Actix `ServiceConfig`.
///
/// Both `main` and the integration tests call this, so the route table lives in
/// exactly one place. App data (pool, config) and session middleware are added
/// by the caller.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/pastes", web::post().to(pastes::create))
            .route("/pastes", web::get().to(pastes::list_mine))
            .route("/pastes/{short_id}", web::get().to(pastes::read))
            .route("/pastes/{short_id}", web::put().to(pastes::update))
            .route("/pastes/{short_id}", web::delete().to(pastes::delete))
            .route("/auth/register", web::post().to(auth::register))
            .route("/auth/login", web::post().to(auth::login))
            .route("/auth/logout", web::post().to(auth::logout))
            .route("/auth/me", web::get().to(auth::me)),
    )
    // HTML UI
    .route("/", web::get().to(web_ui::index))
    .route("/", web::post().to(web_ui::create_form))
    .route("/p/{short_id}", web::get().to(web_ui::view))
    .route("/mine", web::get().to(web_ui::mine_page))
    .route("/login", web::get().to(web_ui::login_page))
    .route("/login", web::post().to(web_ui::login_post))
    .route("/register", web::get().to(web_ui::register_page))
    .route("/register", web::post().to(web_ui::register_post))
    .route("/logout", web::post().to(web_ui::logout));
}

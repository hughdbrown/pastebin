//! End-to-end HTTP tests driving the real handlers against a temp SQLite DB.

use actix_session::{SessionMiddleware, storage::CookieSessionStore};
use actix_web::cookie::{Cookie, Key};
use actix_web::{App, test, web};
use pastebin::config::Config;
use pastebin::db;
use serde_json::{Value, json};
use tempfile::TempDir;

/// Build a test app backed by a fresh temp-file SQLite database.
///
/// We use a real on-disk SQLite file (in a `TempDir` that is cleaned up when
/// dropped) rather than mocks: the handlers exercise the actual SQL, and each
/// test gets an isolated database. A shared, stable key keeps session cookies
/// valid across the requests within a single test.
async fn test_app(
    dir: &TempDir,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let db_path = dir.path().join("test.db");
    let pool = db::init_pool(db_path.to_str().unwrap()).expect("pool");
    db::run_migrations(&pool.get().unwrap()).expect("migrations");

    let config = Config {
        database_path: db_path.to_string_lossy().into_owned(),
        ..Config::default()
    };
    // Fixed 64-byte key so cookies signed in one request verify in the next.
    let key = Key::from(&[7u8; 64]);

    test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config))
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), key)
                    .cookie_secure(false)
                    .build(),
            )
            .configure(pastebin::configure),
    )
    .await
}

#[actix_web::test]
async fn anonymous_create_then_read_roundtrips() {
    let dir = TempDir::new().unwrap();
    let app = test_app(&dir).await;

    let req = test::TestRequest::post()
        .uri("/api/pastes")
        .set_json(json!({ "content": "hello world", "language": "text" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201, "create should return 201");
    let body: Value = test::read_body_json(resp).await;
    let short_id = body["short_id"].as_str().expect("short_id").to_string();
    assert!(body["url"].as_str().unwrap().ends_with(&short_id));

    let req = test::TestRequest::get()
        .uri(&format!("/api/pastes/{short_id}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["content"], "hello world");
    assert_eq!(body["visibility"], "public");
}

#[actix_web::test]
async fn reading_unknown_paste_is_404() {
    let dir = TempDir::new().unwrap();
    let app = test_app(&dir).await;

    let req = test::TestRequest::get()
        .uri("/api/pastes/doesnotexist")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn empty_content_is_rejected() {
    let dir = TempDir::new().unwrap();
    let app = test_app(&dir).await;

    let req = test::TestRequest::post()
        .uri("/api/pastes")
        .set_json(json!({ "content": "" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn anonymous_cannot_create_private_paste() {
    let dir = TempDir::new().unwrap();
    let app = test_app(&dir).await;

    let req = test::TestRequest::post()
        .uri("/api/pastes")
        .set_json(json!({ "content": "secret", "visibility": "private" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn register_sets_session_and_me_returns_user() {
    let dir = TempDir::new().unwrap();
    let app = test_app(&dir).await;

    let req = test::TestRequest::post()
        .uri("/api/auth/register")
        .set_json(json!({ "username": "alice", "password": "supersecret" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let session_cookie: Cookie<'static> = resp
        .response()
        .cookies()
        .find(|c| c.name() == "id")
        .expect("session cookie set")
        .into_owned();

    let req = test::TestRequest::get()
        .uri("/api/auth/me")
        .cookie(session_cookie)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["user"]["username"], "alice");
}

#[actix_web::test]
async fn duplicate_username_conflicts() {
    let dir = TempDir::new().unwrap();
    let app = test_app(&dir).await;

    let payload = json!({ "username": "bob", "password": "supersecret" });
    let req = test::TestRequest::post()
        .uri("/api/auth/register")
        .set_json(&payload)
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/auth/register")
        .set_json(&payload)
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 409);
}

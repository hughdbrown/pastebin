//! HTTP round-trip tests for `upload`, driven against a mock pastebin server
//! (`wiremock`) so no real server is needed.

use std::path::Path;

use paste_cli::{build_paste, upload};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sample_paste() -> paste_cli::CreatePaste {
    build_paste(
        Path::new("hello.txt"),
        "hello world".to_string(),
        None,
        None,
        Some("public".to_string()),
        None,
    )
}

#[tokio::test]
async fn upload_returns_url_on_success() {
    let server = MockServer::start().await;
    let body = json!({
        "short_id": "Ab3kQ9",
        "url": format!("{}/p/Ab3kQ9", server.uri()),
    });
    Mock::given(method("POST"))
        .and(path("/api/pastes"))
        .respond_with(ResponseTemplate::new(201).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    let resp = upload(&client, &server.uri(), &sample_paste())
        .await
        .expect("upload should succeed");

    assert_eq!(resp.short_id, "Ab3kQ9");
    assert!(resp.url.ends_with("/p/Ab3kQ9"));
}

#[tokio::test]
async fn upload_surfaces_server_error_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/pastes"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "validation_error",
            "message": "content must not be empty",
        })))
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    let err = upload(&client, &server.uri(), &sample_paste())
        .await
        .expect_err("upload should fail on 400");

    let msg = err.to_string();
    assert!(msg.contains("content must not be empty"), "got: {msg}");
    assert!(msg.contains("400"), "got: {msg}");
}

#[tokio::test]
async fn upload_reports_connection_failure() {
    // Nothing is listening on this port → connection error, not an HTTP status.
    let client = reqwest::Client::new();
    let err = upload(&client, "http://127.0.0.1:1", &sample_paste())
        .await
        .expect_err("upload should fail to connect");
    assert!(err.to_string().contains("could not reach"), "got: {err}");
}

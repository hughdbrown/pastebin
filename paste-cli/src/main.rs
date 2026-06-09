//! `paste` — read a file and send its contents to a pastebin server, printing
//! the shareable URL on success.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use paste_cli::{build_paste, upload};

/// Send a file to a pastebin server and print the shareable URL.
#[derive(Debug, Parser)]
#[command(name = "paste", version, about)]
struct Cli {
    /// Path to the file to upload.
    file: PathBuf,

    /// Base URL of the pastebin server.
    #[arg(long, env = "PASTEBIN_URL", default_value = "http://127.0.0.1:8080")]
    server: String,

    /// Paste title (defaults to the file name).
    #[arg(long)]
    title: Option<String>,

    /// Language tag (defaults to the file extension).
    #[arg(long)]
    language: Option<String>,

    /// Visibility of the paste.
    #[arg(long, default_value = "public", value_parser = ["public", "unlisted"])]
    visibility: String,

    /// Expire the paste after this many seconds.
    #[arg(long, value_name = "SECONDS")]
    expires_in: Option<i64>,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(url) => {
            println!("{url}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<String> {
    let cli = Cli::parse();

    let content = std::fs::read_to_string(&cli.file)
        .with_context(|| format!("could not read file '{}'", cli.file.display()))?;

    let paste = build_paste(
        &cli.file,
        content,
        cli.title,
        cli.language,
        Some(cli.visibility),
        cli.expires_in,
    );

    let client = reqwest::Client::new();
    let resp = upload(&client, &cli.server, &paste).await?;
    Ok(resp.url)
}

# Pastebin task runner. Run `just` to list recipes.

# Image / registry settings (override on the CLI, e.g. `just deploy image=ghcr.io/me/pastebin`).
image := "pastebin"
tag := "latest"

# Default: show available recipes.
default:
    @just --list

# --- Development ---

# Run the full check suite: format check, clippy (warnings = errors), tests.
test:
    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings
    cargo test

# Auto-format the code.
fmt:
    cargo fmt --all

# Lint (warnings as errors).
lint:
    cargo clippy --all-targets -- -D warnings

# Fast type-check.
check:
    cargo check --all-targets

# Build an optimized release binary.
build:
    cargo build --release

# Run the server locally (debug build). Override env inline, e.g.
# `BIND_ADDR=127.0.0.1:9000 just run`.
run:
    cargo run

# Remove build artifacts.
clean:
    cargo clean

# --- Docker / deploy ---

# Build the production container image.
docker-build:
    docker build -t {{image}}:{{tag}} .

# Run the container locally on port 8080 with a persistent named volume.
docker-run: docker-build
    docker run --rm -p 8080:8080 \
        -e SESSION_SECRET="${SESSION_SECRET:-change-me-in-prod-please-32+bytes}" \
        -e PUBLIC_BASE_URL="${PUBLIC_BASE_URL:-http://localhost:8080}" \
        -v pastebin-data:/data \
        {{image}}:{{tag}}

# Push the image to a registry (set `image` to your registry path).
docker-push: docker-build
    docker push {{image}}:{{tag}}

# Full deploy: verify, build the image, and push it. Run from a clean tree.
# Example: `just deploy image=ghcr.io/you/pastebin tag=v0.1.0`
deploy: test docker-build docker-push
    @echo "Deployed {{image}}:{{tag}}"

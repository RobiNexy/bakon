# Project conventions

> Record only what is confirmed and not obvious from configuration or code. Empty items are not gates. Items marked "detected" were inferred by the agent and await confirmation.

## Commands

- Build: `cargo build --release -p bakon` → `target/release/bakon`; debug build `cargo build --workspace` → `target/debug/bakon` (detected)
- Run locally: `target/debug/bakon <command>` (detected)
- Tests: `cargo test --workspace --all-targets` (detected from `.github/workflows/ci.yml`)
- Rust lint / format: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` (detected)
- 0.x legacy: `internal/`, `main.go`, `go.mod`, and `scripts/build.sh` belong to 0.x on `main`; this branch does not change them. The Go gates in CI stay until that code is removed.

## Environments

- How to tell local from test/staging from production (env vars, config files, hostnames): local only, confirmed — single-machine CLI; state lives in the store directory (`BAKON_HOME` / `BAKON_STORE`, default per `internal/config`)
- Commands or tools that touch non-local environments (treat as high-impact): none — releases are produced by CI artifacts, not local deploys

## Git

- Branching: unknown (e.g., trunk-based / feature branches)
- Commit style: unknown (e.g., Conventional Commits)
- Does the agent commit without being asked: yes (confirmed 2026-11-10)

## Collaboration

- Default mode (see [[AGENT__collaboration]]): Copilot
- Design documents directory: `design/` unless stated otherwise

## Project-specific exceptions

- Code and test layout: follow the existing layout; note only exceptions the agent must know
- Naming or formatting exceptions: none
- Other long-lived conventions: none

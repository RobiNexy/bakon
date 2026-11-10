# Project conventions

> Record only what is confirmed and not obvious from configuration or code. Empty items are not gates. Items marked "detected" were inferred by the agent and await confirmation.

## Commands

- Install dependencies: `go mod download` (detected; Go 1.27 per `go.mod`)
- Run locally: `go run .` (detected; binary name `bakon`)
- Tests: `go test -race -count=1 ./...` (detected from `.github/workflows/ci.yml`)
- Type check: `go vet ./...` (detected from CI)
- Lint / format: `test -z "$(gofmt -l .)"` (detected from CI; fix with `gofmt -w .`)
- Build: `go build .` for a local binary; `scripts/build.sh` for release artifacts in `dist/` (detected)
- Rust build: `cargo build --workspace`; binary at `target/debug/bakon` (detected; toolchain pinned by `rust-toolchain.toml`)
- Rust tests: `cargo test --workspace --all-targets` (detected)
- Rust lint / format: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` (detected)

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

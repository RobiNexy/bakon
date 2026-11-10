# Bakon

> Effortlessly keep version history while editing critical files.

> **Status: v1 rewrite in progress.** `main` still carries 0.x (Go + Git); the `v1-rust` branch is a Rust rewrite with its own content-addressed store and has completed M0–M2. The installation and command text below describes 0.x; see “v1 (in progress)” for building v1 and [`design/`](design/) for the design.

[English](README.en.md) | [中文](README.md)

[![CI](https://github.com/RobiNexy/Bakon/actions/workflows/ci.yml/badge.svg)](https://github.com/RobiNexy/Bakon/actions/workflows/ci.yml)

Bakon is a single-machine command-line file version manager. It lets your usual editor modify critical files such as `nginx.conf`, `hosts`, and crontabs in place while saving every change to a private Git repository. View history, compare versions, export snapshots, and roll back without overwriting history.

- Single machine and single user; no remotes, collaboration, or merge branches
- Per-file append-only integer versions; rollback creates a new version
- The target file is edited in place; Bakon stores copies only in its repository
- Per-file retention, hooks, atomic writes, and serialized write operations

## v1 (in progress)

The Rust workspace lives in `crates/`, does not call git, and defaults to `~/.bakon/store`:

```sh
cargo build --release -p bakon
cargo test --workspace
target/release/bakon edit /etc/nginx/nginx.conf
```

Design entry: [`design/MODULE__bakon-v1-design.md`](design/MODULE__bakon-v1-design.md). Release artifacts and Windows semantics are still open; see the task board.

## Installation

Download a platform archive from [Releases](../../releases). Checksums are in `checksums.txt`.

| Archive | Platform |
|---|---|
| `bakon_<ver>_linux-amd64.tar.gz` | Linux x86-64 |
| `bakon_<ver>_linux-arm64.tar.gz` | Linux arm64 |
| `bakon_<ver>_termux-arm64.tar.gz` | Android Termux aarch64 |
| `bakon_<ver>_linux-armv7.tar.gz` | Linux armv7 / Android Termux 32-bit |
| `bakon_<ver>_darwin-amd64.tar.gz` | macOS Intel |
| `bakon_<ver>_darwin-arm64.tar.gz` | macOS Apple Silicon |
| `bakon_<ver>_windows-amd64.zip` | Windows x86-64 |
| `bakon_<ver>_windows-arm64.zip` | Windows arm64 |

### Linux / macOS

```sh
BAKON_VER=v0.1.0
curl -fLo /tmp/bakon.tar.gz \
  "https://github.com/RobiNexy/Bakon/releases/download/${BAKON_VER}/bakon_${BAKON_VER}_linux-amd64.tar.gz"
tar -xzf /tmp/bakon.tar.gz -C /tmp
install /tmp/bakon_${BAKON_VER}_linux-amd64/bakon ~/.local/bin/bakon
```

### Android Termux

Termux has a dedicated `termux-arm64` archive. `git` is still required at runtime.

```sh
pkg install git
BAKON_VER=v0.1.0
curl -fLo /tmp/bakon.tar.gz \
  "https://github.com/RobiNexy/Bakon/releases/download/${BAKON_VER}/bakon_${BAKON_VER}_termux-arm64.tar.gz"
tar -xzf /tmp/bakon.tar.gz -C /tmp
mkdir -p "$PREFIX/bin"
install /tmp/bakon_${BAKON_VER}_termux-arm64/bakon "$PREFIX/bin/bakon"
```

### From source

```sh
go install github.com/RobiNexy/Bakon@latest
```

Or:

```sh
git clone https://github.com/RobiNexy/Bakon && cd Bakon
go build .
```

Runtime dependencies are the system `git` command. Editors and hooks run through `sh` on Unix and `cmd /C` on Windows.

## Quick Start

```sh
# The first edit stores the pre-adoption content as version 0
bakon edit /etc/nginx/nginx.conf

bakon log /etc/nginx/nginx.conf
bakon diff /etc/nginx/nginx.conf
bakon show /etc/nginx/nginx.conf 2
bakon revert /etc/nginx/nginx.conf 2
bakon hook set /etc/nginx/nginx.conf "systemctl reload nginx"
```

## Commands

| Command | Purpose |
|---|---|
| `bakon edit <file>` | Open the editor and commit a new version when content changes |
| `bakon log <file>` | List history; `--format json` emits JSON Lines |
| `bakon diff <file> [v1] [v2]` | Compare versions; exits 1 when differences are found |
| `bakon show <file> <v>` | Print a version's content |
| `bakon dump <file> <v>` | Write a version as binary-safe pipeline data |
| `bakon revert <file> <v>` | Restore a version and append a new version |
| `bakon ls` | List all managed files |
| `bakon mv <old> <new>` | Update a path mapping while keeping history |
| `bakon prune [<file>]` | Trim history according to retention limits |
| `bakon hook set/unset/show` | Manage a file's change hook |
| `bakon path <file>` | Print the stable internal mapping |
| `bakon verify` | Check repository and index consistency |
| `bakon config show/init` | Show or initialize configuration |
| `bakon version` | Print version and build information |

Global options: `--config`, `--store`, `--format human|plain|json`, `--no-color`, and `--no-hook`.

## Configuration

The default configuration follows XDG locations:

- Config: `$XDG_CONFIG_HOME/bakon/config.toml`
- Repository: `$XDG_DATA_HOME/bakon/repo/`
- When XDG variables are unset, the historical `~/.bakon/` location remains supported
- `BAKON_HOME` places config and repository data in an isolated directory

```toml
editor = ""                 # config > BAKON_EDITOR > VISUAL > EDITOR > vi
store = "~/.bakon/repo"

[retention]
max_versions = 100           # 0 = unlimited

[output]
color = "auto"              # auto | always | never
format = "human"             # human | plain | json
```

Environment overrides include `BAKON_EDITOR`, `BAKON_STORE`, `BAKON_FORMAT`, `BAKON_COLOR`, and `BAKON_RETENTION_MAX_VERSIONS`. Per-file `hook` and `max_versions` settings live in the repository's `index.json`.

## Hooks

A hook runs after a new version is created by `edit` or `revert`, while the repository lock is still held. It receives:

```text
BAKON_FILE       absolute target path
BAKON_VERSION    new version number
BAKON_SOURCE     edit or revert
BAKON_PREV       previous version number; empty for the first version
```

Hook failure does not roll back a saved version and exits with code 5. Use `--no-hook` to skip hooks for an emergency operation.

## Exit Codes

| Code | Meaning |
|---:|---|
| 0 | Success |
| 1 | Diff found differences or generic failure |
| 2 | Usage or argument error |
| 3 | File/version not found, or version was pruned |
| 4 | Lock contention or state conflict |
| 5 | Version saved, but hook failed |
| 6 | Corrupt or inconsistent repository |
| 70 | Internal error |

Version numbers are never recycled by `prune`; pruned versions cannot be recovered. Reverting to content that already matches the current version does not create an empty version.

Design documents: [docs/design.md](docs/design.md) · [docs/new_design.md](docs/new_design.md)

## License

[MIT](LICENSE)

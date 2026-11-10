# Bakon

> Effortlessly keep version history while editing critical files.

[English](README.en.md) | [中文](README.md)

[![CI](https://github.com/RobiNexy/Bakon/actions/workflows/ci.yml/badge.svg)](https://github.com/RobiNexy/Bakon/actions/workflows/ci.yml)

Bakon is a single-machine command-line file version manager for individual developers. It lets your usual editor modify critical files such as `nginx.conf`, `hosts`, and crontabs in place while saving every change to a private local store. View history, compare versions, export snapshots, and roll back safely.

v1 is implemented in Rust with its own content-addressed store. It does not call git or any external command.

- Single machine, single user; no remotes, collaboration, or merge branches
- Integer versions that only increase; rollback appends a new version, so old versions stay reachable
- The target file is edited in place by your own editor; Bakon stores copies only
- Global retention limit, per-file hooks, atomic writes, write locking, and crash recovery
- One binary with no runtime dependencies

The current implementation is v1 (Rust). The 0.x Go implementation has been removed from the tree and remains only in history and old tags.

## Build and install

The toolchain is pinned by `rust-toolchain.toml`:

```sh
cargo build --release -p bakon
cargo test --workspace --all-targets

target/release/bakon version
cargo install --path crates/bakon
```

The v1 release pipeline is not wired up yet; build from source for now.

## Quick start

```sh
bakon init
bakon edit /etc/nginx/nginx.conf

bakon log /etc/nginx/nginx.conf
bakon diff /etc/nginx/nginx.conf
bakon show /etc/nginx/nginx.conf 1
bakon revert /etc/nginx/nginx.conf 1

bakon hook set /etc/nginx/nginx.conf "systemctl reload nginx"
bakon verify
```

The first `edit` tracks the file and saves its pre-edit content as version 0.

## Commands

| Command | Effect |
|---|---|
| `bakon init` | Create the store, or validate an existing one |
| `bakon version` | Print the program version and build target |
| `bakon edit <file>` | Open the editor; append a version when the content changed |
| `bakon log <file>` | List versions, newest first |
| `bakon ls` | List every tracked path, sorted |
| `bakon path <file>` | Show the path, identity, and manifest location |
| `bakon store <path\|size>` | Show the store root or disk usage |
| `bakon show <file> <ver>` | Write one version's raw content to stdout |
| `bakon dump <file> <ver>` | Same as `show`, kept as a pipeline-friendly name |
| `bakon diff <file> [v1] [v2]` | Compare versions; no arguments compares the last two, one compares with its predecessor |
| `bakon revert <file> <ver>` | Restore a version and record the restore as a new version; `--force` records the current content as `salvage` first |
| `bakon prune [<file>]` | Trim by the retention limit; `--dry-run` only reports |
| `bakon mv <old> <new>` | Change the recorded path only; the file on disk is untouched |
| `bakon hook set\|unset\|show <file> [cmd]` | Manage a per-file hook |
| `bakon config show\|init` | Show effective configuration or write a default file |
| `bakon verify` | Check consistency; `--fix` repairs derived data, `--size` reports usage |

Global options: `--store <dir>`, `--config <file>`, `--format human|plain|json`, `--lock-timeout <ms>` (default 2000).

`stdout` carries data only; diagnostics and progress go to `stderr`.

## Store

The default location is `~/.bakon/store`. Precedence:

1. `--store`
2. `BAKON_STORE`
3. the `store` field in the config file
4. `$BAKON_HOME/store`
5. `~/.bakon/store`

```text
store/
├── BAKON_STORE          ownership and schema marker
├── .lock                exclusive/shared lock
├── .pending             pending-apply marker (only when present)
├── index.json           path-to-identity cache; rebuildable
├── files/<id>.json      authoritative version manifests
├── objects/aa/<rest>    content-addressed objects (deflate)
├── objects/tmp/         write staging
└── trash/               objects awaiting deletion
```

Manifests are the only source of version facts. Objects are named by content hash and deduplicated. Back up by copying the whole store directory; run `bakon verify` after restoring. An inconsistent store exits 6; `--fix` repairs only size fields, the index, leftover directories, and orphan objects, never guessed history.

Crash recovery is driven by `.pending`, which records the target hash before write-back and the hash of the content to apply. If the target was changed from outside after a crash, recovery first records that content as `salvage`, then appends a new `revert` and applies it; `verify` reports unfinished write-backs.

v1 does not read 0.x's `~/.bakon/repo` and does not migrate automatically. The old directory and old binary can coexist; export with 0.x before switching if you need that history.

## Configuration

Config file locations, highest priority first:

1. `--config`
2. `BAKON_CONFIG`
3. `$BAKON_HOME/config.toml`
4. `$XDG_CONFIG_HOME/bakon/config.toml`
5. `~/.config/bakon/config.toml`

`bakon config init` writes the default file:

```toml
# store = "/absolute/path/to/store"
# editor = "vi"

[retention]
max_versions = 100   # 0 = unlimited
```

The editor is chosen as `$VISUAL` → `$EDITOR` → configured `editor` → `vi`.

## Hooks

A hook runs after the version is committed and after the lock is released, at most once. It receives:

```text
BAKON_FILE       absolute target path
BAKON_VERSION    new version number
BAKON_SOURCE     edit or revert
BAKON_PREV       previous version number; empty for the first version
BAKON_HOOK=1     prevents recursive hook execution
```

Hook failure does not roll back a saved version and exits with code 5. `--no-hook` skips the hook.

## Platform notes

- Windows has no directory `fsync` equivalent; crash recovery may lose the last commit, but never produces a mixed manifest/object state.
- The editor and hooks run through `sh -c` on Unix and `cmd /C` on Windows.
- The store supports local file systems only; locks and atomic replacement on network file systems are outside the guarantees.

## Exit codes

| Code | Meaning |
|---:|---|
| 0 | Success, no change, or `verify` found the store consistent |
| 1 | `diff` found differences; this is the only use of 1 |
| 2 | Usage, configuration, or argument error |
| 3 | File or version not found, or the version was pruned |
| 4 | Lock timeout, concurrent conflict, or path state conflict |
| 5 | Version saved, but the hook or editor failed |
| 6 | Corrupt or inconsistent store, not a bakon store, or schema mismatch |
| 7 | Version recorded, but writing back to the target file failed |
| 8 | Permission or general I/O error |
| 70 | Internal error |

## Design documents

- Entry point: [`design/MODULE__bakon-v1-design.md`](design/MODULE__bakon-v1-design.md)
- State machine, data formats, write protocol, and CLI contract: [`design/`](design/)
- Formal re-audit of the v1 design and implementation: [`docs/design-audit-v1-2026-11-10.md`](docs/design-audit-v1-2026-11-10.md)
- Recheck after the fixes: [`docs/design-audit-v1-2026-11-10-recheck.md`](docs/design-audit-v1-2026-11-10-recheck.md)
- The 0.x formal audit (historical evidence): [`docs/design-audit-2026-11-10.md`](docs/design-audit-2026-11-10.md)

## License

[MIT](LICENSE)

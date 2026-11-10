# bakon

bakon is a single-machine command-line file version manager. Edit critical files in place with your editor; every change is saved as an append-only version. You can list, compare, export, and revert versions. Version numbers only increase, and a revert appends a new version.

## Build and install

Rust is required; the toolchain is pinned by `rust-toolchain.toml`.

```sh
cargo build --release -p bakon
target/release/bakon version

# or install into cargo's bin directory
cargo install --path crates/bakon
```

You can also copy `target/release/bakon` into a directory on `PATH`.

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
| `bakon version` | Print the version and build target |
| `bakon edit <file>` | Open the editor; append a version when the content changed |
| `bakon log <file>` | List versions, newest first |
| `bakon ls` | List every tracked path |
| `bakon path <file>` | Show the path, identity, and manifest location |
| `bakon store <path\|size>` | Show the store root or disk usage |
| `bakon show <file> <ver>` | Write one version's raw content to stdout |
| `bakon dump <file> <ver>` | Same as `show`; kept for pipelines |
| `bakon diff <file> [v1] [v2]` | Compare versions; exits 1 when they differ |
| `bakon revert <file> <ver>` | Restore a version and append a new one; `--force` records the current content as `salvage` first |
| `bakon prune [<file>]` | Trim by the retention limit; `--dry-run` only reports |
| `bakon mv <old> <new>` | Change the recorded path only; the file on disk is untouched |
| `bakon hook set\|unset\|show <file> [cmd]` | Manage a per-file hook |
| `bakon config show\|init` | Show effective configuration or write a default file |
| `bakon verify` | Check consistency; `--fix` repairs, `--size` reports usage |

Global options: `--store <dir>`, `--config <file>`, `--format human|plain|json`, `--lock-timeout <ms>`.

`stdout` carries data only; diagnostics and progress go to `stderr`.

## Store and configuration

The default store is `~/.bakon/store`. Precedence:

1. `--store`
2. `BAKON_STORE`
3. the `store` field in the config file
4. `$BAKON_HOME/store`
5. `~/.bakon/store`

Config file locations:

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

Back up by copying the whole store directory; run `bakon verify` after restoring. `verify` exits 6 when the store is inconsistent.

## Hooks

A hook runs after the version is committed and after the lock is released, at most once. It receives:

```text
BAKON_FILE       absolute target path
BAKON_VERSION    new version number
BAKON_SOURCE     edit or revert
BAKON_PREV       previous version number; empty for the first version
BAKON_HOOK=1     prevents recursive hook execution
```

Hook failure does not roll back a saved version and exits 5. `--no-hook` skips the hook.

## Exit codes

| Code | Meaning |
|---:|---|
| 0 | Success, no change, or `verify` found the store consistent |
| 1 | `diff` found differences |
| 2 | Usage, configuration, or argument error |
| 3 | File or version not found, or the version was pruned |
| 4 | Lock timeout, concurrent conflict, or path state conflict |
| 5 | Version saved, but the hook or editor failed |
| 6 | Corrupt or inconsistent store, not a bakon store, or schema mismatch |
| 7 | Version recorded, but writing back to the target file failed |
| 8 | Permission or general I/O error |
| 70 | Internal error |

## License

[MIT](LICENSE)

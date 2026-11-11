# bakon user guide

bakon keeps version history for individual files. You edit a file in place with your usual editor; after each save bakon records a new version. Version numbers are increasing integers. Reverting does not delete history; it records a new version.

Terms used throughout:

- **Track**: make bakon manage a file for the first time and record its current content as version 0.
- **Version**: the file content after one save, identified by an integer; 0 is the baseline.
- **Store**: the directory where bakon keeps historical copies; by default `~/.bakon/store`.
- **Hook**: a command that runs after a version is saved.
- **Revert**: restore a file to an old version and append a new version.

## Find by goal

- Install bakon
- Make editing feel like your normal workflow
- Track a file for the first time
- View history, compare, and export old versions
- Revert to an old version
- Handle a file that was changed outside bakon
- Run a command after each save
- Control how many versions are kept and reclaim space
- Rename a tracked file
- List tracked files and find the store
- Change the editor, store location, or retention
- Check, back up, and restore the store
- Deal with errors

## Install

### Prerequisites

- Linux, macOS, or Windows.
- Rust only if you build from source; a prebuilt archive does not need Rust.
- A writable directory for the executable, such as `~/.local/bin`.

### Steps

1. Download the archive for your platform from Releases. Linux/macOS use `.tar.gz`; Windows uses `.zip`.

2. Extract it and put the executable in a directory on `PATH`. Linux example:

   ```sh
   tar -xzf bakon_*_linux-amd64.tar.gz
   mkdir -p "$HOME/.local/bin"
   install -m 755 bakon_*_linux-amd64/bakon "$HOME/.local/bin/bakon"
   ```

   Then run:

   ```sh
   "$HOME/.local/bin/bakon" version
   ```

   You should see a version and a target triple.

3. Make the current shell find it:

   ```sh
   export PATH="$HOME/.local/bin:$PATH"
   ```

   Then run:

   ```sh
   bakon version
   ```

   If you still get `command not found`, the directory is not on `PATH`.

4. Add the export to your shell startup file so it survives new terminals:

   ```sh
   # bash
   echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"

   # zsh
   echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.zshrc"
   ```

   Reopen the terminal, or run `source ~/.bashrc` (use `~/.zshrc` for zsh).

5. To install from source, run this in the repository root:

   ```sh
   cargo install --path crates/bakon
   ```

   The binary is usually at `~/.cargo/bin/bakon`, which is normally already on `PATH`.

6. For a system-wide location on Linux/macOS, use `/usr/local/bin`:

   ```sh
   sudo install -m 755 target/release/bakon /usr/local/bin/bakon
   ```

   This needs sudo. Avoid writing to `/bin`; it is usually managed by the system.

Windows: unzip the archive, add the folder containing `bakon.exe` to the system `Path`, reopen the terminal, and run `bakon version`.

### Common mistakes

- `bakon: command not found`: the executable is not on `PATH`. Run it by absolute path once, confirm it exists, then add its directory to `PATH`.
- `Permission denied`: the file is not executable. Run `chmod +x`, or reinstall with `install -m 755`.
- An old version still runs: another `bakon` comes first on `PATH`. Run `command -v bakon` to see which one.

### Done when

`bakon version` prints a result and `bakon --help` lists commands.

## Make editing feel like your normal workflow

### Prerequisites

- bakon is installed and on `PATH`.
- You already use an editor such as `vim`, `nano`, or `code`.

### Steps

1. Add a short alias for `bakon edit`:

   ```sh
   alias be='bakon edit'
   ```

2. Edit a file:

   ```sh
   be /etc/hosts
   ```

   This is the same as `bakon edit /etc/hosts`.

3. To pin an editor for an alias, add one with `--editor`:

   ```sh
   alias vbe='bakon edit --editor vim'
   alias nbe='bakon edit --editor nano'
   ```

   `vbe /etc/hosts` uses vim; `nbe /etc/hosts` uses nano.

4. Persist the alias:

   ```sh
   echo "alias be='bakon edit'" >> "$HOME/.bashrc"
   ```

### Common mistakes

- Do not use `alias vim='bakon edit'`. That shadows the real editor for every program and is easy to confuse. Use a new name such as `be` or `vbe`.
- Aliases affect the current shell. Put them in a startup file for new terminals.

### Done when

`be <file>` starts the bakon editing flow.

## Track a file for the first time

### Prerequisites

- bakon is available.
- The target exists and you can read and write it.
- It is a regular file, or a symlink to one.

### Steps

1. Initialize the store. You can also let the first `edit` do it; doing it explicitly is clearer:

   ```sh
   bakon init
   ```

   You should see `initialized`. Running it again prints `already initialized` and creates nothing new.

2. Open the file:

   ```sh
   bakon edit /etc/hosts
   ```

   The first run prints:

   ```text
   bakon: tracked /etc/hosts (baseline version 0)
   ```

   The editor then opens. Version 0 has already been saved with the pre-edit content.

3. Change the file, save, and exit.

   You should see:

   ```text
   bakon: saved version 1 of /etc/hosts
   ```

4. Confirm the history:

   ```sh
   bakon log /etc/hosts
   ```

   You should see version 1 (`edit`) and version 0 (`baseline`).

5. If you exit the editor without changes, you see:

   ```text
   bakon: no changes to /etc/hosts
   ```

   No empty version is created.

### Choose an editor

Use another editor for one invocation:

```sh
bakon edit /etc/hosts --editor nano
```

`--editor` affects only that run. You can also set an environment variable:

```sh
EDITOR=nano bakon edit /etc/hosts
```

The order is: `--editor`, then `$VISUAL`, then `$EDITOR`, then the config `editor`, then `vi`.

### Common mistakes

- The file does not exist: create it first, or check the path.
- The target is a directory, device, or pipe: bakon refuses; it only manages regular files.
- The editor does not open: set `--editor`, or check whether `$EDITOR` points to a missing command.
- Permission denied: the editor may open, but saving or writing back fails. Check file permissions.

### Done when

The file is tracked and `bakon log <file>` shows at least version 0; after a change it also shows version 1.

## View history, compare, and export old versions

### Prerequisites

- The file is tracked.

### Steps

1. Show history:

   ```sh
   bakon log ./app.conf
   ```

   Versions are newest first. `source` says whether a version came from the initial baseline, a normal edit, a revert, or a protective `salvage`.

2. Show one version's raw content:

   ```sh
   bakon show ./app.conf 0
   ```

   The content goes to the terminal. `bakon dump ./app.conf 0` does the same and is handy in pipelines.

3. Export an old version to a file:

   ```sh
   bakon show ./app.conf 0 > app.conf.v0
   ```

   The shell does the redirection; bakon adds no extra newline.

4. Compare the last two versions:

   ```sh
   bakon diff ./app.conf
   ```

   Lines starting with `-` are old; `+` are new.

5. Compare two specific versions:

   ```sh
   bakon diff ./app.conf 0 1
   ```

6. When versions differ, `diff` exits 1:

   ```sh
   bakon diff ./app.conf
   echo $?
   ```

   That means "differences found", not a failure. It exits 0 when identical and 3 when a version is missing.

### Common mistakes

- Treating exit 1 as a command error: it is the normal "different" result; handle it separately in scripts.
- Comparing the last two versions when only one exists: bakon reports that there is no previous version. Save another version first.
- Binary content may look garbled in the terminal; redirect it to a file.
- After pruning, `show` on an old version reports it was pruned. Pruned versions are gone.

### Done when

You can list versions, export any retained version, and distinguish "different" from "failed".

## Revert to an old version

### Prerequisites

- The file is tracked.
- The target version is still retained.

### Steps

1. Check the version number:

   ```sh
   bakon log ./app.conf
   ```

2. Revert:

   ```sh
   bakon revert ./app.conf 0
   ```

   You should see:

   ```text
   bakon: reverted ./app.conf to version 0 as version N
   ```

3. Check the file content.

4. Check history:

   ```sh
   bakon log ./app.conf
   ```

   The newest entry is `revert`; old versions are still there. Reverting does not delete history.

### Common mistakes

- The file on disk differs from the newest recorded version: bakon refuses with exit 4. To overwrite, add `--force`; it records the current content as `salvage` first.
- The target version was pruned: exit 3. Restore it from a backup or another copy.
- The file is missing: `revert` recreates it.
- The path is a symlink: bakon writes the real target and keeps the link.

### Done when

The file content equals the target version; history has a new `revert` entry, and old versions remain readable.

## Handle a file that was changed outside bakon

### Prerequisites

- The file is tracked.
- You or another program modified it without bakon.

### Steps

1. Edit again:

   ```sh
   bakon edit ./app.conf
   ```

   The editor opens the current disk content. Saving records it as a new version, so it is not lost.

2. If you revert directly:

   ```sh
   bakon revert ./app.conf 0
   ```

   The current content differs from the newest recorded version, so bakon refuses with exit 4.

3. To discard the current content and revert:

   ```sh
   bakon revert ./app.conf 0 --force
   ```

   The current content is recorded as a `salvage` version before the target version is applied.

4. Check history to confirm the old content is still available:

   ```sh
   bakon log ./app.conf
   ```

   The `salvage` entry is the content from before the overwrite.

### Common mistakes

- Do not delete the file by hand just to avoid the conflict; `--force` with `salvage` is the recoverable path.
- `salvage` counts toward retention and may be pruned later.

### Done when

The external content is either a readable version or saved as `salvage`; no content disappears without a record.

## Run a command after each save

### Prerequisites

- The file is tracked.
- The hook command can run in bakon's current shell environment.

### Steps

1. Set a hook:

   ```sh
   bakon hook set ./nginx.conf 'systemctl reload nginx'
   ```

   For a demo, write a log instead:

   ```sh
   bakon hook set ./nginx.conf 'echo reloaded >> hook.log'
   ```

2. Edit the file:

   ```sh
   bakon edit ./nginx.conf
   ```

   The hook runs after the version is saved.

3. Show the hook:

   ```sh
   bakon hook show ./nginx.conf
   ```

4. Skip the hook once:

   ```sh
   bakon edit ./nginx.conf --no-hook
   ```

5. Remove the hook:

   ```sh
   bakon hook unset ./nginx.conf
   ```

The hook receives:

```text
BAKON_FILE       absolute target path
BAKON_VERSION    new version number
BAKON_SOURCE     edit or revert
BAKON_PREV       previous version number; empty for the first version
BAKON_HOOK=1     set while inside a hook, to prevent recursion
```

### Common mistakes

- The hook fails: the version is already saved, but the command exits 5. Fix the hook; the version is safe.
- A hook calls bakon again on the same file: the child sees `BAKON_HOOK=1` and skips its hook. Do not rely on this for unbounded recursion.
- The hook did not run: check `--no-hook` and whether `BAKON_HOOK` is already set.
- Hooks run at most once; an interrupted hook is not retried.

### Done when

Each successful save runs the hook as expected; failures are visible through exit 5 and stderr.

## Control how many versions are kept and reclaim space

### Prerequisites

- The file is tracked.
- You understand that pruning permanently deletes old versions beyond the limit.

### Steps

1. Show the current configuration:

   ```sh
   bakon config show
   ```

2. Create a config file if needed:

   ```sh
   bakon config init
   ```

3. Edit `max_versions`. To keep 3 versions:

   ```toml
   [retention]
   max_versions = 3
   ```

   `0` means unlimited.

4. Confirm the setting:

   ```sh
   bakon config show
   ```

5. Preview pruning:

   ```sh
   bakon prune --dry-run
   ```

   It only reports what would be removed.

6. Prune:

   ```sh
   bakon prune
   ```

   Or prune one file:

   ```sh
   bakon prune ./nginx.conf
   ```

7. Check the result:

   ```sh
   bakon log ./nginx.conf
   bakon store size
   ```

   The version count is within the limit and storage usage drops.

### Common mistakes

- Pruning cannot be undone. Export anything you need with `show` first.
- Version numbers are not recycled; reverting to a pruned number reports that it was pruned.
- `max_versions = 0` means unlimited, not zero versions.
- Changing the config does not immediately shrink existing files; the next commit or a manual `prune` applies it.

### Done when

`bakon log` shows only the retained versions and `bakon verify` still reports a consistent store.

## Rename a tracked file

### Prerequisites

- The file is tracked.
- No other tracked file already uses the new path.

### Steps

1. Change only bakon's record:

   ```sh
   bakon mv ./old.conf ./new.conf
   ```

   The file on disk is not moved.

2. If you also want the disk file renamed, do it yourself:

   ```sh
   mv old.conf new.conf
   ```

3. Check history under the new path:

   ```sh
   bakon log ./new.conf
   ```

4. The old path is no longer tracked:

   ```sh
   bakon log ./old.conf
   ```

   It reports not managed and exits 3.

5. If you recreate the old path and track it again, it gets a new identity and history:

   ```sh
   printf 'new file\n' > old.conf
   bakon edit ./old.conf
   bakon path ./old.conf
   bakon path ./new.conf
   ```

   The two `path` outputs show different ids.

### Common mistakes

- Editing the new path before moving the disk file reports that the file does not exist. Move the file first, or edit the old path before `mv`.
- The new path is already used by another tracked file: the command refuses, preventing shared history.

### Done when

The recorded path is the new one; the old path is untracked; a re-tracked old path is a separate file.

## List tracked files and find the store

### Prerequisites

- The store is initialized.

### Steps

1. List tracked paths:

   ```sh
   bakon ls
   ```

2. Show a file's path, id, and manifest location:

   ```sh
   bakon path ./app.conf
   ```

3. Show the store root:

   ```sh
   bakon store path
   ```

4. Show disk usage:

   ```sh
   bakon store size
   ```

5. Use JSON in scripts:

   ```sh
   bakon --format json ls
   bakon --format json path ./app.conf
   bakon --format json store size
   ```

### Common mistakes

- `ls` is empty: no files are tracked in this store yet.
- `path` reports not managed: check the spelling, current directory, and `--store`.
- Paths look identical: on case-insensitive systems bakon treats them as one path; use `bakon path` to confirm the id.

### Done when

You know which files are tracked, where their history lives, and how much space the store uses.

## Change the editor, store location, or retention

### Prerequisites

- You know whether the change is global or only for one command.

### Steps

1. Write a default config:

   ```sh
   bakon config init
   ```

   It does not overwrite an existing file.

2. Show the config path:

   ```sh
   bakon config show
   ```

3. Edit fields:

   ```toml
   store = "/absolute/path/to/store"
   editor = "nano"

   [retention]
   max_versions = 100
   ```

4. Check effective values:

   ```sh
   bakon config show
   ```

5. Override the store for one command:

   ```sh
   bakon --store /tmp/other-store log ./app.conf
   ```

6. Override the editor for one command:

   ```sh
   bakon edit ./app.conf --editor nano
   ```

Lookup order:

- Store: `--store` → `BAKON_STORE` → config `store` → `$BAKON_HOME/store` → `~/.bakon/store`.
- Config: `--config` → `BAKON_CONFIG` → `$BAKON_HOME/config.toml` → `$XDG_CONFIG_HOME/bakon/config.toml` → `~/.config/bakon/config.toml`.
- Editor: `--editor` → `$VISUAL` → `$EDITOR` → config `editor` → `vi`.

### Common mistakes

- A relative `store` points to different places from different directories; use an absolute path.
- After changing `store`, old history is not visible: that is a different directory. Switch back.
- A changed `editor` has no effect: `$VISUAL`/`$EDITOR` take precedence; use `--editor` to confirm.

### Done when

`bakon config show` reflects the intended store, editor, and retention values.

## Check, back up, and restore the store

### Prerequisites

- You know the store path (`bakon store path`).

### Steps

1. Check consistency:

   ```sh
   bakon verify
   ```

   It prints `store is consistent` and exits 0 when consistent.

2. Show usage details:

   ```sh
   bakon verify --size
   ```

3. If `verify` reports index or leftover issues, back up first:

   ```sh
   cp -a "$(bakon store path)" "$(bakon store path).backup"
   bakon verify --fix
   bakon verify
   ```

4. Back up by copying the whole store directory.

5. Restore by copying the backup back, or point `--store` at it:

   ```sh
   bakon --store /path/to/store.backup verify
   ```

6. If the index file is lost, do not rebuild it by hand; `bakon log` still works and `bakon verify --fix` rebuilds it.

### If a revert was interrupted

If the process is killed while writing a revert, the next write command completes the write-back. If you edited the target by hand in the meantime, bakon first records that content as `salvage`, then finishes the revert. Run:

```sh
bakon verify --fix
bakon log ./app.conf
```

Confirm the `salvage` or `revert` entry exists.

### Common mistakes

- Exit 6 from `verify` does not mean everything is lost. Back up first and follow the report; do not delete the store.
- Copying only `files/` or `objects/` gives an incomplete history; copy the whole store.
- Do not edit manifests or objects by hand while bakon is running; use `verify --fix`.

### Done when

`bakon verify` is consistent and you have a standalone store copy.

## Deal with errors

| Symptom | Meaning | What to do |
|---|---|---|
| Exit 3 | File not tracked, version missing, or version pruned | Run `bakon log` to check the path and version |
| Exit 4 | Lock timeout, concurrent conflict, or file changed outside | Retry later; use `revert --force` if overwriting is intended |
| Exit 5 | Version saved, but the hook or editor failed | Fix the command or editor; the version is safe |
| Exit 6 | Corrupt or inconsistent store, not a bakon store, or schema mismatch | Back up the whole store, then use `verify --fix` or follow the report |
| Exit 7 | Version recorded, but writing the target failed | Check permissions, read-only mounts, or disk space, then retry |
| Exit 8 | Permission or general I/O error | Check directory permissions and disk health |
| Exit 9 | Version written and visible, but the parent directory `fsync` failed, so durability is unconfirmed | Do not retry blindly; use `bakon log` / `bakon verify` to confirm the visible state and back up; a power loss may lose that version |
| `path is not valid UTF-8` | The target path is not UTF-8 | Use a UTF-8 path; bakon stores paths as text |
| `path contains '..'` | The path contains `..`; lexical folding can differ from kernel resolution | Use a resolved absolute path without `..` |

Scripts should rely on exit codes and `--format json` output, not on error message text.

## Stop using or uninstall

1. Back up the store if you may need the history later.
2. Remove the executable, for example:

   ```sh
   rm "$HOME/.local/bin/bakon"
   ```

3. Remove the store and config:

   ```sh
   rm -rf "$HOME/.bakon"
   rm -rf "$HOME/.config/bakon"
   ```

   This permanently deletes history. Check the paths first.

//! 端到端测试：真实二进制、真实文件系统、真实退出码。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn run(args: &[&str]) -> Output {
    run_with_env(args, &[])
}

fn run_with_env(args: &[&str], environment: &[(&str, &Path)]) -> Output {
    let isolated_config = std::env::temp_dir().join("bakon-tests-no-config.toml");
    let mut command = Command::new(env!("CARGO_BIN_EXE_bakon"));
    command
        .env_remove("BAKON_STORE")
        .env_remove("BAKON_HOME")
        .env_remove("VISUAL")
        .env("BAKON_CONFIG", &isolated_config)
        .args(args);
    for (key, value) in environment {
        command.env(key, value);
    }
    command.output().expect("failed to run bakon")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// 写一个把内容替换为 `body` 的编辑器脚本；路径以 `$1` 传入。
fn editor_script(directory: &Path, body: &str) -> PathBuf {
    editor_script_named(directory, "editor.sh", body)
}

fn editor_script_named(directory: &Path, name: &str, body: &str) -> PathBuf {
    let path = directory.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

fn write_config(directory: &Path, name: &str, max_versions: u32) -> PathBuf {
    let path = directory.join(name);
    fs::write(
        &path,
        format!("[retention]\nmax_versions = {max_versions}\n"),
    )
    .unwrap();
    path
}

struct Fixture {
    _temporary: TempDir,
    store: PathBuf,
    file: PathBuf,
}

impl Fixture {
    fn new(content: &[u8]) -> Self {
        let temporary = TempDir::new().unwrap();
        let store = temporary.path().join("store");
        let file = temporary.path().join("note.txt");
        fs::write(&file, content).unwrap();
        Self {
            _temporary: temporary,
            store,
            file,
        }
    }

    fn store(&self) -> &str {
        self.store.to_str().unwrap()
    }

    fn file(&self) -> &str {
        self.file.to_str().unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut full = vec!["--store", self.store()];
        full.extend_from_slice(args);
        run(&full)
    }

    fn edit(&self, editor: &Path) -> Output {
        let mut full = vec!["--store", self.store(), "edit", self.file()];
        let output = run_with_env(&full, &[("EDITOR", editor)]);
        full.clear();
        output
    }

    fn edit_with_config(&self, config: &Path, editor: &Path) -> Output {
        let full = vec![
            "--store",
            self.store(),
            "--config",
            config.to_str().unwrap(),
            "edit",
            self.file(),
        ];
        run_with_env(&full, &[("EDITOR", editor)])
    }

    fn run_config(&self, config: &Path, args: &[&str]) -> Output {
        let mut full = vec![
            "--store",
            self.store(),
            "--config",
            config.to_str().unwrap(),
        ];
        full.extend_from_slice(args);
        run(&full)
    }

    fn temp(&self) -> &Path {
        self._temporary.path()
    }
}

#[test]
fn version_reports_the_build_target() {
    let output = run(&["version"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).starts_with("bakon 1.0.0 ("),
        "stdout: {}",
        stdout(&output)
    );
}

#[test]
fn version_supports_json() {
    let output = run(&["--format", "json", "version"]);
    let text = stdout(&output);

    assert_eq!(output.status.code(), Some(0));
    assert!(text.contains("\"version\":\"1.0.0\""), "stdout: {text}");
    assert!(text.contains("\"target\":\""), "stdout: {text}");
}

#[test]
fn init_creates_a_store_and_is_idempotent() {
    let temporary = TempDir::new().unwrap();
    let store = temporary.path().join("store");
    let store_arg = store.to_str().unwrap();

    let first = run(&["init", "--store", store_arg]);
    assert_eq!(first.status.code(), Some(0), "stderr: {}", stderr(&first));
    assert!(stdout(&first).starts_with("initialized "));
    assert!(store.join("BAKON_STORE").is_file());
    assert!(store.join("objects/tmp").is_dir());

    let second = run(&["init", "--store", store_arg]);
    assert_eq!(second.status.code(), Some(0), "stderr: {}", stderr(&second));
    assert!(stdout(&second).starts_with("already initialized: "));
}

#[test]
fn init_refuses_a_foreign_non_empty_directory() {
    let temporary = TempDir::new().unwrap();
    let store = temporary.path().join("store");
    fs::create_dir(&store).unwrap();
    fs::write(store.join("notes.txt"), b"someone else's data").unwrap();

    let output = run(&["init", "--store", store.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(6), "stdout: {}", stdout(&output));
    assert!(
        stderr(&output).contains("not a bakon store"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(!Path::new(&store).join("BAKON_STORE").exists());
}

#[test]
fn unknown_subcommands_exit_with_usage_code() {
    let output = run(&["frobnicate"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(!stderr(&output).is_empty());
}

#[test]
fn edit_tracks_then_records_the_editor_result() {
    let fixture = Fixture::new(b"first");
    let editor = editor_script(
        Path::new(fixture.store()).parent().unwrap(),
        "printf 'second' > \"$1\"",
    );

    let output = fixture.edit(&editor);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("tracked"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("saved version 1"),
        "stderr: {}",
        stderr(&output)
    );

    let log = fixture.run(&["--format", "plain", "log", fixture.file()]);
    assert_eq!(log.status.code(), Some(0), "stderr: {}", stderr(&log));
    let log_text = stdout(&log);
    let lines: Vec<&str> = log_text.lines().collect();
    assert_eq!(lines.len(), 2, "stdout: {log_text}");
    assert!(
        lines[0].starts_with("1\t") && lines[0].ends_with("\tedit"),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("0\t") && lines[1].ends_with("\tbaseline"),
        "{}",
        lines[1]
    );

    let baseline = fixture.run(&["show", fixture.file(), "0"]);
    assert_eq!(baseline.stdout, b"first");

    let latest = fixture.run(&["show", fixture.file(), "1"]);
    assert_eq!(latest.stdout, b"second");
}

#[test]
fn edit_without_changes_creates_no_version() {
    let fixture = Fixture::new(b"unchanged");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 0");

    let output = fixture.edit(&editor);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("no changes"),
        "stderr: {}",
        stderr(&output)
    );
    let log = fixture.run(&["--format", "plain", "log", fixture.file()]);
    assert_eq!(stdout(&log).lines().count(), 1);
}

#[test]
fn diff_exits_one_on_text_changes() {
    let fixture = Fixture::new(b"alpha\n");
    let editor = editor_script(
        Path::new(fixture.store()).parent().unwrap(),
        "printf 'beta\\n' > \"$1\"",
    );
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let output = fixture.run(&["diff", fixture.file()]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("-alpha"), "stdout: {text}");
    assert!(text.contains("+beta"), "stdout: {text}");
}

#[test]
fn diff_with_identical_content_exits_zero() {
    let fixture = Fixture::new(b"same\n");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 0");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let output = fixture.run(&["diff", fixture.file(), "0", "0"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(stdout(&output).is_empty());
}

#[test]
fn diff_reports_binary_files_without_printing_bytes() {
    let fixture = Fixture::new(b"a\0b");
    let editor = editor_script(
        Path::new(fixture.store()).parent().unwrap(),
        "printf 'c\\0d' > \"$1\"",
    );
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let output = fixture.run(&["diff", fixture.file()]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("binary files differ"));
    assert!(stdout(&output).is_empty());
}

#[test]
fn dump_matches_show() {
    let fixture = Fixture::new(b"payload");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 0");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let shown = fixture.run(&["show", fixture.file(), "0"]);
    let dumped = fixture.run(&["dump", fixture.file(), "0"]);

    assert_eq!(shown.stdout, b"payload");
    assert_eq!(dumped.stdout, shown.stdout);
}

#[test]
fn log_on_an_untracked_file_exits_three() {
    let fixture = Fixture::new(b"never tracked");

    let output = fixture.run(&["log", fixture.file()]);

    assert_eq!(output.status.code(), Some(3), "stdout: {}", stdout(&output));
    assert!(
        stderr(&output).contains("not managed"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn showing_a_missing_version_exits_three() {
    let fixture = Fixture::new(b"content");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 0");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let output = fixture.run(&["show", fixture.file(), "9"]);

    assert_eq!(output.status.code(), Some(3), "stdout: {}", stdout(&output));
    assert!(
        stderr(&output).contains("does not exist"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn diff_with_a_single_version_exits_three() {
    let fixture = Fixture::new(b"only");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 0");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let output = fixture.run(&["diff", fixture.file()]);

    assert_eq!(output.status.code(), Some(3), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("no previous version"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn a_failing_editor_exits_five_and_keeps_the_baseline() {
    let fixture = Fixture::new(b"kept");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 7");

    let output = fixture.edit(&editor);

    assert_eq!(output.status.code(), Some(5), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("editor failed"),
        "stderr: {}",
        stderr(&output)
    );
    let log = fixture.run(&["--format", "plain", "log", fixture.file()]);
    assert_eq!(stdout(&log).lines().count(), 1);
}

#[test]
fn editing_a_directory_exits_three() {
    let fixture = Fixture::new(b"x");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 0");
    let directory = fixture.file.parent().unwrap().join("folder");
    fs::create_dir(&directory).unwrap();

    let directory = directory.to_str().unwrap();
    let output = run_with_env(
        &["--store", fixture.store(), "edit", directory],
        &[("EDITOR", &editor)],
    );

    assert_eq!(output.status.code(), Some(3), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("not a regular file"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn revert_restores_content_and_appends_a_version() {
    let fixture = Fixture::new(b"one");
    let editor = editor_script(
        Path::new(fixture.store()).parent().unwrap(),
        "printf 'two' > \"$1\"",
    );
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let output = fixture.run(&["revert", fixture.file(), "0"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("as version 2"),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(fs::read(&fixture.file).unwrap(), b"one");

    let log = fixture.run(&["--format", "plain", "log", fixture.file()]);
    let text = stdout(&log);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert!(
        lines[0].starts_with("2\t") && lines[0].ends_with("\trevert"),
        "{}",
        lines[0]
    );
}

#[test]
fn revert_refuses_external_divergence_without_force() {
    let fixture = Fixture::new(b"one");
    let editor = editor_script(
        Path::new(fixture.store()).parent().unwrap(),
        "printf 'two' > \"$1\"",
    );
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));
    fs::write(&fixture.file, b"outside edit").unwrap();

    let output = fixture.run(&["revert", fixture.file(), "0"]);

    assert_eq!(output.status.code(), Some(4), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("diverged"),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(fs::read(&fixture.file).unwrap(), b"outside edit");
}

#[test]
fn revert_force_records_salvage_before_restoring() {
    let fixture = Fixture::new(b"one");
    let editor = editor_script(
        Path::new(fixture.store()).parent().unwrap(),
        "printf 'two' > \"$1\"",
    );
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));
    fs::write(&fixture.file, b"outside edit").unwrap();

    let output = fixture.run(&["revert", fixture.file(), "0", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(fs::read(&fixture.file).unwrap(), b"one");

    let log = fixture.run(&["--format", "plain", "log", fixture.file()]);
    let text = stdout(&log);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4, "{text}");
    assert!(lines[0].ends_with("\trevert"), "{}", lines[0]);
    assert!(lines[1].ends_with("\tsalvage"), "{}", lines[1]);

    let salvaged = fixture.run(&["show", fixture.file(), "2"]);
    assert_eq!(salvaged.stdout, b"outside edit");
}

#[cfg(unix)]
#[test]
fn revert_preserves_a_symlink() {
    let fixture = Fixture::new(b"one");
    let target = fixture.file.clone();
    let link = target.parent().unwrap().join("link.txt");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let editor = editor_script(
        Path::new(fixture.store()).parent().unwrap(),
        "printf 'two' > \"$1\"",
    );
    let link_arg = link.to_str().unwrap();
    let edited = run_with_env(
        &["--store", fixture.store(), "edit", link_arg],
        &[("EDITOR", &editor)],
    );
    assert_eq!(edited.status.code(), Some(0), "stderr: {}", stderr(&edited));

    let output = fixture.run(&["revert", link_arg, "0"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&target).unwrap(), b"one");
}

#[test]
fn hook_runs_after_commit_outside_the_lock() {
    let fixture = Fixture::new(b"one");
    let directory = fixture.temp();
    let no_change = editor_script_named(directory, "noop.sh", "exit 0");
    assert_eq!(fixture.edit(&no_change).status.code(), Some(0));

    let record = directory.join("hook.log");
    let hook = format!(
        "printf '%s %s\\n' \"$BAKON_VERSION\" \"$BAKON_SOURCE\" >> {}; {} --store {} log {} >/dev/null",
        record.display(),
        env!("CARGO_BIN_EXE_bakon"),
        fixture.store(),
        fixture.file()
    );
    let set = fixture.run(&["hook", "set", fixture.file(), &hook]);
    assert_eq!(set.status.code(), Some(0), "stderr: {}", stderr(&set));

    let editor = editor_script(directory, "printf 'two' > \"$1\"");
    let output = fixture.edit(&editor);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(fs::read_to_string(&record).unwrap().trim(), "1 edit");
}

#[test]
fn a_failing_hook_exits_five_and_keeps_the_version() {
    let fixture = Fixture::new(b"one");
    let directory = fixture.temp();
    let no_change = editor_script_named(directory, "noop.sh", "exit 0");
    assert_eq!(fixture.edit(&no_change).status.code(), Some(0));
    assert_eq!(
        fixture
            .run(&["hook", "set", fixture.file(), "exit 9"])
            .status
            .code(),
        Some(0)
    );

    let editor = editor_script(directory, "printf 'two' > \"$1\"");
    let output = fixture.edit(&editor);

    assert_eq!(output.status.code(), Some(5), "stderr: {}", stderr(&output));
    let log = fixture.run(&["--format", "plain", "log", fixture.file()]);
    assert_eq!(stdout(&log).lines().count(), 2);
}

#[test]
fn hook_can_be_shown_and_unset() {
    let fixture = Fixture::new(b"one");
    let editor = editor_script(fixture.temp(), "exit 0");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));
    assert_eq!(
        fixture
            .run(&["hook", "set", fixture.file(), "true"])
            .status
            .code(),
        Some(0)
    );

    let shown = fixture.run(&["hook", "show", fixture.file()]);
    assert_eq!(stdout(&shown).trim(), "true");

    assert_eq!(
        fixture
            .run(&["hook", "unset", fixture.file()])
            .status
            .code(),
        Some(0)
    );
    let shown = fixture.run(&["hook", "show", fixture.file()]);
    assert_eq!(stdout(&shown).trim(), "(not set)");
}

#[test]
fn prune_trims_old_versions_and_dry_run_changes_nothing() {
    let fixture = Fixture::new(b"one");
    let directory = fixture.temp();
    let unlimited = write_config(directory, "unlimited.toml", 0);
    let limited = write_config(directory, "limited.toml", 2);

    let second = editor_script_named(directory, "second.sh", "printf 'two' > \"$1\"");
    let third = editor_script_named(directory, "third.sh", "printf 'three' > \"$1\"");
    assert_eq!(
        fixture.edit_with_config(&unlimited, &second).status.code(),
        Some(0)
    );
    assert_eq!(
        fixture.edit_with_config(&unlimited, &third).status.code(),
        Some(0)
    );

    let dry = fixture.run_config(&limited, &["prune", fixture.file(), "--dry-run"]);
    assert_eq!(dry.status.code(), Some(0), "stderr: {}", stderr(&dry));
    assert!(
        stdout(&dry).contains("would prune"),
        "stdout: {}",
        stdout(&dry)
    );
    let log = fixture.run_config(&limited, &["--format", "plain", "log", fixture.file()]);
    assert_eq!(stdout(&log).lines().count(), 3);

    let real = fixture.run_config(&limited, &["prune", fixture.file()]);
    assert_eq!(real.status.code(), Some(0), "stderr: {}", stderr(&real));
    assert!(
        stdout(&real).contains("pruned 1 version(s)"),
        "stdout: {}",
        stdout(&real)
    );

    let log = fixture.run_config(&limited, &["--format", "plain", "log", fixture.file()]);
    assert_eq!(stdout(&log).lines().count(), 2);
    let pruned = fixture.run(&["show", fixture.file(), "0"]);
    assert_eq!(pruned.status.code(), Some(3));
}

#[test]
fn prune_keeps_objects_shared_with_other_files() {
    let fixture = Fixture::new(b"shared");
    let directory = fixture.temp();
    let unlimited = write_config(directory, "unlimited.toml", 0);
    let limited = write_config(directory, "limited.toml", 2);

    let second = editor_script_named(directory, "second.sh", "printf 'a2' > \"$1\"");
    let third = editor_script_named(directory, "third.sh", "printf 'a3' > \"$1\"");
    assert_eq!(
        fixture.edit_with_config(&unlimited, &second).status.code(),
        Some(0)
    );
    assert_eq!(
        fixture.edit_with_config(&unlimited, &third).status.code(),
        Some(0)
    );

    let other = directory.join("other.txt");
    fs::write(&other, b"shared").unwrap();
    let no_change = editor_script_named(directory, "noop.sh", "exit 0");
    let tracked = run_with_env(
        &[
            "--store",
            fixture.store(),
            "--config",
            unlimited.to_str().unwrap(),
            "edit",
            other.to_str().unwrap(),
        ],
        &[("EDITOR", &no_change)],
    );
    assert_eq!(
        tracked.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&tracked)
    );

    let real = fixture.run_config(&limited, &["prune", fixture.file()]);
    assert_eq!(real.status.code(), Some(0), "stderr: {}", stderr(&real));

    let shown = fixture.run(&["show", other.to_str().unwrap(), "0"]);
    assert_eq!(shown.stdout, b"shared");
}

#[test]
fn mv_preserves_history_and_retracking_does_not_alias() {
    let fixture = Fixture::new(b"one");
    let directory = fixture.temp();
    let editor = editor_script(directory, "printf 'two' > \"$1\"");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));
    let renamed = directory.join("renamed.txt");

    let moved = fixture.run(&["mv", fixture.file(), renamed.to_str().unwrap()]);
    assert_eq!(moved.status.code(), Some(0), "stderr: {}", stderr(&moved));

    let log = fixture.run(&["--format", "plain", "log", renamed.to_str().unwrap()]);
    assert_eq!(stdout(&log).lines().count(), 2);
    let old = fixture.run(&["log", fixture.file()]);
    assert_eq!(old.status.code(), Some(3));

    let no_change = editor_script_named(directory, "noop.sh", "exit 0");
    let retracked = run_with_env(
        &["--store", fixture.store(), "edit", fixture.file()],
        &[("EDITOR", &no_change)],
    );
    assert_eq!(
        retracked.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&retracked)
    );

    let old = fixture.run(&["--format", "plain", "log", fixture.file()]);
    assert_eq!(stdout(&old).lines().count(), 1);
    let new = fixture.run(&["--format", "plain", "log", renamed.to_str().unwrap()]);
    assert_eq!(stdout(&new).lines().count(), 2);
}

#[test]
fn verify_reports_and_fixes_derived_data() {
    let fixture = Fixture::new(b"one");
    let editor = editor_script(fixture.temp(), "printf 'two' > \"$1\"");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    fs::write(fixture.store.join("index.json"), b"broken").unwrap();
    fs::write(fixture.store.join("objects/tmp/leftover"), b"x").unwrap();

    let broken = fixture.run(&["verify"]);
    assert_eq!(broken.status.code(), Some(6), "stderr: {}", stderr(&broken));
    let text = stdout(&broken);
    assert!(text.contains("index does not match"), "{text}");
    assert!(text.contains("leftover"), "{text}");

    let fixed = fixture.run(&["verify", "--fix"]);
    assert_eq!(fixed.status.code(), Some(0), "stderr: {}", stderr(&fixed));
    assert!(
        stdout(&fixed).contains("[fixed]"),
        "stdout: {}",
        stdout(&fixed)
    );

    let clean = fixture.run(&["verify"]);
    assert_eq!(clean.status.code(), Some(0));
    assert!(
        stdout(&clean).contains("store is consistent"),
        "stdout: {}",
        stdout(&clean)
    );
}

#[test]
fn verify_detects_a_missing_object() {
    let fixture = Fixture::new(b"one");
    let editor = editor_script(fixture.temp(), "printf 'two' > \"$1\"");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let objects = fixture.store.join("objects");
    let shard = fs::read_dir(&objects)
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| entry.file_name() != "tmp")
        .unwrap()
        .path();
    let victim = fs::read_dir(&shard)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::remove_file(&victim).unwrap();

    let output = fixture.run(&["verify"]);

    assert_eq!(output.status.code(), Some(6), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("missing_blob"),
        "stdout: {}",
        stdout(&output)
    );
}

#[test]
fn verify_size_reports_object_bytes() {
    let fixture = Fixture::new(b"one");
    let editor = editor_script(fixture.temp(), "printf 'two' > \"$1\"");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let output = fixture.run(&["--format", "json", "verify", "--size"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("\"unique_object_bytes\":"), "{text}");
    assert!(text.contains("\"checked_files\":1"), "{text}");
}

#[test]
fn config_init_and_show_are_usable() {
    let temporary = TempDir::new().unwrap();
    let path = temporary.path().join("config.toml");
    let path_arg = path.to_str().unwrap();

    let init = run(&["--config", path_arg, "config", "init"]);
    assert_eq!(init.status.code(), Some(0), "stderr: {}", stderr(&init));
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("max_versions = 100")
    );

    let again = run(&["--config", path_arg, "config", "init"]);
    assert!(stdout(&again).contains("already exists"));

    let show = run(&["--config", path_arg, "--format", "json", "config", "show"]);
    assert_eq!(show.status.code(), Some(0), "stderr: {}", stderr(&show));
    let text = stdout(&show);
    assert!(text.contains("\"max_versions\":100"), "{text}");
    assert!(text.contains(path_arg), "{text}");
}

#[test]
fn global_retention_trims_during_commit() {
    let fixture = Fixture::new(b"one");
    let directory = fixture.temp();
    let config = write_config(directory, "config.toml", 2);
    let second = editor_script_named(directory, "second.sh", "printf 'two' > \"$1\"");
    let third = editor_script_named(directory, "third.sh", "printf 'three' > \"$1\"");

    assert_eq!(
        fixture.edit_with_config(&config, &second).status.code(),
        Some(0)
    );
    assert_eq!(
        fixture.edit_with_config(&config, &third).status.code(),
        Some(0)
    );

    let log = fixture.run_config(&config, &["--format", "plain", "log", fixture.file()]);
    let text = stdout(&log);
    assert_eq!(text.lines().count(), 2, "{text}");
    let oldest = fixture.run(&["show", fixture.file(), "0"]);
    assert_eq!(oldest.status.code(), Some(3));
}

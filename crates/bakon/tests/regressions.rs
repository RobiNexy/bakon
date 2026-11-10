//! 审计缺陷的回归用例。
//!
//! 每条对应 `docs/design-audit-2026-11-10.md` 的一项缺陷：旧行为在 v1 中要么不可
//! 表达，要么被显式策略拒绝。测试名保留 F 编号，便于把审计条目与证据对应起来。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn run(args: &[&str]) -> Output {
    run_in(None, args, &[])
}

fn run_in(directory: Option<&Path>, args: &[&str], environment: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bakon"));
    command
        .env_remove("BAKON_STORE")
        .env_remove("BAKON_HOME")
        .env_remove("VISUAL")
        .env(
            "BAKON_CONFIG",
            std::env::temp_dir().join("bakon-tests-no-config.toml"),
        )
        .args(args);
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
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

struct Case {
    temporary: TempDir,
    store: PathBuf,
    file: PathBuf,
}

impl Case {
    fn new(content: &[u8]) -> Self {
        let temporary = TempDir::new().unwrap();
        let store = temporary.path().join("store");
        let file = temporary.path().join("note.txt");
        fs::write(&file, content).unwrap();
        Self {
            temporary,
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

    fn run_env(&self, args: &[&str], environment: &[(&str, &Path)]) -> Output {
        let mut full = vec!["--store", self.store()];
        full.extend_from_slice(args);
        run_in(None, &full, environment)
    }

    fn track(&self) -> Output {
        self.edit_body("exit 0")
    }

    fn edit_body(&self, body: &str) -> Output {
        self.edit_path(&self.file, body)
    }

    fn edit_path(&self, path: &Path, body: &str) -> Output {
        let editor = self.write_editor(body);
        self.run_env(&["edit", path.to_str().unwrap()], &[("EDITOR", &editor)])
    }

    fn edit_config(&self, config: &Path, body: &str) -> Output {
        let editor = self.write_editor(body);
        let full = vec![
            "--store",
            self.store(),
            "--config",
            config.to_str().unwrap(),
            "edit",
            self.file(),
        ];
        run_in(None, &full, &[("EDITOR", &editor)])
    }

    fn write_editor(&self, body: &str) -> PathBuf {
        let editor = self.temporary.path().join("editor.sh");
        fs::write(&editor, format!("#!/bin/sh\n{body}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&editor, fs::Permissions::from_mode(0o755)).unwrap();
        }
        editor
    }

    fn config(&self, name: &str, max_versions: u32) -> PathBuf {
        let path = self.temporary.path().join(name);
        fs::write(
            &path,
            format!("[retention]\nmax_versions = {max_versions}\n"),
        )
        .unwrap();
        path
    }

    fn log_count(&self) -> usize {
        let output = self.run(&["--format", "plain", "log", self.file()]);
        stdout(&output).lines().count()
    }
}

#[test]
fn f1_init_never_writes_into_a_foreign_directory() {
    let temporary = TempDir::new().unwrap();
    let foreign = temporary.path().join("someone-elses-worktree");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join(".git"), b"gitdir: elsewhere").unwrap();

    let output = run(&["init", "--store", foreign.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(6), "stderr: {}", stderr(&output));
    assert!(!foreign.join("BAKON_STORE").exists());
    assert_eq!(fs::read_dir(&foreign).unwrap().count(), 1);
}

#[test]
fn f2_recorded_paths_are_absolute_not_worktree_relative() {
    let case = Case::new(b"content");
    let relative = case.file.file_name().unwrap().to_str().unwrap();
    let missing_editor = case.temporary.path().join("absent-editor");
    let output = run_in(
        Some(case.temporary.path()),
        &["--store", case.store(), "edit", relative],
        &[("EDITOR", &missing_editor)],
    );
    // 编辑器不存在会以 5 失败，但纳管与基线已先完成。
    assert_eq!(output.status.code(), Some(5), "stderr: {}", stderr(&output));

    let files = case.store.join("files");
    let manifest = fs::read_to_string(
        fs::read_dir(&files)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path(),
    )
    .unwrap();
    assert!(manifest.contains("\"path\": \"/"), "manifest: {manifest}");
}

#[cfg(unix)]
#[test]
fn f3_write_back_failure_keeps_the_version_and_recovers_later() {
    let case = Case::new(b"one");
    assert_eq!(case.track().status.code(), Some(0));
    assert_eq!(
        case.edit_body("printf 'two' > \"$1\"").status.code(),
        Some(0)
    );

    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&case.file, fs::Permissions::from_mode(0o444)).unwrap();
    let failed = case.run(&["revert", case.file(), "0"]);
    assert_eq!(failed.status.code(), Some(7), "stderr: {}", stderr(&failed));
    assert_eq!(case.log_count(), 3, "the revert version must be recorded");

    fs::set_permissions(&case.file, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(case.track().status.code(), Some(0));
    assert_eq!(fs::read(&case.file).unwrap(), b"one");
}

#[cfg(unix)]
#[test]
fn f4_revert_keeps_symbolic_links() {
    let case = Case::new(b"one");
    let link = case.temporary.path().join("link.txt");
    std::os::unix::fs::symlink(&case.file, &link).unwrap();
    assert_eq!(case.edit_path(&link, "exit 0").status.code(), Some(0));
    assert_eq!(
        case.edit_path(&link, "printf 'two' > \"$1\"").status.code(),
        Some(0)
    );

    let output = case.run(&["revert", link.to_str().unwrap(), "0"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&case.file).unwrap(), b"one");
}

#[test]
fn f5_divergence_is_refused_unless_forced_and_salvaged() {
    let case = Case::new(b"one");
    assert_eq!(case.track().status.code(), Some(0));
    assert_eq!(
        case.edit_body("printf 'two' > \"$1\"").status.code(),
        Some(0)
    );
    fs::write(&case.file, b"outside edit").unwrap();

    let refused = case.run(&["revert", case.file(), "0"]);
    assert_eq!(
        refused.status.code(),
        Some(4),
        "stderr: {}",
        stderr(&refused)
    );

    let forced = case.run(&["revert", case.file(), "0", "--force"]);
    assert_eq!(forced.status.code(), Some(0), "stderr: {}", stderr(&forced));
    let log = case.run(&["--format", "plain", "log", case.file()]);
    assert!(
        stdout(&log).contains("\tsalvage"),
        "stdout: {}",
        stdout(&log)
    );
    assert_eq!(
        case.run(&["show", case.file(), "2"]).stdout,
        b"outside edit"
    );
}

#[test]
fn f6_a_lost_index_does_not_lose_tracking() {
    let case = Case::new(b"content");
    assert_eq!(case.track().status.code(), Some(0));
    fs::remove_file(case.store.join("index.json")).unwrap();

    let output = case.run(&["log", case.file()]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let fixed = case.run(&["verify", "--fix"]);
    assert_eq!(fixed.status.code(), Some(0), "stderr: {}", stderr(&fixed));
    assert!(case.store.join("index.json").is_file());
}

#[test]
fn f7_verify_reports_a_missing_object() {
    let case = Case::new(b"one");
    assert_eq!(case.track().status.code(), Some(0));
    assert_eq!(
        case.edit_body("printf 'two' > \"$1\"").status.code(),
        Some(0)
    );
    let shard = fs::read_dir(case.store.join("objects"))
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| entry.file_name() != "tmp")
        .unwrap()
        .path();
    let victim = fs::read_dir(shard).unwrap().next().unwrap().unwrap().path();
    fs::remove_file(victim).unwrap();

    let output = case.run(&["verify"]);

    assert_eq!(output.status.code(), Some(6));
    assert!(
        stdout(&output).contains("missing_blob"),
        "stdout: {}",
        stdout(&output)
    );
}

#[test]
fn f8_mv_does_not_create_path_aliases() {
    let case = Case::new(b"one");
    assert_eq!(case.track().status.code(), Some(0));
    assert_eq!(
        case.edit_body("printf 'two' > \"$1\"").status.code(),
        Some(0)
    );
    let renamed = case.temporary.path().join("renamed.txt");
    assert_eq!(
        case.run(&["mv", case.file(), renamed.to_str().unwrap()])
            .status
            .code(),
        Some(0)
    );

    assert_eq!(case.track().status.code(), Some(0));

    let old = case.run(&["--format", "plain", "log", case.file()]);
    let new = case.run(&["--format", "plain", "log", renamed.to_str().unwrap()]);
    assert_eq!(stdout(&old).lines().count(), 1);
    assert_eq!(stdout(&new).lines().count(), 2);
}

#[test]
fn f9_zero_retention_means_unlimited() {
    let case = Case::new(b"v0");
    let config = case.config("unlimited.toml", 0);
    for content in ["v1", "v2", "v3"] {
        let body = format!("printf '{content}' > \"$1\"");
        assert_eq!(case.edit_config(&config, &body).status.code(), Some(0));
    }

    let log = case.run_config(&config, &["--format", "plain", "log", case.file()]);
    assert_eq!(stdout(&log).lines().count(), 4);
}

#[test]
fn f10_diff_errors_are_not_mapped_to_one() {
    let case = Case::new(b"one");
    assert_eq!(case.track().status.code(), Some(0));

    let output = case.run(&["diff", case.file(), "0", "9"]);

    assert_eq!(output.status.code(), Some(3), "stderr: {}", stderr(&output));
}

#[test]
fn f11_hooks_run_outside_the_lock() {
    let case = Case::new(b"one");
    assert_eq!(case.track().status.code(), Some(0));
    let hook = format!(
        "{} --store {} log {} >/dev/null",
        env!("CARGO_BIN_EXE_bakon"),
        case.store(),
        case.file()
    );
    assert_eq!(
        case.run(&["hook", "set", case.file(), &hook]).status.code(),
        Some(0)
    );

    let output = case.edit_body("printf 'two' > \"$1\"");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

#[test]
fn f12_prune_does_not_touch_other_files() {
    let case = Case::new(b"shared");
    let limited = case.config("limited.toml", 2);
    assert_eq!(case.track().status.code(), Some(0));
    assert_eq!(
        case.edit_body("printf 'a2' > \"$1\"").status.code(),
        Some(0)
    );
    assert_eq!(
        case.edit_body("printf 'a3' > \"$1\"").status.code(),
        Some(0)
    );

    let other = case.temporary.path().join("other.txt");
    fs::write(&other, b"shared").unwrap();
    assert_eq!(case.edit_path(&other, "exit 0").status.code(), Some(0));

    assert_eq!(
        case.run_config(&limited, &["prune", case.file()])
            .status
            .code(),
        Some(0)
    );

    let untouched = case.run(&["--format", "plain", "log", other.to_str().unwrap()]);
    assert_eq!(stdout(&untouched).lines().count(), 1);
    assert_eq!(
        case.run(&["show", other.to_str().unwrap(), "0"]).stdout,
        b"shared"
    );
}

#[test]
fn f13_git_environment_variables_have_no_effect() {
    let case = Case::new(b"one");
    let bogus = case.temporary.path().join("bogus-git-dir");
    let editor = case.write_editor("printf 'two' > \"$1\"");

    let output = case.run_env(
        &["edit", case.file()],
        &[
            ("EDITOR", &editor),
            ("GIT_DIR", &bogus),
            ("GIT_WORK_TREE", &bogus),
        ],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(case.log_count(), 2);
    assert!(!bogus.exists(), "no git command may run");
}

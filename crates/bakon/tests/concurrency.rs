//! 并发用例：编辑器会话不持锁，提交时用 head 比较交换。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};

use tempfile::TempDir;

fn command(store: &Path, file: &Path, editor: &Path, barrier: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bakon"));
    command
        .env_remove("BAKON_STORE")
        .env_remove("BAKON_HOME")
        .env_remove("VISUAL")
        .env(
            "BAKON_CONFIG",
            std::env::temp_dir().join("bakon-tests-no-config.toml"),
        )
        .env("EDITOR", editor)
        .env("BARRIER", barrier)
        .args([
            "--store",
            store.to_str().unwrap(),
            "edit",
            file.to_str().unwrap(),
        ]);
    command
}

fn wait(child: Child) -> Output {
    child.wait_with_output().expect("failed to wait for bakon")
}

#[test]
fn two_editors_racing_produce_one_conflict() {
    let temporary = TempDir::new().unwrap();
    let store = temporary.path().join("store");
    let file = temporary.path().join("note.txt");
    let barrier = temporary.path().join("barrier");
    fs::create_dir(&barrier).unwrap();
    fs::write(&file, b"base").unwrap();

    let noop = temporary.path().join("noop.sh");
    fs::write(&noop, "#!/bin/sh\nexit 0\n").unwrap();
    make_executable(&noop);
    assert_eq!(
        command(&store, &file, &noop, &barrier)
            .output()
            .unwrap()
            .status
            .code(),
        Some(0)
    );

    let first_editor = editor(&temporary, "one", "a");
    let second_editor = editor(&temporary, "two", "b");

    let first = command(&store, &file, &first_editor, &barrier)
        .spawn()
        .unwrap();
    let second = command(&store, &file, &second_editor, &barrier)
        .spawn()
        .unwrap();

    // 编辑器阶段不持锁：读命令在两者都未提交时仍可完成。
    let log = Command::new(env!("CARGO_BIN_EXE_bakon"))
        .env_remove("BAKON_STORE")
        .env_remove("BAKON_HOME")
        .env(
            "BAKON_CONFIG",
            std::env::temp_dir().join("bakon-tests-no-config.toml"),
        )
        .args([
            "--store",
            store.to_str().unwrap(),
            "log",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        log.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&log.stderr)
    );

    let mut codes = vec![wait(first).status.code(), wait(second).status.code()];
    codes.sort();
    assert_eq!(codes, vec![Some(0), Some(4)]);

    let log = Command::new(env!("CARGO_BIN_EXE_bakon"))
        .env_remove("BAKON_STORE")
        .env_remove("BAKON_HOME")
        .env(
            "BAKON_CONFIG",
            std::env::temp_dir().join("bakon-tests-no-config.toml"),
        )
        .args([
            "--store",
            store.to_str().unwrap(),
            "--format",
            "plain",
            "log",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&log.stdout).lines().count(), 2);
}

fn editor(temporary: &TempDir, content: &str, name: &str) -> PathBuf {
    let path = temporary.path().join(format!("editor-{name}.sh"));
    let script = format!(
        "#!/bin/sh\n\
         touch \"$BARRIER/{name}\"\n\
         while [ ! -f \"$BARRIER/{other}\" ]; do sleep 0.01; done\n\
         printf '{content}' > \"$1\"\n",
        other = if name == "a" { "b" } else { "a" }
    );
    fs::write(&path, script).unwrap();
    make_executable(&path);
    path
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

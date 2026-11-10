//! 命令实现：I/O、输出格式与事务编排。语义判断全部委托给 `bakon-core`。

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::Duration;

use bakon_core::manifest::{Manifest, Source, VersionRecord};
use bakon_core::object::hash_bytes;
use bakon_core::{AbsPath, Error, Result, Store, Ver};

use crate::cli::{Cli, Format};

pub fn init(cli: &Cli) -> Result<u8> {
    let (store, created) = Store::init(store_path(cli)?)?;
    let root = store.root().as_path().display().to_string();

    let line = match cli.format {
        Format::Human if created => format!("initialized {root}"),
        Format::Human => format!("already initialized: {root}"),
        Format::Plain => format!("{root}\t{}", if created { "created" } else { "existing" }),
        Format::Json => serde_json::json!({
            "store": root,
            "created": created,
            "schema": store.marker().schema,
        })
        .to_string(),
    };
    emit(&line)?;
    Ok(0)
}

pub fn version(format: Format) -> Result<u8> {
    let version = env!("CARGO_PKG_VERSION");
    let target = env!("BAKON_TARGET");

    let line = match format {
        Format::Human => format!("bakon {version} ({target})"),
        Format::Plain => format!("{version}\t{target}"),
        Format::Json => serde_json::json!({ "version": version, "target": target }).to_string(),
    };
    emit(&line)?;
    Ok(0)
}

/// `edit`：首次自动纳管；编辑器退出后按需提交。编辑器阶段不持锁。
pub fn edit(cli: &Cli, file: &Path) -> Result<u8> {
    let path = AbsPath::new(file)?;
    let mut store = open_store(cli)?;
    let timeout = lock_timeout(cli);

    let (id, head, content_hash) = {
        let _guard = store.begin_write(timeout)?;
        match store.resolve(&path)? {
            Some(id) => {
                let manifest = store.manifest(id)?;
                let head = manifest.max_ver().ok_or_else(|| Error::Corrupt {
                    what: "manifest",
                    detail: format!("{id} has no versions"),
                })?;
                let record = manifest.find(head).ok_or_else(|| Error::Corrupt {
                    what: "manifest",
                    detail: format!("{id} has no head record"),
                })?;
                (id, head, record.blob)
            }
            None => {
                let content = read_target(&path)?;
                let prepared = store.prepare_track(&path, &content)?;
                let committed = store.commit(prepared, None)?;
                eprintln!(
                    "bakon: tracked {} (baseline version {})",
                    path,
                    committed.version()
                );
                (committed.id(), committed.version(), hash_bytes(&content))
            }
        }
    };

    run_editor(&path)?;
    let edited = read_target(&path)?;
    if hash_bytes(&edited) == content_hash {
        eprintln!("bakon: no changes to {}", path);
        return Ok(0);
    }

    let _guard = store.begin_write(timeout)?;
    if store.resolve(&path)? != Some(id) {
        return Err(Error::Internal(format!(
            "{} was re-tracked while the editor was open",
            path
        )));
    }
    let prepared = store.prepare_commit(id, &edited, Source::Edit)?;
    let committed = store.commit(prepared, Some(head))?;
    eprintln!("bakon: saved version {} of {}", committed.version(), path);
    Ok(0)
}

/// `log`：最新在前，数据走 stdout。
pub fn log(cli: &Cli, file: &Path) -> Result<u8> {
    let path = AbsPath::new(file)?;
    let mut store = open_store(cli)?;
    let _guard = store.lock_shared(lock_timeout(cli))?;
    let id = store.resolve(&path)?.ok_or(Error::NotManaged(path))?;
    let manifest = store.manifest(id)?;

    match cli.format {
        Format::Human => {
            emit("  ver  time                  size  source")?;
            for record in manifest.versions.iter().rev() {
                emit(&format!(
                    "{:>5}  {:<22}{:>6}  {}",
                    record.ver.get(),
                    display_time(&record.time),
                    record.size,
                    source_name(record.source)
                ))?;
            }
        }
        Format::Plain => {
            for record in manifest.versions.iter().rev() {
                emit(&format!(
                    "{}\t{}\t{}\t{}",
                    record.ver.get(),
                    display_time(&record.time),
                    record.size,
                    source_name(record.source)
                ))?;
            }
        }
        Format::Json => {
            for record in manifest.versions.iter().rev() {
                emit(
                    &serde_json::json!({
                        "ver": record.ver.get(),
                        "time": display_time(&record.time),
                        "size": record.size,
                        "source": source_name(record.source),
                    })
                    .to_string(),
                )?;
            }
        }
    }
    Ok(0)
}

/// `show` / `dump`：原始字节，不加换行。
pub fn show(cli: &Cli, file: &Path, version: u32) -> Result<u8> {
    let path = AbsPath::new(file)?;
    let mut store = open_store(cli)?;
    let _guard = store.lock_shared(lock_timeout(cli))?;
    let id = store.resolve(&path)?.ok_or(Error::NotManaged(path))?;
    let manifest = store.manifest(id)?;
    let record = find_version(&manifest, Ver::new(version)?)?;
    let content = store.content(record.blob)?;

    let mut stdout = io::stdout().lock();
    stdout.write_all(&content).map_err(Error::Io)?;
    stdout.flush().map_err(Error::Io)?;
    Ok(0)
}

/// `diff`：0 个参数比较最近两版；1 个参数与该版的前一版比较。
pub fn diff(cli: &Cli, file: &Path, first: Option<u32>, second: Option<u32>) -> Result<u8> {
    let path = AbsPath::new(file)?;
    let mut store = open_store(cli)?;
    let _guard = store.lock_shared(lock_timeout(cli))?;
    let id = store
        .resolve(&path)?
        .ok_or(Error::NotManaged(path.clone()))?;
    let manifest = store.manifest(id)?;

    let (older, newer) = match (first, second) {
        (None, None) => {
            let newer = manifest.max_ver().ok_or_else(|| Error::Corrupt {
                what: "manifest",
                detail: format!("{id} has no versions"),
            })?;
            (previous_version(&manifest, newer)?, newer)
        }
        (Some(newer), None) => {
            let newer = Ver::new(newer)?;
            (previous_version(&manifest, newer)?, newer)
        }
        (Some(older), Some(newer)) => (Ver::new(older)?, Ver::new(newer)?),
        (None, Some(_)) => {
            return Err(Error::Usage("a second version cannot stand alone".into()));
        }
    };

    let older_record = find_version(&manifest, older)?;
    let newer_record = find_version(&manifest, newer)?;
    let older_bytes = store.content(older_record.blob)?;
    let newer_bytes = store.content(newer_record.blob)?;

    if older_bytes == newer_bytes {
        return Ok(0);
    }

    let (Some(older_text), Some(newer_text)) = (as_text(&older_bytes), as_text(&newer_bytes))
    else {
        eprintln!("bakon: binary files differ");
        return Ok(1);
    };

    let diff = similar::TextDiff::from_lines(older_text, newer_text);
    let header_older = format!("{path} v{older}");
    let header_newer = format!("{path} v{newer}");
    let text = diff
        .unified_diff()
        .context_radius(3)
        .header(&header_older, &header_newer)
        .to_string();

    let mut stdout = io::stdout().lock();
    stdout.write_all(text.as_bytes()).map_err(Error::Io)?;
    stdout.flush().map_err(Error::Io)?;
    Ok(1)
}

fn find_version(manifest: &Manifest, ver: Ver) -> Result<&VersionRecord> {
    let latest = manifest.max_ver().ok_or_else(|| Error::Corrupt {
        what: "manifest",
        detail: format!("{} has no versions", manifest.id),
    })?;
    if ver > latest {
        return Err(Error::NoSuchVersion { ver, latest });
    }
    let oldest = manifest.oldest_ver().unwrap_or(Ver::ZERO);
    if ver < oldest {
        return Err(Error::Pruned { ver, oldest });
    }
    manifest
        .find(ver)
        .ok_or(Error::NoSuchVersion { ver, latest })
}

fn previous_version(manifest: &Manifest, ver: Ver) -> Result<Ver> {
    let latest = manifest.max_ver().ok_or_else(|| Error::Corrupt {
        what: "manifest",
        detail: format!("{} has no versions", manifest.id),
    })?;
    let index = manifest
        .versions
        .iter()
        .position(|record| record.ver == ver)
        .ok_or(Error::NoSuchVersion { ver, latest })?;
    let Some(previous) = index.checked_sub(1) else {
        return Err(Error::NoPreviousVersion { ver });
    };
    Ok(manifest.versions[previous].ver)
}

fn source_name(source: Source) -> &'static str {
    match source {
        Source::Baseline => "baseline",
        Source::Edit => "edit",
        Source::Revert => "revert",
        Source::Salvage => "salvage",
    }
}

/// 存储保留完整精度，输出收敛到秒，匹配文档中的时间格式。
fn display_time(time: &str) -> String {
    match time.split_once('.') {
        Some((prefix, fraction)) if fraction.ends_with('Z') => format!("{prefix}Z"),
        _ => time.to_owned(),
    }
}

fn as_text(bytes: &[u8]) -> Option<&str> {
    if bytes.contains(&0) {
        return None;
    }
    std::str::from_utf8(bytes).ok()
}

fn read_target(path: &AbsPath) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path.as_path()).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            Error::NotFound(path.clone())
        } else {
            Error::Io(error)
        }
    })?;
    if !metadata.is_file() {
        return Err(Error::NotAFile { path: path.clone() });
    }
    fs::read(path.as_path()).map_err(Error::Io)
}

fn run_editor(path: &AbsPath) -> Result<()> {
    let editor = env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| "vi".into());
    let command = format!(
        "{editor} {}",
        shell_quote(&path.as_path().to_string_lossy())
    );
    let status = ProcessCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .status()
        .map_err(Error::Io)?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::EditorFailed {
            status: status.code().unwrap_or(-1),
        })
    }
}

/// 单引号转义，供 `sh -c` 使用。
fn shell_quote(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('\'');
    for character in text.chars() {
        match character {
            '\'' => quoted.push_str("'\\''"),
            character => quoted.push(character),
        }
    }
    quoted.push('\'');
    quoted
}

fn open_store(cli: &Cli) -> Result<Store> {
    Store::open(store_path(cli)?)
}

/// 存储位置优先级：`--store` > `BAKON_STORE` > `BAKON_HOME/store` > `~/.bakon/store`。
fn store_path(cli: &Cli) -> Result<PathBuf> {
    if let Some(path) = &cli.store {
        return Ok(path.clone());
    }
    if let Some(path) = env::var_os("BAKON_STORE").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    if let Some(home) = env::var_os("BAKON_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home).join("store"));
    }
    let home = home_dir()
        .ok_or_else(|| Error::Usage("cannot determine the home directory; pass --store".into()))?;
    Ok(home.join(".bakon").join("store"))
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn lock_timeout(cli: &Cli) -> Duration {
    Duration::from_millis(cli.lock_timeout)
}

/// stdout 只放数据；写入失败视为 I/O 错误而不是 panic。
fn emit(line: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{line}").map_err(Error::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quoting_escapes_single_quotes() {
        assert_eq!(shell_quote("/tmp/a b"), "'/tmp/a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn text_detection_rejects_nul_and_invalid_utf8() {
        assert_eq!(as_text(b"hello"), Some("hello"));
        assert_eq!(as_text(b"a\0b"), None);
        assert_eq!(as_text(&[0xff, 0xfe]), None);
    }

    #[test]
    fn display_time_drops_subseconds() {
        assert_eq!(
            display_time("2026-11-10T14:15:55.311118339Z"),
            "2026-11-10T14:15:55Z"
        );
        assert_eq!(display_time("2026-11-10T14:15:55Z"), "2026-11-10T14:15:55Z");
    }
}

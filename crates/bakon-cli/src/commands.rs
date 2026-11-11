//! 命令实现：I/O、输出格式与事务编排。语义判断全部委托给 `bakon-core`。

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::Duration;

use bakon_core::manifest::{Manifest, Source, VersionRecord};
use bakon_core::object::hash_bytes;
use bakon_core::store::{Committed, VerifyOptions};
use bakon_core::{AbsPath, Error, Result, Store, Ver};

use crate::cli::{Cli, ConfigAction, Format, HookAction, StoreAction};
use crate::config::{self, Config};

pub fn init(cli: &Cli) -> Result<u8> {
    let config = load_config(cli)?;
    let (store, created) = Store::init(store_path(cli, &config)?)?;
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
pub fn edit(cli: &Cli, file: &Path, no_hook: bool, editor: Option<&str>) -> Result<u8> {
    let path = AbsPath::new(file)?;
    let (mut store, config) = open_store_with(cli)?;
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
                warn_trim(&committed);
                eprintln!(
                    "bakon: tracked {} (baseline version {})",
                    path,
                    committed.version()
                );
                (committed.id(), committed.version(), hash_bytes(&content))
            }
        }
    };

    run_editor(&path, &editor_command(&config, editor))?;
    let edited = read_target(&path)?;
    if hash_bytes(&edited) == content_hash {
        eprintln!("bakon: no changes to {}", path);
        return Ok(0);
    }

    let (committed, hook_manifest) = {
        let _guard = store.begin_write(timeout)?;
        if store.resolve(&path)? != Some(id) {
            return Err(Error::Internal(format!(
                "{} was re-tracked while the editor was open",
                path
            )));
        }
        let prepared = store.prepare_commit(id, &edited, Source::Edit)?;
        let committed = store.commit(prepared, Some(head))?;
        warn_trim(&committed);
        let manifest = store.manifest(id)?;
        (committed, manifest)
    };

    if !no_hook {
        run_hook(
            &hook_manifest,
            committed.version(),
            Source::Edit,
            Some(head),
        )?;
    }
    eprintln!("bakon: saved version {} of {}", committed.version(), path);
    Ok(0)
}

/// `revert`：恢复一个已记录版本，并把恢复本身记为新的 `revert` 版本。
pub fn revert(cli: &Cli, file: &Path, version: u32, force: bool, no_hook: bool) -> Result<u8> {
    let path = AbsPath::new(file)?;
    let ver = Ver::parse(version)?;
    let mut store = open_store(cli)?;
    let timeout = lock_timeout(cli);

    let (committed, hook_manifest, previous) = {
        let _guard = store.begin_write(timeout)?;
        let id = store
            .resolve(&path)?
            .ok_or(Error::NotManaged(path.clone()))?;
        let manifest = store.manifest(id)?;
        let target = find_version(&manifest, ver)?.clone();
        let target_content = store.content(target.blob)?;

        let head = manifest.head().cloned().ok_or_else(|| Error::Corrupt {
            what: "manifest",
            detail: format!("{id} has no versions"),
        })?;
        let current = read_if_exists(&path)?;
        let old_blob = current.as_ref().map(|bytes| hash_bytes(bytes));
        let new_blob = hash_bytes(&target_content);
        let diverged = current
            .as_ref()
            .is_some_and(|bytes| hash_bytes(bytes) != head.blob);

        if diverged && !force {
            return Err(Error::Diverged {
                path,
                hint: "target differs from the recorded head; use --force to record it as salvage first",
            });
        }

        let mut previous = head.ver;
        if diverged {
            let content = current.expect("diverged implies content");
            let salvage = store.prepare_commit(id, &content, Source::Salvage)?;
            let salvaged = store.commit(salvage, Some(head.ver))?;
            warn_trim(&salvaged);
            previous = salvaged.version();
        }

        let manifest = store.manifest(id)?;
        let head = manifest.head().cloned().ok_or_else(|| Error::Corrupt {
            what: "manifest",
            detail: format!("{id} has no versions"),
        })?;

        if head.blob == target.blob {
            store
                .committed(id, head.ver)?
                .apply(&store, &path)
                .map_err(|error| applied(error, head.ver))?;
            (None, manifest, previous)
        } else {
            let mut prepared = store.prepare_commit(id, &target_content, Source::Revert)?;
            prepared.set_pending(true);
            let pending_version = prepared
                .manifest()
                .head()
                .ok_or_else(|| Error::Corrupt {
                    what: "manifest",
                    detail: format!("{id} has no versions"),
                })?
                .ver;
            store.begin_apply(id, pending_version, old_blob, new_blob)?;
            let committed = store.commit(prepared, Some(head.ver))?;
            warn_trim(&committed);
            committed
                .apply(&store, &path)
                .map_err(|error| applied(error, committed.version()))?;
            store.finish_apply(id)?;
            let manifest = store.manifest(id)?;
            (Some(committed), manifest, previous)
        }
    };

    match committed {
        Some(committed) => {
            if !no_hook {
                run_hook(
                    &hook_manifest,
                    committed.version(),
                    Source::Revert,
                    Some(previous),
                )?;
            }
            eprintln!(
                "bakon: reverted {} to version {} as version {}",
                path,
                ver,
                committed.version()
            );
        }
        None => eprintln!("bakon: {} already matches version {}", path, ver),
    }
    Ok(0)
}

/// `prune`：按生效上限裁剪；`--dry-run` 只报告。
pub fn prune(cli: &Cli, file: Option<&Path>, dry_run: bool) -> Result<u8> {
    let mut store = open_store(cli)?;
    let timeout = lock_timeout(cli);
    let _guard = if dry_run {
        store.lock_shared(timeout)?
    } else {
        store.begin_write(timeout)?
    };

    let ids = match file {
        Some(file) => {
            let path = AbsPath::new(file)?;
            vec![store.resolve(&path)?.ok_or(Error::NotManaged(path))?]
        }
        None => store.all_ids()?,
    };
    let report = store.prune(&ids, dry_run)?;

    if report.files.is_empty() {
        match cli.format {
            Format::Human => emit("nothing to prune")?,
            Format::Plain => {}
            Format::Json => {
                emit(&serde_json::json!({ "files": [], "dry_run": dry_run }).to_string())?
            }
        }
    }
    for file in &report.files {
        match cli.format {
            Format::Human => emit(&format!(
                "{} {} version(s) from {}",
                if dry_run { "would prune" } else { "pruned" },
                file.versions.len(),
                file.path
            ))?,
            Format::Plain => emit(&format!(
                "{}\t{}\t{}",
                file.path,
                file.versions.len(),
                dry_run
            ))?,
            Format::Json => emit(
                &serde_json::json!({
                    "path": file.path.as_path().to_string_lossy(),
                    "removed": file.versions.iter().map(|ver| ver.get()).collect::<Vec<_>>(),
                    "dry_run": dry_run,
                })
                .to_string(),
            )?,
        }
    }
    Ok(0)
}

/// `mv`：只改记录中的路径，不动磁盘文件。
pub fn mv(cli: &Cli, old: &Path, new: &Path) -> Result<u8> {
    let old = AbsPath::new(old)?;
    let new = AbsPath::new(new)?;
    let mut store = open_store(cli)?;
    let _guard = store.begin_write(lock_timeout(cli))?;

    let id = store
        .resolve(&old)?
        .ok_or_else(|| Error::NotManaged(old.clone()))?;
    store.rename(id, new.clone())?;

    match cli.format {
        Format::Human => emit(&format!("renamed {old} -> {new}"))?,
        Format::Plain => emit(&format!("{old}\t{new}"))?,
        Format::Json => emit(
            &serde_json::json!({
                "old": old.as_path().to_string_lossy(),
                "new": new.as_path().to_string_lossy(),
            })
            .to_string(),
        )?,
    }
    Ok(0)
}

/// `hook`：设置、清除或查看单文件钩子。
pub fn hook(cli: &Cli, action: HookAction, file: &Path, command: Option<&str>) -> Result<u8> {
    let path = AbsPath::new(file)?;
    let mut store = open_store(cli)?;
    let timeout = lock_timeout(cli);

    match action {
        HookAction::Show => {
            let _guard = store.lock_shared(timeout)?;
            let id = store
                .resolve(&path)?
                .ok_or_else(|| Error::NotManaged(path.clone()))?;
            let manifest = store.manifest(id)?;
            match cli.format {
                Format::Json => emit(&serde_json::json!({ "hook": manifest.hook }).to_string())?,
                _ => emit(manifest.hook.as_deref().unwrap_or("(not set)"))?,
            }
        }
        HookAction::Set => {
            let command =
                command.ok_or_else(|| Error::Usage("hook set needs a shell command".into()))?;
            if command.trim().is_empty() {
                return Err(Error::Usage("hook command cannot be empty".into()));
            }
            let _guard = store.begin_write(timeout)?;
            let id = store
                .resolve(&path)?
                .ok_or_else(|| Error::NotManaged(path.clone()))?;
            store.set_hook(id, Some(command.to_owned()))?;
            emit(&format!("hook set for {path}"))?;
        }
        HookAction::Unset => {
            let _guard = store.begin_write(timeout)?;
            let id = store
                .resolve(&path)?
                .ok_or_else(|| Error::NotManaged(path.clone()))?;
            store.set_hook(id, None)?;
            emit(&format!("hook unset for {path}"))?;
        }
    }
    Ok(0)
}

/// `config`：展示生效配置或写入默认文件。
pub fn config(cli: &Cli, action: ConfigAction) -> Result<u8> {
    let path = config::path(cli.config.as_deref())?;
    match action {
        ConfigAction::Init => {
            let written = config::init(&path)?;
            let line = if written {
                format!("wrote {}", path.display())
            } else {
                format!("already exists, left untouched: {}", path.display())
            };
            emit(&line)?;
        }
        ConfigAction::Show => {
            let config = config::load(&path)?;
            let store = store_path(cli, &config)?;
            let editor = editor_command(&config, None);
            let max_versions = config.max_versions();
            let line = match cli.format {
                Format::Human => format!(
                    "config:       {}\nstore:        {}\neditor:       {}\nmax_versions: {} (global, 0 = unlimited)",
                    path.display(),
                    store.display(),
                    editor,
                    max_versions
                ),
                Format::Plain => format!(
                    "{}\t{}\t{}\t{}",
                    path.display(),
                    store.display(),
                    editor,
                    max_versions
                ),
                Format::Json => serde_json::json!({
                    "config": path.display().to_string(),
                    "store": store.display().to_string(),
                    "editor": editor,
                    "max_versions": max_versions,
                })
                .to_string(),
            };
            emit(&line)?;
        }
    }
    Ok(0)
}

/// `verify`：一致返回 0，发现不一致返回 6。
pub fn verify(cli: &Cli, fix: bool, size: bool) -> Result<u8> {
    let mut store = open_store(cli)?;
    let timeout = lock_timeout(cli);
    let _guard = if fix {
        store.begin_verify(timeout)?
    } else {
        store.lock_shared(timeout)?
    };
    let report = store.verify(VerifyOptions { fix, size })?;

    match cli.format {
        Format::Human => {
            if report.ok() {
                emit("store is consistent")?;
            }
            for item in &report.items {
                let location = match (&item.path, item.ver) {
                    (Some(path), Some(ver)) => format!(" {path} v{ver}"),
                    (Some(path), None) => format!(" {path}"),
                    _ => String::new(),
                };
                emit(&format!(
                    "{}{}  {}{}",
                    item.kind,
                    location,
                    item.detail,
                    if item.fixed { " [fixed]" } else { "" }
                ))?;
            }
            if size {
                emit(&format!(
                    "checked {} file(s); unique object bytes {}",
                    report.checked_files, report.unique_object_bytes
                ))?;
                for entry in &report.sizes {
                    emit(&format!(
                        "  {}  versions {}  objects {} B  manifest {} B",
                        entry.path, entry.versions, entry.object_bytes, entry.manifest_bytes
                    ))?;
                }
            }
        }
        Format::Plain => {
            for item in &report.items {
                emit(&format!(
                    "{}\t{}\t{}\t{}\t{}",
                    item.kind,
                    item.path
                        .as_ref()
                        .map(|path| path.as_path().to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    item.ver
                        .map(Ver::get)
                        .map(|ver| ver.to_string())
                        .unwrap_or_default(),
                    item.detail,
                    item.fixed
                ))?;
            }
            if size {
                for entry in &report.sizes {
                    emit(&format!(
                        "size\t{}\t{}\t{}\t{}",
                        entry.path, entry.versions, entry.object_bytes, entry.manifest_bytes
                    ))?;
                }
                emit(&format!("unique\t{}", report.unique_object_bytes))?;
            }
        }
        Format::Json => {
            let items: Vec<serde_json::Value> = report
                .items
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "kind": item.kind,
                        "path": item.path.as_ref().map(|path| path.as_path().to_string_lossy().into_owned()),
                        "ver": item.ver.map(Ver::get),
                        "detail": item.detail,
                        "fixed": item.fixed,
                    })
                })
                .collect();
            let mut value = serde_json::json!({ "ok": report.ok(), "items": items });
            if size {
                value["size"] = serde_json::json!({
                    "checked_files": report.checked_files,
                    "unique_object_bytes": report.unique_object_bytes,
                    "files": report.sizes.iter().map(|entry| serde_json::json!({
                        "path": entry.path.as_path().to_string_lossy(),
                        "versions": entry.versions,
                        "object_bytes": entry.object_bytes,
                        "manifest_bytes": entry.manifest_bytes,
                    })).collect::<Vec<_>>(),
                });
            }
            emit(&value.to_string())?;
        }
    }

    Ok(if report.ok() { 0 } else { 6 })
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

/// `ls`：列出全部被管理路径，按路径排序。
pub fn ls(cli: &Cli) -> Result<u8> {
    let store = open_store(cli)?;
    let _guard = store.lock_shared(lock_timeout(cli))?;
    let mut paths = BTreeMap::new();
    for id in store.all_ids()? {
        let path = store.manifest(id)?.path;
        paths.insert(path.key(), path);
    }
    for path in paths.into_values() {
        match cli.format {
            Format::Human | Format::Plain => emit(&path.as_path().to_string_lossy())?,
            Format::Json => {
                emit(&serde_json::json!({ "path": path.as_path().to_string_lossy() }).to_string())?
            }
        }
    }
    Ok(0)
}

/// `path`：输出路径、稳定身份与清单文件位置。
pub fn path(cli: &Cli, file: &Path) -> Result<u8> {
    let path = AbsPath::new(file)?;
    let mut store = open_store(cli)?;
    let _guard = store.lock_shared(lock_timeout(cli))?;
    let id = store
        .resolve(&path)?
        .ok_or_else(|| Error::NotManaged(path.clone()))?;
    let manifest = store.manifest_path(id);
    match cli.format {
        Format::Human => emit(&format!(
            "path:     {path}\nid:       {id}\nmanifest: {}",
            manifest.as_path().display()
        ))?,
        Format::Plain => emit(&format!(
            "{}\t{}\t{}",
            path.as_path().display(),
            id,
            manifest.as_path().display()
        ))?,
        Format::Json => emit(
            &serde_json::json!({
                "path": path.as_path().to_string_lossy(),
                "id": id.to_hex(),
                "manifest": manifest.as_path().to_string_lossy(),
            })
            .to_string(),
        )?,
    }
    Ok(0)
}

/// `store`：输出存储根目录或磁盘占用。
pub fn store(cli: &Cli, action: StoreAction) -> Result<u8> {
    let store = open_store(cli)?;
    let _guard = store.lock_shared(lock_timeout(cli))?;
    match action {
        StoreAction::Path => match cli.format {
            Format::Human => emit(&format!("store: {}", store.root().as_path().display()))?,
            Format::Plain => emit(&store.root().as_path().display().to_string())?,
            Format::Json => emit(
                &serde_json::json!({
                    "store": store.root().as_path().to_string_lossy(),
                    "schema": store.marker().schema,
                })
                .to_string(),
            )?,
        },
        StoreAction::Size => {
            let (bytes, files) = tree_size(store.root().as_path())?;
            match cli.format {
                Format::Human => emit(&format!(
                    "store: {}\nbytes: {bytes}\nfiles: {files}",
                    store.root().as_path().display()
                ))?,
                Format::Plain => emit(&format!(
                    "{}\t{bytes}\t{files}",
                    store.root().as_path().display()
                ))?,
                Format::Json => emit(
                    &serde_json::json!({
                        "store": store.root().as_path().to_string_lossy(),
                        "bytes": bytes,
                        "files": files,
                    })
                    .to_string(),
                )?,
            }
        }
    }
    Ok(0)
}

fn tree_size(root: &Path) -> Result<(u64, u64)> {
    fn walk(path: &Path, bytes: &mut u64, files: &mut u64) -> io::Result<()> {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                walk(&entry.path(), bytes, files)?;
            } else if file_type.is_file() {
                *bytes += entry.metadata()?.len();
                *files += 1;
            }
        }
        Ok(())
    }

    let mut bytes = 0;
    let mut files = 0;
    walk(root, &mut bytes, &mut files).map_err(Error::Io)?;
    Ok((bytes, files))
}

/// `show` / `dump`：原始字节，不加换行。
pub fn show(cli: &Cli, file: &Path, version: u32) -> Result<u8> {
    let path = AbsPath::new(file)?;
    let mut store = open_store(cli)?;
    let _guard = store.lock_shared(lock_timeout(cli))?;
    let id = store.resolve(&path)?.ok_or(Error::NotManaged(path))?;
    let manifest = store.manifest(id)?;
    let record = find_version(&manifest, Ver::parse(version)?)?;
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
            let newer = Ver::parse(newer)?;
            (previous_version(&manifest, newer)?, newer)
        }
        (Some(older), Some(newer)) => (Ver::parse(older)?, Ver::parse(newer)?),
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

/// 读取目标文件；不存在返回 `None`，存在但不是普通文件返回 3。
fn read_if_exists(path: &AbsPath) -> Result<Option<Vec<u8>>> {
    match fs::metadata(path.as_path()) {
        Ok(metadata) if metadata.is_file() => fs::read(path.as_path()).map(Some).map_err(Error::Io),
        Ok(_) => Err(Error::NotAFile { path: path.clone() }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match fs::symlink_metadata(path.as_path()) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    Err(Error::NotAFile { path: path.clone() })
                }
                Ok(_) => Ok(None),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(Error::Io(error)),
            }
        }
        Err(error) => Err(Error::Io(error)),
    }
}

fn warn_trim(committed: &Committed) {
    if committed.trim_failed() {
        eprintln!(
            "bakon: saved version {}, but old objects could not be reclaimed; run `bakon verify --fix`",
            committed.version()
        );
    }
}

/// 写回失败中属于环境问题的部分映射为退出码 7（版本已记录，但文件未更新）。
fn applied(error: Error, ver: Ver) -> Error {
    match error {
        Error::Io(source) => Error::ApplyFailed { ver, source },
        Error::NotAFile { path } => Error::ApplyFailed {
            ver,
            source: io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("not a regular file: {path}"),
            ),
        },
        other => other,
    }
}

/// 在锁外执行文件的变更钩子；至多一次，失败不回滚版本。
fn run_hook(
    manifest: &Manifest,
    version: Ver,
    source: Source,
    previous: Option<Ver>,
) -> Result<()> {
    let Some(hook) = manifest.hook.as_deref() else {
        return Ok(());
    };
    if env::var_os("BAKON_HOOK").is_some() {
        eprintln!("bakon: hook skipped (already inside a hook)");
        return Ok(());
    }

    let status = run_shell_with_env(hook, manifest, version, source, previous)?;

    if status.success() {
        Ok(())
    } else {
        Err(Error::HookFailed {
            ver: version,
            status: status.code().unwrap_or(-1),
        })
    }
}

fn run_shell_with_env(
    hook: &str,
    manifest: &Manifest,
    version: Ver,
    source: Source,
    previous: Option<Ver>,
) -> Result<std::process::ExitStatus> {
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut command = ProcessCommand::new("cmd");
        command.arg("/C").raw_arg(hook);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = ProcessCommand::new("sh");
        command.arg("-c").arg(hook);
        command
    };

    command
        .env("BAKON_FILE", manifest.path.as_path())
        .env("BAKON_VERSION", version.get().to_string())
        .env("BAKON_SOURCE", source_name(source))
        .env(
            "BAKON_PREV",
            previous
                .map(|ver| ver.get().to_string())
                .unwrap_or_default(),
        )
        .env("BAKON_HOOK", "1")
        .status()
        .map_err(Error::Io)
}

fn run_editor(path: &AbsPath, editor: &str) -> Result<()> {
    let command = format!(
        "{editor} {}",
        shell_quote(&path.as_path().to_string_lossy())
    );
    let status = run_shell(&command)?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::EditorFailed {
            status: status.code().unwrap_or(-1),
        })
    }
}

/// Unix 用 `sh -c`，Windows 用 `cmd /C`；钩子与编辑器共用。
fn run_shell(command: &str) -> Result<std::process::ExitStatus> {
    #[cfg(windows)]
    let status = {
        use std::os::windows::process::CommandExt;
        ProcessCommand::new("cmd")
            .arg("/C")
            .raw_arg(command)
            .status()
    };
    #[cfg(not(windows))]
    let status = ProcessCommand::new("sh").arg("-c").arg(command).status();
    status.map_err(Error::Io)
}

/// 单引号转义，供 `sh -c` 使用。
fn shell_quote(text: &str) -> String {
    #[cfg(windows)]
    {
        format!("\"{}\"", text.replace('"', "\"\""))
    }
    #[cfg(not(windows))]
    {
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
}

fn load_config(cli: &Cli) -> Result<Config> {
    config::load(&config::path(cli.config.as_deref())?)
}

fn open_store(cli: &Cli) -> Result<Store> {
    open_store_with(cli).map(|(store, _)| store)
}

fn open_store_with(cli: &Cli) -> Result<(Store, Config)> {
    let config = load_config(cli)?;
    let mut store = Store::open(store_path(cli, &config)?)?;
    store.set_default_max_versions(config.max_versions());
    Ok((store, config))
}

/// 存储位置优先级：`--store` > `BAKON_STORE` > 配置 `store` > `BAKON_HOME/store` > `~/.bakon/store`。
fn store_path(cli: &Cli, config: &Config) -> Result<PathBuf> {
    if let Some(path) = &cli.store {
        return Ok(path.clone());
    }
    if let Some(path) = env::var_os("BAKON_STORE").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = config.store_path() {
        return Ok(path);
    }
    if let Some(home) = env::var_os("BAKON_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home).join("store"));
    }
    let home = home_dir()
        .ok_or_else(|| Error::Usage("cannot determine the home directory; pass --store".into()))?;
    Ok(home.join(".bakon").join("store"))
}

/// 编辑器优先级：`--editor` > `$VISUAL` > `$EDITOR` > 配置 `editor` > `vi`。
fn editor_command(config: &Config, override_editor: Option<&str>) -> String {
    if let Some(editor) = override_editor.filter(|value| !value.trim().is_empty()) {
        return editor.to_owned();
    }
    env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| {
            config
                .editor
                .clone()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| "vi".into())
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

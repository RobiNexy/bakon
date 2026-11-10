//! 存储打开、锁、清单读写与提交。
//!
//! 打开规则严格按 `design/OPS__store-identity-recovery-and-retention.md` §1 执行：
//! 不存在的目录会被创建；非空且无标记的目录被拒绝，避免写入他人的数据。

mod marker;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::clock::now_rfc3339;
use crate::fsutil::{remove_dir_contents, write_atomic};
use crate::manifest::{Manifest, Source, VersionRecord};
use crate::object::{ObjectStore, hash_bytes};
use crate::types::{AbsPath, BlobHash, FileId, Ver};
use crate::{Error, Result};

pub use crate::SUPPORTED_SCHEMA;
pub use marker::{MARKER_FILE, Marker};

const LOCK_FILE: &str = ".lock";
const PENDING_FILE: &str = ".pending";
const INDEX_FILE: &str = "index.json";
const FILES_DIR: &str = "files";
const TRASH_DIR: &str = "trash";

/// 存储创建时必须存在的子目录。
const LAYOUT_DIRS: [&str; 4] = ["files", "objects", "objects/tmp", "trash"];

/// 已打开并校验过的存储。
#[derive(Debug)]
pub struct Store {
    root: AbsPath,
    marker: Marker,
    objects: ObjectStore,
    index: Option<IndexCache>,
}

/// 已准备好、尚未持久化的提交。
#[derive(Debug)]
pub struct Prepared {
    manifest: Manifest,
    content: Vec<u8>,
}

impl Prepared {
    pub fn id(&self) -> FileId {
        self.manifest.id
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
}

/// 已提交的版本；只有它才能触发目标文件写回（M2）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Committed {
    id: FileId,
    version: Ver,
    source: Source,
}

impl Committed {
    pub fn id(self) -> FileId {
        self.id
    }

    pub fn version(self) -> Ver {
        self.version
    }

    pub fn source(self) -> Source {
        self.source
    }
}

impl Store {
    /// 打开存储；不存在或为空时初始化。返回 `(store, created)`。
    pub fn init(root: impl AsRef<Path>) -> Result<(Self, bool)> {
        let root = AbsPath::new(root)?;
        let marker_path = root.as_path().join(MARKER_FILE);

        match fs::metadata(root.as_path()) {
            Ok(metadata) if !metadata.is_dir() => Err(Error::Io(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("not a directory: {}", root.as_path().display()),
            ))),
            Ok(_) if marker_path.is_file() => {
                let marker = read_marker(&marker_path)?;
                marker.validate()?;
                ensure_layout(&root)?;
                Ok((Self::compose(root, marker), false))
            }
            Ok(_) if is_empty_dir(&root)? => {
                let marker = initialize(&root)?;
                Ok((Self::compose(root, marker), true))
            }
            Ok(_) => Err(Error::NotAStore { root }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir_all(root.as_path()).map_err(Error::Io)?;
                let marker = initialize(&root)?;
                Ok((Self::compose(root, marker), true))
            }
            Err(error) => Err(Error::Io(error)),
        }
    }

    fn compose(root: AbsPath, marker: Marker) -> Self {
        let objects = ObjectStore::new(&root);
        Self {
            root,
            marker,
            objects,
            index: None,
        }
    }

    /// 打开已存在的存储；不关心是否刚刚创建。
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Self::init(root).map(|(store, _)| store)
    }

    pub fn root(&self) -> &AbsPath {
        &self.root
    }

    pub fn marker(&self) -> &Marker {
        &self.marker
    }

    /// 取得排他写锁，并执行恢复动作（清理暂存、检查待写回标记）。
    pub fn begin_write(&self, timeout: Duration) -> Result<StoreLock> {
        let guard = self.acquire(true, timeout)?;
        self.recover()?;
        Ok(guard)
    }

    /// 取得共享读锁。
    pub fn lock_shared(&self, timeout: Duration) -> Result<StoreLock> {
        self.acquire(false, timeout)
    }

    /// 路径 → 身份；缓存未命中时扫描清单并修复缓存。
    pub fn resolve(&mut self, path: &AbsPath) -> Result<Option<FileId>> {
        let key = path.as_path().to_string_lossy().into_owned();
        let cached = self.index_cache().entries.get(&key).copied();

        if let Some(id) = cached {
            match self.load_manifest_optional(id)? {
                Some(manifest) if manifest.path == *path => return Ok(Some(id)),
                _ => {
                    self.index_cache().entries.remove(&key);
                }
            }
        }

        for id in self.list_ids()? {
            let manifest = self.manifest(id)?;
            if manifest.path == *path {
                self.index_cache().entries.insert(key, id);
                self.save_index();
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// 读取并校验清单。
    pub fn manifest(&self, id: FileId) -> Result<Manifest> {
        let manifest = self
            .load_manifest_optional(id)?
            .ok_or_else(|| Error::Corrupt {
                what: "manifest",
                detail: format!("{id} is missing"),
            })?;
        Ok(manifest)
    }

    /// 读取对象内容。
    pub fn content(&self, hash: BlobHash) -> Result<Vec<u8>> {
        self.objects.get(hash)
    }

    /// 最新版本记录。
    pub fn head(&self, id: FileId) -> Result<Option<VersionRecord>> {
        Ok(self.manifest(id)?.head().cloned())
    }

    /// 构造纳管提交：生成身份与基线版本。调用方必须持有写锁。
    pub fn prepare_track(&self, path: &AbsPath, content: &[u8]) -> Result<Prepared> {
        let id = self.next_file_id()?;
        let baseline = VersionRecord::new(
            Ver::ZERO,
            hash_bytes(content),
            content.len() as u64,
            now_rfc3339()?,
            Source::Baseline,
        );
        let manifest = Manifest::baseline(id, path.clone(), baseline);
        Ok(Prepared {
            manifest,
            content: content.to_vec(),
        })
    }

    /// 构造追加提交；版本号与时间在这里生成。调用方必须持有写锁。
    pub fn prepare_commit(&self, id: FileId, content: &[u8], source: Source) -> Result<Prepared> {
        let mut manifest = self.manifest(id)?;
        let head = manifest.max_ver().ok_or_else(|| Error::Corrupt {
            what: "manifest",
            detail: format!("{id} has no versions"),
        })?;
        let record = VersionRecord::new(
            Ver::next(head)?,
            hash_bytes(content),
            content.len() as u64,
            now_rfc3339()?,
            source,
        );
        manifest.append(record);
        manifest.trim(manifest.effective_max_versions());
        Ok(Prepared {
            manifest,
            content: content.to_vec(),
        })
    }

    /// 持久化提交：先写对象，再原子替换清单，最后更新派生缓存。
    ///
    /// `expected_head` 是调用方在进入编辑器前读到的 head；不一致即冲突退出。
    pub fn commit(&mut self, prepared: Prepared, expected_head: Option<Ver>) -> Result<Committed> {
        let id = prepared.manifest.id;
        let current = self.load_manifest_optional(id)?;
        let actual = current.as_ref().and_then(Manifest::max_ver);

        match (expected_head, actual) {
            (None, None) => {}
            (Some(expected), Some(actual)) if expected == actual => {}
            (Some(expected), Some(actual)) => {
                return Err(Error::HeadChanged { expected, actual });
            }
            _ => {
                return Err(Error::Internal(format!(
                    "manifest head for {id} changed while tracking"
                )));
            }
        }

        let newest = prepared
            .manifest
            .head()
            .cloned()
            .ok_or_else(|| Error::Corrupt {
                what: "manifest",
                detail: format!("{id} has no versions"),
            })?;
        if newest.blob != hash_bytes(&prepared.content) {
            return Err(Error::Internal(format!(
                "prepared content does not match {id}"
            )));
        }

        self.objects.put(&prepared.content)?;
        self.save_manifest(&prepared.manifest)?;
        self.remember(&prepared.manifest);

        Ok(Committed {
            id,
            version: newest.ver,
            source: newest.source,
        })
    }

    fn manifest_path(&self, id: FileId) -> AbsPath {
        self.root.join(FILES_DIR).join(format!("{id}.json"))
    }

    fn index_path(&self) -> AbsPath {
        self.root.join(INDEX_FILE)
    }

    fn load_manifest_optional(&self, id: FileId) -> Result<Option<Manifest>> {
        let path = self.manifest_path(id);
        let bytes = match fs::read(path.as_path()) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(Error::Io(error)),
        };
        let manifest: Manifest =
            serde_json::from_slice(&bytes).map_err(|error| Error::Corrupt {
                what: "manifest",
                detail: format!("{id}: {error}"),
            })?;
        manifest.validate()?;
        if manifest.id != id {
            return Err(Error::Corrupt {
                what: "manifest",
                detail: format!("{id} contains a different id"),
            });
        }
        Ok(Some(manifest))
    }

    fn save_manifest(&self, manifest: &Manifest) -> Result<()> {
        let encoded = serde_json::to_vec_pretty(manifest)
            .map_err(|error| Error::Internal(format!("cannot encode manifest: {error}")))?;
        let files = self.root.join(FILES_DIR);
        write_atomic(&self.manifest_path(manifest.id), &files, &encoded)
    }

    fn next_file_id(&self) -> Result<FileId> {
        loop {
            let id = FileId::generate();
            if !self.manifest_path(id).as_path().exists() {
                return Ok(id);
            }
        }
    }

    fn list_ids(&self) -> Result<Vec<FileId>> {
        let directory = self.root.join(FILES_DIR);
        let entries = fs::read_dir(directory.as_path()).map_err(Error::Io)?;
        let mut ids = BTreeSet::new();
        for entry in entries {
            let entry = entry.map_err(Error::Io)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(stem) = name.strip_suffix(".json") else {
                continue;
            };
            if let Some(id) = FileId::from_hex(stem) {
                ids.insert(id);
            }
        }
        Ok(ids.into_iter().collect())
    }

    fn index_cache(&mut self) -> &mut IndexCache {
        if self.index.is_none() {
            self.index = Some(self.load_index());
        }
        self.index.as_mut().expect("index was just initialized")
    }

    fn load_index(&self) -> IndexCache {
        let bytes = match fs::read(self.index_path().as_path()) {
            Ok(bytes) => bytes,
            Err(_) => return IndexCache::blank(),
        };
        match serde_json::from_slice::<IndexCache>(&bytes) {
            Ok(index) if index.schema == SUPPORTED_SCHEMA => index,
            _ => IndexCache::blank(),
        }
    }

    /// 缓存是派生数据；写入失败不影响只读结果的正确性。
    fn save_index(&self) {
        let Some(index) = self.index.as_ref() else {
            return;
        };
        let Ok(encoded) = serde_json::to_vec_pretty(index) else {
            return;
        };
        let _ = write_atomic(&self.index_path(), &self.root, &encoded);
    }

    fn remember(&mut self, manifest: &Manifest) {
        let key = manifest.path.as_path().to_string_lossy().into_owned();
        self.index_cache().entries.insert(key, manifest.id);
        self.save_index();
    }

    fn recover(&self) -> Result<()> {
        if self.root.join(PENDING_FILE).as_path().exists() {
            return Err(Error::Corrupt {
                what: PENDING_FILE,
                detail: "a pending apply was found; recovery needs revert support".into(),
            });
        }
        self.objects.cleanup_tmp()?;
        remove_dir_contents(self.root.join(TRASH_DIR).as_path())
    }

    fn acquire(&self, exclusive: bool, timeout: Duration) -> Result<StoreLock> {
        let path = self.root.join(LOCK_FILE);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.as_path())
            .map_err(Error::Io)?;

        let started = Instant::now();
        loop {
            let attempt: io::Result<()> = if exclusive {
                file.try_lock()
            } else {
                file.try_lock_shared()
            }
            .map_err(io::Error::from);
            match attempt {
                Ok(()) => {
                    if exclusive {
                        self.write_holder(&file);
                    }
                    return Ok(StoreLock { file });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    let waited = started.elapsed();
                    if waited >= timeout {
                        return Err(Error::LockTimeout {
                            holder: self.read_holder(),
                            waited,
                        });
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => return Err(Error::Io(error)),
            }
        }
    }

    /// `.lock` 的内容只用于错误提示，锁语义由 flock 决定。
    fn write_holder(&self, file: &File) {
        let mut handle = file;
        let _ = handle.set_len(0);
        let _ = writeln!(handle, "{}", std::process::id());
        let _ = handle.sync_all();
    }

    fn read_holder(&self) -> Option<u32> {
        let text = fs::read_to_string(self.root.join(LOCK_FILE).as_path()).ok()?;
        text.trim().parse().ok()
    }
}

/// 持锁守卫；drop 时解锁。进程崩溃时内核也会释放 flock。
#[derive(Debug)]
pub struct StoreLock {
    file: File,
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexCache {
    #[serde(default)]
    schema: u32,
    #[serde(default)]
    entries: BTreeMap<String, FileId>,
}

impl IndexCache {
    fn blank() -> Self {
        Self {
            schema: SUPPORTED_SCHEMA,
            entries: BTreeMap::new(),
        }
    }
}

/// 原子写入标记：临时文件 → fsync → rename → 目录 fsync。
fn initialize(root: &AbsPath) -> Result<Marker> {
    let marker = Marker::new()?;
    let mut encoded = serde_json::to_vec_pretty(&marker)
        .map_err(|error| Error::Internal(format!("cannot encode store marker: {error}")))?;
    encoded.push(b'\n');
    write_atomic(&root.join(MARKER_FILE), root, &encoded)?;
    ensure_layout(root)?;
    Ok(marker)
}

/// 创建缺失的子目录；已有的目录保持不变。
fn ensure_layout(root: &AbsPath) -> Result<()> {
    for directory in LAYOUT_DIRS {
        fs::create_dir_all(root.join(directory).as_path()).map_err(Error::Io)?;
    }
    crate::fsutil::fsync_dir(root.as_path())
}

fn is_empty_dir(root: &AbsPath) -> Result<bool> {
    let mut entries = fs::read_dir(root.as_path()).map_err(Error::Io)?;
    Ok(entries.next().is_none())
}

fn read_marker(path: &Path) -> Result<Marker> {
    let bytes = fs::read(path).map_err(Error::Io)?;
    serde_json::from_slice(&bytes).map_err(|error| Error::Corrupt {
        what: "store marker",
        detail: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;

    fn marker_json(schema: u32) -> String {
        serde_json::json!({
            "schema": schema,
            "uuid": "00000000-0000-4000-8000-000000000000",
            "created": "2026-11-10T12:00:00Z",
            "tool": "bakon/1.0"
        })
        .to_string()
    }

    fn rewrite_marker(store: &Path, schema: u32) {
        fs::write(store.join(MARKER_FILE), marker_json(schema)).unwrap();
    }

    fn open_store() -> (TempDir, Store) {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        let (store, _) = Store::init(&root).unwrap();
        (temporary, store)
    }

    fn path(name: &str) -> AbsPath {
        AbsPath::new(format!("/tmp/bakon-tests/{name}")).unwrap()
    }

    #[test]
    fn init_creates_marker_and_layout() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");

        let (store, created) = Store::init(&root).unwrap();

        assert!(created);
        assert!(store.root().as_path().join(MARKER_FILE).is_file());
        for directory in LAYOUT_DIRS {
            assert!(
                store.root().as_path().join(directory).is_dir(),
                "missing {directory}"
            );
        }
        assert_eq!(store.marker().schema, SUPPORTED_SCHEMA);
        store.marker().validate().unwrap();
    }

    #[test]
    fn init_is_idempotent() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");

        let (_, created) = Store::init(&root).unwrap();
        let (_, created_again) = Store::init(&root).unwrap();

        assert!(created);
        assert!(!created_again);
    }

    #[test]
    fn empty_existing_directory_gets_a_marker() {
        let temporary = TempDir::new().unwrap();
        let (_, created) = Store::init(temporary.path()).unwrap();

        assert!(created);
        assert!(temporary.path().join(MARKER_FILE).is_file());
    }

    #[test]
    fn non_empty_directory_without_marker_is_refused() {
        let temporary = TempDir::new().unwrap();
        fs::write(temporary.path().join("notes.txt"), b"not a store").unwrap();

        let error = Store::init(temporary.path()).unwrap_err();

        assert!(matches!(error, Error::NotAStore { .. }));
        assert_eq!(error.exit_code(), 6);
    }

    #[test]
    fn newer_schema_is_refused() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        Store::init(&root).unwrap();
        rewrite_marker(&root, SUPPORTED_SCHEMA + 1);

        let error = Store::init(&root).unwrap_err();

        assert!(
            matches!(error, Error::SchemaTooNew { found, .. } if found == SUPPORTED_SCHEMA + 1)
        );
        assert_eq!(error.exit_code(), 6);
    }

    #[test]
    fn older_schema_is_refused() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        Store::init(&root).unwrap();
        rewrite_marker(&root, SUPPORTED_SCHEMA - 1);

        let error = Store::init(&root).unwrap_err();

        assert!(
            matches!(error, Error::SchemaTooOld { found, .. } if found == SUPPORTED_SCHEMA - 1)
        );
        assert_eq!(error.exit_code(), 6);
    }

    #[test]
    fn corrupt_marker_is_refused() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        Store::init(&root).unwrap();
        fs::write(root.join(MARKER_FILE), b"{").unwrap();

        let error = Store::init(&root).unwrap_err();

        assert!(matches!(error, Error::Corrupt { .. }));
        assert_eq!(error.exit_code(), 6);
    }

    #[test]
    fn a_file_in_place_of_the_store_is_an_io_error() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        fs::write(&root, b"file").unwrap();

        let error = Store::init(&root).unwrap_err();

        assert!(matches!(error, Error::Io(_)));
        assert_eq!(error.exit_code(), 8);
    }

    #[test]
    fn tracking_then_committing_builds_a_history() {
        let (_temporary, mut store) = open_store();
        let file = path("config");

        let baseline = store.prepare_track(&file, b"one").unwrap();
        let committed = store.commit(baseline, None).unwrap();
        assert_eq!(committed.version(), Ver::ZERO);

        let next = store
            .prepare_commit(committed.id(), b"two", Source::Edit)
            .unwrap();
        let committed = store.commit(next, Some(Ver::ZERO)).unwrap();
        assert_eq!(committed.version().get(), 1);

        let manifest = store.manifest(committed.id()).unwrap();
        assert_eq!(manifest.versions.len(), 2);
        assert_eq!(manifest.path, file);
        assert_eq!(store.content(manifest.versions[0].blob).unwrap(), b"one");
        assert_eq!(store.content(manifest.versions[1].blob).unwrap(), b"two");
    }

    #[test]
    fn commit_conflict_keeps_the_old_manifest() {
        let (_temporary, mut store) = open_store();
        let file = path("conflict");
        let baseline = store.prepare_track(&file, b"base").unwrap();
        let committed = store.commit(baseline, None).unwrap();

        let first = store
            .prepare_commit(committed.id(), b"first", Source::Edit)
            .unwrap();
        store.commit(first, Some(Ver::ZERO)).unwrap();

        let stale = store
            .prepare_commit(committed.id(), b"stale", Source::Edit)
            .unwrap();
        let error = store.commit(stale, Some(Ver::ZERO)).unwrap_err();

        assert!(matches!(error, Error::HeadChanged { expected, actual }
            if expected == Ver::ZERO && actual.get() == 1));
        assert_eq!(error.exit_code(), 4);
        assert_eq!(store.manifest(committed.id()).unwrap().versions.len(), 2);
    }

    #[test]
    fn resolve_uses_and_repairs_the_cache() {
        let (_temporary, mut store) = open_store();
        let file = path("resolved");
        let baseline = store.prepare_track(&file, b"content").unwrap();
        let committed = store.commit(baseline, None).unwrap();

        assert_eq!(store.resolve(&file).unwrap(), Some(committed.id()));

        fs::write(store.index_path().as_path(), b"not json").unwrap();
        store.index = None;
        assert_eq!(store.resolve(&file).unwrap(), Some(committed.id()));

        let index = fs::read_to_string(store.index_path().as_path()).unwrap();
        assert!(index.contains(&committed.id().to_hex()));
    }

    #[test]
    fn resolve_returns_none_for_unknown_paths() {
        let (_temporary, mut store) = open_store();
        assert_eq!(store.resolve(&path("absent")).unwrap(), None);
    }

    #[test]
    fn a_corrupt_manifest_is_reported_not_skipped() {
        let (_temporary, mut store) = open_store();
        let file = path("corrupt");
        let baseline = store.prepare_track(&file, b"content").unwrap();
        let committed = store.commit(baseline, None).unwrap();

        let manifest_path = store.manifest_path(committed.id());
        fs::write(manifest_path.as_path(), b"{").unwrap();
        store.index = None;

        assert!(matches!(store.resolve(&file), Err(Error::Corrupt { .. })));
    }

    #[test]
    fn trim_runs_during_commit() {
        let (_temporary, mut store) = open_store();
        let file = path("trimmed");
        let baseline = store.prepare_track(&file, b"v0").unwrap();
        let committed = store.commit(baseline, None).unwrap();

        let mut manifest = store.manifest(committed.id()).unwrap();
        manifest.retention =
            crate::manifest::Retention::Limit(std::num::NonZeroU32::new(2).unwrap());
        store.save_manifest(&manifest).unwrap();

        let mut head = committed.version();
        for index in 1..5 {
            let prepared = store
                .prepare_commit(committed.id(), format!("v{index}").as_bytes(), Source::Edit)
                .unwrap();
            store.commit(prepared, Some(head)).unwrap();
            head = Ver::new(index).unwrap();
        }

        let manifest = store.manifest(committed.id()).unwrap();
        let versions: Vec<u32> = manifest
            .versions
            .iter()
            .map(|record| record.ver.get())
            .collect();
        assert_eq!(versions, vec![3, 4]);
    }

    #[test]
    fn write_lock_excludes_a_second_writer() {
        let (_temporary, store) = open_store();
        let first = store.begin_write(Duration::from_millis(50)).unwrap();
        let error = store.begin_write(Duration::from_millis(50)).unwrap_err();
        assert!(matches!(error, Error::LockTimeout { .. }));
        drop(first);

        assert!(store.begin_write(Duration::from_millis(50)).is_ok());
    }

    #[test]
    fn recovery_clears_temporary_files_and_trash() {
        let (_temporary, store) = open_store();
        fs::write(
            store.root().join("objects/tmp/leftover.tmp").as_path(),
            b"x",
        )
        .unwrap();
        fs::write(store.root().join("trash/old.object").as_path(), b"y").unwrap();

        let _guard = store.begin_write(Duration::from_millis(50)).unwrap();

        assert_eq!(
            fs::read_dir(store.root().join("objects/tmp").as_path())
                .unwrap()
                .count(),
            0
        );
        assert_eq!(
            fs::read_dir(store.root().join("trash").as_path())
                .unwrap()
                .count(),
            0
        );
    }
}

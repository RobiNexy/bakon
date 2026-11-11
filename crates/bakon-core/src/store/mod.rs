//! 存储打开、锁、清单读写与提交。
//!
//! 打开规则严格按 `design/OPS__store-identity-recovery-and-retention.md` §1 执行：
//! 不存在的目录会被创建；非空且无标记的目录被拒绝，避免写入他人的数据。

mod marker;
mod verify;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::clock::now_rfc3339;
use crate::fsutil::{WriteOutcome, remove_dir_contents, write_atomic};
use crate::manifest::{Codec, Manifest, Source, VersionRecord};
use crate::object::{ObjectStore, hash_bytes};
use crate::types::{AbsPath, BlobHash, FileId, Ver};
use crate::{Error, Result};

pub use crate::SUPPORTED_SCHEMA;
pub use marker::{MARKER_FILE, Marker};
pub use verify::{SizeEntry, VerifyItem, VerifyOptions, VerifyReport};

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
    default_max_versions: u32,
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

    /// 标记最新版本为「已记录、待写回」。
    pub fn set_pending(&mut self, pending: bool) {
        if let Some(head) = self.manifest.head_mut() {
            head.pending_apply = pending;
        }
    }

    /// 清除任意一个历史版本的待写回标记。
    pub fn clear_pending(&mut self, version: Ver) {
        for record in &mut self.manifest.versions {
            if record.ver == version {
                record.pending_apply = false;
            }
        }
    }
}

/// 已提交的版本；只有它才能触发目标文件写回（M2）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Committed {
    id: FileId,
    version: Ver,
    source: Source,
    trim_failure: Option<TrimFailure>,
}

/// 已提交版本的旧对象回收失败；提交本身仍然有效。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrimFailure {
    pub hash: Option<BlobHash>,
    pub detail: String,
}

/// 一个文件被裁剪的版本集合。
#[derive(Clone, Debug)]
pub struct PrunedFile {
    pub id: FileId,
    pub path: AbsPath,
    pub versions: Vec<Ver>,
}

/// 裁剪结果；`dry_run` 时只报告、不改变持久状态。
#[derive(Clone, Debug, Default)]
pub struct PruneReport {
    pub files: Vec<PrunedFile>,
}

impl PruneReport {
    pub fn removed(&self) -> usize {
        self.files.iter().map(|file| file.versions.len()).sum()
    }
}

impl Committed {
    pub fn id(&self) -> FileId {
        self.id
    }

    pub fn version(&self) -> Ver {
        self.version
    }

    pub fn source(&self) -> Source {
        self.source
    }

    /// 裁剪后的对象回收是否失败；失败不影响已经提交的版本。
    pub fn trim_failed(&self) -> bool {
        self.trim_failure.is_some()
    }

    /// 具体失败对象与原因；成功时为 `None`。
    pub fn trim_failure(&self) -> Option<&TrimFailure> {
        self.trim_failure.as_ref()
    }

    /// 只有已提交的版本才能写回目标文件；调用方必须持有写锁。
    pub fn apply(self, _lock: &WriteLock, store: &Store, target: &AbsPath) -> Result<()> {
        let manifest = store.manifest(self.id)?;
        let record = manifest
            .find(self.version)
            .ok_or_else(|| Error::NoSuchVersion {
                ver: self.version,
                latest: manifest.max_ver().unwrap_or(self.version),
            })?;
        let content = store.content_for(record)?;
        crate::apply::write_target(target, &content)
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
                ensure_layout(&root, true)?;
                Ok((Self::compose(root, marker), false))
            }
            Ok(_) if is_empty_dir(&root)? => {
                let marker = initialize(&root)?;
                Ok((Self::compose(root, marker), true))
            }
            Ok(_) => Err(Error::NotAStore { root }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                crate::fsutil::create_private_dir_all(root.as_path())?;
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
            default_max_versions: crate::manifest::DEFAULT_MAX_VERSIONS,
        }
    }

    /// 打开已存在的存储；不存在、空目录或无标记的目录返回 `NotAStore`，不创建。
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
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
                ensure_layout(&root, false)?;
                Ok(Self::compose(root, marker))
            }
            Ok(_) => Err(Error::NotAStore { root }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Err(Error::NotAStore { root }),
            Err(error) => Err(Error::Io(error)),
        }
    }

    pub fn root(&self) -> &AbsPath {
        &self.root
    }

    pub fn marker(&self) -> &Marker {
        &self.marker
    }

    /// 设置全局保留上限；`0` 表示不限制。缺省为 [`crate::manifest::DEFAULT_MAX_VERSIONS`]。
    pub fn set_default_max_versions(&mut self, value: u32) {
        self.default_max_versions = value;
    }

    /// 取得排他写锁，并执行恢复动作（清理暂存、检查待写回标记）。
    pub fn begin_write(&mut self, timeout: Duration) -> Result<WriteLock> {
        let guard = self.begin_verify(timeout)?;
        self.recover(&guard)?;
        Ok(guard)
    }

    /// 取得排他锁但不自动恢复；`verify --fix` 在自己的报告流程里处理 `.pending`。
    pub fn begin_verify(&self, timeout: Duration) -> Result<WriteLock> {
        self.acquire_exclusive(timeout)
    }

    /// 取得共享读锁。
    pub fn lock_shared(&self, timeout: Duration) -> Result<ReadLock> {
        self.acquire_shared(timeout)
    }

    /// 路径 → 身份；缓存未命中时扫描清单并修复缓存。
    pub fn resolve(&mut self, path: &AbsPath) -> Result<Option<FileId>> {
        let key = path.key();
        let cached = self.index_cache().entries.get(&key).copied();

        if let Some(id) = cached {
            match self.load_manifest_optional(id)? {
                Some(manifest) if manifest.path.key() == key => return Ok(Some(id)),
                _ => {
                    self.index_cache().entries.remove(&key);
                }
            }
        }

        for id in self.list_ids()? {
            let manifest = self.manifest(id)?;
            if manifest.path.key() == key {
                self.index_cache().entries.insert(key, id);
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

    /// 按版本记录声明的编码读取内容。
    pub fn content_for(&self, record: &VersionRecord) -> Result<Vec<u8>> {
        match record.codec {
            Codec::Deflate => self.objects.get(record.blob),
        }
    }

    /// 为已存在的版本构造写回句柄；调用方必须持有写锁。
    pub fn committed(&self, _lock: &WriteLock, id: FileId, version: Ver) -> Result<Committed> {
        let manifest = self.manifest(id)?;
        let record = manifest.find(version).ok_or_else(|| Error::NoSuchVersion {
            ver: version,
            latest: manifest.max_ver().unwrap_or(version),
        })?;
        Ok(Committed {
            id,
            version,
            source: record.source,
            trim_failure: None,
        })
    }

    /// 最新版本记录。
    pub fn head(&self, id: FileId) -> Result<Option<VersionRecord>> {
        Ok(self.manifest(id)?.head().cloned())
    }

    /// 按升序返回全部版本记录。
    pub fn versions(&self, id: FileId) -> Result<Vec<VersionRecord>> {
        Ok(self.manifest(id)?.versions)
    }

    /// 构造纳管提交：生成身份与基线版本。
    pub fn prepare_track(
        &self,
        _lock: &WriteLock,
        path: &AbsPath,
        content: &[u8],
    ) -> Result<Prepared> {
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

    /// 构造追加提交；版本号与时间在这里生成。
    pub fn prepare_commit(
        &self,
        _lock: &WriteLock,
        id: FileId,
        content: &[u8],
        source: Source,
    ) -> Result<Prepared> {
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
        manifest.trim(manifest.effective_max_versions(self.default_max_versions));
        Ok(Prepared {
            manifest,
            content: content.to_vec(),
        })
    }

    /// 持久化提交：先写对象，再原子替换清单，最后更新派生缓存。
    ///
    /// `expected_head` 是调用方在进入编辑器前读到的 head；不一致即冲突退出。
    pub fn commit(
        &mut self,
        lock: &WriteLock,
        prepared: Prepared,
        expected_head: Option<Ver>,
    ) -> Result<Committed> {
        let id = prepared.manifest.id;
        if expected_head.is_none() && self.resolve(&prepared.manifest.path)?.is_some() {
            return Err(Error::AlreadyManaged {
                path: prepared.manifest.path.clone(),
            });
        }
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

        let removed: Vec<VersionRecord> = current
            .as_ref()
            .map(|manifest| {
                let kept: BTreeSet<Ver> = prepared
                    .manifest
                    .versions
                    .iter()
                    .map(|record| record.ver)
                    .collect();
                manifest
                    .versions
                    .iter()
                    .filter(|record| !kept.contains(&record.ver))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        self.objects.put(&prepared.content)?;
        self.save_manifest(lock, &prepared.manifest)?;
        self.remember(lock, &prepared.manifest);

        // 裁剪已经作为清单提交的一部分生效；对象回收失败时保留孤儿，
        // 由 verify --fix 或下一次 prune 收敛，不回滚已提交的版本。
        let trim_failure = if removed.is_empty() {
            None
        } else {
            self.reclaim_removed(lock, &removed).err()
        };

        Ok(Committed {
            id,
            version: newest.ver,
            source: newest.source,
            trim_failure,
        })
    }

    pub fn manifest_path(&self, id: FileId) -> AbsPath {
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

    pub(crate) fn save_manifest(&self, _lock: &WriteLock, manifest: &Manifest) -> Result<()> {
        let encoded = serde_json::to_vec_pretty(manifest)
            .map_err(|error| Error::Internal(format!("cannot encode manifest: {error}")))?;
        let files = self.root.join(FILES_DIR);
        match write_atomic(&self.manifest_path(manifest.id), &files, &encoded)? {
            WriteOutcome::Durable => Ok(()),
            WriteOutcome::VisibleButNotDurable(source) => Err(Error::DurabilityUncertain {
                id: manifest.id,
                version: manifest.max_ver(),
                source,
            }),
        }
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

    /// 全部被管理文件的身份，按清单文件名排序。
    pub fn all_ids(&self) -> Result<Vec<FileId>> {
        self.list_ids()
    }

    /// 只改路径映射，不动磁盘文件。
    pub fn rename(&mut self, lock: &WriteLock, id: FileId, new_path: AbsPath) -> Result<()> {
        let mut manifest = self.manifest(id)?;
        if manifest.path.key() == new_path.key() {
            return Ok(());
        }
        if self.resolve(&new_path)?.is_some() {
            return Err(Error::AlreadyManaged { path: new_path });
        }

        manifest.path = new_path;
        self.save_manifest(lock, &manifest)?;

        let keep = manifest.path.key();
        let stale: Vec<String> = self
            .index_cache()
            .entries
            .iter()
            .filter(|(key, value)| **value == id && key.as_str() != keep)
            .map(|(key, _)| key.clone())
            .collect();
        for key in stale {
            self.index_cache().entries.remove(&key);
        }
        // 不依赖 remember 的“键已存在则早退”：改名必须把磁盘索引里的旧键删掉。
        self.index_cache().entries.insert(keep, id);
        let _ = self.save_index(lock);
        Ok(())
    }

    /// 设置或清除该文件的变更钩子。
    pub fn set_hook(&self, lock: &WriteLock, id: FileId, hook: Option<String>) -> Result<()> {
        let mut manifest = self.manifest(id)?;
        manifest.hook = hook;
        self.save_manifest(lock, &manifest)
    }

    /// 按生效上限裁剪，并回收不再被任何清单引用的对象。
    pub fn prune(&mut self, lock: &WriteLock, ids: &[FileId]) -> Result<PruneReport> {
        self.prune_inner(Some(lock), ids, false)
    }

    /// `--dry-run` 只报告，不移动对象、不写清单；调用方持共享锁。
    pub fn prune_dry_run(&mut self, ids: &[FileId]) -> Result<PruneReport> {
        self.prune_inner(None, ids, true)
    }

    fn prune_inner(
        &mut self,
        lock: Option<&WriteLock>,
        ids: &[FileId],
        dry_run: bool,
    ) -> Result<PruneReport> {
        let lock =
            if dry_run {
                None
            } else {
                Some(lock.ok_or_else(|| {
                    Error::Internal("prune write path requires a write lock".into())
                })?)
            };
        let unique: Vec<FileId> = ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let ids = unique.as_slice();
        let all_ids = self.list_ids()?;
        let selected: BTreeSet<FileId> = ids.iter().copied().collect();
        let mut selected_manifests = BTreeMap::new();
        for id in &all_ids {
            if selected.contains(id) {
                selected_manifests.insert(*id, self.manifest(*id)?);
            }
        }

        let needs_trim = selected_manifests.values().any(|manifest| {
            let limit = manifest.effective_max_versions(self.default_max_versions);
            limit > 0 && manifest.versions.len() > limit as usize
        });
        if !needs_trim {
            if !dry_run && selected.len() == all_ids.len() {
                // 全库 prune 仍负责回收崩溃留下的孤儿。
                self.reclaim_orphans(lock.expect("write mode has a lock"))?;
            }
            return Ok(PruneReport::default());
        }

        let mut manifests = BTreeMap::new();
        for id in all_ids {
            manifests.insert(id, self.manifest(id)?);
        }

        let mut counts: std::collections::BTreeMap<BlobHash, usize> =
            std::collections::BTreeMap::new();
        if !dry_run {
            for manifest in manifests.values() {
                for record in &manifest.versions {
                    *counts.entry(record.blob).or_default() += 1;
                }
            }
        }

        let mut report = PruneReport::default();
        for id in ids {
            let Some(manifest) = manifests.get(id).cloned() else {
                continue;
            };
            let limit = manifest.effective_max_versions(self.default_max_versions);
            if limit == 0 || manifest.versions.len() <= limit as usize {
                continue;
            }

            let remove_count = manifest.versions.len() - limit as usize;
            let removed: Vec<VersionRecord> = manifest.versions[..remove_count].to_vec();
            report.files.push(PrunedFile {
                id: *id,
                path: manifest.path.clone(),
                versions: removed.iter().map(|record| record.ver).collect(),
            });
            if dry_run {
                continue;
            }

            let mut candidates = Vec::new();
            for record in &removed {
                let count = counts.get_mut(&record.blob).ok_or_else(|| {
                    Error::Internal(format!("blob {} was not counted", record.blob))
                })?;
                *count = count.checked_sub(1).ok_or_else(|| {
                    Error::Internal(format!("blob {} count underflow", record.blob))
                })?;
                if *count == 0 {
                    candidates.push(record.blob);
                }
            }
            let mut trimmed = manifest;
            trimmed.versions.drain(..remove_count);
            self.save_manifest(lock.expect("write mode has a lock"), &trimmed)?;

            // 清单先行：提交后这些对象才不可达，崩溃残留最多是垃圾。
            for hash in candidates {
                self.move_to_trash(lock.expect("write mode has a lock"), hash)?;
            }
        }

        if !dry_run {
            // 兜底回收：提交裁剪、prune 崩溃或失败提交留下的孤儿对象。
            self.reclaim_orphans(lock.expect("write mode has a lock"))?;
        }
        Ok(report)
    }

    pub(crate) fn move_to_trash(&self, _lock: &WriteLock, hash: BlobHash) -> Result<()> {
        let from = self.objects.path(hash);
        let to = self.root.join(TRASH_DIR).join(hash.to_hex());
        match fs::rename(from.as_path(), to.as_path()) {
            Ok(()) => {
                crate::fsutil::durability_point();
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Error::Io(error)),
        }
    }

    /// 当前所有清单引用的对象集合。
    fn referenced_blobs(&self) -> Result<BTreeSet<BlobHash>> {
        let mut referenced = BTreeSet::new();
        for id in self.list_ids()? {
            for record in &self.manifest(id)?.versions {
                referenced.insert(record.blob);
            }
        }
        Ok(referenced)
    }

    /// 移动被裁剪版本引用、且已无任何清单引用的对象。
    fn reclaim_removed(
        &self,
        lock: &WriteLock,
        removed: &[VersionRecord],
    ) -> std::result::Result<usize, TrimFailure> {
        let candidates: BTreeSet<BlobHash> = removed.iter().map(|record| record.blob).collect();
        if candidates.is_empty() {
            return Ok(0);
        }
        let referenced = self.referenced_blobs().map_err(|error| TrimFailure {
            hash: None,
            detail: error.to_string(),
        })?;
        let mut moved = 0;
        for hash in candidates {
            if !referenced.contains(&hash) {
                self.move_to_trash(lock, hash)
                    .map_err(|error| TrimFailure {
                        hash: Some(hash),
                        detail: error.to_string(),
                    })?;
                moved += 1;
            }
        }
        remove_dir_contents(self.root.join(TRASH_DIR).as_path()).map_err(|error| TrimFailure {
            hash: None,
            detail: error.to_string(),
        })?;
        Ok(moved)
    }

    /// 回收给定的孤儿对象；调用方必须已经证明这些哈希不可达。
    pub(crate) fn reclaim_orphan_set(
        &self,
        lock: &WriteLock,
        orphans: &[BlobHash],
    ) -> Result<usize> {
        let mut moved = 0;
        for hash in orphans {
            self.move_to_trash(lock, *hash)?;
            moved += 1;
        }
        remove_dir_contents(self.root.join(TRASH_DIR).as_path())?;
        Ok(moved)
    }

    /// 回收所有不在任何清单中的对象。
    pub(crate) fn reclaim_orphans(&self, lock: &WriteLock) -> Result<usize> {
        let referenced = self.referenced_blobs()?;
        let mut moved = 0;
        for hash in self.objects.list_hashes()? {
            if !referenced.contains(&hash) {
                self.move_to_trash(lock, hash)?;
                moved += 1;
            }
        }
        remove_dir_contents(self.root.join(TRASH_DIR).as_path())?;
        Ok(moved)
    }

    /// 列出不在任何清单中的对象，不改变持久状态。
    #[cfg(test)]
    pub(crate) fn orphan_hashes(&self) -> Result<Vec<BlobHash>> {
        let referenced = self.referenced_blobs()?;
        Ok(self
            .objects
            .list_hashes()?
            .difference(&referenced)
            .copied()
            .collect())
    }

    pub(crate) fn objects(&self) -> &ObjectStore {
        &self.objects
    }

    /// 列出严格符合原子写临时文件命名的残留文件；不删除任何东西。
    fn temporary_files(&self) -> Result<Vec<AbsPath>> {
        let mut found = Vec::new();
        collect_temporary_files(
            &self.root,
            |target| [INDEX_FILE, PENDING_FILE, MARKER_FILE].contains(&target),
            &mut found,
        )?;
        collect_temporary_files(
            &self.root.join(FILES_DIR),
            |target| {
                target
                    .strip_suffix(".json")
                    .and_then(FileId::from_hex)
                    .is_some()
            },
            &mut found,
        )?;
        found.sort();
        Ok(found)
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
    fn save_index(&self, _lock: &WriteLock) -> Result<()> {
        let Some(index) = self.index.as_ref() else {
            return Ok(());
        };
        let encoded = serde_json::to_vec_pretty(index)
            .map_err(|error| Error::Internal(format!("cannot encode index: {error}")))?;
        write_atomic(&self.index_path(), &self.root, &encoded)?.require_durable()
    }

    fn remember(&mut self, lock: &WriteLock, manifest: &Manifest) {
        let key = manifest.path.key();
        let entry = self.index_cache().entries.get(&key).copied();
        if entry == Some(manifest.id) {
            return;
        }
        self.index_cache().entries.insert(key, manifest.id);
        let _ = self.save_index(lock);
    }

    /// 恢复 `.pending` 中最多一条记录。
    ///
    /// 返回被证明为“从未提交”的过期记录，供 `verify --fix` 报告并丢弃。
    /// 无法证明的记录保留 `.pending` 并返回 `Corrupt`，不得猜测后继续。
    fn recover(&mut self, lock: &WriteLock) -> Result<Vec<PendingEntry>> {
        let mut stale = Vec::new();
        for entry in self.read_pending()? {
            match self.recover_entry(lock, &entry)? {
                RecoveryStep::Recovered => {}
                RecoveryStep::Stale => stale.push(entry),
            }
        }
        self.clear_pending(lock)?;
        self.objects.cleanup_tmp()?;
        remove_dir_contents(self.root.join(TRASH_DIR).as_path())?;
        Ok(stale)
    }

    /// 恢复一条待写回记录。
    ///
    /// 目标内容等于新内容：只清标记。
    /// 目标内容等于崩溃前内容或缺失：写回 pending 版本。
    /// 目标内容既非旧值也非新值：先把外部内容记为 `salvage`，再追加一个
    /// `revert` 版本并写回，保证不丢弃崩溃后的外部编辑。
    ///
    /// 清单缺失、pending 版本不匹配、版本被裁剪或目标偏离且无法证明是 no-op 时，
    /// 保留 `.pending` 并返回损坏；只有“清单头小于记录版本”或“版本已记录且目标
    /// 已等于新内容”才判定为过期并丢弃。
    fn recover_entry(&mut self, lock: &WriteLock, entry: &PendingEntry) -> Result<RecoveryStep> {
        let manifest = self
            .load_manifest_optional(entry.id)?
            .ok_or_else(|| Error::Corrupt {
                what: PENDING_FILE,
                detail: format!("{} has no manifest", entry.id),
            })?;

        let pending = manifest
            .versions
            .iter()
            .find(|record| record.ver == entry.version && record.pending_apply)
            .cloned();

        let Some(pending) = pending else {
            if manifest.versions.iter().any(|record| record.pending_apply) {
                return Err(Error::Corrupt {
                    what: PENDING_FILE,
                    detail: format!(
                        "{} has a pending version that does not match version {} in .pending",
                        entry.id, entry.version
                    ),
                });
            }
            if let Some(record) = manifest
                .versions
                .iter()
                .find(|record| record.ver == entry.version)
            {
                if record.blob != entry.new_blob {
                    return Err(Error::Corrupt {
                        what: PENDING_FILE,
                        detail: format!(
                            "{} version {} has a different blob than .pending",
                            entry.id, entry.version
                        ),
                    });
                }
                let current = recovery_target(&manifest.path, entry.version)?;
                if current.as_ref().map(|bytes| hash_bytes(bytes)) == Some(entry.new_blob) {
                    return Ok(RecoveryStep::Stale);
                }
                return Err(Error::Corrupt {
                    what: PENDING_FILE,
                    detail: format!(
                        "{} version {} is recorded without pending_apply, but the target does not match",
                        entry.id, entry.version
                    ),
                });
            }
            if manifest.max_ver().is_none_or(|head| head < entry.version) {
                return Ok(RecoveryStep::Stale);
            }
            return Err(Error::Corrupt {
                what: PENDING_FILE,
                detail: format!(
                    "{} version {} was pruned or is otherwise unverifiable",
                    entry.id, entry.version
                ),
            });
        };

        if pending.blob != entry.new_blob {
            return Err(Error::Corrupt {
                what: PENDING_FILE,
                detail: format!(
                    "{} version {} blob does not match .pending",
                    entry.id, entry.version
                ),
            });
        }

        let content = self.content_for(&pending)?;
        let current = recovery_target(&manifest.path, pending.ver)?;
        let current_hash = current.as_ref().map(|bytes| hash_bytes(bytes));

        if current_hash == Some(pending.blob) {
            self.clear_pending_version(lock, entry.id, pending.ver)?;
            return Ok(RecoveryStep::Recovered);
        }

        let head_is_pending = manifest
            .head()
            .map(|head| head.ver == pending.ver)
            .unwrap_or(false);
        if (current.is_none() || current_hash == entry.old_blob) && head_is_pending {
            self.apply_pending(lock, &manifest.path, &content, pending.ver)?;
            self.clear_pending_version(lock, entry.id, pending.ver)?;
            return Ok(RecoveryStep::Recovered);
        }

        if let Some(bytes) = &current
            && current_hash != entry.old_blob
        {
            let manifest = self.manifest(entry.id)?;
            let already_salvaged = manifest.head().is_some_and(|head| {
                head.blob == hash_bytes(bytes)
                    && head.source == Source::Salvage
                    && head.ver > pending.ver
            });
            if !already_salvaged {
                let salvage = self.prepare_commit(lock, entry.id, bytes, Source::Salvage)?;
                let expected = manifest.max_ver().ok_or_else(|| Error::Corrupt {
                    what: "manifest",
                    detail: format!("{} has no versions", entry.id),
                })?;
                self.commit(lock, salvage, Some(expected))?;
            }
        }

        self.append_revert_and_apply(
            lock,
            entry.id,
            pending.ver,
            current_hash,
            &content,
            &manifest.path,
        )?;
        Ok(RecoveryStep::Recovered)
    }

    fn append_revert_and_apply(
        &mut self,
        lock: &WriteLock,
        id: FileId,
        pending_version: Ver,
        old_blob: Option<BlobHash>,
        content: &[u8],
        path: &AbsPath,
    ) -> Result<()> {
        let manifest = self.manifest(id)?;
        let expected = manifest.max_ver().ok_or_else(|| Error::Corrupt {
            what: "manifest",
            detail: format!("{id} has no versions"),
        })?;
        let mut prepared = self.prepare_commit(lock, id, content, Source::Revert)?;
        prepared.set_pending(true);
        prepared.clear_pending(pending_version);
        let new_version = prepared
            .manifest()
            .head()
            .ok_or_else(|| Error::Corrupt {
                what: "manifest",
                detail: format!("{id} has no versions"),
            })?
            .ver;
        self.write_pending(
            lock,
            &[PendingEntry {
                id,
                version: new_version,
                old_blob,
                new_blob: hash_bytes(content),
            }],
        )?;
        self.commit(lock, prepared, Some(expected))?;
        self.apply_pending(lock, path, content, new_version)?;
        self.finish_apply(lock, id, new_version)
    }

    fn apply_pending(
        &self,
        _lock: &WriteLock,
        path: &AbsPath,
        content: &[u8],
        version: Ver,
    ) -> Result<()> {
        crate::apply::write_target(path, content).map_err(|error| match error {
            Error::Io(source) => Error::ApplyFailed {
                ver: version,
                source,
            },
            Error::NotAFile { path } => Error::ApplyFailed {
                ver: version,
                source: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("not a regular file: {path}"),
                ),
            },
            other => other,
        })
    }

    fn clear_pending_version(&self, lock: &WriteLock, id: FileId, version: Ver) -> Result<()> {
        let mut manifest = self.manifest(id)?;
        if let Some(record) = manifest
            .versions
            .iter_mut()
            .find(|record| record.ver == version)
            && record.pending_apply
        {
            record.pending_apply = false;
            self.save_manifest(lock, &manifest)?;
        }
        Ok(())
    }

    fn pending_path(&self) -> AbsPath {
        self.root.join(PENDING_FILE)
    }

    fn read_pending(&self) -> Result<Vec<PendingEntry>> {
        let text = match fs::read_to_string(self.pending_path().as_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(Error::Io(error)),
        };
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let file: PendingFile = serde_json::from_str(&text).map_err(|error| Error::Corrupt {
            what: PENDING_FILE,
            detail: error.to_string(),
        })?;
        if file.schema > SUPPORTED_SCHEMA {
            return Err(Error::SchemaTooNew {
                found: file.schema,
                supported: SUPPORTED_SCHEMA,
            });
        }
        if file.schema < SUPPORTED_SCHEMA {
            return Err(Error::SchemaTooOld {
                found: file.schema,
                supported: SUPPORTED_SCHEMA,
            });
        }
        if file.entries.len() > 1 {
            return Err(Error::Corrupt {
                what: PENDING_FILE,
                detail: "multiple pending entries are not supported".into(),
            });
        }
        Ok(file.entries)
    }

    fn write_pending(&self, _lock: &WriteLock, entries: &[PendingEntry]) -> Result<()> {
        let file = PendingFile {
            schema: SUPPORTED_SCHEMA,
            entries: entries.to_vec(),
        };
        let encoded = serde_json::to_vec(&file)
            .map_err(|error| Error::Internal(format!("cannot encode pending file: {error}")))?;
        write_atomic(&self.pending_path(), &self.root, &encoded)?.require_durable()
    }

    fn clear_pending(&self, _lock: &WriteLock) -> Result<()> {
        match fs::remove_file(self.pending_path().as_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Error::Io(error)),
        }
    }

    /// 标记一次写回开始；必须先于「pending=1 的清单提交」。
    pub fn begin_apply(
        &self,
        lock: &WriteLock,
        id: FileId,
        version: Ver,
        old_blob: Option<BlobHash>,
        new_blob: BlobHash,
    ) -> Result<()> {
        let manifest = self.manifest(id)?;
        let head = manifest.head().ok_or_else(|| Error::Corrupt {
            what: "manifest",
            detail: format!("{id} has no versions"),
        })?;
        let expected = Ver::next(head.ver)?;
        if version != expected {
            return Err(Error::Corrupt {
                what: PENDING_FILE,
                detail: format!("begin_apply for {id} expected version {expected}, got {version}"),
            });
        }
        if manifest.versions.iter().any(|record| record.pending_apply) {
            return Err(Error::Corrupt {
                what: PENDING_FILE,
                detail: format!("{id} already has a pending version"),
            });
        }
        if old_blob.is_some_and(|blob| blob != head.blob) {
            return Err(Error::Corrupt {
                what: PENDING_FILE,
                detail: format!("begin_apply for {id} old blob does not match the head"),
            });
        }
        self.write_pending(
            lock,
            &[PendingEntry {
                id,
                version,
                old_blob,
                new_blob,
            }],
        )
    }

    /// 写回成功后清除指定版本的 pending 标记与 `.pending`。
    pub fn finish_apply(&self, lock: &WriteLock, id: FileId, version: Ver) -> Result<()> {
        let mut manifest = self.manifest(id)?;
        let record = manifest
            .versions
            .iter_mut()
            .find(|record| record.ver == version)
            .ok_or_else(|| Error::Corrupt {
                what: PENDING_FILE,
                detail: format!("finish_apply for missing version {version} of {id}"),
            })?;
        if !record.pending_apply {
            return Err(Error::Corrupt {
                what: PENDING_FILE,
                detail: format!("version {version} of {id} is not pending"),
            });
        }
        record.pending_apply = false;
        self.save_manifest(lock, &manifest)?;
        self.clear_pending(lock)
    }

    fn acquire_shared(&self, timeout: Duration) -> Result<ReadLock> {
        self.acquire(false, timeout).map(|file| ReadLock { file })
    }

    fn acquire_exclusive(&self, timeout: Duration) -> Result<WriteLock> {
        self.acquire(true, timeout).map(|file| WriteLock { file })
    }

    /// 共享锁只以只读方式打开 `.lock`，只读挂载与 0444 锁文件仍可读。
    fn open_lock_file(&self, exclusive: bool) -> Result<File> {
        let path = self.root.join(LOCK_FILE);
        if exclusive {
            return crate::fsutil::private_file_options()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path.as_path())
                .map_err(Error::Io);
        }

        #[cfg(unix)]
        {
            match crate::fsutil::private_file_options()
                .read(true)
                .open(path.as_path())
            {
                Ok(file) => Ok(file),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let created = crate::fsutil::private_file_options()
                        .read(true)
                        .write(true)
                        .create_new(true)
                        .open(path.as_path())
                        .map_err(Error::Io)?;
                    drop(created);
                    crate::fsutil::private_file_options()
                        .read(true)
                        .open(path.as_path())
                        .map_err(Error::Io)
                }
                Err(error) => Err(Error::Io(error)),
            }
        }

        #[cfg(not(unix))]
        {
            crate::fsutil::private_file_options()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path.as_path())
                .map_err(Error::Io)
        }
    }

    fn acquire(&self, exclusive: bool, timeout: Duration) -> Result<File> {
        let file = self.open_lock_file(exclusive)?;

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
                    return Ok(file);
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

    /// `.lock` 的内容只用于错误提示；写入不是原子的，读者可能看到空内容。
    /// 锁语义由 flock 决定，不依赖这里的 PID 或 fsync。
    fn write_holder(&self, file: &File) {
        let mut handle = file;
        let _ = handle.set_len(0);
        let _ = writeln!(handle, "{}", std::process::id());
    }

    fn read_holder(&self) -> Option<u32> {
        let text = fs::read_to_string(self.root.join(LOCK_FILE).as_path()).ok()?;
        text.trim().parse().ok()
    }
}

/// 共享读锁；写接口不接受它。
///
/// ```compile_fail,E0308
/// use std::time::Duration;
/// use bakon_core::{AbsPath, Store};
///
/// # fn main() -> bakon_core::Result<()> {
/// let temporary = std::env::temp_dir().join("bakon-read-lock-doc");
/// let (store, _) = Store::init(&temporary)?;
/// let read = store.lock_shared(Duration::from_millis(10))?;
/// let path = AbsPath::new(temporary.join("target"))?;
/// let _ = store.prepare_track(&read, &path, b"x")?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct ReadLock {
    file: File,
}

impl Drop for ReadLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// 排他写锁；所有写接口都要求它。只能由 [`Store::begin_write`] 或
/// [`Store::begin_verify`] 产生，字段私有，调用方无法伪造。
#[derive(Debug)]
pub struct WriteLock {
    file: File,
}

impl Drop for WriteLock {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PendingEntry {
    id: FileId,
    version: Ver,
    old_blob: Option<BlobHash>,
    new_blob: BlobHash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecoveryStep {
    Recovered,
    Stale,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PendingFile {
    #[serde(default)]
    schema: u32,
    #[serde(default)]
    entries: Vec<PendingEntry>,
}

/// 原子写入标记：临时文件 → fsync → rename → 目录 fsync。
fn initialize(root: &AbsPath) -> Result<Marker> {
    let marker = Marker::new()?;
    let mut encoded = serde_json::to_vec_pretty(&marker)
        .map_err(|error| Error::Internal(format!("cannot encode store marker: {error}")))?;
    encoded.push(b'\n');
    write_atomic(&root.join(MARKER_FILE), root, &encoded)?.require_durable()?;
    ensure_layout(root, true)?;
    Ok(marker)
}

/// 创建缺失的子目录与 `.lock`；已有条目保持不变。
///
/// `durable` 为真时（`init`）总是 fsync 根目录。`open` 只在确实创建了条目时
/// 才 fsync，因此完整布局的只读存储不会被打开动作写失败。
fn ensure_layout(root: &AbsPath, durable: bool) -> Result<()> {
    let mut created = false;
    for directory in LAYOUT_DIRS {
        let path = root.join(directory);
        if !path.as_path().is_dir() {
            crate::fsutil::create_private_dir_all(path.as_path())?;
            created = true;
        }
    }
    let lock = root.join(LOCK_FILE);
    if !lock.as_path().exists() {
        match crate::fsutil::private_file_options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(lock.as_path())
        {
            Ok(_) => created = true,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(Error::Io(error)),
        }
    }
    if durable || created {
        crate::fsutil::fsync_dir(root.as_path())?;
    }
    Ok(())
}

fn is_empty_dir(root: &AbsPath) -> Result<bool> {
    let mut entries = fs::read_dir(root.as_path()).map_err(Error::Io)?;
    for entry in &mut entries {
        let entry = entry.map_err(Error::Io)?;
        if !is_init_temp(&entry)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// 初始化标记的临时文件：崩溃可能把它们留在空目录中，不能因此拒绝整个目录。
///
/// 只接受 `BAKON_STORE.<pid>.<sequence>.<random>.tmp` 的严格形状；匹配的文件
/// **只会被忽略，不会被删除**。即使与用户文件同名，最坏也是在它旁边创建存储，
/// 不会丢掉用户数据。
fn is_init_temp(entry: &fs::DirEntry) -> Result<bool> {
    let name = entry.file_name();
    let Some(name) = name.to_str() else {
        return Ok(false);
    };
    let prefix = format!("{MARKER_FILE}.");
    let Some(rest) = name.strip_prefix(&prefix) else {
        return Ok(false);
    };
    let Some(rest) = rest.strip_suffix(".tmp") else {
        return Ok(false);
    };
    let mut parts = rest.split('.');
    let numeric = |part: Option<&str>| {
        part.is_some_and(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    };
    let valid = numeric(parts.next()) && numeric(parts.next()) && numeric(parts.next());
    Ok(valid && parts.next().is_none() && entry.file_type().map_err(Error::Io)?.is_file())
}

/// 原子写临时文件的目标名：`<target>.<pid>.<sequence>.<random>.tmp`。
/// 目标名本身可以含点（如 `<id>.json`），因此从右侧切分。
fn atomic_temp_target(name: &str) -> Option<&str> {
    let rest = name.strip_suffix(".tmp")?;
    let mut parts = rest.rsplitn(4, '.');
    let numeric = |part: Option<&str>| {
        part.is_some_and(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    };
    let valid = numeric(parts.next()) && numeric(parts.next()) && numeric(parts.next());
    let target = parts.next()?;
    valid.then_some(target)
}

fn collect_temporary_files(
    directory: &AbsPath,
    keep: impl Fn(&str) -> bool,
    found: &mut Vec<AbsPath>,
) -> Result<()> {
    let entries = match fs::read_dir(directory.as_path()) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Error::Io(error)),
    };
    for entry in entries {
        let entry = entry.map_err(Error::Io)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if let Some(target) = atomic_temp_target(name)
            && keep(target)
        {
            found.push(directory.join(name));
        }
    }
    Ok(())
}

fn read_marker(path: &Path) -> Result<Marker> {
    let bytes = fs::read(path).map_err(Error::Io)?;
    serde_json::from_slice(&bytes).map_err(|error| Error::Corrupt {
        what: "store marker",
        detail: error.to_string(),
    })
}

/// 读取恢复流程的目标内容；非普通文件表示写回失败（退出 7），不是路径用法错误。
fn recovery_target(path: &AbsPath, version: Ver) -> Result<Option<Vec<u8>>> {
    read_recovery_target(path).map_err(|error| match error {
        Error::NotAFile { path } => Error::ApplyFailed {
            ver: version,
            source: io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("not a regular file: {path}"),
            ),
        },
        other => other,
    })
}

/// 读取恢复流程的目标内容；缺失返回 `None`，非普通文件返回 `NotAFile`。
fn read_recovery_target(path: &AbsPath) -> Result<Option<Vec<u8>>> {
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

    fn write_lock(store: &mut Store) -> WriteLock {
        store.begin_write(Duration::from_millis(50)).unwrap()
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

    #[cfg(unix)]
    #[test]
    fn store_layout_and_records_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        let (mut store, _) = Store::init(&root).unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;

        assert_eq!(mode(&root), 0o700);
        for directory in LAYOUT_DIRS {
            assert_eq!(mode(&root.join(directory)), 0o700, "{directory}");
        }
        assert_eq!(mode(&root.join(MARKER_FILE)), 0o600);

        let guard = store.begin_write(Duration::from_millis(50)).unwrap();
        assert_eq!(mode(&root.join(LOCK_FILE)), 0o600);

        let file = temporary.path().join("target");
        fs::write(&file, b"secret").unwrap();
        let target = AbsPath::new(&file).unwrap();
        let prepared = store.prepare_track(&guard, &target, b"secret").unwrap();
        let committed = store.commit(&guard, prepared, None).unwrap();
        assert_eq!(mode(store.manifest_path(committed.id()).as_path()), 0o600);

        let record = store
            .manifest(committed.id())
            .unwrap()
            .head()
            .unwrap()
            .clone();
        let object = store.objects().path(record.blob);
        assert_eq!(mode(object.as_path()), 0o600);
        assert_eq!(mode(object.as_path().parent().unwrap()), 0o700);
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
    fn open_does_not_create_a_missing_store() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");

        let error = Store::open(&root).unwrap_err();

        assert!(matches!(error, Error::NotAStore { .. }));
        assert_eq!(error.exit_code(), 6);
        assert!(!root.exists());
    }

    #[test]
    fn open_does_not_initialize_an_empty_directory() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        fs::create_dir_all(&root).unwrap();

        let error = Store::open(&root).unwrap_err();

        assert!(matches!(error, Error::NotAStore { .. }));
        assert!(!root.join(MARKER_FILE).exists());
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
        let lock = write_lock(&mut store);
        let file = path("config");

        let baseline = store.prepare_track(&lock, &file, b"one").unwrap();
        let committed = store.commit(&lock, baseline, None).unwrap();
        assert_eq!(committed.version(), Ver::ZERO);

        let next = store
            .prepare_commit(&lock, committed.id(), b"two", Source::Edit)
            .unwrap();
        let committed = store.commit(&lock, next, Some(Ver::ZERO)).unwrap();
        assert_eq!(committed.version().get(), 1);

        let manifest = store.manifest(committed.id()).unwrap();
        assert_eq!(manifest.versions.len(), 2);
        assert_eq!(store.versions(committed.id()).unwrap().len(), 2);
        assert_eq!(manifest.path, file);
        assert_eq!(store.content(manifest.versions[0].blob).unwrap(), b"one");
        assert_eq!(store.content(manifest.versions[1].blob).unwrap(), b"two");
    }

    #[test]
    fn commit_refuses_a_duplicate_tracked_path() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let file = path("duplicate-track");
        let first = store.prepare_track(&lock, &file, b"one").unwrap();
        store.commit(&lock, first, None).unwrap();
        let second = store.prepare_track(&lock, &file, b"two").unwrap();

        let error = store.commit(&lock, second, None).unwrap_err();

        assert!(matches!(error, Error::AlreadyManaged { .. }));
        assert_eq!(error.exit_code(), 4);
        assert_eq!(store.all_ids().unwrap().len(), 1);
    }

    #[test]
    fn commit_conflict_keeps_the_old_manifest() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let file = path("conflict");
        let baseline = store.prepare_track(&lock, &file, b"base").unwrap();
        let committed = store.commit(&lock, baseline, None).unwrap();

        let first = store
            .prepare_commit(&lock, committed.id(), b"first", Source::Edit)
            .unwrap();
        store.commit(&lock, first, Some(Ver::ZERO)).unwrap();

        let stale = store
            .prepare_commit(&lock, committed.id(), b"stale", Source::Edit)
            .unwrap();
        let error = store.commit(&lock, stale, Some(Ver::ZERO)).unwrap_err();

        assert!(matches!(error, Error::HeadChanged { expected, actual }
            if expected == Ver::ZERO && actual.get() == 1));
        assert_eq!(error.exit_code(), 4);
        assert_eq!(store.manifest(committed.id()).unwrap().versions.len(), 2);
    }

    #[test]
    fn resolve_uses_and_repairs_the_cache() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let file = path("resolved");
        let baseline = store.prepare_track(&lock, &file, b"content").unwrap();
        let committed = store.commit(&lock, baseline, None).unwrap();

        assert_eq!(store.resolve(&file).unwrap(), Some(committed.id()));

        fs::write(store.index_path().as_path(), b"not json").unwrap();
        store.index = None;
        assert_eq!(store.resolve(&file).unwrap(), Some(committed.id()));

        let report = store
            .verify_fix(
                &lock,
                VerifyOptions {
                    fix: true,
                    size: false,
                },
            )
            .unwrap();
        assert!(report.ok());
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
        let lock = write_lock(&mut store);
        let file = path("corrupt");
        let baseline = store.prepare_track(&lock, &file, b"content").unwrap();
        let committed = store.commit(&lock, baseline, None).unwrap();

        let manifest_path = store.manifest_path(committed.id());
        fs::write(manifest_path.as_path(), b"{").unwrap();
        store.index = None;

        assert!(matches!(store.resolve(&file), Err(Error::Corrupt { .. })));
    }

    #[test]
    fn verify_reports_other_items_when_a_manifest_is_corrupt() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let good = path("verify-good");
        let bad = path("verify-bad");
        let prepared = store.prepare_track(&lock, &good, b"good").unwrap();
        store.commit(&lock, prepared, None).unwrap();
        let prepared = store.prepare_track(&lock, &bad, b"bad").unwrap();
        let bad_commit = store.commit(&lock, prepared, None).unwrap();
        let orphan = store.objects.put(b"orphan").unwrap();
        fs::write(store.manifest_path(bad_commit.id()).as_path(), b"{").unwrap();

        let report = store
            .verify(VerifyOptions {
                fix: false,
                size: false,
            })
            .unwrap();

        assert!(
            report
                .items
                .iter()
                .any(|item| item.kind == "manifest" && !item.fixed)
        );
        assert!(
            report
                .items
                .iter()
                .any(|item| item.kind == "orphan_scan" && !item.fixed)
        );
        assert_eq!(report.checked_files, 1);

        let report = store
            .verify_fix(
                &lock,
                VerifyOptions {
                    fix: true,
                    size: false,
                },
            )
            .unwrap();
        assert!(report.items.iter().any(|item| item.kind == "manifest"));
        assert!(
            store.objects().path(orphan).as_path().exists(),
            "objects must not be deleted while reachability is incomplete"
        );
    }

    #[test]
    fn trim_runs_during_commit() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let file = path("trimmed");
        let baseline = store.prepare_track(&lock, &file, b"v0").unwrap();
        let committed = store.commit(&lock, baseline, None).unwrap();

        let mut manifest = store.manifest(committed.id()).unwrap();
        manifest.retention =
            crate::manifest::Retention::Limit(std::num::NonZeroU32::new(2).unwrap());
        store.save_manifest(&lock, &manifest).unwrap();

        let mut head = committed.version();
        for index in 1..5 {
            let prepared = store
                .prepare_commit(
                    &lock,
                    committed.id(),
                    format!("v{index}").as_bytes(),
                    Source::Edit,
                )
                .unwrap();
            store.commit(&lock, prepared, Some(head)).unwrap();
            head = Ver::new(index).unwrap();
        }

        let manifest = store.manifest(committed.id()).unwrap();
        let versions: Vec<u32> = manifest
            .versions
            .iter()
            .map(|record| record.ver.get())
            .collect();
        assert_eq!(versions, vec![3, 4]);
        assert_eq!(
            store.objects().list_hashes().unwrap().len(),
            2,
            "trimmed objects must be reclaimed"
        );
        assert!(store.orphan_hashes().unwrap().is_empty());
    }

    #[test]
    fn init_accepts_a_crashed_marker_temp_without_deleting_it() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        fs::create_dir_all(&root).unwrap();
        let leftover = root.join("BAKON_STORE.1.2.3.tmp");
        fs::write(&leftover, b"partial").unwrap();

        let (store, created) = Store::init(&root).unwrap();

        assert!(created);
        assert!(root.join(MARKER_FILE).is_file());
        assert!(leftover.exists(), "init must not delete a matching temp");
        assert_eq!(store.marker().schema, SUPPORTED_SCHEMA);
    }

    #[test]
    fn init_preserves_a_user_file_that_is_not_a_marker_temp() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        fs::create_dir_all(&root).unwrap();
        let user_file = root.join("BAKON_STORE.notes.tmp");
        fs::write(&user_file, b"important user data").unwrap();

        let error = Store::init(&root).unwrap_err();

        assert!(matches!(error, Error::NotAStore { .. }));
        assert_eq!(error.exit_code(), 6);
        assert!(user_file.exists(), "init must never delete user data");
        assert!(!root.join(MARKER_FILE).exists());
    }

    #[test]
    fn verify_detects_and_fixes_orphan_objects() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let file = path("kept");
        let baseline = store.prepare_track(&lock, &file, b"keep").unwrap();
        store.commit(&lock, baseline, None).unwrap();
        store.objects.put(b"orphan").unwrap();

        let report = store
            .verify(VerifyOptions {
                fix: false,
                size: false,
            })
            .unwrap();
        assert!(
            report.items.iter().any(|item| item.kind == "orphan_object"),
            "{:?}",
            report.items
        );

        let report = store
            .verify_fix(
                &lock,
                VerifyOptions {
                    fix: true,
                    size: false,
                },
            )
            .unwrap();
        assert!(report.ok());
        assert!(store.orphan_hashes().unwrap().is_empty());
    }

    #[test]
    fn rename_rewrites_the_disk_index() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let old = path("renamed-old");
        let new = path("renamed-new");
        let prepared = store.prepare_track(&lock, &old, b"one").unwrap();
        let committed = store.commit(&lock, prepared, None).unwrap();

        store.rename(&lock, committed.id(), new.clone()).unwrap();

        let index = fs::read_to_string(store.index_path().as_path()).unwrap();
        assert!(index.contains(&new.key()), "{index}");
        assert!(!index.contains(&old.key()), "{index}");
    }

    #[test]
    fn verify_reports_and_fixes_stale_temporary_files() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let file = path("temp-clean");
        let prepared = store.prepare_track(&lock, &file, b"one").unwrap();
        let committed = store.commit(&lock, prepared, None).unwrap();

        let manifest_temp = store
            .root()
            .join(FILES_DIR)
            .join(format!("{}.json.1.2.3.tmp", committed.id()));
        let index_temp = store.root().join("index.json.1.2.3.tmp");
        let user_file = store.root().join("notes.tmp");
        fs::write(manifest_temp.as_path(), b"x").unwrap();
        fs::write(index_temp.as_path(), b"x").unwrap();
        fs::write(user_file.as_path(), b"keep").unwrap();

        let report = store
            .verify(VerifyOptions {
                fix: false,
                size: false,
            })
            .unwrap();
        assert!(
            report.items.iter().any(|item| item.kind == "temp_file"),
            "{:?}",
            report.items
        );

        let report = store
            .verify_fix(
                &lock,
                VerifyOptions {
                    fix: true,
                    size: false,
                },
            )
            .unwrap();
        assert!(
            report
                .items
                .iter()
                .any(|item| item.kind == "temp_file" && item.fixed),
            "{:?}",
            report.items
        );
        assert!(!manifest_temp.as_path().exists());
        assert!(!index_temp.as_path().exists());
        assert!(
            user_file.as_path().exists(),
            "unmatched user file must stay"
        );
    }

    #[test]
    fn verify_reports_invalid_object_names() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let directory = store.root().join("objects").join("zz");
        fs::create_dir_all(directory.as_path()).unwrap();
        fs::write(directory.join("junk").as_path(), b"x").unwrap();

        let report = store
            .verify_fix(
                &lock,
                VerifyOptions {
                    fix: true,
                    size: false,
                },
            )
            .unwrap();

        assert!(
            report.items.iter().any(|item| {
                item.kind == "invalid_object"
                    && item
                        .path
                        .as_ref()
                        .is_some_and(|path| path.as_path().ends_with("objects/zz"))
                    && !item.fixed
            }),
            "{:?}",
            report.items
        );
        assert!(directory.join("junk").as_path().exists());
    }

    #[test]
    fn prune_accepts_duplicate_ids() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let file = path("duplicate-prune");
        let baseline = store.prepare_track(&lock, &file, b"one").unwrap();
        let committed = store.commit(&lock, baseline, None).unwrap();
        let next = store
            .prepare_commit(&lock, committed.id(), b"two", Source::Edit)
            .unwrap();
        store.commit(&lock, next, Some(Ver::ZERO)).unwrap();
        store.set_default_max_versions(1);

        let report = store
            .prune(&lock, &[committed.id(), committed.id()])
            .unwrap();

        assert_eq!(report.files.len(), 1);
        assert_eq!(report.removed(), 1);
        assert_eq!(store.manifest(committed.id()).unwrap().versions.len(), 1);
        assert_eq!(store.objects().list_hashes().unwrap().len(), 1);
    }

    #[test]
    fn prune_without_trim_does_not_read_other_manifests() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let first = path("first");
        let second = path("second");
        let first_commit = {
            let prepared = store.prepare_track(&lock, &first, b"one").unwrap();
            store.commit(&lock, prepared, None).unwrap()
        };
        let second_commit = {
            let prepared = store.prepare_track(&lock, &second, b"two").unwrap();
            store.commit(&lock, prepared, None).unwrap()
        };

        fs::write(store.manifest_path(second_commit.id()).as_path(), b"{").unwrap();
        let report = store.prune(&lock, &[first_commit.id()]).unwrap();
        assert!(report.files.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn shared_lock_works_with_a_read_only_lock_file() {
        use std::os::unix::fs::PermissionsExt;

        let (_temporary, store) = open_store();
        let lock_path = store.root().join(LOCK_FILE);
        fs::set_permissions(lock_path.as_path(), fs::Permissions::from_mode(0o444)).unwrap();

        let guard = store.lock_shared(Duration::from_millis(50)).unwrap();
        drop(guard);

        fs::set_permissions(lock_path.as_path(), fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn open_and_shared_lock_work_on_a_read_only_layout() {
        use std::os::unix::fs::PermissionsExt;

        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        let (store, _) = Store::init(&root).unwrap();
        let marker = root.join(MARKER_FILE);
        let lock = root.join(LOCK_FILE);
        drop(store);

        let directories: Vec<_> = LAYOUT_DIRS
            .iter()
            .map(|directory| root.join(directory))
            .chain(std::iter::once(root.clone()))
            .collect();
        for directory in &directories {
            fs::set_permissions(directory.as_path(), fs::Permissions::from_mode(0o555)).unwrap();
        }
        fs::set_permissions(marker.as_path(), fs::Permissions::from_mode(0o444)).unwrap();
        fs::set_permissions(lock.as_path(), fs::Permissions::from_mode(0o444)).unwrap();

        let store = Store::open(&root).unwrap();
        let guard = store.lock_shared(Duration::from_millis(50)).unwrap();
        drop(guard);
        drop(store);

        fs::set_permissions(marker.as_path(), fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(lock.as_path(), fs::Permissions::from_mode(0o600)).unwrap();
        for directory in &directories {
            fs::set_permissions(directory.as_path(), fs::Permissions::from_mode(0o700)).unwrap();
        }
    }

    #[test]
    fn write_lock_excludes_a_second_writer() {
        let (_temporary, mut store) = open_store();
        let first = store.begin_write(Duration::from_millis(50)).unwrap();
        let error = store.begin_write(Duration::from_millis(50)).unwrap_err();
        assert!(matches!(error, Error::LockTimeout { .. }));
        drop(first);

        assert!(store.begin_write(Duration::from_millis(50)).is_ok());
    }

    #[test]
    fn apply_markers_are_bound_to_the_head_successor() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let file = path("apply-marker");
        let prepared = store.prepare_track(&lock, &file, b"one").unwrap();
        let committed = store.commit(&lock, prepared, None).unwrap();
        let old = Some(hash_bytes(b"one"));
        let new = hash_bytes(b"two");

        let error = store
            .begin_apply(&lock, committed.id(), Ver::new(2).unwrap(), old, new)
            .unwrap_err();
        assert!(matches!(error, Error::Corrupt { .. }));
        assert!(!store.pending_path().as_path().exists());

        store
            .begin_apply(&lock, committed.id(), Ver::new(1).unwrap(), old, new)
            .unwrap();
        let mut prepared = store
            .prepare_commit(&lock, committed.id(), b"two", Source::Revert)
            .unwrap();
        prepared.set_pending(true);
        store.commit(&lock, prepared, Some(Ver::ZERO)).unwrap();

        let error = store
            .finish_apply(&lock, committed.id(), Ver::new(2).unwrap())
            .unwrap_err();
        assert!(matches!(error, Error::Corrupt { .. }));
        assert!(store.pending_path().as_path().exists());

        let error = store
            .finish_apply(&lock, committed.id(), Ver::ZERO)
            .unwrap_err();
        assert!(matches!(error, Error::Corrupt { .. }));
        assert!(store.pending_path().as_path().exists());

        store
            .finish_apply(&lock, committed.id(), Ver::new(1).unwrap())
            .unwrap();
        assert!(!store.pending_path().as_path().exists());
    }

    #[test]
    fn recovery_clears_temporary_files_and_trash() {
        let (_temporary, mut store) = open_store();
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

    #[test]
    fn recovery_applies_a_pending_version() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        let (mut store, _) = Store::init(&root).unwrap();
        let file = temporary.path().join("target");
        fs::write(&file, b"old").unwrap();
        let tracked = AbsPath::new(&file).unwrap();
        let lock = write_lock(&mut store);

        let baseline = store.prepare_track(&lock, &tracked, b"old").unwrap();
        let committed = store.commit(&lock, baseline, None).unwrap();

        store
            .begin_apply(
                &lock,
                committed.id(),
                Ver::new(1).unwrap(),
                Some(hash_bytes(b"old")),
                hash_bytes(b"restored"),
            )
            .unwrap();
        let mut prepared = store
            .prepare_commit(&lock, committed.id(), b"restored", Source::Revert)
            .unwrap();
        prepared.set_pending(true);
        store.commit(&lock, prepared, Some(Ver::ZERO)).unwrap();
        drop(lock);
        drop(store);

        let (mut store, _) = Store::init(&root).unwrap();
        let _guard = store.begin_write(Duration::from_millis(50)).unwrap();

        assert_eq!(fs::read(&file).unwrap(), b"restored");
        assert!(!root.join(PENDING_FILE).exists());
        assert!(
            !store
                .manifest(committed.id())
                .unwrap()
                .head()
                .unwrap()
                .pending_apply
        );
    }

    #[test]
    fn recovery_maps_a_non_file_target_to_apply_failed() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        let (mut store, _) = Store::init(&root).unwrap();
        let file = temporary.path().join("target");
        fs::write(&file, b"old").unwrap();
        let tracked = AbsPath::new(&file).unwrap();
        let lock = write_lock(&mut store);

        let baseline = store.prepare_track(&lock, &tracked, b"old").unwrap();
        let committed = store.commit(&lock, baseline, None).unwrap();
        store
            .begin_apply(
                &lock,
                committed.id(),
                Ver::new(1).unwrap(),
                Some(hash_bytes(b"old")),
                hash_bytes(b"new"),
            )
            .unwrap();
        let mut prepared = store
            .prepare_commit(&lock, committed.id(), b"new", Source::Revert)
            .unwrap();
        prepared.set_pending(true);
        store.commit(&lock, prepared, Some(Ver::ZERO)).unwrap();
        drop(lock);
        drop(store);

        fs::remove_file(&file).unwrap();
        fs::create_dir(&file).unwrap();

        let (mut store, _) = Store::init(&root).unwrap();
        let error = store.begin_write(Duration::from_millis(50)).unwrap_err();

        assert!(matches!(error, Error::ApplyFailed { .. }));
        assert_eq!(error.exit_code(), 7);
        assert!(store.pending_path().as_path().exists());
    }

    #[test]
    fn recovery_keeps_a_pending_record_for_a_missing_manifest() {
        let (_temporary, mut store) = open_store();
        let lock = store.begin_verify(Duration::from_millis(50)).unwrap();
        let id = FileId::from_bytes([9; 16]);
        store
            .write_pending(
                &lock,
                &[PendingEntry {
                    id,
                    version: Ver::new(1).unwrap(),
                    old_blob: None,
                    new_blob: hash_bytes(b"lost"),
                }],
            )
            .unwrap();

        let error = store.recover(&lock).unwrap_err();

        assert!(matches!(error, Error::Corrupt { .. }));
        assert_eq!(error.exit_code(), 6);
        assert!(store.pending_path().as_path().exists());
    }

    #[test]
    fn recovery_discards_an_uncommitted_pending_record() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let file = path("uncommitted");
        let prepared = store.prepare_track(&lock, &file, b"one").unwrap();
        let committed = store.commit(&lock, prepared, None).unwrap();
        store
            .write_pending(
                &lock,
                &[PendingEntry {
                    id: committed.id(),
                    version: Ver::new(1).unwrap(),
                    old_blob: Some(hash_bytes(b"one")),
                    new_blob: hash_bytes(b"two"),
                }],
            )
            .unwrap();

        let stale = store.recover(&lock).unwrap();

        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].version, Ver::new(1).unwrap());
        assert!(!store.pending_path().as_path().exists());
        assert_eq!(store.manifest(committed.id()).unwrap().versions.len(), 1);
    }

    #[test]
    fn verify_fix_reports_and_discards_an_uncommitted_pending_record() {
        let (_temporary, mut store) = open_store();
        let lock = write_lock(&mut store);
        let file = path("verify-uncommitted");
        let prepared = store.prepare_track(&lock, &file, b"one").unwrap();
        let committed = store.commit(&lock, prepared, None).unwrap();
        store
            .write_pending(
                &lock,
                &[PendingEntry {
                    id: committed.id(),
                    version: Ver::new(1).unwrap(),
                    old_blob: Some(hash_bytes(b"one")),
                    new_blob: hash_bytes(b"two"),
                }],
            )
            .unwrap();

        let report = store
            .verify_fix(
                &lock,
                VerifyOptions {
                    fix: true,
                    size: false,
                },
            )
            .unwrap();

        assert!(
            report
                .items
                .iter()
                .any(|item| item.kind == "pending_stale" && item.fixed),
            "{:?}",
            report.items
        );
        assert!(!store.pending_path().as_path().exists());
    }
}

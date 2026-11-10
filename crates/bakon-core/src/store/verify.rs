//! 一致性检查与修复。
//!
//! 检查项见 `design/MODULE__bakon-v1-design.md` §5.6。`--fix` 只修复「派生的」
//! 数据（尺寸字段、索引、残留目录），不猜测、不重写历史。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use crate::fsutil::remove_dir_contents;
use crate::types::{AbsPath, BlobHash, FileId, Ver};
use crate::{Error, Result};

use super::Store;

#[derive(Clone, Copy, Debug, Default)]
pub struct VerifyOptions {
    pub fix: bool,
    pub size: bool,
}

#[derive(Clone, Debug)]
pub struct VerifyItem {
    pub kind: &'static str,
    pub path: Option<AbsPath>,
    pub ver: Option<Ver>,
    pub detail: String,
    pub fixed: bool,
}

#[derive(Clone, Debug)]
pub struct SizeEntry {
    pub path: AbsPath,
    pub versions: usize,
    pub object_bytes: u64,
    pub manifest_bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub struct VerifyReport {
    pub items: Vec<VerifyItem>,
    pub sizes: Vec<SizeEntry>,
    pub unique_object_bytes: u64,
    pub checked_files: usize,
}

impl VerifyReport {
    /// `--fix` 已解决的项不再让存储处于不一致状态。
    pub fn ok(&self) -> bool {
        self.items.iter().all(|item| item.fixed)
    }
}

impl Store {
    /// 检查全部清单、对象、索引与残留目录。`fix` 需要调用方持有排他锁。
    pub fn verify(&mut self, options: VerifyOptions) -> Result<VerifyReport> {
        let mut report = VerifyReport::default();
        let mut manifests = Vec::new();

        for id in self.list_ids()? {
            match self.manifest(id) {
                Ok(manifest) => manifests.push((id, manifest)),
                Err(error) => report.items.push(VerifyItem {
                    kind: "manifest",
                    path: None,
                    ver: None,
                    detail: format!("{id}: {error}"),
                    fixed: false,
                }),
            }
        }
        report.checked_files = manifests.len();

        let mut paths: BTreeMap<String, FileId> = BTreeMap::new();
        let mut unique_hashes: BTreeSet<BlobHash> = BTreeSet::new();

        for (id, manifest) in &manifests {
            let key = manifest.path.as_path().to_string_lossy().into_owned();
            if let Some(existing) = paths.insert(key, *id) {
                report.items.push(VerifyItem {
                    kind: "duplicate_path",
                    path: Some(manifest.path.clone()),
                    ver: None,
                    detail: format!("manifest {existing} uses the same path"),
                    fixed: false,
                });
            }
            unique_hashes.extend(manifest.versions.iter().map(|record| record.blob));

            let mut repaired = manifest.clone();
            let mut repaired_any = false;
            let mut object_bytes = 0_u64;

            for (index, record) in manifest.versions.iter().enumerate() {
                let object_path = self.objects().path(record.blob);
                match fs::metadata(object_path.as_path()) {
                    Ok(metadata) => object_bytes += metadata.len(),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        report.items.push(VerifyItem {
                            kind: "missing_blob",
                            path: Some(manifest.path.clone()),
                            ver: Some(record.ver),
                            detail: record.blob.to_hex(),
                            fixed: false,
                        });
                        continue;
                    }
                    Err(error) => return Err(Error::Io(error)),
                }

                match self.objects().get(record.blob) {
                    Ok(content) => {
                        if content.len() as u64 != record.size {
                            report.items.push(VerifyItem {
                                kind: "size_mismatch",
                                path: Some(manifest.path.clone()),
                                ver: Some(record.ver),
                                detail: format!(
                                    "recorded {} bytes, stored {} bytes",
                                    record.size,
                                    content.len()
                                ),
                                fixed: options.fix,
                            });
                            if options.fix {
                                repaired.versions[index].size = content.len() as u64;
                                repaired_any = true;
                            }
                        }
                    }
                    Err(error) => report.items.push(VerifyItem {
                        kind: "corrupt_blob",
                        path: Some(manifest.path.clone()),
                        ver: Some(record.ver),
                        detail: error.to_string(),
                        fixed: false,
                    }),
                }
            }

            if repaired_any {
                self.save_manifest(&repaired)?;
            }
            if options.size {
                let manifest_bytes = fs::metadata(self.manifest_path(*id).as_path())
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                report.sizes.push(SizeEntry {
                    path: manifest.path.clone(),
                    versions: manifest.versions.len(),
                    object_bytes,
                    manifest_bytes,
                });
            }
        }

        let expected: BTreeMap<String, FileId> = manifests
            .iter()
            .map(|(id, manifest)| (manifest.path.as_path().to_string_lossy().into_owned(), *id))
            .collect();
        if self.load_index().entries != expected {
            let fixed = options.fix;
            if fixed {
                self.index = Some(super::IndexCache {
                    schema: crate::SUPPORTED_SCHEMA,
                    entries: expected,
                });
                self.save_index();
            }
            report.items.push(VerifyItem {
                kind: "index",
                path: None,
                ver: None,
                detail: "index does not match manifests".into(),
                fixed,
            });
        }

        for directory in ["objects/tmp", "trash"] {
            let path = self.root.join(directory);
            let count = match fs::read_dir(path.as_path()) {
                Ok(entries) => entries.count(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
                Err(error) => return Err(Error::Io(error)),
            };
            if count > 0 {
                let fixed = options.fix;
                if fixed {
                    remove_dir_contents(path.as_path())?;
                }
                report.items.push(VerifyItem {
                    kind: "leftover",
                    path: None,
                    ver: None,
                    detail: format!("{directory} has {count} entries"),
                    fixed,
                });
            }
        }

        if options.size {
            let mut unique = 0_u64;
            for hash in &unique_hashes {
                if let Ok(metadata) = fs::metadata(self.objects().path(*hash).as_path()) {
                    unique += metadata.len();
                }
            }
            report.unique_object_bytes = unique;
        }

        Ok(report)
    }
}

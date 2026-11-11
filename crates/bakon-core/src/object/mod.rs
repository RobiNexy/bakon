//! 对象层：内容寻址的不可变字节串。
//!
//! 契约见 `design/DATA__blob-store-and-durability.md`。对外只有两个操作：
//! [`ObjectStore::put`] 与 [`ObjectStore::get`]，两者都幂等且不依赖历史。

use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use sha2::{Digest, Sha256};

use crate::fsutil::{remove_dir_contents, write_atomic};
use crate::types::{AbsPath, BlobHash};
use crate::{Error, Result};

/// 压缩级别固定，保证同一输入产生同一字节串（去重依赖这一点）。
/// 取最快级别：1 MB 提交的预算由 fsync 与哈希决定，压缩不应成为瓶颈。
const COMPRESSION: Compression = Compression::fast();

/// 对象库的根：`<store>/objects/`。
#[derive(Debug)]
pub struct ObjectStore {
    root: AbsPath,
    temporary: AbsPath,
}

impl ObjectStore {
    pub fn new(store_root: &AbsPath) -> Self {
        let root = store_root.join("objects");
        let temporary = root.join("tmp");
        Self { root, temporary }
    }

    /// 写入对象并返回内容哈希；相同内容重复写入是零操作。
    pub fn put(&self, plain: &[u8]) -> Result<BlobHash> {
        let hash = hash_bytes(plain);
        let path = self.path(hash);
        if path.as_path().exists() {
            match self.get(hash) {
                Ok(_) => return Ok(hash),
                Err(Error::Corrupt { .. }) => {
                    // 既有对象已损坏；本次写入会用同一地址覆盖它。
                }
                Err(error) => return Err(error),
            }
        }

        let compressed = deflate(plain)?;
        if let Some(shard) = path.as_path().parent() {
            crate::fsutil::create_private_dir_all(shard)?;
            crate::fsutil::fsync_dir(self.root.as_path())?;
        }
        write_atomic(&path, &self.temporary, &compressed)?.require_durable()?;
        Ok(hash)
    }

    /// 读取并校验对象；缺失、解压失败或哈希不符都视为存储损坏。
    pub fn get(&self, hash: BlobHash) -> Result<Vec<u8>> {
        let path = self.path(hash);
        let compressed = fs::read(path.as_path()).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::Corrupt {
                    what: "object",
                    detail: format!("{hash} is missing"),
                }
            } else {
                Error::Io(error)
            }
        })?;

        let mut decoder = DeflateDecoder::new(compressed.as_slice());
        let mut plain = Vec::new();
        decoder
            .read_to_end(&mut plain)
            .map_err(|error| Error::Corrupt {
                what: "object",
                detail: format!("{hash} cannot be inflated: {error}"),
            })?;

        if hash_bytes(&plain) != hash {
            return Err(Error::Corrupt {
                what: "object",
                detail: format!("{hash} does not match its content"),
            });
        }
        Ok(plain)
    }

    /// 对象路径：`objects/aa/<rest>`。
    pub fn path(&self, hash: BlobHash) -> AbsPath {
        let text = hash.to_hex();
        self.root.join(format!("{}/{}", &text[..2], &text[2..]))
    }

    /// 列出对象库中实际存在的对象哈希；只扫描两层分片，忽略暂存区与非法文件名。
    pub fn list_hashes(&self) -> Result<BTreeSet<BlobHash>> {
        let mut hashes = BTreeSet::new();
        let shards = match fs::read_dir(self.root.as_path()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(hashes),
            Err(error) => return Err(Error::Io(error)),
        };
        for shard in shards {
            let shard = shard.map_err(Error::Io)?;
            let Some(shard_name) = shard.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if shard_name.len() != 2
                || !shard_name
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                || !shard.file_type().map_err(Error::Io)?.is_dir()
            {
                continue;
            }
            for entry in fs::read_dir(shard.path()).map_err(Error::Io)? {
                let entry = entry.map_err(Error::Io)?;
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if let Some(hash) = BlobHash::from_hex(&format!("{shard_name}{name}")) {
                    hashes.insert(hash);
                }
            }
        }
        Ok(hashes)
    }

    /// 列出对象目录中不符合“两层分片 + 合法哈希 + 普通文件”的条目。
    pub fn invalid_entries(&self) -> Result<Vec<AbsPath>> {
        let mut invalid = Vec::new();
        let shards = match fs::read_dir(self.root.as_path()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(invalid),
            Err(error) => return Err(Error::Io(error)),
        };
        for shard in shards {
            let shard = shard.map_err(Error::Io)?;
            let shard_name = shard.file_name();
            let Some(shard_text) = shard_name.to_str() else {
                invalid.push(self.root.join(&shard_name));
                continue;
            };
            let shard_is_dir = shard.file_type().map_err(Error::Io)?.is_dir();
            if shard_text == "tmp" {
                if !shard_is_dir {
                    invalid.push(self.root.join(&shard_name));
                }
                continue;
            }
            let valid_shard = shard_text.len() == 2
                && shard_text
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                && shard_is_dir;
            if !valid_shard {
                invalid.push(self.root.join(&shard_name));
                continue;
            }
            for entry in fs::read_dir(shard.path()).map_err(Error::Io)? {
                let entry = entry.map_err(Error::Io)?;
                let name = entry.file_name();
                let Some(name_text) = name.to_str() else {
                    invalid.push(self.root.join(Path::new(shard_text).join(&name)));
                    continue;
                };
                let valid = BlobHash::from_hex(&format!("{shard_text}{name_text}")).is_some()
                    && entry.file_type().map_err(Error::Io)?.is_file();
                if !valid {
                    invalid.push(self.root.join(format!("{shard_text}/{name_text}")));
                }
            }
        }
        Ok(invalid)
    }

    /// 清空写入暂存区；只能在持排他锁时调用。
    pub fn cleanup_tmp(&self) -> Result<()> {
        remove_dir_contents(self.temporary.as_path())
    }
}

/// 计算内容哈希，不写对象。
pub fn hash_bytes(plain: &[u8]) -> BlobHash {
    let mut hasher = Sha256::new();
    hasher.update(plain);
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&digest);
    BlobHash::from_bytes(bytes)
}

fn deflate(plain: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = DeflateEncoder::new(Vec::new(), COMPRESSION);
    encoder.write_all(plain).map_err(Error::Io)?;
    encoder.finish().map_err(Error::Io)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn store() -> (TempDir, ObjectStore) {
        let temporary = TempDir::new().unwrap();
        let root = AbsPath::new(temporary.path()).unwrap();
        let objects = ObjectStore::new(&root);
        (temporary, objects)
    }

    #[test]
    fn round_trips_binary_content() {
        let (_temporary, objects) = store();
        let content = b"\x00\x01\xff binary \n";
        let hash = objects.put(content).unwrap();
        assert_eq!(objects.get(hash).unwrap(), content);
    }

    #[test]
    fn empty_content_is_a_valid_object() {
        let (_temporary, objects) = store();
        let hash = objects.put(b"").unwrap();
        assert_eq!(objects.get(hash).unwrap(), b"");
    }

    #[test]
    fn same_content_is_stored_once() {
        let (_temporary, objects) = store();
        let first = objects.put(b"duplicate").unwrap();
        let second = objects.put(b"duplicate").unwrap();
        assert_eq!(first, second);

        let path = objects.path(first);
        let shard = path.as_path().parent().unwrap();
        let entries = fs::read_dir(shard).unwrap().count();
        assert_eq!(entries, 1);
    }

    #[test]
    fn put_repairs_a_corrupt_existing_object() {
        let (_temporary, objects) = store();
        let hash = objects.put(b"repair me").unwrap();
        let path = objects.path(hash);
        fs::write(path.as_path(), b"garbage").unwrap();

        assert_eq!(objects.put(b"repair me").unwrap(), hash);
        assert_eq!(objects.get(hash).unwrap(), b"repair me");
    }

    #[test]
    fn different_content_gets_different_hashes() {
        let (_temporary, objects) = store();
        assert_ne!(objects.put(b"one").unwrap(), objects.put(b"two").unwrap());
    }

    #[test]
    fn a_missing_object_is_corruption() {
        let (_temporary, objects) = store();
        let error = objects.get(hash_bytes(b"absent")).unwrap_err();
        assert!(matches!(error, Error::Corrupt { .. }));
        assert_eq!(error.exit_code(), 6);
    }

    #[test]
    fn a_truncated_object_is_corruption() {
        let (_temporary, objects) = store();
        let hash = objects.put(b"truncate me").unwrap();
        let path = objects.path(hash);
        let bytes = fs::read(path.as_path()).unwrap();
        fs::write(path.as_path(), &bytes[..bytes.len() / 2]).unwrap();

        assert!(matches!(objects.get(hash), Err(Error::Corrupt { .. })));
    }

    #[test]
    fn a_flipped_byte_is_corruption() {
        let (_temporary, objects) = store();
        let hash = objects.put(b"the quick brown fox").unwrap();
        let path = objects.path(hash);
        let mut bytes = fs::read(path.as_path()).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        fs::write(path.as_path(), bytes).unwrap();

        assert!(matches!(objects.get(hash), Err(Error::Corrupt { .. })));
    }

    #[test]
    fn invalid_entries_reports_unknown_names_without_removing_them() {
        let (_temporary, objects) = store();
        objects.put(b"good").unwrap();
        fs::create_dir_all(objects.root.join("zz")).unwrap();
        fs::write(objects.root.join("zz/junk"), b"x").unwrap();
        fs::create_dir_all(objects.root.join("aa")).unwrap();
        fs::write(objects.root.join("aa/nothex"), b"x").unwrap();

        let invalid = objects.invalid_entries().unwrap();

        assert_eq!(invalid.len(), 2, "{invalid:?}");
        assert!(objects.root.join("zz/junk").as_path().exists());
        assert!(objects.root.join("aa/nothex").as_path().exists());
    }

    #[test]
    fn cleanup_removes_only_temporary_entries() {
        let (_temporary, objects) = store();
        let hash = objects.put(b"keep me").unwrap();
        fs::create_dir_all(objects.temporary.as_path()).unwrap();
        fs::write(objects.temporary.as_path().join("leftover.tmp"), b"x").unwrap();

        objects.cleanup_tmp().unwrap();

        assert_eq!(objects.get(hash).unwrap(), b"keep me");
        assert_eq!(
            fs::read_dir(objects.temporary.as_path()).unwrap().count(),
            0
        );
    }
}

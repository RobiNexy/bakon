//! 清单：版本事实与每文件配置的唯一权威。
//!
//! 结构见 `design/DATA__version-manifest-and-numbering.md`。清单整体通过一次
//! `rename` 替换，因此读者只会看到旧值或新值。

use std::num::NonZeroU32;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::types::{AbsPath, BlobHash, FileId, Ver};
use crate::{Error, Result, SUPPORTED_SCHEMA};

/// 未配置保留上限时的全局默认；`0` 表示不限制。
pub const DEFAULT_MAX_VERSIONS: u32 = 100;

/// 一个被管理文件的完整历史。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub id: FileId,
    pub path: AbsPath,
    #[serde(default, skip_serializing_if = "Retention::is_inherit")]
    pub retention: Retention,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook: Option<String>,
    pub versions: Vec<VersionRecord>,
}

impl Manifest {
    /// 纳管时创建只含基线版本的清单。
    pub fn baseline(id: FileId, path: AbsPath, baseline: VersionRecord) -> Self {
        Self {
            schema: SUPPORTED_SCHEMA,
            id,
            path,
            retention: Retention::Inherit,
            hook: None,
            versions: vec![baseline],
        }
    }

    pub fn head(&self) -> Option<&VersionRecord> {
        self.versions.last()
    }

    pub fn max_ver(&self) -> Option<Ver> {
        self.head().map(|record| record.ver)
    }

    pub fn oldest_ver(&self) -> Option<Ver> {
        self.versions.first().map(|record| record.ver)
    }

    pub fn find(&self, ver: Ver) -> Option<&VersionRecord> {
        self.versions.iter().find(|record| record.ver == ver)
    }

    /// 追加版本；递增约束由 [`Ver::next`] 保证，这里只检查不倒退。
    pub fn append(&mut self, record: VersionRecord) {
        debug_assert!(
            self.max_ver().is_none_or(|head| record.ver > head),
            "versions must be appended in increasing order"
        );
        self.versions.push(record);
    }

    /// 保留最新 `max_versions` 个版本；`0` 表示不限制。
    pub fn trim(&mut self, max_versions: u32) {
        if max_versions == 0 {
            return;
        }
        let keep = max_versions as usize;
        if self.versions.len() > keep {
            let excess = self.versions.len() - keep;
            self.versions.drain(..excess);
        }
    }

    /// 生效上限：文件级配置优先，缺省取全局默认，`0` 表示不限制。
    pub fn effective_max_versions(&self, global: u32) -> u32 {
        match self.retention {
            Retention::Inherit => global,
            Retention::Unlimited => 0,
            Retention::Limit(limit) => limit.get(),
        }
    }

    pub fn head_mut(&mut self) -> Option<&mut VersionRecord> {
        self.versions.last_mut()
    }

    /// schema 门与结构校验；任何不符都拒绝解析结果。
    pub fn validate(&self) -> Result<()> {
        if self.schema > SUPPORTED_SCHEMA {
            return Err(Error::SchemaTooNew {
                found: self.schema,
                supported: SUPPORTED_SCHEMA,
            });
        }
        if self.schema < SUPPORTED_SCHEMA {
            return Err(Error::SchemaTooOld {
                found: self.schema,
                supported: SUPPORTED_SCHEMA,
            });
        }
        if self.versions.is_empty() {
            return Err(Error::Corrupt {
                what: "manifest",
                detail: format!("{} has no versions", self.id),
            });
        }
        for pair in self.versions.windows(2) {
            if pair[0].ver >= pair[1].ver {
                return Err(Error::Corrupt {
                    what: "manifest",
                    detail: format!(
                        "{} versions are not strictly increasing: {} then {}",
                        self.id, pair[0].ver, pair[1].ver
                    ),
                });
            }
            if pair[0].ver.get().checked_add(1) != Some(pair[1].ver.get()) {
                return Err(Error::Corrupt {
                    what: "manifest",
                    detail: format!(
                        "{} versions are not contiguous: {} then {}",
                        self.id, pair[0].ver, pair[1].ver
                    ),
                });
            }
        }
        for record in &self.versions {
            if OffsetDateTime::parse(&record.time, &Rfc3339).is_err() {
                return Err(Error::Corrupt {
                    what: "manifest",
                    detail: format!(
                        "{} version {} has an invalid timestamp: {}",
                        self.id, record.ver, record.time
                    ),
                });
            }
        }
        Ok(())
    }
}

/// 每文件保留策略的三种状态。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Retention {
    /// 字段缺失：继承全局。
    #[default]
    Inherit,
    /// `0`：不限制，显式覆盖全局。
    Unlimited,
    /// `n > 0`：保留最新 n 个版本。
    Limit(NonZeroU32),
}

impl Retention {
    pub fn is_inherit(&self) -> bool {
        matches!(self, Self::Inherit)
    }

    pub fn from_max_versions(value: u32) -> Self {
        match value {
            0 => Self::Unlimited,
            value => Self::Limit(NonZeroU32::new(value).expect("value is non-zero")),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetentionWire {
    max_versions: u32,
}

impl Serialize for Retention {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::Inherit => serializer.serialize_none(),
            Self::Unlimited => RetentionWire { max_versions: 0 }.serialize(serializer),
            Self::Limit(limit) => RetentionWire {
                max_versions: limit.get(),
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Retention {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire = RetentionWire::deserialize(deserializer)?;
        Ok(Self::from_max_versions(wire.max_versions))
    }
}

/// 一条版本记录；版本事实只从这里读取。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionRecord {
    pub ver: Ver,
    pub blob: BlobHash,
    pub size: u64,
    pub codec: Codec,
    pub time: String,
    pub source: Source,
    #[serde(default, skip_serializing_if = "is_false")]
    pub pending_apply: bool,
}

impl VersionRecord {
    pub fn new(ver: Ver, blob: BlobHash, size: u64, time: String, source: Source) -> Self {
        Self {
            ver,
            blob,
            size,
            codec: Codec::Deflate,
            time,
            source,
            pending_apply: false,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    Deflate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Baseline,
    Edit,
    Revert,
    Salvage,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(ver: u32) -> VersionRecord {
        VersionRecord::new(
            Ver::new(ver).unwrap(),
            BlobHash::from_bytes([ver as u8; 32]),
            u64::from(ver),
            format!("2026-11-10T12:00:{ver:02}Z"),
            Source::Edit,
        )
    }

    fn manifest_with(versions: &[u32]) -> Manifest {
        let mut manifest = Manifest::baseline(
            FileId::from_bytes([1; 16]),
            AbsPath::new("/tmp/file").unwrap(),
            record(versions[0]),
        );
        for ver in &versions[1..] {
            manifest.append(record(*ver));
        }
        manifest
    }

    #[test]
    fn json_matches_the_documented_shape() {
        let manifest = manifest_with(&[0, 1]);
        let value: serde_json::Value = serde_json::to_value(&manifest).unwrap();

        assert_eq!(value["schema"], 1);
        assert_eq!(value["path"], "/tmp/file");
        assert!(value.get("retention").is_none());
        assert_eq!(value["versions"][0]["ver"], 0);
        assert_eq!(value["versions"][0]["codec"], "deflate");
        assert_eq!(value["versions"][0]["source"], "edit");
        assert!(value["versions"][0].get("pending_apply").is_none());
    }

    #[test]
    fn retention_round_trips_all_three_states() {
        let mut manifest = manifest_with(&[0]);

        manifest.retention = Retention::Unlimited;
        let value = serde_json::to_value(&manifest).unwrap();
        assert_eq!(value["retention"]["max_versions"], 0);
        assert_eq!(
            serde_json::from_value::<Manifest>(value).unwrap().retention,
            Retention::Unlimited
        );

        manifest.retention = Retention::Limit(NonZeroU32::new(7).unwrap());
        let value = serde_json::to_value(&manifest).unwrap();
        assert_eq!(value["retention"]["max_versions"], 7);
        assert_eq!(manifest.effective_max_versions(DEFAULT_MAX_VERSIONS), 7);

        manifest.retention = Retention::Inherit;
        let value = serde_json::to_value(&manifest).unwrap();
        assert!(value.get("retention").is_none());
        assert_eq!(
            manifest.effective_max_versions(DEFAULT_MAX_VERSIONS),
            DEFAULT_MAX_VERSIONS
        );
        assert_eq!(manifest.effective_max_versions(25), 25);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let mut value = serde_json::to_value(manifest_with(&[0])).unwrap();
        value["surprise"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Manifest>(value).is_err());
    }

    #[test]
    fn non_increasing_versions_are_corruption() {
        let mut manifest = manifest_with(&[0, 2]);
        manifest.versions[1].ver = Ver::ZERO;
        let error = manifest.validate().unwrap_err();
        assert!(matches!(error, Error::Corrupt { .. }));
    }

    #[test]
    fn version_gaps_are_corruption() {
        let mut manifest = manifest_with(&[0, 1]);
        manifest.versions[1].ver = Ver::new(2).unwrap();
        let error = manifest.validate().unwrap_err();
        assert!(matches!(error, Error::Corrupt { .. }));
    }

    #[test]
    fn invalid_timestamps_are_corruption() {
        let mut manifest = manifest_with(&[0]);
        manifest.versions[0].time = "not-a-time".into();
        let error = manifest.validate().unwrap_err();
        assert!(matches!(error, Error::Corrupt { .. }));
    }

    #[test]
    fn empty_versions_are_corruption() {
        let mut manifest = manifest_with(&[0]);
        manifest.versions.clear();
        assert!(matches!(manifest.validate(), Err(Error::Corrupt { .. })));
    }

    #[test]
    fn schema_gate_is_enforced_on_read() {
        let mut manifest = manifest_with(&[0]);
        manifest.schema = SUPPORTED_SCHEMA + 1;
        assert!(matches!(
            manifest.validate(),
            Err(Error::SchemaTooNew { .. })
        ));
    }

    #[test]
    fn trim_keeps_the_newest_suffix_and_never_renumbers() {
        let mut manifest = manifest_with(&[0, 1, 2, 3, 4]);
        manifest.trim(2);

        let versions: Vec<u32> = manifest
            .versions
            .iter()
            .map(|record| record.ver.get())
            .collect();
        assert_eq!(versions, vec![3, 4]);
        assert_eq!(manifest.max_ver().unwrap().get(), 4);

        manifest.append(record(5));
        assert_eq!(manifest.max_ver().unwrap().get(), 5);
    }

    #[test]
    fn trim_zero_means_unlimited() {
        let mut manifest = manifest_with(&[0, 1, 2]);
        manifest.trim(0);
        assert_eq!(manifest.versions.len(), 3);
    }

    /// 用确定性伪随机序列覆盖编号与裁剪的组合，断言设计中的三条性质。
    #[test]
    fn generated_sequences_keep_numbering_and_trimming_invariants() {
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        for _ in 0..200 {
            let mut manifest = manifest_with(&[0]);
            let mut assigned = vec![0_u32];

            for _ in 0..60 {
                match next() % 4 {
                    0 => {
                        let ver = manifest.max_ver().unwrap().get() + 1;
                        manifest.append(record(ver));
                        assigned.push(ver);
                    }
                    1 => {
                        let limit = (next() % 4) as u32;
                        let before = manifest.versions.clone();
                        manifest.trim(limit);
                        if limit > 0 {
                            assert_eq!(manifest.versions.len(), before.len().min(limit as usize));
                            assert_eq!(
                                manifest.versions.last().map(|record| record.ver),
                                before.last().map(|record| record.ver)
                            );
                        } else {
                            assert_eq!(manifest.versions.len(), before.len());
                        }
                    }
                    _ => {
                        let head = manifest.max_ver().unwrap();
                        let expected = head.get() + 1;
                        manifest.append(record(expected));
                        assigned.push(expected);
                    }
                }

                let versions: Vec<u32> = manifest
                    .versions
                    .iter()
                    .map(|record| record.ver.get())
                    .collect();
                assert!(versions.windows(2).all(|pair| pair[0] < pair[1]));
                assert_eq!(
                    manifest.max_ver().unwrap().get(),
                    *assigned.iter().max().unwrap()
                );
                assert!(versions.iter().all(|ver| assigned.contains(ver)));
            }
        }
    }
}

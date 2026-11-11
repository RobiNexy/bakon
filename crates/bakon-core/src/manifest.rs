//! 清单：版本事实与每文件配置的唯一权威。
//!
//! 结构见 `design/DATA__version-manifest-and-numbering.md`。清单整体通过一次
//! `rename` 替换，因此读者只会看到旧值或新值。

use std::num::NonZeroU32;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::types::{AbsPath, BlobHash, FileId, Ver};
use crate::{Error, MANIFEST_MIN_SCHEMA, MANIFEST_SCHEMA, Result};

/// 未配置保留上限时的全局默认；`0` 表示不限制。
pub const DEFAULT_MAX_VERSIONS: u32 = 100;

/// 备注的最大字符数（Unicode scalar value，不是字节）。
pub const MAX_NOTE_CHARS: usize = 500;

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
            schema: MANIFEST_MIN_SCHEMA,
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

    /// 区分「从未存在」与「已被裁剪」的版本查找。
    pub fn find_checked(&self, ver: Ver) -> Result<&VersionRecord> {
        let latest = self.max_ver().ok_or_else(|| Error::Corrupt {
            what: "manifest",
            detail: format!("{} has no versions", self.id),
        })?;
        if ver > latest {
            return Err(Error::NoSuchVersion { ver, latest });
        }
        let oldest = self.oldest_ver().unwrap_or(Ver::ZERO);
        if ver < oldest {
            return Err(Error::Pruned { ver, oldest });
        }
        self.find(ver).ok_or(Error::NoSuchVersion { ver, latest })
    }

    /// 让 schema 与备注内容保持一致：有备注用最新 schema，无备注回到最旧 schema。
    pub(crate) fn refresh_schema(&mut self) {
        self.schema = if self.versions.iter().any(|record| record.note.is_some()) {
            MANIFEST_SCHEMA
        } else {
            MANIFEST_MIN_SCHEMA
        };
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
        if max_versions > 0 {
            let keep = max_versions as usize;
            if self.versions.len() > keep {
                let excess = self.versions.len() - keep;
                self.versions.drain(..excess);
            }
        }
        self.refresh_schema();
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
        if self.schema > MANIFEST_SCHEMA {
            return Err(Error::SchemaTooNew {
                found: self.schema,
                supported: MANIFEST_SCHEMA,
            });
        }
        if self.schema < MANIFEST_MIN_SCHEMA {
            return Err(Error::SchemaTooOld {
                found: self.schema,
                supported: MANIFEST_SCHEMA,
            });
        }
        if self.versions.is_empty() {
            return Err(Error::Corrupt {
                what: "manifest",
                detail: format!("{} has no versions", self.id),
            });
        }
        if self.max_ver().is_some_and(|head| head.get() == u32::MAX) {
            return Err(Error::Corrupt {
                what: "manifest",
                detail: format!(
                    "{} has version {} and cannot allocate a successor",
                    self.id,
                    u32::MAX
                ),
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
        let mut pending = 0;
        for record in &self.versions {
            let Ok(timestamp) = OffsetDateTime::parse(&record.time, &Rfc3339) else {
                return Err(Error::Corrupt {
                    what: "manifest",
                    detail: format!(
                        "{} version {} has an invalid timestamp: {}",
                        self.id, record.ver, record.time
                    ),
                });
            };
            if timestamp.offset() != time::UtcOffset::UTC {
                return Err(Error::Corrupt {
                    what: "manifest",
                    detail: format!(
                        "{} version {} timestamp is not UTC: {}",
                        self.id, record.ver, record.time
                    ),
                });
            }
            if record.note.is_some() && self.schema < MANIFEST_SCHEMA {
                return Err(Error::Corrupt {
                    what: "manifest",
                    detail: format!(
                        "{} version {} has a note in schema {}",
                        self.id, record.ver, self.schema
                    ),
                });
            }
            if record.pending_apply {
                pending += 1;
                if record.source != Source::Revert {
                    return Err(Error::Corrupt {
                        what: "manifest",
                        detail: format!(
                            "{} version {} is pending but has source {:?}",
                            self.id, record.ver, record.source
                        ),
                    });
                }
            }
        }
        if pending > 1 {
            return Err(Error::Corrupt {
                what: "manifest",
                detail: format!("{} has {pending} pending versions", self.id),
            });
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<Note>,
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
            note: None,
            pending_apply: false,
        }
    }

    /// 附加备注；输入已经过 [`Note::parse`] 校验。
    pub fn with_note(mut self, note: Note) -> Self {
        self.note = Some(note);
        self
    }
}

/// 已校验的单行备注：去掉首尾空白后 1 到 [`MAX_NOTE_CHARS`] 个字符，且不含控制字符。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Note(String);

impl Note {
    /// 解析用户输入；首尾空白会被去掉。
    pub fn parse(input: &str) -> Result<Self> {
        let trimmed = input.trim();
        check_note(trimmed).map_err(Error::Usage)?;
        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Note {
    type Error = Error;

    /// 反序列化路径更严格：清单里保存的备注必须已经是规范化形式。
    fn try_from(value: String) -> Result<Self> {
        if value != value.trim() {
            return Err(Error::Corrupt {
                what: "note",
                detail: "has leading or trailing whitespace".into(),
            });
        }
        check_note(&value).map_err(|detail| Error::Corrupt {
            what: "note",
            detail,
        })?;
        Ok(Self(value))
    }
}

impl From<Note> for String {
    fn from(note: Note) -> Self {
        note.0
    }
}

fn check_note(value: &str) -> std::result::Result<(), String> {
    if value.is_empty() {
        return Err("note cannot be empty".into());
    }
    let count = value.chars().count();
    if count > MAX_NOTE_CHARS {
        return Err(format!(
            "note is {count} characters; the limit is {MAX_NOTE_CHARS}"
        ));
    }
    if let Some(character) = value.chars().find(|character| character.is_control()) {
        return Err(format!(
            "note contains control character U+{:04X}; notes are single-line",
            u32::from(character)
        ));
    }
    Ok(())
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
    fn exhausted_version_numbers_are_corruption() {
        let manifest = manifest_with(&[u32::MAX]);
        let error = manifest.validate().unwrap_err();
        assert!(matches!(error, Error::Corrupt { .. }));
    }

    #[test]
    fn non_utc_timestamps_are_corruption() {
        let mut manifest = manifest_with(&[0]);
        manifest.versions[0].time = "2026-11-10T12:00:00+08:00".into();

        let error = manifest.validate().unwrap_err();
        assert!(matches!(error, Error::Corrupt { .. }));
    }

    #[test]
    fn pending_apply_requires_a_revert_and_at_most_one_record() {
        let mut manifest = manifest_with(&[0, 1, 2]);
        manifest.versions[1].pending_apply = true;
        assert!(matches!(manifest.validate(), Err(Error::Corrupt { .. })));

        manifest.versions[1].source = Source::Revert;
        assert!(
            manifest.validate().is_ok(),
            "recovery may briefly leave a historical pending revert"
        );

        manifest.versions[2].pending_apply = true;
        manifest.versions[2].source = Source::Revert;
        assert!(matches!(manifest.validate(), Err(Error::Corrupt { .. })));
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
        manifest.schema = MANIFEST_SCHEMA + 1;
        assert!(matches!(
            manifest.validate(),
            Err(Error::SchemaTooNew { .. })
        ));

        manifest.schema = MANIFEST_MIN_SCHEMA - 1;
        assert!(matches!(
            manifest.validate(),
            Err(Error::SchemaTooOld { .. })
        ));
    }

    #[test]
    fn notes_round_trip_and_require_schema_two() {
        let mut manifest = manifest_with(&[0, 1]);
        manifest.versions[1].note = Some(Note::parse("把端口改成8843").unwrap());
        manifest.refresh_schema();

        assert_eq!(manifest.schema, MANIFEST_SCHEMA);
        let value = serde_json::to_value(&manifest).unwrap();
        assert_eq!(value["schema"], MANIFEST_SCHEMA);
        assert_eq!(value["versions"][1]["note"], "把端口改成8843");
        assert!(value["versions"][0].get("note").is_none());

        let parsed: Manifest = serde_json::from_value(value).unwrap();
        assert_eq!(
            parsed.versions[1].note.as_ref().map(Note::as_str),
            Some("把端口改成8843")
        );

        parsed.clone().validate().unwrap();
        let mut old_schema = parsed;
        old_schema.schema = MANIFEST_MIN_SCHEMA;
        assert!(matches!(old_schema.validate(), Err(Error::Corrupt { .. })));
    }

    #[test]
    fn note_validation_trims_and_rejects_bad_input() {
        assert_eq!(Note::parse("  hello  ").unwrap().as_str(), "hello");
        assert!(matches!(Note::parse("   "), Err(Error::Usage(_))));
        assert!(matches!(Note::parse("a\nb"), Err(Error::Usage(_))));
        assert!(matches!(Note::parse("a\tb"), Err(Error::Usage(_))));

        let too_long = "x".repeat(MAX_NOTE_CHARS + 1);
        assert!(matches!(Note::parse(&too_long), Err(Error::Usage(_))));

        let at_limit = "x".repeat(MAX_NOTE_CHARS);
        assert_eq!(Note::parse(&at_limit).unwrap().as_str(), at_limit);
    }

    #[test]
    fn invalid_stored_notes_are_rejected_by_deserialization() {
        let mut value = serde_json::to_value(manifest_with(&[0])).unwrap();
        value["versions"][0]["note"] = serde_json::json!("line\nbreak");
        assert!(serde_json::from_value::<Manifest>(value).is_err());

        let mut value = serde_json::to_value(manifest_with(&[0])).unwrap();
        value["versions"][0]["note"] = serde_json::json!(" padded ");
        assert!(serde_json::from_value::<Manifest>(value).is_err());
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

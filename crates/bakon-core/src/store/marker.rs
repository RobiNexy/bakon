//! 存储标记文件：归属与 schema 的唯一依据。
//!
//! 规则见 `design/OPS__store-identity-recovery-and-retention.md` §1。标记由本程序
//! 原子写入；读取时严格解析，不做宽松猜测。

use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::types::uuid;
use crate::{Error, Result, SUPPORTED_SCHEMA};

/// 标记文件名。
pub const MARKER_FILE: &str = "BAKON_STORE";

/// 写入标记的工具标识，形如 `bakon/1.0`。
pub const TOOL_ID: &str = concat!(
    "bakon/",
    env!("CARGO_PKG_VERSION_MAJOR"),
    ".",
    env!("CARGO_PKG_VERSION_MINOR")
);

/// `BAKON_STORE` 的内容。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Marker {
    pub schema: u32,
    pub uuid: String,
    pub created: String,
    pub tool: String,
}

impl Marker {
    /// 为新存储生成标记。
    pub fn new() -> Result<Self> {
        Ok(Self {
            schema: SUPPORTED_SCHEMA,
            uuid: uuid::format(&uuid::v4_bytes()),
            created: crate::clock::now_rfc3339()?,
            tool: TOOL_ID.to_owned(),
        })
    }

    /// 校验 schema 与字段格式。schema 不匹配优先于字段损坏报告。
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
        if !uuid::is_valid(&self.uuid) {
            return Err(Error::Corrupt {
                what: "store marker",
                detail: "uuid is not a lowercase UUID".into(),
            });
        }
        if OffsetDateTime::parse(&self.created, &Rfc3339).is_err() {
            return Err(Error::Corrupt {
                what: "store marker",
                detail: "created is not an RFC 3339 timestamp".into(),
            });
        }
        if self.tool.is_empty() {
            return Err(Error::Corrupt {
                what: "store marker",
                detail: "tool is empty".into(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_marker_is_valid() {
        Marker::new().unwrap().validate().unwrap();
    }

    #[test]
    fn marker_documents_its_tool_version() {
        assert_eq!(TOOL_ID, "bakon/1.0");
    }
}

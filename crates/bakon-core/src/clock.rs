//! 时间格式化：清单与标记统一使用 RFC 3339 UTC。

use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{Error, Result};

pub(crate) fn now_rfc3339() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| Error::Internal(format!("cannot format timestamp: {error}")))
}

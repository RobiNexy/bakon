//! 用户配置：位置、编辑器与全局保留上限。
//!
//! 配置文件是 TOML；缺失时全部取默认值，解析失败按用法错误退出（2）。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use bakon_core::manifest::DEFAULT_MAX_VERSIONS;
use bakon_core::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
    #[serde(default)]
    pub retention: Retention,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Retention {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_versions: Option<u32>,
}

impl Config {
    pub fn store_path(&self) -> Option<PathBuf> {
        self.store.as_deref().map(PathBuf::from)
    }

    /// 全局保留上限；缺省 100，`0` 表示不限制。
    pub fn max_versions(&self) -> u32 {
        self.retention.max_versions.unwrap_or(DEFAULT_MAX_VERSIONS)
    }
}

/// 配置文件位置：`--config` > `BAKON_CONFIG` > `$BAKON_HOME/config.toml` >
/// `$XDG_CONFIG_HOME/bakon/config.toml` > `~/.config/bakon/config.toml`。
///
/// 刻意不读 0.x 的 `~/.bakon/config.toml`：它的 `store` 指向旧 `repo/`，直接沿用会让
/// v1 打开一个不属于自己的目录。
pub fn path(cli_config: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = cli_config {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = env::var_os("BAKON_CONFIG").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    if let Some(home) = env::var_os("BAKON_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home).join("config.toml"));
    }
    if let Some(root) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(root).join("bakon").join("config.toml"));
    }
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::Usage("cannot determine the home directory; pass --config".into()))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("bakon")
        .join("config.toml"))
}

pub fn load(path: &Path) -> Result<Config> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text)
            .map_err(|error| Error::Usage(format!("invalid config {}: {error}", path.display()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(error) => Err(Error::Io(error)),
    }
}

/// 写入默认配置；已存在则保持不变并返回 `false`。
pub fn init(path: &Path) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(Error::Io)?;
    }
    fs::write(path, TEMPLATE).map_err(Error::Io)?;
    Ok(true)
}

const TEMPLATE: &str = "\
# bakon configuration

# Store directory; defaults to ~/.bakon/store.
# store = \"/absolute/path/to/store\"

# Editor used by `bakon edit`; $VISUAL and $EDITOR take precedence.
# editor = \"vi\"

[retention]
# Keep the newest N versions per file; 0 means unlimited.
max_versions = 100
";

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn missing_config_supplies_defaults() {
        let temporary = TempDir::new().unwrap();
        let config = load(&temporary.path().join("absent.toml")).unwrap();
        assert_eq!(config.max_versions(), DEFAULT_MAX_VERSIONS);
        assert!(config.store_path().is_none());
    }

    #[test]
    fn init_writes_once_and_round_trips() {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("config.toml");

        assert!(init(&path).unwrap());
        assert!(!init(&path).unwrap());

        let config = load(&path).unwrap();
        assert_eq!(config.max_versions(), 100);
    }

    #[test]
    fn zero_means_unlimited() {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("config.toml");
        fs::write(&path, "[retention]\nmax_versions = 0\n").unwrap();

        assert_eq!(load(&path).unwrap().max_versions(), 0);
    }

    #[test]
    fn malformed_config_is_a_usage_error() {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("config.toml");
        fs::write(&path, "not = [valid").unwrap();

        let error = load(&path).unwrap_err();
        assert!(matches!(error, Error::Usage(_)));
        assert_eq!(error.exit_code(), 2);
    }
}

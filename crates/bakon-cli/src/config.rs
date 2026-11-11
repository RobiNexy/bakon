//! 用户配置：位置、编辑器与全局保留上限。
//!
//! 配置文件是 TOML；缺失时全部取默认值，解析失败按用法错误退出（2）。

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bakon_core::manifest::DEFAULT_MAX_VERSIONS;
use bakon_core::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
    #[serde(default)]
    pub retention: Retention,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
///
/// 先写同目录临时文件并 fsync，再 rename，避免崩溃留下半截配置。
pub fn init(path: &Path) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(Error::Io)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config.toml".into());
    let temporary = parent.join(format!(".{name}.{}.{nonce}.tmp", std::process::id()));

    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(Error::Io)?;
        file.write_all(TEMPLATE.as_bytes()).map_err(Error::Io)?;
        file.sync_all().map_err(Error::Io)?;
        drop(file);
        fs::rename(&temporary, path).map_err(Error::Io)?;
        #[cfg(unix)]
        {
            fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(Error::Io)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
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
        let entries: Vec<_> = fs::read_dir(temporary.path()).unwrap().collect();
        assert_eq!(entries.len(), 1, "init must not leave a temporary file");
    }

    #[test]
    fn unknown_config_fields_are_rejected() {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("config.toml");
        fs::write(&path, "surprise = true\n").unwrap();

        let error = load(&path).unwrap_err();

        assert!(matches!(error, Error::Usage(_)));
        assert_eq!(error.exit_code(), 2);
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

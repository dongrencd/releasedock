use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    ZhCn,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub github_token: Option<String>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub install_root: Option<PathBuf>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub theme_mode: Option<String>,
    /// 后台定时检查 GitHub Release 更新，默认开启
    #[serde(default)]
    pub background_check_enabled: Option<bool>,
    /// 后台检查间隔（分钟），默认 30
    #[serde(default)]
    pub check_interval_minutes: Option<u32>,
    /// 首次关闭窗口时是否已提示过驻留托盘
    #[serde(default)]
    pub tray_hint_shown: Option<bool>,
}

pub struct ConfigStore {
    path: PathBuf,
}

/// 后台检查默认间隔（分钟）
pub const DEFAULT_CHECK_INTERVAL_MINUTES: u32 = 30;

/// 判断后台检查是否启用（缺省视为开启）
pub fn background_check_enabled(config: Option<&Config>) -> bool {
    config
        .and_then(|c| c.background_check_enabled)
        .unwrap_or(true)
}

/// 获取后台检查间隔（分钟），缺省回退到 DEFAULT_CHECK_INTERVAL_MINUTES
pub fn check_interval_minutes(config: Option<&Config>) -> u32 {
    config
        .and_then(|c| c.check_interval_minutes)
        .unwrap_or(DEFAULT_CHECK_INTERVAL_MINUTES)
        .max(1)
}

pub fn effective_install_root(config: Option<&Config>, fallback_root: Option<PathBuf>) -> PathBuf {
    config
        .and_then(|value| value.install_root.clone())
        .or(fallback_root)
        .unwrap_or_else(|| PathBuf::from("."))
}

impl ConfigStore {
    pub fn default_path() -> Result<PathBuf> {
        let project_dirs = ProjectDirs::from("io", "releasedock", "ReleaseDock")
            .context("failed to resolve platform data directory")?;
        Ok(project_dirs.data_local_dir().join("config.json"))
    }

    pub fn default() -> Result<Self> {
        Ok(Self {
            path: Self::default_path()?,
        })
    }

    pub fn from_env_or_default() -> Result<Self> {
        if let Ok(path) = env::var("GHRM_CONFIG_PATH") {
            return Ok(Self::at_path(PathBuf::from(path)));
        }

        Self::default()
    }

    pub fn at_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Config> {
        if !self.path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read config {}", self.path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse config {}", self.path.display()))
    }

    pub fn save(&self, config: &Config) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        let temp_path = self.path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(config).context("failed to serialize config")?;
        fs::write(&temp_path, content)
            .with_context(|| format!("failed to write temporary config {}", temp_path.display()))?;
        fs::rename(&temp_path, &self.path)
            .with_context(|| format!("failed to replace config {}", self.path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_custom_install_root_when_present() {
        let config = Config {
            install_root: Some(PathBuf::from("/custom/root")),
            ..Default::default()
        };

        let resolved = effective_install_root(Some(&config), Some(PathBuf::from("/fallback/root")));

        assert_eq!(resolved, PathBuf::from("/custom/root"));
    }

    #[test]
    fn falls_back_to_supplied_root_when_custom_root_is_missing() {
        let config = Config::default();

        let resolved = effective_install_root(Some(&config), Some(PathBuf::from("/fallback/root")));

        assert_eq!(resolved, PathBuf::from("/fallback/root"));
    }

    #[test]
    fn falls_back_to_current_directory_when_no_root_is_available() {
        let resolved = effective_install_root(None, None);

        assert_eq!(resolved, PathBuf::from("."));
    }

    #[test]
    fn background_check_defaults_to_enabled_when_unset() {
        assert!(background_check_enabled(None));
        assert!(background_check_enabled(Some(&Config::default())));
    }

    #[test]
    fn background_check_respects_explicit_disable() {
        let config = Config {
            background_check_enabled: Some(false),
            ..Default::default()
        };
        assert!(!background_check_enabled(Some(&config)));
    }

    #[test]
    fn check_interval_defaults_when_unset() {
        assert_eq!(check_interval_minutes(None), DEFAULT_CHECK_INTERVAL_MINUTES);
        assert_eq!(
            check_interval_minutes(Some(&Config::default())),
            DEFAULT_CHECK_INTERVAL_MINUTES
        );
    }

    #[test]
    fn check_interval_clamps_below_one() {
        let config = Config {
            check_interval_minutes: Some(0),
            ..Default::default()
        };
        assert_eq!(check_interval_minutes(Some(&config)), 1);
    }

    #[test]
    fn theme_mode_round_trips_through_json() {
        let config = Config {
            theme_mode: Some("dark".to_string()),
            ..Default::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        let decoded: Config = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.theme_mode.as_deref(), Some("dark"));
    }

    #[test]
    fn theme_mode_defaults_to_missing_when_unset() {
        let decoded: Config = serde_json::from_str("{}").unwrap();

        assert_eq!(decoded.theme_mode, None);
    }
}

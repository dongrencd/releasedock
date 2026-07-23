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
}

pub struct ConfigStore {
    path: PathBuf,
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

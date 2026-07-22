use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub schema_version: u32,
    pub apps: Vec<InstalledApp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub repo_url: String,
    pub installed_version: String,
    pub installed_at: DateTime<Utc>,
    pub asset_name: String,
    pub install_path: PathBuf,
}

pub struct ManifestStore {
    path: PathBuf,
}

impl Manifest {
    pub fn empty() -> Self {
        Self {
            schema_version: 1,
            apps: Vec::new(),
        }
    }
}

impl InstalledApp {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        asset_name: impl Into<String>,
        install_path: PathBuf,
    ) -> Self {
        let id = id.into();
        Self {
            repo_url: format!("https://github.com/{id}"),
            id,
            name: name.into(),
            installed_version: version.into(),
            installed_at: Utc::now(),
            asset_name: asset_name.into(),
            install_path,
        }
    }
}

impl ManifestStore {
    pub fn default_path() -> Result<PathBuf> {
        let project_dirs = ProjectDirs::from("io", "ghrm", "GitHub Release Manager")
            .context("failed to resolve platform data directory")?;
        Ok(project_dirs.data_local_dir().join("apps.json"))
    }

    pub fn default() -> Result<Self> {
        Ok(Self {
            path: Self::default_path()?,
        })
    }

    pub fn at_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Manifest> {
        if !self.path.exists() {
            return Ok(Manifest::empty());
        }

        let content = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read manifest {}", self.path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse manifest {}", self.path.display()))
    }

    pub fn save_apps(&self, apps: &[InstalledApp]) -> Result<()> {
        self.save(&Manifest {
            schema_version: 1,
            apps: apps.to_vec(),
        })
    }

    pub fn save(&self, manifest: &Manifest) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create manifest directory {}", parent.display())
            })?;
        }

        let temp_path = self.path.with_extension("json.tmp");
        let content =
            serde_json::to_string_pretty(manifest).context("failed to serialize manifest")?;
        fs::write(&temp_path, content).with_context(|| {
            format!("failed to write temporary manifest {}", temp_path.display())
        })?;
        fs::rename(&temp_path, &self.path)
            .with_context(|| format!("failed to replace manifest {}", self.path.display()))?;
        Ok(())
    }
}

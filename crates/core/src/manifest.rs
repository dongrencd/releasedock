use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::asset_matcher::{InstallType, is_linux_executable_asset_name};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub apps: Vec<InstalledApp>,
    #[serde(default)]
    pub lifecycle_events: Vec<LifecycleEvent>,
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
    #[serde(default)]
    pub launch_path: Option<PathBuf>,
    #[serde(default)]
    pub system_package_name: Option<String>,
    #[serde(default)]
    pub system_package_manager: Option<SystemPackageManager>,
    #[serde(default = "default_install_type")]
    pub install_type: InstallType,
    #[serde(default = "default_install_path_kind")]
    pub install_path_kind: InstallPathKind,
    #[serde(default = "default_uninstall_supported")]
    pub uninstall_supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LifecycleAction {
    Install,
    Update,
    Uninstall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LifecycleOutcome {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleEvent {
    pub repo_id: String,
    pub repo_name: String,
    pub action: LifecycleAction,
    pub outcome: LifecycleOutcome,
    pub recorded_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_path_kind: Option<InstallPathKind>,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl LifecycleEvent {
    pub fn succeeded(
        repo_id: impl Into<String>,
        repo_name: impl Into<String>,
        action: LifecycleAction,
        summary: impl Into<String>,
        version: Option<String>,
        asset_name: Option<String>,
        install_path: Option<PathBuf>,
        install_path_kind: Option<InstallPathKind>,
    ) -> Self {
        Self {
            repo_id: repo_id.into(),
            repo_name: repo_name.into(),
            action,
            outcome: LifecycleOutcome::Succeeded,
            recorded_at: Utc::now(),
            version,
            asset_name,
            install_path,
            install_path_kind,
            summary: summary.into(),
            error: None,
        }
    }

    pub fn failed(
        repo_id: impl Into<String>,
        repo_name: impl Into<String>,
        action: LifecycleAction,
        summary: impl Into<String>,
        error: impl Into<String>,
        version: Option<String>,
        asset_name: Option<String>,
        install_path: Option<PathBuf>,
        install_path_kind: Option<InstallPathKind>,
    ) -> Self {
        Self {
            repo_id: repo_id.into(),
            repo_name: repo_name.into(),
            action,
            outcome: LifecycleOutcome::Failed,
            recorded_at: Utc::now(),
            version,
            asset_name,
            install_path,
            install_path_kind,
            summary: summary.into(),
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InstallPathKind {
    ManagedPath,
    SystemInstaller,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SystemPackageManager {
    Debian,
    Rpm,
    Pacman,
}

pub struct ManifestStore {
    path: PathBuf,
}

impl Manifest {
    pub fn empty() -> Self {
        Self {
            schema_version: 3,
            apps: Vec::new(),
            lifecycle_events: Vec::new(),
        }
    }

    pub fn normalize(mut self) -> Self {
        if self.schema_version < 3 {
            self.schema_version = 3;
        }

        for app in &mut self.apps {
            app.normalize_legacy();
        }

        self.retain_recent_lifecycle_events(5);

        self
    }

    pub fn latest_lifecycle_event(&self, repo_id: &str) -> Option<&LifecycleEvent> {
        self.lifecycle_events
            .iter()
            .rev()
            .find(|event| event.repo_id == repo_id)
    }

    pub fn recent_lifecycle_events(&self, repo_id: &str, limit: usize) -> Vec<&LifecycleEvent> {
        if limit == 0 {
            return Vec::new();
        }

        self.lifecycle_events
            .iter()
            .rev()
            .filter(|event| event.repo_id == repo_id)
            .take(limit)
            .collect()
    }

    pub fn append_lifecycle_event(&mut self, event: LifecycleEvent) {
        self.lifecycle_events.push(event);
        self.retain_recent_lifecycle_events(5);
    }

    fn retain_recent_lifecycle_events(&mut self, limit_per_repo: usize) {
        if limit_per_repo == 0 {
            self.lifecycle_events.clear();
            return;
        }

        let mut seen_counts: HashMap<String, usize> = HashMap::new();
        let mut retained_indices = Vec::new();

        for (index, event) in self.lifecycle_events.iter().enumerate().rev() {
            let count = seen_counts.entry(event.repo_id.clone()).or_insert(0);
            if *count >= limit_per_repo {
                continue;
            }
            *count += 1;
            retained_indices.push(index);
        }

        retained_indices.sort_unstable();
        self.lifecycle_events = retained_indices
            .into_iter()
            .map(|index| self.lifecycle_events[index].clone())
            .collect();
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
        Self::with_install_metadata(
            id,
            name,
            version,
            asset_name,
            install_path,
            InstallType::Unknown,
            InstallPathKind::ManagedPath,
            true,
        )
    }

    pub fn with_install_metadata(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        asset_name: impl Into<String>,
        install_path: PathBuf,
        install_type: InstallType,
        install_path_kind: InstallPathKind,
        uninstall_supported: bool,
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
            launch_path: None,
            system_package_name: None,
            system_package_manager: None,
            install_type,
            install_path_kind,
            uninstall_supported,
        }
    }

    pub fn normalize_legacy(&mut self) {
        if matches!(self.install_type, InstallType::Unknown) {
            self.install_type = infer_install_type(&self.asset_name);
        }

        if matches!(self.install_path_kind, InstallPathKind::Unknown) {
            self.install_path_kind = match self.install_type {
                InstallType::WindowsInstaller | InstallType::LinuxPackage => {
                    InstallPathKind::SystemInstaller
                }
                InstallType::Unknown => InstallPathKind::ManagedPath,
                _ => InstallPathKind::ManagedPath,
            };
        }

        if matches!(
            self.install_type,
            InstallType::WindowsInstaller | InstallType::LinuxPackage
        ) {
            self.install_path_kind = InstallPathKind::SystemInstaller;
            self.uninstall_supported = matches!(
                self.install_type,
                InstallType::LinuxPackage if self.system_package_name.is_some()
            );
        } else if matches!(self.install_path_kind, InstallPathKind::ManagedPath) {
            self.uninstall_supported = true;
        }

        if matches!(self.install_type, InstallType::Executable) {
            self.launch_path = None;
        } else if self.launch_path.is_none() && matches!(self.install_type, InstallType::AppImage) {
            self.launch_path = Some(self.install_path.clone());
        }
    }
}

impl ManifestStore {
    pub fn default_path() -> Result<PathBuf> {
        let project_dirs = ProjectDirs::from("io", "releasedock", "ReleaseDock")
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
            .map(Manifest::normalize)
    }

    pub fn save_apps(&self, apps: &[InstalledApp]) -> Result<()> {
        let mut manifest = self.load()?;
        manifest.apps = apps.to_vec();
        self.save(&manifest)
    }

    pub fn upsert_app(&self, app: InstalledApp) -> Result<()> {
        let mut manifest = self.load()?;
        manifest.apps.retain(|existing| existing.id != app.id);
        manifest.apps.push(app);
        self.save(&manifest)
    }

    pub fn append_lifecycle_event(&self, event: LifecycleEvent) -> Result<()> {
        let mut manifest = self.load()?;
        manifest.append_lifecycle_event(event);
        self.save(&manifest)
    }

    pub fn remove_app(&self, repo_id: &str) -> Result<Option<InstalledApp>> {
        let mut manifest = self.load()?;
        let index = manifest.apps.iter().position(|app| app.id == repo_id);
        let Some(index) = index else {
            return Ok(None);
        };

        let removed = manifest.apps.remove(index);
        self.save(&manifest)?;
        Ok(Some(removed))
    }

    pub fn save(&self, manifest: &Manifest) -> Result<()> {
        let manifest = manifest.clone().normalize();
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create manifest directory {}", parent.display())
            })?;
        }

        let temp_path = self.path.with_extension("json.tmp");
        let content =
            serde_json::to_string_pretty(&manifest).context("failed to serialize manifest")?;
        fs::write(&temp_path, content).with_context(|| {
            format!("failed to write temporary manifest {}", temp_path.display())
        })?;
        fs::rename(&temp_path, &self.path)
            .with_context(|| format!("failed to replace manifest {}", self.path.display()))?;
        Ok(())
    }
}

fn default_schema_version() -> u32 {
    3
}

fn default_install_type() -> InstallType {
    InstallType::Unknown
}

fn default_install_path_kind() -> InstallPathKind {
    InstallPathKind::Unknown
}

fn default_uninstall_supported() -> bool {
    true
}

fn infer_install_type(asset_name: &str) -> InstallType {
    let lowered = asset_name.to_ascii_lowercase();
    if lowered.ends_with(".msi") || lowered.ends_with(".exe") {
        return InstallType::WindowsInstaller;
    }
    if lowered.ends_with(".deb") || lowered.ends_with(".rpm") {
        return InstallType::LinuxPackage;
    }
    if lowered.ends_with(".appimage") {
        return InstallType::AppImage;
    }
    if is_linux_executable_asset_name(&lowered) {
        return InstallType::Executable;
    }
    if lowered.ends_with(".zip") {
        return InstallType::PortableArchive;
    }
    if lowered.ends_with(".tar.gz") || lowered.ends_with(".tgz") || lowered.ends_with(".tar.xz") {
        return InstallType::Archive;
    }
    InstallType::Unknown
}

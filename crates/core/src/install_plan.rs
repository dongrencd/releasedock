use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::{
    asset_matcher::{InstallType, MatchedAsset},
    config::Language,
    integrity::IntegrityPlan,
    manifest::{InstalledApp, SystemPackageManager},
    release::Release,
    release_policy::{ReleaseDirection, ReleasePolicy},
    repo::RepoRef,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallManagementKind {
    ManagedLocal,
    SystemPackage,
    ExternalInstaller,
}

/// State used when selecting an update target. The installer validates it
/// before reading or downloading an artifact, then retains its existing full
/// app-record comparison before the manifest commit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InstallSelectionGuard {
    ExpectedAbsent,
    ExpectedInstalled {
        installed_version: String,
        release_policy: ReleasePolicy,
    },
}

impl InstallSelectionGuard {
    pub fn from_app(app: &InstalledApp) -> Self {
        Self::ExpectedInstalled {
            installed_version: app.installed_version.clone(),
            release_policy: app.release_policy.clone(),
        }
    }

    /// Validates the repository state captured when a release target was
    /// selected. Callers run this before artifact access; the installer still
    /// performs its final full-record comparison under the manifest lock.
    pub fn validate(&self, installed: Option<&InstalledApp>) -> Result<()> {
        match self {
            Self::ExpectedAbsent => {
                if installed.is_some() {
                    bail!("stale install plan: expected no installed app");
                }
            }
            Self::ExpectedInstalled {
                installed_version,
                release_policy,
            } => {
                let Some(installed) = installed else {
                    bail!("stale install plan: managed app is no longer installed");
                };
                if installed.installed_version != *installed_version {
                    bail!(
                        "stale install plan: installed version changed from {} to {}",
                        installed_version,
                        installed.installed_version
                    );
                }
                if installed.release_policy != *release_policy {
                    bail!("stale install plan: release policy changed after target selection");
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallPlan {
    pub repo_id: String,
    pub repo_url: String,
    pub version: String,
    pub asset_name: String,
    pub download_url: String,
    pub install_type: InstallType,
    pub management_kind: InstallManagementKind,
    pub system_package_manager: Option<SystemPackageManager>,
    pub requires_user_confirmation: bool,
    #[serde(default)]
    pub integrity: IntegrityPlan,
    #[serde(default)]
    pub release_direction: ReleaseDirection,
    #[serde(default)]
    pub selection_guard: Option<InstallSelectionGuard>,
    #[serde(default)]
    pub target_policy: Option<ReleasePolicy>,
    pub notes: Vec<String>,
}

impl InstallPlan {
    pub fn from_match(
        repo: &RepoRef,
        release: &Release,
        matched: &MatchedAsset,
        language: Language,
    ) -> Self {
        let management_kind = match matched.install_type {
            InstallType::AppImage | InstallType::PortableArchive | InstallType::Archive => {
                InstallManagementKind::ManagedLocal
            }
            InstallType::Executable => InstallManagementKind::ManagedLocal,
            InstallType::LinuxPackage => InstallManagementKind::SystemPackage,
            InstallType::WindowsInstaller => InstallManagementKind::ExternalInstaller,
            InstallType::Unknown => InstallManagementKind::ExternalInstaller,
        };
        let system_package_manager = infer_system_package_manager(&matched.asset.name);
        let requires_user_confirmation = matches!(
            management_kind,
            InstallManagementKind::ExternalInstaller | InstallManagementKind::SystemPackage
        );
        let mut notes = Vec::new();

        if requires_user_confirmation {
            notes.push(match matched.install_type {
                InstallType::WindowsInstaller => match language {
                    Language::En => {
                        "Windows .exe/.msi installers are downloaded first and must be confirmed before execution."
                            .to_string()
                    }
                    Language::ZhCn => {
                        "Windows .exe/.msi 安装包会先下载，确认后才会执行。".to_string()
                    }
                },
                InstallType::LinuxPackage => match language {
                    Language::En => {
                        "Linux system packages are downloaded first and must be confirmed before system installation."
                            .to_string()
                    }
                    Language::ZhCn => {
                        "Linux 系统安装包会先下载，确认后才会执行系统安装。".to_string()
                    }
                },
                _ => unreachable!(),
            });
        }

        Self {
            repo_id: repo.id(),
            repo_url: repo.github_url(),
            version: release.tag_name.clone(),
            asset_name: matched.asset.name.clone(),
            download_url: matched.asset.browser_download_url.clone(),
            install_type: matched.install_type,
            management_kind,
            system_package_manager,
            requires_user_confirmation,
            integrity: IntegrityPlan::default(),
            release_direction: ReleaseDirection::Unknown,
            selection_guard: None,
            target_policy: None,
            notes,
        }
    }

    pub fn with_integrity(mut self, integrity: IntegrityPlan) -> Self {
        self.integrity = integrity;
        self
    }

    pub fn with_release_direction(mut self, release_direction: ReleaseDirection) -> Self {
        self.release_direction = release_direction;
        self
    }

    pub fn with_selection_guard(mut self, selection_guard: InstallSelectionGuard) -> Self {
        self.selection_guard = Some(selection_guard);
        self
    }

    pub fn with_target_policy(mut self, target_policy: ReleasePolicy) -> Self {
        self.target_policy = Some(target_policy);
        self
    }
}

fn infer_system_package_manager(asset_name: &str) -> Option<SystemPackageManager> {
    let name = asset_name.to_ascii_lowercase();
    if name.ends_with(".deb") {
        return Some(SystemPackageManager::Debian);
    }
    if name.ends_with(".rpm") {
        return Some(SystemPackageManager::Rpm);
    }
    if name.ends_with(".pkg.tar.zst")
        || name.ends_with(".pkg.tar.xz")
        || name.ends_with(".pkg.tar.gz")
    {
        return Some(SystemPackageManager::Pacman);
    }
    None
}

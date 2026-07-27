use serde::{Deserialize, Serialize};

use crate::{
    asset_matcher::{InstallType, MatchedAsset},
    config::Language,
    manifest::SystemPackageManager,
    release::Release,
    repo::RepoRef,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallManagementKind {
    ManagedLocal,
    SystemPackage,
    ExternalInstaller,
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
            notes,
        }
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

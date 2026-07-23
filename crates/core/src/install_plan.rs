use serde::{Deserialize, Serialize};

use crate::{
    asset_matcher::{InstallType, MatchedAsset},
    config::Language,
    release::Release,
    repo::RepoRef,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallPlan {
    pub repo_id: String,
    pub repo_url: String,
    pub version: String,
    pub asset_name: String,
    pub download_url: String,
    pub install_type: InstallType,
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
        let requires_user_confirmation = matches!(
            matched.install_type,
            InstallType::WindowsInstaller | InstallType::LinuxPackage
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
                        "Linux .deb/.rpm packages are downloaded first and must be confirmed before system installation."
                            .to_string()
                    }
                    Language::ZhCn => {
                        "Linux .deb/.rpm 安装包会先下载，确认后才会执行系统安装。".to_string()
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
            requires_user_confirmation,
            notes,
        }
    }
}

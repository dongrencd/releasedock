use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::release::{Release, ReleaseAsset};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperatingSystem {
    Windows,
    Linux,
    Macos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Architecture {
    X64,
    Arm64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedAsset {
    pub asset: ReleaseAsset,
    pub score: i32,
    pub install_type: InstallType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallType {
    WindowsInstaller,
    PortableArchive,
    AppImage,
    LinuxPackage,
    Archive,
    Unknown,
}

#[derive(Debug, Error)]
pub enum AssetMatchError {
    #[error("no release asset matches the current platform")]
    NotFound,
}

pub struct AssetMatcher {
    os: OperatingSystem,
    arch: Architecture,
}

impl AssetMatcher {
    pub fn new(os: OperatingSystem, arch: Architecture) -> Self {
        Self { os, arch }
    }

    pub fn current() -> Self {
        Self {
            os: current_os(),
            arch: current_arch(),
        }
    }

    pub fn select_best(&self, release: &Release) -> Result<MatchedAsset, AssetMatchError> {
        release
            .assets
            .iter()
            .filter_map(|asset| {
                let (score, install_type) = self.score(asset);
                (score > 0).then(|| MatchedAsset {
                    asset: asset.clone(),
                    score,
                    install_type,
                })
            })
            .max_by_key(|matched| matched.score)
            .ok_or(AssetMatchError::NotFound)
    }

    fn score(&self, asset: &ReleaseAsset) -> (i32, InstallType) {
        let name = asset.name.to_ascii_lowercase();
        let mut score = 0;

        score += match self.os {
            OperatingSystem::Windows
                if contains_any(&name, &["windows", "win32", "win64", "win"]) =>
            {
                50
            }
            OperatingSystem::Linux if contains_any(&name, &["linux", "appimage"]) => 50,
            OperatingSystem::Macos if contains_any(&name, &["macos", "darwin", "apple"]) => 50,
            _ => 0,
        };

        score += match self.arch {
            Architecture::X64 if contains_any(&name, &["x86_64", "amd64", "x64"]) => 30,
            Architecture::Arm64 if contains_any(&name, &["aarch64", "arm64"]) => 30,
            _ => 0,
        };

        let install_type = classify_install_type(self.os, &name);
        score += format_score(self.os, install_type);

        (score, install_type)
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn classify_install_type(os: OperatingSystem, name: &str) -> InstallType {
    if os == OperatingSystem::Windows && (name.ends_with(".msi") || name.ends_with(".exe")) {
        return InstallType::WindowsInstaller;
    }
    if os == OperatingSystem::Linux && name.ends_with(".appimage") {
        return InstallType::AppImage;
    }
    if os == OperatingSystem::Linux && (name.ends_with(".deb") || name.ends_with(".rpm")) {
        return InstallType::LinuxPackage;
    }
    if name.ends_with(".zip") {
        return InstallType::PortableArchive;
    }
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") || name.ends_with(".tar.xz") {
        return InstallType::Archive;
    }
    InstallType::Unknown
}

fn format_score(os: OperatingSystem, install_type: InstallType) -> i32 {
    match (os, install_type) {
        (OperatingSystem::Windows, InstallType::WindowsInstaller) => 20,
        (OperatingSystem::Windows, InstallType::PortableArchive) => 12,
        (OperatingSystem::Linux, InstallType::AppImage) => 24,
        (OperatingSystem::Linux, InstallType::Archive) => 16,
        (OperatingSystem::Linux, InstallType::PortableArchive) => 12,
        (OperatingSystem::Linux, InstallType::LinuxPackage) => 10,
        (_, InstallType::Archive | InstallType::PortableArchive) => 12,
        _ => 0,
    }
}

fn current_os() -> OperatingSystem {
    match std::env::consts::OS {
        "windows" => OperatingSystem::Windows,
        "macos" => OperatingSystem::Macos,
        _ => OperatingSystem::Linux,
    }
}

fn current_arch() -> Architecture {
    match std::env::consts::ARCH {
        "aarch64" => Architecture::Arm64,
        _ => Architecture::X64,
    }
}

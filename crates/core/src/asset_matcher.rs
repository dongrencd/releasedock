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
    Executable,
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
        let install_type = classify_install_type(self.os, self.arch, &name);
        if matches!(install_type, InstallType::Unknown) {
            return (0, install_type);
        }

        let mut score = type_score(install_type);

        score += match self.os {
            OperatingSystem::Windows
                if contains_any(&name, &["windows", "win32", "win64", "win"]) =>
            {
                50
            }
            OperatingSystem::Linux if contains_any(&name, &["linux", "appimage"]) => 50,
            OperatingSystem::Linux if install_type == InstallType::Executable => 50,
            OperatingSystem::Macos if contains_any(&name, &["macos", "darwin", "apple"]) => 50,
            _ => 0,
        };

        score += match self.arch {
            Architecture::X64 if contains_any(&name, &["x86_64", "amd64", "x64"]) => 30,
            Architecture::Arm64 if contains_any(&name, &["aarch64", "arm64"]) => 30,
            _ => 0,
        };

        score += format_score(self.os, install_type, &name);

        (score, install_type)
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn classify_install_type(os: OperatingSystem, arch: Architecture, name: &str) -> InstallType {
    if os == OperatingSystem::Windows && is_windows_installer_asset_name(name) {
        return InstallType::WindowsInstaller;
    }
    if os == OperatingSystem::Windows && is_windows_executable_asset_name(name) {
        return InstallType::Executable;
    }
    if os == OperatingSystem::Linux && name.ends_with(".appimage") {
        return InstallType::AppImage;
    }
    if os == OperatingSystem::Linux
        && (name.ends_with(".deb")
            || name.ends_with(".rpm")
            || name.ends_with(".pkg.tar.zst")
            || name.ends_with(".pkg.tar.xz")
            || name.ends_with(".pkg.tar.gz"))
    {
        return InstallType::LinuxPackage;
    }
    if os == OperatingSystem::Linux && is_linux_executable_candidate(name, arch) {
        return InstallType::Executable;
    }
    if name.ends_with(".zip") {
        return InstallType::PortableArchive;
    }
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") || name.ends_with(".tar.xz") {
        return InstallType::Archive;
    }
    InstallType::Unknown
}

fn is_windows_executable_asset_name(name: &str) -> bool {
    if !name.ends_with(".exe") {
        return false;
    }

    !is_windows_installer_asset_name(name)
}

fn is_windows_installer_asset_name(name: &str) -> bool {
    if name.ends_with(".msi") {
        return true;
    }

    if !name.ends_with(".exe") {
        return false;
    }

    contains_any(
        name,
        &[
            "setup",
            "install",
            "installer",
            "uninstall",
            "bootstrap",
            "updater",
            "update",
            "patch",
        ],
    )
}

pub(crate) fn is_linux_executable_asset_name(name: &str) -> bool {
    if name.contains('.') || is_auxiliary_asset_name(name) {
        return false;
    }

    contains_any(name, &["linux"])
        && contains_any(name, &["x86_64", "amd64", "x64", "aarch64", "arm64"])
}

pub(crate) fn linux_executable_matches_arch(name: &str, arch: Architecture) -> bool {
    match arch {
        Architecture::X64 => contains_any(name, &["x86_64", "amd64", "x64"]),
        Architecture::Arm64 => contains_any(name, &["aarch64", "arm64"]),
    }
}

fn is_linux_executable_candidate(name: &str, arch: Architecture) -> bool {
    is_linux_executable_asset_name(name) && linux_executable_matches_arch(name, arch)
}

fn is_auxiliary_asset_name(name: &str) -> bool {
    contains_any(
        name,
        &[
            "checksum", "sha256", "sha512", "manifest", "readme", "license", "source",
        ],
    )
}

fn format_score(os: OperatingSystem, install_type: InstallType, name: &str) -> i32 {
    match (os, install_type) {
        (OperatingSystem::Windows, InstallType::PortableArchive) => 60,
        (OperatingSystem::Windows, InstallType::Archive) => 55,
        (OperatingSystem::Windows, InstallType::WindowsInstaller) => {
            if name.ends_with(".msi") {
                10
            } else if contains_any(
                name,
                &[
                    "setup",
                    "install",
                    "installer",
                    "bootstrap",
                    "updater",
                    "update",
                    "patch",
                ],
            ) {
                20
            } else {
                15
            }
        }
        (OperatingSystem::Linux, InstallType::AppImage) => 80,
        (OperatingSystem::Linux, InstallType::Executable) => 40,
        (OperatingSystem::Linux, InstallType::Archive) => 70,
        (OperatingSystem::Linux, InstallType::PortableArchive) => 60,
        (OperatingSystem::Linux, InstallType::LinuxPackage) => 10,
        (_, InstallType::Archive | InstallType::PortableArchive) => 50,
        _ => 0,
    }
}

fn type_score(install_type: InstallType) -> i32 {
    match install_type {
        InstallType::AppImage | InstallType::Archive | InstallType::PortableArchive => 1_000,
        InstallType::Executable => 900,
        InstallType::WindowsInstaller | InstallType::LinuxPackage => 100,
        InstallType::Unknown => 0,
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

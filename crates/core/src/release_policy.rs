use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::release::Release;

/// 自动选择 Release 时允许的版本通道。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReleaseChannel {
    #[default]
    Stable,
    Prerelease,
}

/// 仓库级 Release 选择策略。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ReleasePolicy {
    pub channel: ReleaseChannel,
    pub pinned_version: Option<String>,
    pub ignored_versions: Vec<String>,
}

/// 对单个已安装仓库执行的持久策略变更。
///
/// `PinCurrent` 由 `ManifestStore` 在持有写锁时解析，因此不会把并发更新前
/// 读到的旧版本错误地写回为 pin。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyMutation {
    PinCurrent,
    PinVersion(String),
    Unpin,
    IgnoreVersion(String),
    UnignoreVersion(String),
    SetChannel(ReleaseChannel),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyMutationResult {
    pub policy: ReleasePolicy,
    pub changed: bool,
}

impl PolicyMutation {
    pub(crate) fn apply(&self, policy: &mut ReleasePolicy, installed_version: &str) {
        match self {
            Self::PinCurrent => policy.pinned_version = Some(installed_version.to_string()),
            Self::PinVersion(version) => policy.pinned_version = Some(version.clone()),
            Self::Unpin => policy.pinned_version = None,
            Self::IgnoreVersion(version) => {
                // Preserve the first occurrence so an already-normalized ignore is a true no-op.
                let mut found = false;
                policy.ignored_versions.retain(|existing| {
                    if existing != version {
                        return true;
                    }
                    let keep = !found;
                    found = true;
                    keep
                });
                if !found {
                    policy.ignored_versions.push(version.clone());
                }
            }
            Self::UnignoreVersion(version) => {
                policy
                    .ignored_versions
                    .retain(|existing| existing != version);
            }
            Self::SetChannel(channel) => policy.channel = *channel,
        }
    }

    pub(crate) fn summary(&self, policy: &ReleasePolicy) -> String {
        match self {
            Self::PinCurrent | Self::PinVersion(_) => format!(
                "Pinned release {}",
                policy.pinned_version.as_deref().unwrap_or("unknown")
            ),
            Self::Unpin => "Removed release pin".to_string(),
            Self::IgnoreVersion(version) => format!("Ignored release {version}"),
            Self::UnignoreVersion(version) => format!("Stopped ignoring release {version}"),
            Self::SetChannel(ReleaseChannel::Stable) => {
                "Changed release channel to stable".to_string()
            }
            Self::SetChannel(ReleaseChannel::Prerelease) => {
                "Changed release channel to prerelease".to_string()
            }
        }
    }
}

/// 目标 Release 相对当前已安装 tag 的方向。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReleaseDirection {
    Upgrade,
    Downgrade,
    Reinstall,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseSelection {
    pub release: Release,
    pub direction: ReleaseDirection,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReleaseSelectionError {
    #[error("pinned release tag `{0}` was not found in the selected channel")]
    PinnedVersionNotFound(String),
    #[error("manual release tag `{0}` was not found or is a draft")]
    ManualVersionNotFound(String),
    #[error("no release matches the configured release policy")]
    NoMatchingRelease,
}

pub struct ReleaseSelector;

impl ReleaseSelector {
    /// 手动 tag 是单次显式选择，因此优先于持久策略，但 draft 始终不可选。
    pub fn select(
        releases: &[Release],
        policy: &ReleasePolicy,
        current_tag: Option<&str>,
        manual_tag: Option<&str>,
    ) -> Result<ReleaseSelection, ReleaseSelectionError> {
        let target = if let Some(manual_tag) = manual_tag {
            releases
                .iter()
                .find(|release| !release.draft && release.tag_name == manual_tag)
                .ok_or_else(|| {
                    ReleaseSelectionError::ManualVersionNotFound(manual_tag.to_string())
                })?
        } else {
            Self::select_automatically(releases, policy)?
        };

        Ok(ReleaseSelection {
            release: target.clone(),
            direction: release_direction(releases, current_tag, &target.tag_name),
        })
    }

    fn select_automatically<'a>(
        releases: &'a [Release],
        policy: &ReleasePolicy,
    ) -> Result<&'a Release, ReleaseSelectionError> {
        let eligible = || {
            releases.iter().filter(|release| {
                !release.draft
                    && (policy.channel == ReleaseChannel::Prerelease || !release.prerelease)
            })
        };

        if let Some(pinned_version) = policy.pinned_version.as_deref() {
            return eligible()
                .find(|release| release.tag_name == pinned_version)
                .ok_or_else(|| {
                    ReleaseSelectionError::PinnedVersionNotFound(pinned_version.to_string())
                });
        }

        eligible()
            .find(|release| {
                !policy
                    .ignored_versions
                    .iter()
                    .any(|ignored| ignored == &release.tag_name)
            })
            .ok_or(ReleaseSelectionError::NoMatchingRelease)
    }
}

fn release_direction(
    releases: &[Release],
    current_tag: Option<&str>,
    target_tag: &str,
) -> ReleaseDirection {
    let Some(current_tag) = current_tag else {
        return ReleaseDirection::Unknown;
    };
    let Some(current_index) = releases
        .iter()
        .position(|release| release.tag_name == current_tag)
    else {
        return ReleaseDirection::Unknown;
    };
    let Some(target_index) = releases
        .iter()
        .position(|release| release.tag_name == target_tag)
    else {
        return ReleaseDirection::Unknown;
    };

    match target_index.cmp(&current_index) {
        std::cmp::Ordering::Less => ReleaseDirection::Upgrade,
        std::cmp::Ordering::Greater => ReleaseDirection::Downgrade,
        std::cmp::Ordering::Equal => ReleaseDirection::Reinstall,
    }
}

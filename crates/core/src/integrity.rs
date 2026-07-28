use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::release::{Release, ReleaseAsset, ReleaseClient};

/// Checksum files are metadata, not payloads. Keeping a hard upper bound limits
/// memory and network use when a release contains a misleadingly named asset.
pub const MAX_CHECKSUM_ASSET_SIZE: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IntegrityStatus {
    VerifiedChecksum,
    #[default]
    RecordedOnly,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct IntegrityPlan {
    pub expected_sha256: Option<String>,
    pub checksum_asset_name: Option<String>,
    pub status: IntegrityStatus,
}

pub struct IntegrityVerifier;

impl IntegrityVerifier {
    /// Downloads only recognized checksum metadata and resolves one unambiguous
    /// digest. Discovery does not prove artifact integrity; callers must still
    /// run `verify_file_sha256` against the downloaded artifact.
    pub async fn discover(
        client: &ReleaseClient,
        release: &Release,
        target: &ReleaseAsset,
    ) -> Result<IntegrityPlan> {
        let mut downloaded = Vec::new();

        for asset in &release.assets {
            if checksum_asset_kind(&asset.name, &target.name).is_none()
                || asset.size > MAX_CHECKSUM_ASSET_SIZE
            {
                continue;
            }

            let contents = client
                .download_bytes_limited(&asset.browser_download_url, MAX_CHECKSUM_ASSET_SIZE)
                .await
                .with_context(|| format!("failed to download checksum asset `{}`", asset.name))?;
            downloaded.push((asset.name.clone(), contents));
        }

        let sources = downloaded
            .iter()
            .map(|(name, contents)| (name.as_str(), contents.as_slice()))
            .collect::<Vec<_>>();
        Self::from_checksum_contents(&target.name, &sources)
    }

    /// Resolves preloaded checksum assets. Repeated identical digests are safe;
    /// differing digests are rejected rather than trusting release asset order.
    pub fn from_checksum_contents(
        target_asset_name: &str,
        checksum_assets: &[(&str, &[u8])],
    ) -> Result<IntegrityPlan> {
        let mut resolved: Option<(String, String)> = None;

        for (asset_name, contents) in checksum_assets {
            let Some(digest) = checksum_for_asset(asset_name, contents, target_asset_name)? else {
                continue;
            };

            if let Some((current, current_asset)) = &resolved {
                if current != &digest {
                    bail!(
                        "conflicting SHA-256 checksums for `{target_asset_name}` in `{current_asset}` and `{asset_name}`"
                    );
                }
            } else {
                resolved = Some((digest, (*asset_name).to_string()));
            }
        }

        Ok(match resolved {
            Some((expected_sha256, checksum_asset_name)) => IntegrityPlan {
                expected_sha256: Some(expected_sha256),
                checksum_asset_name: Some(checksum_asset_name),
                status: IntegrityStatus::RecordedOnly,
            },
            None => IntegrityPlan::default(),
        })
    }
}

/// Parses one recognized checksum asset for `target_asset_name`.
///
/// A target-specific `<asset>.sha256` accepts a bare digest or the conventional
/// `digest [*]filename` form. Shared manifests require an exact, case-sensitive
/// basename match so a checksum for a similarly named artifact is never reused.
pub fn checksum_for_asset(
    checksum_asset_name: &str,
    contents: &[u8],
    target_asset_name: &str,
) -> Result<Option<String>> {
    let Some(kind) = checksum_asset_kind(checksum_asset_name, target_asset_name) else {
        return Ok(None);
    };
    let text = std::str::from_utf8(contents)
        .with_context(|| format!("checksum asset `{checksum_asset_name}` is not valid UTF-8"))?;
    let mut resolved: Option<String> = None;

    for line in text
        .lines()
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.is_empty())
    {
        if kind == ChecksumAssetKind::TargetSpecific && is_sha256(line) {
            merge_digest(
                &mut resolved,
                line.to_ascii_lowercase(),
                target_asset_name,
                checksum_asset_name,
            )?;
            continue;
        }

        let Some((digest, listed_name)) = parse_checksum_line(line) else {
            continue;
        };

        let listed_basename = listed_name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(listed_name);
        if listed_basename != target_asset_name {
            continue;
        }
        if !is_sha256(digest) {
            invalid_digest(target_asset_name, checksum_asset_name)?;
        }

        merge_digest(
            &mut resolved,
            digest.to_ascii_lowercase(),
            target_asset_name,
            checksum_asset_name,
        )?;
    }

    if kind == ChecksumAssetKind::TargetSpecific && resolved.is_none() {
        // The asset name claims to describe this target, so an empty file,
        // malformed record, comment, or record for another file fails closed.
        invalid_digest(target_asset_name, checksum_asset_name)?;
    }

    Ok(resolved)
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path)
        .with_context(|| format!("failed to open artifact {} for SHA-256", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read artifact {} for SHA-256", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

pub fn verify_file_sha256(path: &Path, expected_sha256: &str) -> Result<String> {
    if !is_sha256(expected_sha256) {
        bail!("invalid expected SHA-256 `{expected_sha256}`");
    }

    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        bail!(
            "SHA-256 mismatch for {}: expected {}, calculated {}",
            path.display(),
            expected_sha256.to_ascii_lowercase(),
            actual
        );
    }
    Ok(actual)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChecksumAssetKind {
    TargetSpecific,
    Manifest,
}

fn checksum_asset_kind(name: &str, target_asset_name: &str) -> Option<ChecksumAssetKind> {
    let name = name.to_ascii_lowercase();
    if name == format!("{}.sha256", target_asset_name.to_ascii_lowercase()) {
        return Some(ChecksumAssetKind::TargetSpecific);
    }
    if matches!(
        name.as_str(),
        "sha256sums" | "sha256sums.txt" | "checksums.txt"
    ) {
        return Some(ChecksumAssetKind::Manifest);
    }
    None
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_checksum_line(line: &str) -> Option<(&str, &str)> {
    let separator = line.find(' ')?;
    let remainder = &line[separator..];
    let mode = *remainder.as_bytes().get(1)?;
    if !matches!(mode, b' ' | b'*') {
        return None;
    }

    // GNU checksum output is `digest`, one separator space, one mode character,
    // then the filename. A `*` after text mode is therefore part of the name.
    Some((&line[..separator], &remainder[2..]))
}

fn merge_digest(
    resolved: &mut Option<String>,
    digest: String,
    target_asset_name: &str,
    checksum_asset_name: &str,
) -> Result<()> {
    if let Some(current) = resolved {
        if current != &digest {
            bail!(
                "conflicting SHA-256 checksums for `{target_asset_name}` in `{checksum_asset_name}`"
            );
        }
    } else {
        *resolved = Some(digest);
    }
    Ok(())
}

fn invalid_digest<T>(target_asset_name: &str, checksum_asset_name: &str) -> Result<T> {
    bail!("invalid SHA-256 for `{target_asset_name}` in checksum asset `{checksum_asset_name}`")
}

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use tar::Archive as TarArchive;
use xz2::read::XzDecoder;
use zip::ZipArchive;

use crate::{
    asset_matcher::InstallType,
    config::Config,
    install_plan::InstallPlan,
    manifest::{InstallPathKind, InstalledApp, ManifestStore},
    release::ReleaseClient,
    repo::RepoRef,
};

#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub app: InstalledApp,
    pub download_path: PathBuf,
    pub install_path: PathBuf,
    pub install_type: InstallType,
    pub install_path_kind: InstallPathKind,
    pub uninstall_supported: bool,
}

pub async fn install_from_plan(
    plan: &InstallPlan,
    manifest_store: &ManifestStore,
    asset_fixture: Option<&Path>,
    runtime_config: Option<&Config>,
) -> Result<InstallOutcome> {
    let repo = RepoRef::parse(&plan.repo_url)?;
    let download_path = download_asset(plan, manifest_store, asset_fixture, runtime_config).await?;
    let (install_path, install_path_kind, uninstall_supported) = match plan.install_type {
        InstallType::AppImage => (
            install_appimage(
                &download_path,
                &repo,
                manifest_store,
                &plan.asset_name,
                runtime_config,
            )?,
            InstallPathKind::ManagedPath,
            true,
        ),
        InstallType::PortableArchive | InstallType::Archive => (
            extract_archive(
                &download_path,
                &repo,
                manifest_store,
                &plan.asset_name,
                runtime_config,
            )?,
            InstallPathKind::ManagedPath,
            true,
        ),
        InstallType::WindowsInstaller => (
            install_windows_installer(
                &download_path,
                &repo,
                manifest_store,
                &plan.asset_name,
                runtime_config,
            )?,
            InstallPathKind::SystemInstaller,
            false,
        ),
        InstallType::LinuxPackage => (
            install_linux_package(
                &download_path,
                &repo,
                manifest_store,
                &plan.asset_name,
                runtime_config,
            )?,
            InstallPathKind::SystemInstaller,
            false,
        ),
        InstallType::Unknown => anyhow::bail!(
            "installing {:?} assets is not implemented yet; use the preview path instead",
            plan.install_type
        ),
    };

    let app = InstalledApp::with_install_metadata(
        repo.id(),
        repo.name.clone(),
        plan.version.clone(),
        plan.asset_name.clone(),
        install_path.clone(),
        plan.install_type,
        install_path_kind,
        uninstall_supported,
    );
    manifest_store.upsert_app(app.clone())?;

    Ok(InstallOutcome {
        app,
        download_path,
        install_path,
        install_type: plan.install_type,
        install_path_kind,
        uninstall_supported,
    })
}

pub fn uninstall_repo(
    manifest_store: &ManifestStore,
    repo_id: &str,
) -> Result<Option<InstalledApp>> {
    let manifest = manifest_store.load()?;
    let Some(app) = manifest.apps.into_iter().find(|app| app.id == repo_id) else {
        return Ok(None);
    };

    if !app.uninstall_supported {
        anyhow::bail!(
            "{} was installed by a system installer and must be removed from the system package manager",
            app.id
        );
    }

    let Some(app) = manifest_store.remove_app(repo_id)? else {
        return Ok(None);
    };

    remove_path(&app.install_path)?;
    Ok(Some(app))
}

async fn download_asset(
    plan: &InstallPlan,
    manifest_store: &ManifestStore,
    asset_fixture: Option<&Path>,
    runtime_config: Option<&Config>,
) -> Result<PathBuf> {
    let repo = RepoRef::parse(&plan.repo_url)?;
    let download_dir = cache_dir(manifest_store, runtime_config).join(repo.id().replace('/', "_"));
    fs::create_dir_all(&download_dir)
        .with_context(|| format!("failed to create download cache {}", download_dir.display()))?;

    let download_path = download_dir.join(&plan.asset_name);
    if let Some(asset_fixture) = asset_fixture {
        fs::copy(asset_fixture, &download_path).with_context(|| {
            format!(
                "failed to copy fixture asset {} to {}",
                asset_fixture.display(),
                download_path.display()
            )
        })?;
        return Ok(download_path);
    }

    let token = runtime_config.and_then(|config| config.github_token.as_deref());
    let proxy = runtime_config.and_then(|config| config.proxy_url.as_deref());
    let client = ReleaseClient::new(token, proxy)?;
    let bytes = client.download_bytes(&plan.download_url).await?;
    fs::write(&download_path, bytes)
        .with_context(|| format!("failed to write download {}", download_path.display()))?;
    Ok(download_path)
}

fn install_appimage(
    downloaded: &Path,
    repo: &RepoRef,
    manifest_store: &ManifestStore,
    asset_name: &str,
    runtime_config: Option<&Config>,
) -> Result<PathBuf> {
    let install_dir = install_dir(manifest_store, repo, runtime_config);
    fs::create_dir_all(&install_dir).with_context(|| {
        format!(
            "failed to create install directory {}",
            install_dir.display()
        )
    })?;

    let install_path = install_dir.join(asset_name);
    fs::copy(downloaded, &install_path).with_context(|| {
        format!(
            "failed to copy AppImage from {} to {}",
            downloaded.display(),
            install_path.display()
        )
    })?;
    mark_executable(&install_path)?;
    Ok(install_path)
}

fn extract_archive(
    downloaded: &Path,
    repo: &RepoRef,
    manifest_store: &ManifestStore,
    asset_name: &str,
    runtime_config: Option<&Config>,
) -> Result<PathBuf> {
    let install_dir = install_dir(manifest_store, repo, runtime_config);
    fs::create_dir_all(&install_dir).with_context(|| {
        format!(
            "failed to create install directory {}",
            install_dir.display()
        )
    })?;

    if asset_name.ends_with(".zip") {
        extract_zip(downloaded, &install_dir)?;
    } else if asset_name.ends_with(".tar.gz") || asset_name.ends_with(".tgz") {
        extract_tar_archive(GzDecoder::new(open_archive(downloaded)?), &install_dir)?;
    } else if asset_name.ends_with(".tar.xz") {
        extract_tar_archive(XzDecoder::new(open_archive(downloaded)?), &install_dir)?;
    } else {
        anyhow::bail!("archive format for {} is not supported yet", asset_name);
    }

    Ok(install_dir)
}

fn extract_zip(downloaded: &Path, install_dir: &Path) -> Result<()> {
    let file = fs::File::open(downloaded)
        .with_context(|| format!("failed to open archive {}", downloaded.display()))?;
    let mut archive = ZipArchive::new(file).context("failed to read zip archive")?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("failed to read zip entry #{index}"))?;
        let Some(path) = entry.enclosed_name().map(|path| path.to_owned()) else {
            anyhow::bail!("zip entry contains an unsafe path");
        };
        let out_path = install_dir.join(path);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .with_context(|| format!("failed to create directory {}", out_path.display()))?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        let mut out_file = fs::File::create(&out_path)
            .with_context(|| format!("failed to create file {}", out_path.display()))?;
        io::copy(&mut entry, &mut out_file)
            .with_context(|| format!("failed to extract {}", out_path.display()))?;
    }

    Ok(())
}

fn open_archive(downloaded: &Path) -> Result<fs::File> {
    fs::File::open(downloaded)
        .with_context(|| format!("failed to open archive {}", downloaded.display()))
}

fn extract_tar_archive<R: io::Read>(reader: R, install_dir: &Path) -> Result<()> {
    let mut archive = TarArchive::new(reader);
    archive
        .unpack(install_dir)
        .with_context(|| format!("failed to extract archive into {}", install_dir.display()))?;
    Ok(())
}

fn install_windows_installer(
    downloaded: &Path,
    repo: &RepoRef,
    manifest_store: &ManifestStore,
    asset_name: &str,
    runtime_config: Option<&Config>,
) -> Result<PathBuf> {
    let install_dir = install_dir(manifest_store, repo, runtime_config);
    fs::create_dir_all(&install_dir).with_context(|| {
        format!(
            "failed to create install directory {}",
            install_dir.display()
        )
    })?;

    let target_path = install_dir.join(asset_name);
    fs::copy(downloaded, &target_path).with_context(|| {
        format!(
            "failed to copy Windows installer from {} to {}",
            downloaded.display(),
            target_path.display()
        )
    })?;

    let status = run_windows_installer(&target_path)?;
    if !status.success() {
        anyhow::bail!(
            "Windows installer exited with status {} for {}",
            status,
            target_path.display()
        );
    }

    Ok(target_path)
}

fn install_linux_package(
    downloaded: &Path,
    repo: &RepoRef,
    manifest_store: &ManifestStore,
    asset_name: &str,
    runtime_config: Option<&Config>,
) -> Result<PathBuf> {
    let install_dir = install_dir(manifest_store, repo, runtime_config);
    fs::create_dir_all(&install_dir).with_context(|| {
        format!(
            "failed to create install directory {}",
            install_dir.display()
        )
    })?;

    let target_path = install_dir.join(asset_name);
    fs::copy(downloaded, &target_path).with_context(|| {
        format!(
            "failed to copy Linux package from {} to {}",
            downloaded.display(),
            target_path.display()
        )
    })?;

    let status = run_linux_package_installer(&target_path)?;
    if !status.success() {
        anyhow::bail!(
            "Linux package installer exited with status {} for {}",
            status,
            target_path.display()
        );
    }

    Ok(target_path)
}

fn run_windows_installer(path: &Path) -> Result<std::process::ExitStatus> {
    #[cfg(target_os = "windows")]
    {
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if ext == "msi" {
            return Command::new("msiexec")
                .args(["/i", path.to_string_lossy().as_ref(), "/passive"])
                .status()
                .with_context(|| format!("failed to run msiexec for {}", path.display()));
        }

        return Command::new(path)
            .status()
            .with_context(|| format!("failed to run installer {}", path.display()));
    }

    #[cfg(not(target_os = "windows"))]
    {
        anyhow::bail!(
            "Windows installers can only be executed on Windows; downloaded file kept at {}",
            path.display()
        );
    }
}

fn run_linux_package_installer(path: &Path) -> Result<std::process::ExitStatus> {
    #[cfg(target_os = "linux")]
    {
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if ext == "deb" {
            return Command::new("pkexec")
                .args(["apt", "install", "-y", path.to_string_lossy().as_ref()])
                .status()
                .with_context(|| format!("failed to run apt for {}", path.display()));
        }

        if ext == "rpm" {
            return Command::new("pkexec")
                .args(["dnf", "install", "-y", path.to_string_lossy().as_ref()])
                .status()
                .with_context(|| format!("failed to run dnf for {}", path.display()));
        }

        anyhow::bail!("unsupported Linux package type for {}", path.display())
    }

    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!(
            "Linux packages can only be executed on Linux; downloaded file kept at {}",
            path.display()
        );
    }
}

fn remove_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to inspect installed path {}", path.display()))?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove installed directory {}", path.display()))?;
    } else {
        fs::remove_file(path)
            .with_context(|| format!("failed to remove installed file {}", path.display()))?;
    }

    Ok(())
}

fn install_dir(
    manifest_store: &ManifestStore,
    repo: &RepoRef,
    runtime_config: Option<&Config>,
) -> PathBuf {
    cache_root(manifest_store, runtime_config)
        .join("apps")
        .join(format!("{}-{}", repo.owner, repo.name))
}

fn cache_dir(manifest_store: &ManifestStore, runtime_config: Option<&Config>) -> PathBuf {
    cache_root(manifest_store, runtime_config).join("downloads")
}

fn cache_root(manifest_store: &ManifestStore, runtime_config: Option<&Config>) -> PathBuf {
    runtime_config
        .and_then(|config| config.install_root.clone())
        .or_else(|| manifest_store.path().parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn mark_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)
            .with_context(|| format!("failed to inspect executable {}", path.display()))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .with_context(|| format!("failed to mark {} executable", path.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        asset_matcher::{Architecture, AssetMatcher, OperatingSystem},
        release::{Release, ReleaseAsset},
    };
    use flate2::{Compression, write::GzEncoder};
    use tar::Builder;
    use xz2::write::XzEncoder;

    fn sample_plan(install_type: InstallType, asset_name: &str) -> InstallPlan {
        InstallPlan {
            repo_id: "owner/project".to_string(),
            repo_url: "https://github.com/owner/project".to_string(),
            version: "v1.2.3".to_string(),
            asset_name: asset_name.to_string(),
            download_url: format!(
                "https://github.com/owner/project/releases/download/v1.2.3/{asset_name}"
            ),
            install_type,
            requires_user_confirmation: false,
            notes: Vec::new(),
        }
    }

    fn write_tar_gz_fixture(path: &Path) {
        let file = fs::File::create(path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        let contents = b"hello world";

        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_path("bundle/hello.txt").expect("set path");
        header.set_cksum();
        builder
            .append_data(&mut header, "bundle/hello.txt", &contents[..])
            .expect("append tar entry");
        builder.finish().expect("finish tar");
    }

    fn write_tar_xz_fixture(path: &Path) {
        let file = fs::File::create(path).unwrap();
        let encoder = XzEncoder::new(file, 6);
        let mut builder = Builder::new(encoder);
        let contents = b"hello xz world";

        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_path("bundle/hello-xz.txt").expect("set path");
        header.set_cksum();
        builder
            .append_data(&mut header, "bundle/hello-xz.txt", &contents[..])
            .expect("append tar entry");
        builder.finish().expect("finish tar");
    }

    #[tokio::test]
    async fn installs_tar_gz_fixture_and_updates_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let archive_path = temp.path().join("fixture.tar.gz");
        write_tar_gz_fixture(&archive_path);

        let plan = sample_plan(InstallType::Archive, "fixture.tar.gz");
        let outcome = install_from_plan(&plan, &manifest, Some(&archive_path), None)
            .await
            .expect("install should succeed");

        assert!(outcome.install_path.exists());
        assert!(outcome.install_path.join("bundle/hello.txt").exists());
        let stored = manifest.load().unwrap();
        assert_eq!(stored.apps.len(), 1);
        assert_eq!(stored.apps[0].id, "owner/project");
        assert_eq!(stored.apps[0].installed_version, "v1.2.3");

        let removed = uninstall_repo(&manifest, "owner/project")
            .expect("uninstall should succeed")
            .expect("installed app should be removed");
        assert_eq!(removed.id, "owner/project");
        assert!(!outcome.install_path.exists());
        assert!(manifest.load().unwrap().apps.is_empty());
    }

    #[tokio::test]
    async fn installs_tar_xz_fixture_and_updates_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let archive_path = temp.path().join("fixture.tar.xz");
        write_tar_xz_fixture(&archive_path);

        let plan = sample_plan(InstallType::Archive, "fixture.tar.xz");
        let outcome = install_from_plan(&plan, &manifest, Some(&archive_path), None)
            .await
            .expect("install should succeed");

        assert!(outcome.install_path.exists());
        assert!(outcome.install_path.join("bundle/hello-xz.txt").exists());
        let stored = manifest.load().unwrap();
        assert_eq!(stored.apps.len(), 1);
        assert_eq!(stored.apps[0].asset_name, "fixture.tar.xz");
    }

    #[test]
    fn linux_packages_require_confirmation_for_install_plan() {
        let repo = RepoRef::parse("owner/project").unwrap();
        let release = Release::fixture(
            "v2.0.0",
            vec![ReleaseAsset::fixture("project-linux-amd64.deb")],
        );
        let matched = AssetMatcher::new(OperatingSystem::Linux, Architecture::X64)
            .select_best(&release)
            .unwrap();

        let plan = InstallPlan::from_match(&repo, &release, &matched);

        assert!(plan.requires_user_confirmation);
        assert!(
            plan.notes
                .iter()
                .any(|note| note.contains("Linux .deb/.rpm packages"))
        );
    }

    #[test]
    fn linux_package_keeps_support_error_on_non_linux_or_unsupported_extension() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("package.bin");
        fs::write(&path, b"package").unwrap();

        let error = run_linux_package_installer(&path).unwrap_err();
        if cfg!(target_os = "linux") {
            assert!(error.to_string().contains("unsupported Linux package type"));
        } else {
            assert!(
                error
                    .to_string()
                    .contains("Linux packages can only be executed on Linux")
            );
        }
    }

    #[test]
    fn windows_package_keeps_support_error_on_non_windows() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("package.exe");
        fs::write(&path, b"package").unwrap();

        let error = run_windows_installer(&path).unwrap_err();
        if cfg!(target_os = "windows") {
            assert!(error.to_string().contains("failed to run installer"));
        } else {
            assert!(
                error
                    .to_string()
                    .contains("Windows installers can only be executed on Windows")
            );
        }
    }
}

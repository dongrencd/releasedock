use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use serde::Serialize;
use tar::Archive as TarArchive;
use xz2::read::XzDecoder;
use zip::ZipArchive;

use crate::{
    asset_matcher::InstallType,
    config::{Config, Language, effective_install_root},
    install_plan::InstallPlan,
    manifest::{
        InstallPathKind, InstalledApp, LifecycleAction, LifecycleEvent, ManifestStore,
        SystemPackageManager,
    },
    release::ReleaseClient,
    repo::RepoRef,
};

pub type ProgressReporter = Arc<dyn Fn(TaskProgress) + Send + Sync>;

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

fn record_lifecycle_success(
    manifest_store: &ManifestStore,
    repo: &RepoRef,
    action: LifecycleAction,
    summary: String,
    version: Option<String>,
    asset_name: Option<String>,
    install_path: Option<PathBuf>,
    install_path_kind: Option<InstallPathKind>,
) {
    let event = LifecycleEvent::succeeded(
        repo.id(),
        repo.name.clone(),
        action,
        summary,
        version,
        asset_name,
        install_path,
        install_path_kind,
    );
    let _ = manifest_store.append_lifecycle_event(event);
}

fn record_lifecycle_failure(
    manifest_store: &ManifestStore,
    repo: &RepoRef,
    action: LifecycleAction,
    summary: String,
    error: String,
    version: Option<String>,
    asset_name: Option<String>,
    install_path: Option<PathBuf>,
    install_path_kind: Option<InstallPathKind>,
) {
    let event = LifecycleEvent::failed(
        repo.id(),
        repo.name.clone(),
        action,
        summary,
        error,
        version,
        asset_name,
        install_path,
        install_path_kind,
    );
    let _ = manifest_store.append_lifecycle_event(event);
}

#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub app: InstalledApp,
    pub download_path: PathBuf,
    pub install_path: PathBuf,
    pub install_type: InstallType,
    pub install_path_kind: InstallPathKind,
    pub uninstall_supported: bool,
}

#[derive(Debug, Clone)]
struct SystemPackageMetadata {
    package_name: String,
    manager: SystemPackageManager,
}

#[derive(Debug, Clone, Copy)]
struct LinuxPackageCommandSpec {
    manager: SystemPackageManager,
    inspect_program: &'static str,
    install_program: &'static str,
    remove_program: &'static str,
}

impl LinuxPackageCommandSpec {
    fn inspect_args(&self, path: &Path) -> Vec<String> {
        let path = path.to_string_lossy().to_string();
        match self.manager {
            SystemPackageManager::Debian => vec!["-f".to_string(), path, "Package".to_string()],
            SystemPackageManager::Rpm => {
                vec![
                    "-qp".to_string(),
                    "--queryformat".to_string(),
                    "%{NAME}".to_string(),
                    path,
                ]
            }
            SystemPackageManager::Pacman => {
                vec!["-xOf".to_string(), path, ".PKGINFO".to_string()]
            }
        }
    }

    fn install_args(&self, path: &Path) -> Vec<String> {
        let path = path.to_string_lossy().to_string();
        match self.manager {
            SystemPackageManager::Debian => {
                vec![
                    "apt".to_string(),
                    "install".to_string(),
                    "-y".to_string(),
                    path,
                ]
            }
            SystemPackageManager::Rpm => {
                vec![
                    "dnf".to_string(),
                    "install".to_string(),
                    "-y".to_string(),
                    path,
                ]
            }
            SystemPackageManager::Pacman => {
                vec![
                    "pacman".to_string(),
                    "-U".to_string(),
                    "--noconfirm".to_string(),
                    path,
                ]
            }
        }
    }

    fn remove_args(&self, package_name: &str) -> Vec<String> {
        match self.manager {
            SystemPackageManager::Debian => vec![
                "apt".to_string(),
                "remove".to_string(),
                "-y".to_string(),
                package_name.to_string(),
            ],
            SystemPackageManager::Rpm => vec![
                "dnf".to_string(),
                "remove".to_string(),
                "-y".to_string(),
                package_name.to_string(),
            ],
            SystemPackageManager::Pacman => vec![
                "pacman".to_string(),
                "-R".to_string(),
                "--noconfirm".to_string(),
                package_name.to_string(),
            ],
        }
    }
}

fn linux_package_command_spec(manager: SystemPackageManager) -> LinuxPackageCommandSpec {
    match manager {
        SystemPackageManager::Debian => LinuxPackageCommandSpec {
            manager,
            inspect_program: "dpkg-deb",
            install_program: "apt",
            remove_program: "apt",
        },
        SystemPackageManager::Rpm => LinuxPackageCommandSpec {
            manager,
            inspect_program: "rpm",
            install_program: "dnf",
            remove_program: "dnf",
        },
        SystemPackageManager::Pacman => LinuxPackageCommandSpec {
            manager,
            inspect_program: "bsdtar",
            install_program: "pacman",
            remove_program: "pacman",
        },
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskAction {
    Install,
    Uninstall,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStage {
    Preparing,
    Downloading,
    CopyingAsset,
    ExtractingArchive,
    RunningSystemInstaller,
    UpdatingManifest,
    LocatingRecord,
    RemovingFiles,
    Finished,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    pub repo_id: String,
    pub action: TaskAction,
    pub stage: TaskStage,
    pub message: String,
    pub percent: Option<u8>,
}

pub async fn install_from_plan(
    plan: &InstallPlan,
    manifest_store: &ManifestStore,
    asset_fixture: Option<&Path>,
    runtime_config: Option<&Config>,
    language: Language,
    progress: Option<ProgressReporter>,
) -> Result<InstallOutcome> {
    let repo = RepoRef::parse(&plan.repo_url)?;
    let previous_app = manifest_store
        .load()
        .ok()
        .and_then(|manifest| manifest.apps.into_iter().find(|app| app.id == repo.id()));
    let lifecycle_summary = match previous_app.as_ref() {
        Some(app) if app.installed_version != plan.version => {
            format!(
                "{} {} {}",
                tr(language, "Updated", "已更新"),
                repo.name,
                plan.version
            )
        }
        _ => format!(
            "{} {} {}",
            tr(language, "Installed", "已安装"),
            repo.name,
            plan.version
        ),
    };
    let lifecycle_failure_summary = match previous_app.as_ref() {
        Some(app) if app.installed_version != plan.version => {
            format!(
                "{} {} {}",
                tr(language, "Failed to update", "更新失败"),
                repo.name,
                plan.version
            )
        }
        _ => format!(
            "{} {} {}",
            tr(language, "Failed to install", "安装失败"),
            repo.name,
            plan.version
        ),
    };
    report_progress(
        progress.as_ref(),
        TaskProgress {
            repo_id: repo.id(),
            action: TaskAction::Install,
            stage: TaskStage::Preparing,
            message: format!(
                "{} {}",
                tr(language, "Preparing to install", "正在准备安装"),
                repo.name
            ),
            percent: Some(0),
        },
    );
    let download_path = download_asset(
        plan,
        manifest_store,
        asset_fixture,
        runtime_config,
        language,
        progress.clone(),
        &repo,
    )
    .await
    .map_err(|error| {
        record_lifecycle_failure(
            manifest_store,
            &repo,
            match previous_app.as_ref() {
                Some(app) if app.installed_version != plan.version => LifecycleAction::Update,
                _ => LifecycleAction::Install,
            },
            lifecycle_failure_summary.clone(),
            error.to_string(),
            Some(plan.version.clone()),
            Some(plan.asset_name.clone()),
            None,
            None,
        );
        error
    })?;
    let (install_path, install_path_kind, uninstall_supported, system_package_metadata) =
        match plan.install_type {
            InstallType::AppImage | InstallType::Executable => {
                let install_path = install_managed_executable(
                    &download_path,
                    &repo,
                    manifest_store,
                    &plan.asset_name,
                    runtime_config,
                    language,
                    progress.as_ref(),
                )
                .map_err(|error| {
                    record_lifecycle_failure(
                        manifest_store,
                        &repo,
                        match previous_app.as_ref() {
                            Some(app) if app.installed_version != plan.version => {
                                LifecycleAction::Update
                            }
                            _ => LifecycleAction::Install,
                        },
                        lifecycle_failure_summary.clone(),
                        error.to_string(),
                        Some(plan.version.clone()),
                        Some(plan.asset_name.clone()),
                        None,
                        Some(InstallPathKind::ManagedPath),
                    );
                    error
                })?;
                (install_path, InstallPathKind::ManagedPath, true, None)
            }
            InstallType::PortableArchive | InstallType::Archive => {
                let install_path = extract_archive(
                    &download_path,
                    &repo,
                    manifest_store,
                    &plan.asset_name,
                    runtime_config,
                    language,
                    progress.as_ref(),
                )
                .map_err(|error| {
                    record_lifecycle_failure(
                        manifest_store,
                        &repo,
                        match previous_app.as_ref() {
                            Some(app) if app.installed_version != plan.version => {
                                LifecycleAction::Update
                            }
                            _ => LifecycleAction::Install,
                        },
                        lifecycle_failure_summary.clone(),
                        error.to_string(),
                        Some(plan.version.clone()),
                        Some(plan.asset_name.clone()),
                        None,
                        Some(InstallPathKind::ManagedPath),
                    );
                    error
                })?;
                (install_path, InstallPathKind::ManagedPath, true, None)
            }
            InstallType::WindowsInstaller => {
                let install_path = install_windows_installer(
                    &download_path,
                    &repo,
                    manifest_store,
                    &plan.asset_name,
                    runtime_config,
                    language,
                    progress.as_ref(),
                )
                .map_err(|error| {
                    record_lifecycle_failure(
                        manifest_store,
                        &repo,
                        match previous_app.as_ref() {
                            Some(app) if app.installed_version != plan.version => {
                                LifecycleAction::Update
                            }
                            _ => LifecycleAction::Install,
                        },
                        lifecycle_failure_summary.clone(),
                        error.to_string(),
                        Some(plan.version.clone()),
                        Some(plan.asset_name.clone()),
                        None,
                        Some(InstallPathKind::SystemInstaller),
                    );
                    error
                })?;
                (install_path, InstallPathKind::SystemInstaller, false, None)
            }
            InstallType::LinuxPackage => {
                let (install_path, system_package_metadata) = install_linux_package(
                    &download_path,
                    &repo,
                    manifest_store,
                    &plan.asset_name,
                    runtime_config,
                    language,
                    progress.as_ref(),
                )
                .map_err(|error| {
                    record_lifecycle_failure(
                        manifest_store,
                        &repo,
                        match previous_app.as_ref() {
                            Some(app) if app.installed_version != plan.version => {
                                LifecycleAction::Update
                            }
                            _ => LifecycleAction::Install,
                        },
                        lifecycle_failure_summary.clone(),
                        error.to_string(),
                        Some(plan.version.clone()),
                        Some(plan.asset_name.clone()),
                        None,
                        Some(InstallPathKind::SystemInstaller),
                    );
                    error
                })?;
                (
                    install_path,
                    InstallPathKind::SystemInstaller,
                    true,
                    Some(system_package_metadata),
                )
            }
            InstallType::Unknown => anyhow::bail!(
                "installing {:?} assets is not implemented yet; use the preview path instead",
                plan.install_type
            ),
        };
    let launch_path = infer_launch_target(
        &install_path,
        plan.install_type,
        &repo.name,
        &plan.asset_name,
    );
    let lifecycle_action = previous_app
        .as_ref()
        .map(|app| {
            if app.installed_version == plan.version {
                LifecycleAction::Install
            } else {
                LifecycleAction::Update
            }
        })
        .unwrap_or(LifecycleAction::Install);

    let mut app = InstalledApp::with_install_metadata(
        repo.id(),
        repo.name.clone(),
        plan.version.clone(),
        plan.asset_name.clone(),
        install_path.clone(),
        plan.install_type,
        install_path_kind,
        uninstall_supported,
    );
    app.launch_path = launch_path;
    if let Some(system_package_metadata) = system_package_metadata {
        app.system_package_name = Some(system_package_metadata.package_name);
        app.system_package_manager = Some(system_package_metadata.manager);
    }
    manifest_store.upsert_app(app.clone()).map_err(|error| {
        record_lifecycle_failure(
            manifest_store,
            &repo,
            lifecycle_action.clone(),
            lifecycle_failure_summary.clone(),
            error.to_string(),
            Some(plan.version.clone()),
            Some(plan.asset_name.clone()),
            Some(install_path.clone()),
            Some(install_path_kind),
        );
        error
    })?;
    record_lifecycle_success(
        manifest_store,
        &repo,
        lifecycle_action.clone(),
        lifecycle_summary,
        Some(plan.version.clone()),
        Some(plan.asset_name.clone()),
        Some(install_path.clone()),
        Some(install_path_kind),
    );
    if let Some(previous_app) = previous_app {
        if matches!(previous_app.install_path_kind, InstallPathKind::ManagedPath)
            && previous_app.install_path != app.install_path
        {
            let _ = remove_path(&previous_app.install_path);
        }
    }
    report_progress(
        progress.as_ref(),
        TaskProgress {
            repo_id: repo.id(),
            action: TaskAction::Install,
            stage: TaskStage::UpdatingManifest,
            message: format!(
                "{} {}",
                tr(
                    language,
                    "Updating install record for",
                    "正在更新安装记录："
                ),
                repo.name
            ),
            percent: Some(95),
        },
    );
    report_progress(
        progress.as_ref(),
        TaskProgress {
            repo_id: repo.id(),
            action: TaskAction::Install,
            stage: TaskStage::Finished,
            message: format!(
                "{} {}",
                tr(language, "Finished installing", "已完成安装"),
                repo.name
            ),
            percent: Some(100),
        },
    );
    let _ = cleanup_download_cache(&download_path);

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
    language: Language,
    progress: Option<ProgressReporter>,
) -> Result<Option<InstalledApp>> {
    let manifest = manifest_store.load()?;
    let Some(app) = manifest.apps.into_iter().find(|app| app.id == repo_id) else {
        return Ok(None);
    };
    let repo = RepoRef::parse(&app.repo_url)?;

    report_progress(
        progress.as_ref(),
        TaskProgress {
            repo_id: app.id.clone(),
            action: TaskAction::Uninstall,
            stage: TaskStage::LocatingRecord,
            message: format!(
                "{} {}",
                tr(
                    language,
                    "Locating install record for",
                    "正在定位安装记录："
                ),
                app.name
            ),
            percent: Some(10),
        },
    );

    if !app.uninstall_supported {
        record_lifecycle_failure(
            manifest_store,
            &repo,
            LifecycleAction::Uninstall,
            format!("{} {}", tr(language, "Failed to uninstall", "卸载失败："), app.name),
            tr(
                language,
                "was installed by a system installer and must be removed from the system package manager",
                "是由系统安装器安装的，必须通过系统卸载"
            )
            .to_string(),
            Some(app.installed_version.clone()),
            Some(app.asset_name.clone()),
            Some(app.install_path.clone()),
            Some(app.install_path_kind),
        );
        anyhow::bail!(
            "{} {}",
            app.id,
            tr(
                language,
                "was installed by a system installer and must be removed from the system package manager",
                "是由系统安装器安装的，必须通过系统卸载"
            )
        );
    }

    if matches!(app.install_type, InstallType::LinuxPackage) {
        let package_name = app
            .system_package_name
            .clone()
            .or_else(|| {
                resolve_linux_package_metadata(&app.install_path)
                    .ok()
                    .map(|metadata| metadata.package_name)
            })
            .ok_or_else(|| {
                anyhow::anyhow!("missing Linux system package metadata for {}", app.id)
            })?;
        let manager = app
            .system_package_manager
            .or_else(|| {
                resolve_linux_package_metadata(&app.install_path)
                    .ok()
                    .map(|metadata| metadata.manager)
            })
            .ok_or_else(|| {
                anyhow::anyhow!("missing Linux system package manager for {}", app.id)
            })?;
        let status = uninstall_linux_package(&package_name, manager).map_err(|error| {
            record_lifecycle_failure(
                manifest_store,
                &repo,
                LifecycleAction::Uninstall,
                format!(
                    "{} {}",
                    tr(language, "Failed to uninstall", "卸载失败："),
                    app.name
                ),
                error.to_string(),
                Some(app.installed_version.clone()),
                Some(app.asset_name.clone()),
                Some(app.install_path.clone()),
                Some(app.install_path_kind),
            );
            error
        })?;
        if !status.success() {
            let error = anyhow::anyhow!(
                "Linux package remover exited with status {} for {}",
                status,
                package_name
            );
            record_lifecycle_failure(
                manifest_store,
                &repo,
                LifecycleAction::Uninstall,
                format!(
                    "{} {}",
                    tr(language, "Failed to uninstall", "卸载失败："),
                    app.name
                ),
                error.to_string(),
                Some(app.installed_version.clone()),
                Some(app.asset_name.clone()),
                Some(app.install_path.clone()),
                Some(app.install_path_kind),
            );
            return Err(error);
        }
    }

    let Some(app) = manifest_store.remove_app(repo_id)? else {
        return Ok(None);
    };

    report_progress(
        progress.as_ref(),
        TaskProgress {
            repo_id: app.id.clone(),
            action: TaskAction::Uninstall,
            stage: TaskStage::RemovingFiles,
            message: format!(
                "{} {}",
                tr(language, "Removing install files for", "正在删除安装文件："),
                app.name
            ),
            percent: Some(70),
        },
    );
    if let Err(error) = remove_path(&app.install_path) {
        let repo = RepoRef::parse(&app.repo_url)?;
        record_lifecycle_failure(
            manifest_store,
            &repo,
            LifecycleAction::Uninstall,
            format!(
                "{} {}",
                tr(language, "Failed to uninstall", "卸载失败："),
                app.name
            ),
            error.to_string(),
            Some(app.installed_version.clone()),
            Some(app.asset_name.clone()),
            Some(app.install_path.clone()),
            Some(app.install_path_kind),
        );
        return Err(error);
    }
    record_lifecycle_success(
        manifest_store,
        &repo,
        LifecycleAction::Uninstall,
        format!("{} {}", tr(language, "Uninstalled", "已卸载"), app.name),
        Some(app.installed_version.clone()),
        Some(app.asset_name.clone()),
        Some(app.install_path.clone()),
        Some(app.install_path_kind),
    );
    report_progress(
        progress.as_ref(),
        TaskProgress {
            repo_id: app.id.clone(),
            action: TaskAction::Uninstall,
            stage: TaskStage::Finished,
            message: format!(
                "{} {}",
                tr(language, "Finished uninstalling", "已完成卸载"),
                app.name
            ),
            percent: Some(100),
        },
    );
    Ok(Some(app))
}

async fn download_asset(
    plan: &InstallPlan,
    manifest_store: &ManifestStore,
    asset_fixture: Option<&Path>,
    runtime_config: Option<&Config>,
    language: Language,
    progress: Option<ProgressReporter>,
    repo: &RepoRef,
) -> Result<PathBuf> {
    let download_dir = cache_dir(manifest_store, runtime_config).join(repo.id().replace('/', "_"));
    fs::create_dir_all(&download_dir)
        .with_context(|| format!("failed to create download cache {}", download_dir.display()))?;

    let download_path = download_dir.join(&plan.asset_name);
    if let Some(asset_fixture) = asset_fixture {
        report_progress(
            progress.as_ref(),
            TaskProgress {
                repo_id: repo.id(),
                action: TaskAction::Install,
                stage: TaskStage::CopyingAsset,
                message: format!(
                    "{} {}",
                    tr(language, "Copying local fixture", "正在复制本地 fixture"),
                    plan.asset_name
                ),
                percent: Some(30),
            },
        );
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
    report_progress(
        progress.as_ref(),
        TaskProgress {
            repo_id: repo.id(),
            action: TaskAction::Install,
            stage: TaskStage::Downloading,
            message: format!(
                "{} {}",
                tr(language, "Downloading", "正在下载"),
                plan.asset_name
            ),
            percent: Some(0),
        },
    );
    let repo_id = repo.id();
    let asset_name = plan.asset_name.clone();
    let progress_for_download = progress.clone();
    client
        .download_to_path(
            &plan.download_url,
            &download_path,
            move |downloaded, total| {
                let percent = total.and_then(|total| {
                    if total == 0 {
                        return None;
                    }
                    Some(
                        ((downloaded as f64 / total as f64) * 100.0)
                            .round()
                            .clamp(0.0, 100.0) as u8,
                    )
                });
                report_progress(
                    progress_for_download.as_ref(),
                    TaskProgress {
                        repo_id: repo_id.clone(),
                        action: TaskAction::Install,
                        stage: TaskStage::Downloading,
                        message: format!(
                            "{} {}",
                            tr(language, "Downloading", "正在下载"),
                            asset_name
                        ),
                        percent,
                    },
                );
            },
        )
        .await?;
    Ok(download_path)
}

fn install_managed_executable(
    downloaded: &Path,
    repo: &RepoRef,
    manifest_store: &ManifestStore,
    asset_name: &str,
    runtime_config: Option<&Config>,
    language: Language,
    progress: Option<&ProgressReporter>,
) -> Result<PathBuf> {
    let install_dir = install_dir(manifest_store, repo, runtime_config);
    fs::create_dir_all(&install_dir).with_context(|| {
        format!(
            "failed to create install directory {}",
            install_dir.display()
        )
    })?;

    let install_path = install_dir.join(asset_name);
    let staged_path = staging_path(&install_path, "staging");
    if staged_path.exists() {
        let _ = remove_path(&staged_path);
    }
    report_progress(
        progress,
        TaskProgress {
            repo_id: repo.id(),
            action: TaskAction::Install,
            stage: TaskStage::CopyingAsset,
            message: format!("{} {}", tr(language, "Copying", "正在复制"), asset_name),
            percent: Some(75),
        },
    );
    fs::copy(downloaded, &staged_path).with_context(|| {
        format!(
            "failed to copy executable asset from {} to {}",
            downloaded.display(),
            staged_path.display()
        )
    })?;
    mark_executable(&staged_path)?;
    replace_managed_file(&staged_path, &install_path)?;
    Ok(install_path)
}

fn extract_archive(
    downloaded: &Path,
    repo: &RepoRef,
    manifest_store: &ManifestStore,
    asset_name: &str,
    runtime_config: Option<&Config>,
    language: Language,
    progress: Option<&ProgressReporter>,
) -> Result<PathBuf> {
    let install_dir = install_dir(manifest_store, repo, runtime_config);
    let parent_dir = install_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("install path {} has no parent", install_dir.display()))?;
    fs::create_dir_all(parent_dir)
        .with_context(|| format!("failed to create install root {}", parent_dir.display()))?;
    let staged_dir = staging_path(&install_dir, "staging");
    if staged_dir.exists() {
        let _ = remove_path(&staged_dir);
    }
    fs::create_dir_all(&staged_dir).with_context(|| {
        format!(
            "failed to create staging directory {}",
            staged_dir.display()
        )
    })?;

    report_progress(
        progress,
        TaskProgress {
            repo_id: repo.id(),
            action: TaskAction::Install,
            stage: TaskStage::ExtractingArchive,
            message: format!("{} {}", tr(language, "Extracting", "正在解压"), asset_name),
            percent: Some(75),
        },
    );
    let extract_result = if asset_name.ends_with(".zip") {
        extract_zip(downloaded, &staged_dir)
    } else if asset_name.ends_with(".tar.gz") || asset_name.ends_with(".tgz") {
        extract_tar_archive(GzDecoder::new(open_archive(downloaded)?), &staged_dir)
    } else if asset_name.ends_with(".tar.xz") {
        extract_tar_archive(XzDecoder::new(open_archive(downloaded)?), &staged_dir)
    } else {
        anyhow::bail!("archive format for {} is not supported yet", asset_name);
    };

    if let Err(error) = extract_result {
        let _ = remove_path(&staged_dir);
        return Err(error);
    }

    replace_managed_directory(&staged_dir, &install_dir)?;

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
    language: Language,
    progress: Option<&ProgressReporter>,
) -> Result<PathBuf> {
    let install_dir = install_dir(manifest_store, repo, runtime_config);
    fs::create_dir_all(&install_dir).with_context(|| {
        format!(
            "failed to create install directory {}",
            install_dir.display()
        )
    })?;

    let target_path = install_dir.join(asset_name);
    report_progress(
        progress,
        TaskProgress {
            repo_id: repo.id(),
            action: TaskAction::Install,
            stage: TaskStage::RunningSystemInstaller,
            message: format!(
                "{} {}",
                tr(language, "Running system installer", "正在执行系统安装器"),
                asset_name
            ),
            percent: Some(85),
        },
    );
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
    language: Language,
    progress: Option<&ProgressReporter>,
) -> Result<(PathBuf, SystemPackageMetadata)> {
    let install_dir = install_dir(manifest_store, repo, runtime_config);
    fs::create_dir_all(&install_dir).with_context(|| {
        format!(
            "failed to create install directory {}",
            install_dir.display()
        )
    })?;

    let target_path = install_dir.join(asset_name);
    report_progress(
        progress,
        TaskProgress {
            repo_id: repo.id(),
            action: TaskAction::Install,
            stage: TaskStage::RunningSystemInstaller,
            message: format!(
                "{} {}",
                tr(language, "Running system installer", "正在执行系统安装器"),
                asset_name
            ),
            percent: Some(85),
        },
    );
    fs::copy(downloaded, &target_path).with_context(|| {
        format!(
            "failed to copy Linux package from {} to {}",
            downloaded.display(),
            target_path.display()
        )
    })?;

    let metadata = resolve_linux_package_metadata(&target_path)?;
    let status = run_linux_package_installer(&target_path, metadata.manager)?;
    if !status.success() {
        anyhow::bail!(
            "Linux package installer exited with status {} for {}",
            status,
            target_path.display()
        );
    }

    Ok((target_path, metadata))
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

fn resolve_linux_package_metadata(path: &Path) -> Result<SystemPackageMetadata> {
    let manager = if is_pacman_package(path) {
        SystemPackageManager::Pacman
    } else {
        match path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "deb" => SystemPackageManager::Debian,
            "rpm" => SystemPackageManager::Rpm,
            _ => anyhow::bail!("unsupported Linux package type for {}", path.display()),
        }
    };

    let package_name = query_linux_package_name(path, manager)?;
    Ok(SystemPackageMetadata {
        package_name,
        manager,
    })
}

fn is_pacman_package(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|value| {
            value.ends_with(".pkg.tar.zst")
                || value.ends_with(".pkg.tar.xz")
                || value.ends_with(".pkg.tar.gz")
        })
        .unwrap_or(false)
}

fn query_linux_package_name(path: &Path, manager: SystemPackageManager) -> Result<String> {
    #[cfg(target_os = "linux")]
    {
        let spec = linux_package_command_spec(manager);
        let output = Command::new(spec.inspect_program)
            .args(spec.inspect_args(path))
            .output()
            .with_context(|| {
                format!("failed to inspect {:?} package {}", manager, path.display())
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "failed to read Linux package metadata from {}: {}",
                path.display(),
                stderr.trim()
            );
        }

        let package_name = match manager {
            SystemPackageManager::Pacman => String::from_utf8_lossy(&output.stdout)
                .lines()
                .find_map(|line| line.strip_prefix("pkgname = ").map(str::trim))
                .unwrap_or_default()
                .to_string(),
            _ => String::from_utf8_lossy(&output.stdout).trim().to_string(),
        };
        if package_name.is_empty() {
            anyhow::bail!(
                "Linux package metadata did not include a package name for {}",
                path.display()
            );
        }

        Ok(package_name)
    }

    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!(
            "Linux package metadata can only be inspected on Linux; downloaded file kept at {}",
            path.display()
        );
    }
}

fn run_linux_package_installer(
    path: &Path,
    manager: SystemPackageManager,
) -> Result<std::process::ExitStatus> {
    #[cfg(target_os = "linux")]
    {
        let spec = linux_package_command_spec(manager);
        return Command::new("pkexec")
            .args(spec.install_args(path))
            .status()
            .with_context(|| {
                format!(
                    "failed to run {} for {}",
                    spec.install_program,
                    path.display()
                )
            });
    }

    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!(
            "Linux packages can only be executed on Linux; downloaded file kept at {}",
            path.display()
        );
    }
}

fn uninstall_linux_package(
    package_name: &str,
    manager: SystemPackageManager,
) -> Result<std::process::ExitStatus> {
    #[cfg(target_os = "linux")]
    {
        let spec = linux_package_command_spec(manager);
        return Command::new("pkexec")
            .args(spec.remove_args(package_name))
            .status()
            .with_context(|| {
                format!(
                    "failed to run {} remove for {package_name}",
                    spec.remove_program
                )
            });
    }

    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!(
            "Linux packages can only be removed on Linux; package {} was kept in the manifest",
            package_name
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

fn replace_managed_file(staged_file: &Path, install_path: &Path) -> Result<()> {
    let backup_path = staging_path(install_path, "backup");
    let had_previous = install_path.exists();

    if had_previous {
        fs::rename(install_path, &backup_path).with_context(|| {
            format!(
                "failed to move existing install file {} aside",
                install_path.display()
            )
        })?;
    }

    let staged_result = fs::rename(staged_file, install_path).with_context(|| {
        format!(
            "failed to move staged install {} into {}",
            staged_file.display(),
            install_path.display()
        )
    });

    if let Err(error) = staged_result {
        if had_previous {
            let _ = fs::rename(&backup_path, install_path);
        }
        return Err(error);
    }

    if had_previous {
        let _ = remove_path(&backup_path);
    }

    Ok(())
}

fn replace_managed_directory(staged_dir: &Path, install_dir: &Path) -> Result<()> {
    let backup_dir = staging_path(install_dir, "backup");
    let had_previous = install_dir.exists();

    if had_previous {
        fs::rename(install_dir, &backup_dir).with_context(|| {
            format!(
                "failed to move existing install directory {} aside",
                install_dir.display()
            )
        })?;
    }

    let staged_result = fs::rename(staged_dir, install_dir).with_context(|| {
        format!(
            "failed to move staged install {} into {}",
            staged_dir.display(),
            install_dir.display()
        )
    });

    if let Err(error) = staged_result {
        if had_previous {
            let _ = fs::rename(&backup_dir, install_dir);
        }
        return Err(error);
    }

    if had_previous {
        let _ = remove_path(&backup_dir);
    }

    Ok(())
}

fn staging_path(base: &Path, label: &str) -> PathBuf {
    let parent = base.parent().unwrap_or_else(|| Path::new("."));
    let file_name = base
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("staging");
    let token = unique_staging_token();
    parent.join(format!(".{file_name}.{label}.{token}"))
}

fn unique_staging_token() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{}", std::process::id(), now, counter)
}

fn cleanup_download_cache(download_path: &Path) -> Result<()> {
    if download_path.exists() {
        fs::remove_file(download_path).with_context(|| {
            format!(
                "failed to remove download cache {}",
                download_path.display()
            )
        })?;
    }

    if let Some(repo_dir) = download_path.parent() {
        prune_empty_dir(repo_dir)?;
    }

    Ok(())
}

fn prune_empty_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    if fs::read_dir(dir)
        .with_context(|| format!("failed to inspect cache directory {}", dir.display()))?
        .next()
        .is_some()
    {
        return Ok(());
    }

    fs::remove_dir(dir)
        .with_context(|| format!("failed to remove empty cache directory {}", dir.display()))?;
    if let Some(downloads_dir) = dir.parent() {
        if downloads_dir.file_name().and_then(|value| value.to_str()) == Some("downloads")
            && fs::read_dir(downloads_dir)
                .with_context(|| {
                    format!(
                        "failed to inspect cache directory {}",
                        downloads_dir.display()
                    )
                })?
                .next()
                .is_none()
        {
            let _ = fs::remove_dir(downloads_dir);
        }
    }

    Ok(())
}

pub fn infer_launch_target(
    install_path: &Path,
    install_type: InstallType,
    app_name: &str,
    asset_name: &str,
) -> Option<PathBuf> {
    match install_type {
        InstallType::AppImage => return Some(install_path.to_path_buf()),
        InstallType::Executable => return None,
        InstallType::WindowsInstaller | InstallType::LinuxPackage => return None,
        _ => {}
    }

    if install_path.is_file() && is_launchable_candidate(install_path) {
        return Some(install_path.to_path_buf());
    }

    if !install_path.is_dir() {
        return None;
    }

    let mut candidates = Vec::new();
    collect_launch_candidates(install_path, app_name, asset_name, 3, 0, &mut candidates);
    candidates
        .into_iter()
        .max_by_key(|(score, _, _)| *score)
        .map(|(_, path, _)| path)
}

fn collect_launch_candidates(
    root: &Path,
    app_name: &str,
    asset_name: &str,
    remaining_depth: usize,
    depth: usize,
    candidates: &mut Vec<(i32, PathBuf, usize)>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            if remaining_depth > 0 {
                collect_launch_candidates(
                    &path,
                    app_name,
                    asset_name,
                    remaining_depth - 1,
                    depth + 1,
                    candidates,
                );
            }

            continue;
        }

        if !is_launchable_candidate(&path) {
            continue;
        }

        let score = score_launch_candidate(&path, app_name, asset_name, depth);
        if score > 0 {
            candidates.push((score, path, depth));
        }
    }
}

fn score_launch_candidate(path: &Path, app_name: &str, asset_name: &str, depth: usize) -> i32 {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let normalized_file_name = normalize_launch_name(file_name);
    let normalized_stem = normalize_launch_name(stem);
    let normalized_app_name = normalize_launch_name(app_name);
    let normalized_asset_stem = normalize_launch_name(
        Path::new(asset_name)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default(),
    );

    let mut score = 0;
    if !normalized_app_name.is_empty() {
        if normalized_file_name == normalized_app_name {
            score += 200;
        } else if normalized_stem == normalized_app_name {
            score += 180;
        } else if normalized_file_name.starts_with(&normalized_app_name) {
            score += 120;
        } else if normalized_stem.starts_with(&normalized_app_name) {
            score += 100;
        } else if normalized_file_name.contains(&normalized_app_name)
            || normalized_stem.contains(&normalized_app_name)
        {
            score += 60;
        }
    }

    if !normalized_asset_stem.is_empty() {
        if normalized_file_name == normalized_asset_stem || normalized_stem == normalized_asset_stem
        {
            score += 40;
        } else if normalized_file_name.contains(&normalized_asset_stem)
            || normalized_stem.contains(&normalized_asset_stem)
        {
            score += 20;
        }
    }

    score + (10_i32 - depth as i32).max(0)
}

fn normalize_launch_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(|character| character.to_lowercase())
        .collect()
}

fn is_launchable_candidate(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if file_name.ends_with(".app") {
        return true;
    }

    #[cfg(target_os = "windows")]
    {
        matches!(
            path.extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase())
                .as_deref(),
            Some("exe") | Some("com") | Some("bat") | Some("cmd")
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;

        match fs::metadata(path) {
            Ok(metadata) => metadata.permissions().mode() & 0o111 != 0,
            Err(_) => false,
        }
    }
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
    effective_install_root(
        runtime_config,
        manifest_store.path().parent().map(Path::to_path_buf),
    )
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

fn report_progress(progress: Option<&ProgressReporter>, event: TaskProgress) {
    if let Some(progress) = progress {
        progress(event);
    }
}

fn tr(language: Language, english: &'static str, chinese: &'static str) -> &'static str {
    match language {
        Language::En => english,
        Language::ZhCn => chinese,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        asset_matcher::{Architecture, AssetMatcher, OperatingSystem},
        install_plan::InstallManagementKind,
        release::{Release, ReleaseAsset},
    };
    use flate2::{Compression, write::GzEncoder};
    use std::{
        env,
        sync::{Mutex, OnceLock},
    };
    use tar::Builder;
    use xz2::write::XzEncoder;

    static PATH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn sample_plan(install_type: InstallType, asset_name: &str) -> InstallPlan {
        let management_kind = match install_type {
            InstallType::AppImage | InstallType::PortableArchive | InstallType::Archive => {
                InstallManagementKind::ManagedLocal
            }
            InstallType::Executable => InstallManagementKind::ManagedLocal,
            InstallType::LinuxPackage => InstallManagementKind::SystemPackage,
            InstallType::WindowsInstaller | InstallType::Unknown => {
                InstallManagementKind::ExternalInstaller
            }
        };
        InstallPlan {
            repo_id: "owner/project".to_string(),
            repo_url: "https://github.com/owner/project".to_string(),
            version: "v1.2.3".to_string(),
            asset_name: asset_name.to_string(),
            download_url: format!(
                "https://github.com/owner/project/releases/download/v1.2.3/{asset_name}"
            ),
            install_type,
            management_kind,
            system_package_manager: if asset_name.ends_with(".deb") {
                Some(SystemPackageManager::Debian)
            } else if asset_name.ends_with(".rpm") {
                Some(SystemPackageManager::Rpm)
            } else if asset_name.ends_with(".pkg.tar.zst")
                || asset_name.ends_with(".pkg.tar.xz")
                || asset_name.ends_with(".pkg.tar.gz")
            {
                Some(SystemPackageManager::Pacman)
            } else {
                None
            },
            requires_user_confirmation: false,
            notes: Vec::new(),
        }
    }

    fn write_tar_gz_fixture(path: &Path) {
        write_tar_gz_fixture_with_entry(path, "bundle/hello.txt", b"hello world");
    }

    fn write_tar_gz_fixture_with_entry(path: &Path, entry_path: &str, contents: &[u8]) {
        let file = fs::File::create(path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_path(entry_path).expect("set path");
        header.set_cksum();
        builder
            .append_data(&mut header, entry_path, contents)
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

    fn write_script(dir: &Path, name: &str, body: &str) {
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).unwrap();
        }
    }

    struct PathEnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        old_path: Option<std::ffi::OsString>,
    }

    impl Drop for PathEnvGuard {
        fn drop(&mut self) {
            match self.old_path.take() {
                Some(value) => unsafe {
                    env::set_var("PATH", value);
                },
                None => unsafe {
                    env::remove_var("PATH");
                },
            }
        }
    }

    fn push_temp_path(script_dir: &Path) -> PathEnvGuard {
        let lock = PATH_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let old_path = env::var_os("PATH");
        let new_path = match &old_path {
            Some(existing) => {
                let mut value = script_dir.as_os_str().to_os_string();
                value.push(":");
                value.push(existing);
                value
            }
            None => script_dir.as_os_str().to_os_string(),
        };
        unsafe {
            env::set_var("PATH", &new_path);
        }
        PathEnvGuard {
            _lock: lock,
            old_path,
        }
    }

    #[tokio::test]
    async fn installs_tar_gz_fixture_and_updates_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let archive_path = temp.path().join("fixture.tar.gz");
        write_tar_gz_fixture(&archive_path);

        let plan = sample_plan(InstallType::Archive, "fixture.tar.gz");
        let outcome = install_from_plan(
            &plan,
            &manifest,
            Some(&archive_path),
            None,
            Language::En,
            None,
        )
        .await
        .expect("install should succeed");

        assert!(outcome.install_path.exists());
        assert!(outcome.install_path.join("bundle/hello.txt").exists());
        assert!(!outcome.download_path.exists());
        let stored = manifest.load().unwrap();
        assert_eq!(stored.apps.len(), 1);
        assert_eq!(stored.apps[0].id, "owner/project");
        assert_eq!(stored.apps[0].installed_version, "v1.2.3");

        let removed = uninstall_repo(&manifest, "owner/project", Language::En, None)
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
        let outcome = install_from_plan(
            &plan,
            &manifest,
            Some(&archive_path),
            None,
            Language::En,
            None,
        )
        .await
        .expect("install should succeed");

        assert!(outcome.install_path.exists());
        assert!(outcome.install_path.join("bundle/hello-xz.txt").exists());
        assert!(!outcome.download_path.exists());
        let stored = manifest.load().unwrap();
        assert_eq!(stored.apps.len(), 1);
        assert_eq!(stored.apps[0].asset_name, "fixture.tar.xz");
    }

    #[tokio::test]
    async fn appimage_updates_remove_previous_asset_file() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"fake appimage payload").unwrap();

        let first_plan = sample_plan(InstallType::AppImage, "demo-v1.AppImage");
        install_from_plan(
            &first_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .expect("first install should succeed");

        let first_install_path = temp
            .path()
            .join("apps")
            .join("owner-project")
            .join("demo-v1.AppImage");
        assert!(first_install_path.exists());

        let second_plan = sample_plan(InstallType::AppImage, "demo-v2.AppImage");
        let outcome = install_from_plan(
            &second_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .expect("second install should succeed");

        let second_install_path = temp
            .path()
            .join("apps")
            .join("owner-project")
            .join("demo-v2.AppImage");
        assert!(outcome.install_path.exists());
        assert!(second_install_path.exists());
        assert!(!first_install_path.exists());
        let stored = manifest.load().unwrap();
        assert_eq!(stored.apps.len(), 1);
        assert_eq!(stored.apps[0].asset_name, "demo-v2.AppImage");
    }

    #[tokio::test]
    async fn executable_install_does_not_track_launch_path_but_keeps_executable_bit() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("releasedock-linux-x64");
        fs::write(&fixture, b"fake linux executable payload").unwrap();

        let plan = sample_plan(InstallType::Executable, "releasedock-linux-x64");
        let outcome = install_from_plan(&plan, &manifest, Some(&fixture), None, Language::En, None)
            .await
            .expect("install should succeed");

        assert_eq!(outcome.install_type, InstallType::Executable);
        assert_eq!(outcome.install_path, outcome.app.install_path);
        assert!(outcome.app.launch_path.is_none());
        assert!(outcome.install_path.exists());
        assert_eq!(
            fs::read(&outcome.install_path).unwrap(),
            b"fake linux executable payload"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = fs::metadata(&outcome.install_path)
                .unwrap()
                .permissions()
                .mode();
            assert!(mode & 0o111 != 0);
        }

        let stored = manifest.load().unwrap();
        assert_eq!(stored.apps.len(), 1);
        assert_eq!(stored.apps[0].install_type, InstallType::Executable);
        assert!(stored.apps[0].launch_path.is_none());
    }

    #[test]
    fn managed_file_replacement_restores_previous_file_when_staged_move_fails() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("demo.AppImage");
        let staged = temp.path().join("missing-staged.AppImage");
        fs::write(&target, b"old appimage").unwrap();

        let error = replace_managed_file(&staged, &target).unwrap_err();

        assert!(error.to_string().contains("failed to move staged install"));
        assert_eq!(fs::read(&target).unwrap(), b"old appimage");
    }

    #[tokio::test]
    async fn keeps_download_cache_when_install_fails() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let bad_fixture = temp.path().join("fixture.zip");
        fs::write(&bad_fixture, b"not a real archive").unwrap();

        let plan = sample_plan(InstallType::Archive, "fixture.zip");
        let result = install_from_plan(
            &plan,
            &manifest,
            Some(&bad_fixture),
            None,
            Language::En,
            None,
        )
        .await;

        assert!(result.is_err());
        let cache_path = temp
            .path()
            .join("downloads")
            .join("owner_project")
            .join("fixture.zip");
        assert!(cache_path.exists());
        assert!(manifest.load().unwrap().apps.is_empty());
    }

    #[tokio::test]
    async fn archive_updates_replace_previous_directory_contents() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let first_archive = temp.path().join("first.tar.gz");
        let second_archive = temp.path().join("second.tar.gz");
        write_tar_gz_fixture_with_entry(&first_archive, "bundle/first.txt", b"first");
        write_tar_gz_fixture_with_entry(&second_archive, "bundle/second.txt", b"second");

        let first_plan = sample_plan(InstallType::Archive, "fixture-v1.tar.gz");
        install_from_plan(
            &first_plan,
            &manifest,
            Some(&first_archive),
            None,
            Language::En,
            None,
        )
        .await
        .expect("first archive install should succeed");

        let install_dir = temp.path().join("apps").join("owner-project");
        assert!(install_dir.join("bundle/first.txt").exists());

        let second_plan = sample_plan(InstallType::Archive, "fixture-v2.tar.gz");
        let outcome = install_from_plan(
            &second_plan,
            &manifest,
            Some(&second_archive),
            None,
            Language::En,
            None,
        )
        .await
        .expect("second archive install should succeed");

        assert!(outcome.install_path.join("bundle/second.txt").exists());
        assert!(!outcome.install_path.join("bundle/first.txt").exists());
        let stored = manifest.load().unwrap();
        assert_eq!(stored.apps.len(), 1);
        assert_eq!(stored.apps[0].asset_name, "fixture-v2.tar.gz");
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

        let plan = InstallPlan::from_match(&repo, &release, &matched, Language::En);

        assert!(plan.requires_user_confirmation);
        assert!(
            plan.notes
                .iter()
                .any(|note| note.contains("Linux system packages"))
        );
    }

    #[test]
    fn linux_package_keeps_support_error_on_non_linux_or_unsupported_extension() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("package.bin");
        fs::write(&path, b"package").unwrap();

        let error = resolve_linux_package_metadata(&path).unwrap_err();
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
    fn linux_package_command_spec_generates_manager_specific_commands() {
        let package_path = Path::new("/tmp/demo.pkg.tar.zst");
        let pacman = linux_package_command_spec(SystemPackageManager::Pacman);
        assert_eq!(pacman.inspect_program, "bsdtar");
        assert_eq!(
            pacman.inspect_args(package_path),
            vec!["-xOf", "/tmp/demo.pkg.tar.zst", ".PKGINFO"]
        );
        assert_eq!(
            pacman.install_args(package_path),
            vec!["pacman", "-U", "--noconfirm", "/tmp/demo.pkg.tar.zst"]
        );
        assert_eq!(
            pacman.remove_args("demo-package"),
            vec!["pacman", "-R", "--noconfirm", "demo-package"]
        );

        let debian = linux_package_command_spec(SystemPackageManager::Debian);
        assert_eq!(debian.inspect_program, "dpkg-deb");
        assert_eq!(debian.install_args(Path::new("/tmp/demo.deb"))[0], "apt");
        assert_eq!(debian.remove_args("demo-package")[0], "apt");
    }

    #[tokio::test]
    async fn linux_package_install_and_uninstall_track_package_metadata() {
        if !cfg!(target_os = "linux") {
            return;
        }

        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let artifact = temp.path().join("demo-linux-amd64.deb");
        fs::write(&artifact, b"fake deb payload").unwrap();
        let scripts = temp.path().join("bin");
        fs::create_dir_all(&scripts).unwrap();

        write_script(&scripts, "dpkg-deb", "printf '%s\\n' demo-package");
        write_script(
            &scripts,
            "pkexec",
            "cmd=\"$1\"\nshift\nexec \"$cmd\" \"$@\"",
        );
        write_script(
            &scripts,
            "apt",
            "printf '%s\\n' \"$*\" >> \"$APT_LOG\"\nexit 0",
        );

        let apt_log = temp.path().join("apt.log");
        unsafe {
            env::set_var("APT_LOG", &apt_log);
        }
        let _path_guard = push_temp_path(&scripts);

        let plan = sample_plan(InstallType::LinuxPackage, "demo-linux-amd64.deb");
        let outcome =
            install_from_plan(&plan, &manifest, Some(&artifact), None, Language::En, None)
                .await
                .expect("install should succeed");

        assert_eq!(
            outcome.app.system_package_name.as_deref(),
            Some("demo-package")
        );
        assert_eq!(
            outcome.app.system_package_manager,
            Some(SystemPackageManager::Debian)
        );
        assert!(outcome.app.uninstall_supported);

        let stored = manifest.load().unwrap();
        assert_eq!(
            stored.apps[0].system_package_name.as_deref(),
            Some("demo-package")
        );
        assert_eq!(
            stored.apps[0].system_package_manager,
            Some(SystemPackageManager::Debian)
        );

        let removed = uninstall_repo(&manifest, "owner/project", Language::En, None)
            .expect("uninstall should succeed")
            .expect("installed app should be removed");
        assert_eq!(removed.id, "owner/project");
        assert!(!outcome.install_path.exists());
        assert!(manifest.load().unwrap().apps.is_empty());

        let apt_log = fs::read_to_string(&apt_log).unwrap();
        assert!(apt_log.contains("install -y"));
        assert!(apt_log.contains("remove -y demo-package"));
        unsafe {
            env::remove_var("APT_LOG");
        }
    }

    #[tokio::test]
    async fn pacman_package_install_and_uninstall_track_package_metadata() {
        if !cfg!(target_os = "linux") {
            return;
        }

        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let artifact = temp.path().join("demo-linux-amd64.pkg.tar.zst");
        fs::write(&artifact, b"fake pacman payload").unwrap();
        let scripts = temp.path().join("bin-pacman");
        fs::create_dir_all(&scripts).unwrap();

        write_script(&scripts, "bsdtar", "printf 'pkgname = pacman-package\\n'");
        write_script(
            &scripts,
            "pkexec",
            "printf '%s\\n' \"$*\" >> \"$PKG_LOG\"\nexit 0",
        );

        let pkg_log = temp.path().join("pkg.log");
        unsafe {
            env::set_var("PKG_LOG", &pkg_log);
        }
        let _path_guard = push_temp_path(&scripts);

        let plan = sample_plan(InstallType::LinuxPackage, "demo-linux-amd64.pkg.tar.zst");
        let outcome =
            install_from_plan(&plan, &manifest, Some(&artifact), None, Language::En, None)
                .await
                .expect("install should succeed");

        assert_eq!(
            outcome.app.system_package_name.as_deref(),
            Some("pacman-package")
        );
        assert_eq!(
            outcome.app.system_package_manager,
            Some(SystemPackageManager::Pacman)
        );
        assert!(outcome.app.uninstall_supported);

        let stored = manifest.load().unwrap();
        assert_eq!(
            stored.apps[0].system_package_name.as_deref(),
            Some("pacman-package")
        );
        assert_eq!(
            stored.apps[0].system_package_manager,
            Some(SystemPackageManager::Pacman)
        );

        let removed = uninstall_repo(&manifest, "owner/project", Language::En, None)
            .expect("uninstall should succeed")
            .expect("installed app should be removed");
        assert_eq!(removed.id, "owner/project");
        assert!(!outcome.install_path.exists());
        assert!(manifest.load().unwrap().apps.is_empty());

        let pkg_log = fs::read_to_string(&pkg_log).unwrap();
        assert!(pkg_log.contains("pacman -U --noconfirm"));
        assert!(pkg_log.contains("pacman -R --noconfirm pacman-package"));
        unsafe {
            env::remove_var("PKG_LOG");
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

    #[test]
    fn infer_launch_target_prefers_matching_executable_in_archive_tree() {
        let temp = tempfile::tempdir().unwrap();
        let install_dir = temp.path().join("project");
        let nested_dir = install_dir.join("bundle/bin");
        fs::create_dir_all(&nested_dir).unwrap();

        let launcher = nested_dir.join(if cfg!(target_os = "windows") {
            "project.exe"
        } else {
            "project"
        });
        fs::write(&launcher, b"launcher").unwrap();

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&launcher).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&launcher, permissions).unwrap();
        }

        let inferred = infer_launch_target(
            &install_dir,
            InstallType::Archive,
            "project",
            "project-linux-x86_64.tar.gz",
        );

        assert_eq!(inferred.as_deref(), Some(launcher.as_path()));
    }

    #[test]
    fn infer_launch_target_returns_appimage_path() {
        let temp = tempfile::tempdir().unwrap();
        let install_path = temp.path().join("project.AppImage");
        fs::write(&install_path, b"appimage").unwrap();

        let inferred = infer_launch_target(
            &install_path,
            InstallType::AppImage,
            "project",
            "project-linux-x86_64.AppImage",
        );

        assert_eq!(inferred.as_deref(), Some(install_path.as_path()));
    }
}

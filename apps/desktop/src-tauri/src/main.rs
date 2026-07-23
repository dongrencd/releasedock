use std::{
    collections::HashSet,
    env,
    fs,
    process::Command,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use ghrm_core::{
    asset_matcher::{Architecture, AssetMatcher, OperatingSystem},
    config::{Config, ConfigStore},
    installer::install_from_plan,
    install_plan::InstallPlan,
    manifest::{InstallPathKind, ManifestStore},
    release::{Release, ReleaseClient},
    repo::RepoRef,
};
use serde::{Deserialize, Serialize};

mod tracking;

use tracking::TrackedRepoStore;

const DEFAULT_TRACKED_REPO_ID: &str = "dongrencd/gh-release-manager";

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum AppStatus {
    UpdateAvailable,
    Current,
    NeedsChoice,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagedAppView {
    id: String,
    name: String,
    current_version: String,
    latest_version: String,
    status: AppStatus,
    source: String,
    release_title: Option<String>,
    release_note: Option<String>,
    release_url: Option<String>,
    published_at: Option<String>,
    asset_name: Option<String>,
    install_path: String,
    install_type: String,
    install_path_kind: InstallPathKind,
    uninstall_supported: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BulkRemoveResultView {
    apps: Vec<ManagedAppView>,
    removed_count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum UiOs {
    Windows,
    Linux,
    Macos,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum UiArch {
    X64,
    Arm64,
}

#[tokio::main]
async fn main() -> Result<()> {
    if should_run_cli() {
        ghrm_cli::run_from_args(env::args_os()).await?;
        return Ok(());
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_dashboard,
            load_config,
            save_config,
            add_repo,
            preview_install,
            install_repo,
            uninstall_repo,
            remove_tracked_repo,
            bulk_remove_tracked_repos,
            open_url
        ])
        .run(tauri::generate_context!())
        .expect("failed to run GitHub Release Manager desktop app");

    Ok(())
}

fn should_run_cli() -> bool {
    if cfg!(target_os = "windows") {
        return false;
    }

    let args: Vec<_> = env::args_os().collect();
    match args.get(1).and_then(|arg| arg.to_str()) {
        None => false,
        Some("--gui") => false,
        Some(_) => true,
    }
}

#[tauri::command]
async fn load_dashboard() -> Result<Vec<ManagedAppView>, String> {
    build_dashboard().await.map_err(format_error)
}

#[tauri::command]
async fn load_config() -> Result<DesktopConfig, String> {
    runtime_config().map(DesktopConfig::from).map_err(format_error)
}

#[tauri::command]
async fn save_config(config: DesktopConfig) -> Result<DesktopConfig, String> {
    let store = config_store().map_err(format_error)?;
    let runtime_config = Config::from(config.clone());
    store.save(&runtime_config).map_err(format_error)?;
    Ok(DesktopConfig::from(runtime_config))
}

#[tauri::command]
async fn add_repo(repo_input: String) -> Result<Vec<ManagedAppView>, String> {
    add_repo_to_tracking(&repo_input)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn preview_install(
    repo_input: String,
    release_fixture: Option<PathBuf>,
    os: Option<UiOs>,
    arch: Option<UiArch>,
) -> Result<InstallPlan, String> {
    build_install_plan(&repo_input, release_fixture, os, arch)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn install_repo(repo_input: String) -> Result<Vec<ManagedAppView>, String> {
    install_repo_to_tracking(&repo_input)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn uninstall_repo(repo_input: String) -> Result<Vec<ManagedAppView>, String> {
    uninstall_repo_from_tracking(&repo_input)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn remove_tracked_repo(repo_input: String) -> Result<Vec<ManagedAppView>, String> {
    remove_tracked_repo_from_tracking(&repo_input)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn bulk_remove_tracked_repos(repo_inputs: Vec<String>) -> Result<BulkRemoveResultView, String> {
    bulk_remove_tracked_repos_from_tracking(repo_inputs)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    open_url_in_system(&url).map_err(format_error)
}

async fn build_dashboard() -> Result<Vec<ManagedAppView>> {
    let store = ManifestStore::default()?;
    let manifest = store.load()?;
    let tracked_store = TrackedRepoStore::default()?;
    tracked_store.seed_if_missing(&[DEFAULT_TRACKED_REPO_ID])?;
    let tracked_repos = tracked_store.load()?;
    let installed_ids: HashSet<String> = manifest.apps.iter().map(|app| app.id.clone()).collect();
    let runtime_config = runtime_config()?;
    let client = release_client(Some(&runtime_config))?;

    let mut dashboard = Vec::with_capacity(manifest.apps.len() + tracked_repos.len());
    for app in manifest.apps {
        dashboard.push(enrich_installed_app(&client, app).await);
    }

    for tracked_repo in tracked_repos {
        if installed_ids.contains(&tracked_repo.repo_id) {
            continue;
        }
        dashboard.push(enrich_tracked_repo(&client, &tracked_repo.repo_id).await);
    }

    Ok(dashboard)
}

async fn enrich_installed_app(client: &ReleaseClient, app: ghrm_core::manifest::InstalledApp) -> ManagedAppView {
    match RepoRef::parse(&app.repo_url) {
        Ok(repo) => match client.latest_release(&repo).await {
            Ok(release) => render_app(app, repo, release),
            Err(error) => build_failed_view(
                app.id,
                app.name,
                Some(repo.github_url()),
                Some(format!("failed to request latest release: {error}")),
            ),
        },
        Err(error) => build_failed_view(
            app.id,
            app.name,
            None,
            Some(format!("invalid repository URL: {error}")),
        ),
    }
}

async fn enrich_tracked_repo(client: &ReleaseClient, repo_id: &str) -> ManagedAppView {
    match RepoRef::parse(repo_id) {
        Ok(repo) => match client.latest_release(&repo).await {
            Ok(release) => render_tracked_repo(repo, release),
            Err(error) => build_failed_view(
                repo.id(),
                repo.name.clone(),
                Some(repo.github_url()),
                Some(format!("failed to request latest release: {error}")),
            ),
        },
        Err(error) => build_failed_view(
            repo_id.to_string(),
            repo_id.to_string(),
            None,
            Some(format!("invalid repository URL: {error}")),
        ),
    }
}

fn render_app(
    app: ghrm_core::manifest::InstalledApp,
    repo: RepoRef,
    release: Release,
) -> ManagedAppView {
    let matcher = AssetMatcher::current();
    match matcher.select_best(&release) {
        Ok(matched) => {
            let status = if app.installed_version == release.tag_name {
                AppStatus::Current
            } else {
                AppStatus::UpdateAvailable
            };

            ManagedAppView {
                id: app.id,
                name: app.name,
                current_version: app.installed_version,
                latest_version: release.tag_name.clone(),
                status,
                source: "GitHub".to_string(),
                release_title: release.name.clone(),
                release_note: release
                    .release_note()
                    .map(|note| note.to_string())
                    .or_else(|| Some("This release does not include a release note.".to_string())),
                release_url: release.html_url.clone().or_else(|| Some(repo.github_url())),
                published_at: release.published_at.as_ref().map(|value| value.to_rfc3339()),
                asset_name: Some(matched.asset.name.clone()),
                install_path: app.install_path.display().to_string(),
                install_type: format!("{:?}", app.install_type),
                install_path_kind: app.install_path_kind,
                uninstall_supported: app.uninstall_supported,
            }
        }
        Err(error) => build_failed_view(
            app.id,
            app.name,
            Some(repo.github_url()),
            Some(error.to_string()),
        ),
    }
}

fn render_tracked_repo(repo: RepoRef, release: Release) -> ManagedAppView {
    let matcher = AssetMatcher::current();
    let matched = matcher.select_best(&release).ok();
    let install_path = default_install_path(&repo);
    ManagedAppView {
        id: repo.id(),
        name: repo.name.clone(),
        current_version: "未安装".to_string(),
        latest_version: release.tag_name.clone(),
        status: AppStatus::NeedsChoice,
        source: "GitHub".to_string(),
        release_title: release.name.clone(),
        release_note: release
            .release_note()
            .map(|note| note.to_string())
            .or_else(|| Some("This release does not include a release note.".to_string())),
        release_url: release.html_url.clone().or_else(|| Some(repo.github_url())),
        published_at: release.published_at.as_ref().map(|value| value.to_rfc3339()),
        asset_name: matched.map(|asset| asset.asset.name),
        install_path: install_path.display().to_string(),
        install_type: "Unknown".to_string(),
        install_path_kind: InstallPathKind::Unknown,
        uninstall_supported: false,
    }
}

fn build_failed_view(
    id: String,
    name: String,
    release_url: Option<String>,
    reason: Option<String>,
) -> ManagedAppView {
    ManagedAppView {
        id,
        name,
        current_version: "未知".to_string(),
        latest_version: "未知".to_string(),
        status: AppStatus::Failed,
        source: "GitHub".to_string(),
        release_title: Some("Unable to load release".to_string()),
        release_note: reason,
        release_url,
        published_at: None,
        asset_name: None,
        install_path: "unknown".to_string(),
        install_type: "Unknown".to_string(),
        install_path_kind: InstallPathKind::Unknown,
        uninstall_supported: false,
    }
}

async fn add_repo_to_tracking(repo_input: &str) -> Result<Vec<ManagedAppView>> {
    let repo = RepoRef::parse(repo_input)?;
    let runtime_config = runtime_config()?;
    let client = release_client(Some(&runtime_config))?;
    client
        .latest_release(&repo)
        .await
        .with_context(|| format!("failed to fetch latest release for {}", repo.id()))?;

    let store = TrackedRepoStore::default()?;
    store.upsert(&repo.id())?;

    build_dashboard().await
}

async fn install_repo_to_tracking(repo_input: &str) -> Result<Vec<ManagedAppView>> {
    let repo = RepoRef::parse(repo_input)?;
    let runtime_config = runtime_config()?;
    let client = release_client(Some(&runtime_config))?;
    let release = client
        .latest_release(&repo)
        .await
        .with_context(|| format!("failed to fetch latest release for {}", repo.id()))?;

    let matched = AssetMatcher::current().select_best(&release)?;
    let plan = InstallPlan::from_match(&repo, &release, &matched);
    let store = ManifestStore::default()?;
    install_from_plan(&plan, &store, None, Some(&runtime_config)).await?;

    build_dashboard().await
}

async fn uninstall_repo_from_tracking(repo_input: &str) -> Result<Vec<ManagedAppView>> {
    let repo = RepoRef::parse(repo_input)?;
    let store = ManifestStore::default()?;
    let removed = ghrm_core::installer::uninstall_repo(&store, &repo.id())?;
    if removed.is_none() {
        anyhow::bail!("no managed app matched {}", repo.id());
    }

    build_dashboard().await
}

async fn remove_tracked_repo_from_tracking(repo_input: &str) -> Result<Vec<ManagedAppView>> {
    let repo = RepoRef::parse(repo_input)?;
    let store = TrackedRepoStore::default()?;
    let removed = store.remove(&repo.id())?;
    if !removed {
        anyhow::bail!("no tracked repo matched {}", repo.id());
    }

    build_dashboard().await
}

async fn bulk_remove_tracked_repos_from_tracking(
    repo_inputs: Vec<String>,
) -> Result<BulkRemoveResultView> {
    let repo_ids = repo_inputs
        .into_iter()
        .map(|repo_input| RepoRef::parse(&repo_input).map(|repo| repo.id()))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let store = TrackedRepoStore::default()?;
    let removed_ids = store.remove_many(&repo_ids)?;
    if removed_ids.is_empty() {
        anyhow::bail!("no tracked repos matched the provided selection");
    }

    let apps = build_dashboard().await?;
    Ok(BulkRemoveResultView {
        apps,
        removed_count: removed_ids.len(),
    })
}

fn open_url_in_system(url: &str) -> Result<()> {
    let parsed = url::Url::parse(url).context("failed to parse URL")?;
    validate_github_url(&parsed)?;
    open_url_with_platform(url)
}

async fn build_install_plan(
    repo_input: &str,
    release_fixture: Option<PathBuf>,
    os: Option<UiOs>,
    arch: Option<UiArch>,
) -> Result<InstallPlan> {
    let repo = RepoRef::parse(repo_input)?;
    let runtime_config = runtime_config()?;
    let release = match release_fixture {
        Some(path) => read_fixture_release(&path)?,
        None => {
            let client = release_client(Some(&runtime_config))?;
            client
                .latest_release(&repo)
                .await
                .with_context(|| format!("failed to fetch latest release for {}", repo.id()))?
        }
    };

    let matcher = match (os, arch) {
        (Some(os), Some(arch)) => AssetMatcher::new(os.into(), arch.into()),
        _ => AssetMatcher::current(),
    };
    let matched = matcher.select_best(&release)?;
    Ok(InstallPlan::from_match(&repo, &release, &matched))
}

fn default_install_path(repo: &RepoRef) -> PathBuf {
    let base_dir = runtime_config()
        .ok()
        .and_then(|config| config.install_root)
        .or_else(|| {
            ManifestStore::default_path()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    base_dir
        .join("apps")
        .join(format!("{}-{}", repo.owner, repo.name))
}

fn config_store() -> Result<ConfigStore> {
    ConfigStore::from_env_or_default()
}

fn runtime_config() -> Result<Config> {
    config_store()?.load()
}

fn release_client(runtime_config: Option<&Config>) -> Result<ReleaseClient> {
    let token = runtime_config
        .and_then(|config| config.github_token.as_deref())
        .map(str::to_string)
        .or_else(|| env::var("GITHUB_TOKEN").ok());
    let proxy = runtime_config
        .and_then(|config| config.proxy_url.as_deref())
        .map(str::to_string)
        .or_else(|| env::var("HTTPS_PROXY").ok());
    ReleaseClient::new(token.as_deref(), proxy.as_deref())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopConfig {
    github_token: Option<String>,
    proxy_url: Option<String>,
    install_root: Option<PathBuf>,
}

impl From<Config> for DesktopConfig {
    fn from(value: Config) -> Self {
        Self {
            github_token: value.github_token,
            proxy_url: value.proxy_url,
            install_root: value.install_root,
        }
    }
}

impl From<DesktopConfig> for Config {
    fn from(value: DesktopConfig) -> Self {
        Self {
            github_token: value.github_token,
            proxy_url: value.proxy_url,
            install_root: value.install_root,
        }
    }
}

fn validate_github_url(url: &url::Url) -> Result<()> {
    if url.scheme() != "https" {
        anyhow::bail!("only https URLs are allowed");
    }

    match url.host_str() {
        Some("github.com") | Some("www.github.com") => Ok(()),
        _ => anyhow::bail!("only github.com URLs are allowed"),
    }
}

fn open_url_with_platform(url: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .with_context(|| format!("failed to open {url}"))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .with_context(|| format!("failed to open {url}"))?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .with_context(|| format!("failed to open {url}"))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    {
        anyhow::bail!("opening URLs is not supported on this platform");
    }
}

fn read_fixture_release(path: &PathBuf) -> Result<Release> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read release fixture {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse release fixture {}", path.display()))
}

fn format_error(error: anyhow::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::validate_github_url;

    #[test]
    fn allows_github_release_pages() {
        let url = url::Url::parse("https://github.com/dongrencd/gh-release-manager/releases/tag/v0.2.0")
            .expect("valid URL");
        assert!(validate_github_url(&url).is_ok());
    }

    #[test]
    fn rejects_non_github_urls() {
        let url = url::Url::parse("https://example.com/releases")
            .expect("valid URL");
        assert!(validate_github_url(&url).is_err());
    }
}

impl From<UiOs> for OperatingSystem {
    fn from(value: UiOs) -> Self {
        match value {
            UiOs::Windows => OperatingSystem::Windows,
            UiOs::Linux => OperatingSystem::Linux,
            UiOs::Macos => OperatingSystem::Macos,
        }
    }
}

impl From<UiArch> for Architecture {
    fn from(value: UiArch) -> Self {
        match value {
            UiArch::X64 => Architecture::X64,
            UiArch::Arm64 => Architecture::Arm64,
        }
    }
}

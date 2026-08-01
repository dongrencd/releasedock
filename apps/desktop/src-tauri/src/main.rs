#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::{
    collections::{HashMap, HashSet},
    env,
    ffi::OsStr,
    fs,
    future::Future,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

#[cfg(not(target_os = "windows"))]
use std::process::Command;

use anyhow::{Context, Result};
use releasedock_core::{
    asset_matcher::{Architecture, AssetMatcher, InstallType, OperatingSystem},
    config::{Config, ConfigStore, Language},
    config::{
        background_check_enabled, check_interval_minutes, download_acceleration_enabled,
        download_max_connections, effective_install_root,
    },
    install_plan::{InstallManagementKind, InstallPlan, InstallSelectionGuard},
    installer::{
        ProgressReporter, RollbackGuard, TaskProgress, adopt_pending_system_installer_apps,
        adopt_system_installer_app, infer_launch_target, install_from_plan,
        repair_managed_windows_executable_records,
        rollback_repo_guarded as core_rollback_repo_guarded, uninstall_repo as core_uninstall_repo,
    },
    integrity::{IntegrityPlan, IntegrityStatus, IntegrityVerifier},
    manifest::{
        InstallPathKind, InstalledApp, LifecycleEvent, ManifestStore, SystemPackageManager,
    },
    release::{Release, ReleaseClient, ReleasePage, RepositorySearchResult},
    release_policy::{
        PolicyMutation, ReleaseChannel, ReleaseDirection, ReleasePolicy, ReleaseSelection,
        ReleaseSelector,
    },
    repo::RepoRef,
};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as AutostartManagerExt};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Mutex;
use tokio::task::{JoinHandle, JoinSet};

#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Shell::ShellExecuteW;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;

mod background_check;
mod tracking;
mod tray;

use tracking::{TrackedRepo, TrackedRepoStore};

const DEFAULT_TRACKED_REPO_ID: &str = "dongrencd/releasedock";
const TASK_PROGRESS_EVENT: &str = "task-progress";
const DASHBOARD_ITEM_EVENT: &str = "dashboard-item-updated";
const DASHBOARD_PROGRESS_EVENT: &str = "dashboard-progress";
const DASHBOARD_CONCURRENCY: usize = 6;

/// 持有当前后台检查任务句柄，支持保存设置后热重启
static BACKGROUND_TASK: LazyLock<Mutex<Option<JoinHandle<()>>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum AppStatus {
    UpdateAvailable,
    DowngradeAvailable,
    Current,
    NeedsChoice,
    NoRelease,
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
    launch_path: Option<String>,
    installer_path: Option<String>,
    system_package_name: Option<String>,
    system_package_manager: Option<SystemPackageManager>,
    management_kind: Option<InstallManagementKind>,
    install_path: String,
    install_type: String,
    install_path_kind: InstallPathKind,
    uninstall_supported: bool,
    release_policy: ReleasePolicy,
    artifact_sha256: Option<String>,
    integrity_status: Option<IntegrityStatus>,
    checksum_asset_name: Option<String>,
    rollback: Option<RollbackSnapshotView>,
    release_direction: ReleaseDirection,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    recent_activities: Vec<LifecycleEvent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RollbackSnapshotView {
    version: String,
    asset_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryDiscoveryView {
    repo_id: String,
    name: String,
    description: Option<String>,
    stars: u64,
    latest_tag: Option<String>,
    latest_release_name: Option<String>,
    has_installable_asset: bool,
    installable_asset_name: Option<String>,
    html_url: String,
}

impl From<&releasedock_core::manifest::RollbackSnapshot> for RollbackSnapshotView {
    fn from(snapshot: &releasedock_core::manifest::RollbackSnapshot) -> Self {
        Self {
            version: snapshot.version.clone(),
            asset_name: snapshot.asset_name.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseVersionView {
    tag_name: String,
    name: Option<String>,
    prerelease: bool,
    published_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct InstallPlanView {
    repo_id: String,
    version: String,
    asset_name: String,
    install_type: InstallType,
    management_kind: InstallManagementKind,
    system_package_manager: Option<SystemPackageManager>,
    requires_user_confirmation: bool,
    integrity: IntegrityPlan,
    release_direction: ReleaseDirection,
    selection_guard: Option<InstallSelectionGuard>,
    target_policy: Option<ReleasePolicy>,
    notes: Vec<String>,
}

impl From<&InstallPlan> for InstallPlanView {
    fn from(plan: &InstallPlan) -> Self {
        Self {
            repo_id: plan.repo_id.clone(),
            version: plan.version.clone(),
            asset_name: plan.asset_name.clone(),
            install_type: plan.install_type,
            management_kind: plan.management_kind,
            system_package_manager: plan.system_package_manager,
            requires_user_confirmation: plan.requires_user_confirmation,
            integrity: plan.integrity.clone(),
            release_direction: plan.release_direction,
            selection_guard: plan.selection_guard.clone(),
            target_policy: plan.target_policy.clone(),
            notes: plan.notes.clone(),
        }
    }
}

fn ensure_install_preview_matches(
    preview: &InstallPlanView,
    rebuilt: &InstallPlanView,
) -> Result<()> {
    let mut preview = preview.clone();
    let mut rebuilt = rebuilt.clone();
    // Notes are explanatory copy and may change independently of the fields
    // that determine what the installer will fetch and execute.
    preview.notes.clear();
    rebuilt.notes.clear();
    if preview != rebuilt {
        anyhow::bail!("stale install preview: release plan changed after confirmation");
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RollbackPreview {
    repo_id: String,
    active_version: String,
    snapshot_version: String,
    snapshot_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BulkRemoveResultView {
    apps: Vec<ManagedAppView>,
    removed_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GithubConnectivityTestResult {
    ok: bool,
    message: String,
    problem: String,
    used_token: bool,
    used_proxy: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardItemEvent {
    refresh_id: u64,
    index: usize,
    total: usize,
    item: ManagedAppView,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardProgressEvent {
    refresh_id: u64,
    total: usize,
    completed: usize,
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
        releasedock_cli::run_from_args(env::args_os()).await?;
        return Ok(());
    }

    tauri::Builder::default()
        // 单实例插件必须先注册，确保重复双击 exe 时新实例会立即退出，
        // 并把已有的托盘窗口拉回前台。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            restore_main_window(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            load_dashboard,
            load_local_dashboard,
            load_config,
            save_config,
            notification_permission_state,
            request_notification_permission,
            test_github_connectivity,
            is_background_start,
            is_main_window_visible,
            add_repo,
            search_github_repos,
            list_release_versions,
            preview_install,
            install_repo,
            set_release_channel,
            set_release_pin,
            set_release_ignored,
            preview_rollback,
            rollback_repo,
            uninstall_repo,
            remove_tracked_repo,
            bulk_remove_tracked_repos,
            adopt_system_install,
            open_app,
            open_url,
            open_path,
            open_install_location,
            open_installer_folder,
            open_notification_settings,
            open_system_uninstall_settings
        ])
        // 系统托盘：创建图标、菜单
        .setup(|app| {
            // 读取语言配置，用于 tray 菜单 i18n
            let lang = runtime_config()
                .ok()
                .and_then(|c| c.language)
                .map(|l| match l.as_str() {
                    "zh-CN" => Language::ZhCn,
                    _ => Language::En,
                })
                .unwrap_or(Language::En);
            tray::build_tray(app.handle(), lang)?;
            if let Ok(config) = runtime_config() {
                let _ = sync_autostart_setting(app.handle(), config.autostart_enabled);
            }

            // 启动后台定时检查（走 restart 路径以确保统一管理句柄）
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                restart_background_checker(app_handle).await;
            });

            if should_start_hidden() {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            Ok(())
        })
        // 关闭窗口时仅隐藏到托盘，不退出程序
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 首次关闭时发系统通知提示已驻留托盘
                let app = window.app_handle();
                let app_clone = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(store) = config_store() {
                        if let Ok(config) = store.load() {
                            if !config.tray_hint_shown.unwrap_or(false) {
                                // 发提示通知
                                let _ = app_clone
                                    .notification()
                                    .builder()
                                    .title("ReleaseDock")
                                    .body("ReleaseDock stays in the system tray. Click the tray icon to reopen.")
                                    .show();
                                // 标记已提示
                                let mut updated = config;
                                updated.tray_hint_shown = Some(true);
                                let _ = store.save(&updated);
                            }
                        }
                    }
                });

                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run ReleaseDock desktop app");

    Ok(())
}

pub(crate) fn restore_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = app.emit("restore-main-window", ());
    }
}

fn should_run_cli() -> bool {
    if cfg!(target_os = "windows") {
        return false;
    }

    let args: Vec<_> = env::args_os().collect();
    match args.get(1).and_then(|arg| arg.to_str()) {
        None => false,
        Some("--gui") => false,
        Some("--background") => false,
        Some(_) => true,
    }
}

fn should_start_hidden() -> bool {
    background_start_from_args(env::args_os())
}

fn background_start_from_args<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    args.into_iter()
        .any(|arg| arg.as_ref() == OsStr::new("--background"))
}

#[tauri::command]
fn is_background_start() -> bool {
    should_start_hidden()
}

#[tauri::command]
fn is_main_window_visible(app: tauri::AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(true)
}

fn sync_autostart_setting<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    enabled: Option<bool>,
) -> Result<()> {
    let Some(enabled) = enabled else {
        return Ok(());
    };

    let autostart = app.autolaunch();
    if enabled {
        autostart.enable().context("failed to enable autostart")?;
    } else {
        autostart.disable().context("failed to disable autostart")?;
    }
    Ok(())
}

#[tauri::command]
async fn load_dashboard(
    app: tauri::AppHandle,
    refresh_id: u64,
) -> Result<Vec<ManagedAppView>, String> {
    build_dashboard(&app, refresh_id)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn load_local_dashboard() -> Result<Vec<ManagedAppView>, String> {
    let language = runtime_config()
        .map(|config| ui_language(&config))
        .unwrap_or(Language::En);
    build_local_dashboard(language).map_err(format_error)
}

#[tauri::command]
async fn load_config() -> Result<DesktopConfig, String> {
    runtime_config()
        .map(desktop_config_from_runtime)
        .map_err(format_error)
}

#[tauri::command]
async fn save_config(
    app: tauri::AppHandle,
    config: DesktopConfig,
) -> Result<DesktopConfig, String> {
    let store = config_store().map_err(format_error)?;
    let runtime_config = Config::from(config.clone());
    sync_autostart_setting(&app, runtime_config.autostart_enabled).map_err(format_error)?;
    store.save(&runtime_config).map_err(format_error)?;

    // 保存后热重启后台检查任务
    restart_background_checker(app).await;

    Ok(desktop_config_from_runtime(runtime_config))
}

#[tauri::command]
async fn notification_permission_state(app: tauri::AppHandle) -> Result<String, String> {
    app.notification()
        .permission_state()
        .map(|state| state.to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn request_notification_permission(app: tauri::AppHandle) -> Result<String, String> {
    app.notification()
        .request_permission()
        .map(|state| state.to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn test_github_connectivity(
    config: DesktopConfig,
) -> Result<GithubConnectivityTestResult, String> {
    // 使用设置页当前草稿值创建临时 client，用户不必先保存配置再验证网络路径。
    let token = non_empty_config_value(config.github_token);
    let proxy = non_empty_config_value(config.proxy_url);
    let used_token = token.is_some();
    let used_proxy = proxy.is_some();

    let client = match ReleaseClient::new(token.as_deref(), proxy.as_deref()) {
        Ok(client) => client,
        Err(error) => {
            let message = sanitize_connectivity_message(
                &format_error(error),
                token.as_deref(),
                proxy.as_deref(),
            );
            let problem = classify_connectivity_problem(&message, used_proxy);
            return Ok(github_connectivity_result(
                false, message, problem, used_token, used_proxy,
            ));
        }
    };

    let result = match client.check_connectivity().await {
        Ok(()) => github_connectivity_result(
            true,
            "GitHub API is reachable with the current settings.".to_string(),
            "none",
            used_token,
            used_proxy,
        ),
        Err(error) => {
            let message = sanitize_connectivity_message(
                &format_error(error),
                token.as_deref(),
                proxy.as_deref(),
            );
            let problem = classify_connectivity_problem(&message, used_proxy);
            github_connectivity_result(false, message, problem, used_token, used_proxy)
        }
    };

    Ok(result)
}

#[tauri::command]
async fn add_repo(
    app: tauri::AppHandle,
    repo_input: String,
) -> Result<Vec<ManagedAppView>, String> {
    add_repo_to_tracking(&app, &repo_input)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn search_github_repos(query: String) -> Result<Vec<RepositoryDiscoveryView>, String> {
    search_github_repos_for_discovery(&query)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn list_release_versions(repo_input: String) -> Result<Vec<ReleaseVersionView>, String> {
    let repo = RepoRef::parse(&repo_input).map_err(|error| format_error(error.into()))?;
    let runtime_config = runtime_config().map_err(format_error)?;
    let client = release_client(Some(&runtime_config)).map_err(format_error)?;
    let releases = load_release_catalog_with(
        |page| client.releases_page(&repo, page, 100),
        |releases| releases.iter().filter(|release| !release.draft).count() >= 100,
    )
    .await
    .map_err(format_error)?;
    Ok(release_versions_from_catalog(&releases))
}

#[tauri::command]
async fn preview_install(
    repo_input: String,
    version: Option<String>,
    target_channel: Option<ReleaseChannel>,
) -> Result<InstallPlanView, String> {
    build_install_plan(
        &repo_input,
        version.as_deref(),
        target_channel.unwrap_or_default(),
        None,
        None,
        None,
    )
    .await
    .map(|plan| InstallPlanView::from(&plan))
    .map_err(format_error)
}

#[tauri::command]
async fn install_repo(
    app: tauri::AppHandle,
    preview: InstallPlanView,
) -> Result<Vec<ManagedAppView>, String> {
    install_repo_to_tracking(&app, &preview)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn set_release_channel(
    app: tauri::AppHandle,
    repo_input: String,
    channel: ReleaseChannel,
) -> Result<Vec<ManagedAppView>, String> {
    mutate_release_policy_and_reload(&app, &repo_input, PolicyMutation::SetChannel(channel))
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn set_release_pin(
    app: tauri::AppHandle,
    repo_input: String,
    version: Option<String>,
) -> Result<Vec<ManagedAppView>, String> {
    let mutation = version
        .map(PolicyMutation::PinVersion)
        .unwrap_or(PolicyMutation::Unpin);
    mutate_release_policy_and_reload(&app, &repo_input, mutation)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn set_release_ignored(
    app: tauri::AppHandle,
    repo_input: String,
    version: String,
    ignored: bool,
) -> Result<Vec<ManagedAppView>, String> {
    let mutation = if ignored {
        PolicyMutation::IgnoreVersion(version)
    } else {
        PolicyMutation::UnignoreVersion(version)
    };
    mutate_release_policy_and_reload(&app, &repo_input, mutation)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn preview_rollback(repo_input: String) -> Result<RollbackPreview, String> {
    build_rollback_preview(&repo_input).map_err(format_error)
}

#[tauri::command]
async fn rollback_repo(
    app: tauri::AppHandle,
    preview: RollbackPreview,
) -> Result<Vec<ManagedAppView>, String> {
    rollback_repo_from_preview(&app, &preview)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn uninstall_repo(
    app: tauri::AppHandle,
    repo_input: String,
) -> Result<Vec<ManagedAppView>, String> {
    uninstall_repo_from_tracking(&app, &repo_input)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn remove_tracked_repo(
    app: tauri::AppHandle,
    repo_input: String,
) -> Result<Vec<ManagedAppView>, String> {
    remove_tracked_repo_from_tracking(&app, &repo_input)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn bulk_remove_tracked_repos(
    app: tauri::AppHandle,
    repo_inputs: Vec<String>,
) -> Result<BulkRemoveResultView, String> {
    bulk_remove_tracked_repos_from_tracking(&app, repo_inputs)
        .await
        .map_err(format_error)
}

#[tauri::command]
async fn open_app(repo_input: String) -> Result<(), String> {
    open_app_in_system(&repo_input).map_err(format_error)
}

#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    open_url_in_system(&url).map_err(format_error)
}

#[tauri::command]
async fn open_path(path: String) -> Result<(), String> {
    open_path_in_system(&path).map_err(format_error)
}

#[tauri::command]
async fn open_install_location(
    path: String,
    install_path_kind: InstallPathKind,
) -> Result<(), String> {
    open_install_location_in_system(&path, install_path_kind).map_err(format_error)
}

#[tauri::command]
async fn open_installer_folder(path: String) -> Result<(), String> {
    open_installer_folder_in_system(&path).map_err(format_error)
}

#[tauri::command]
async fn open_notification_settings() -> Result<(), String> {
    open_notification_settings_in_system().map_err(format_error)
}

#[tauri::command]
async fn open_system_uninstall_settings() -> Result<(), String> {
    open_system_uninstall_settings_in_system().map_err(format_error)
}

async fn build_dashboard(app: &tauri::AppHandle, refresh_id: u64) -> Result<Vec<ManagedAppView>> {
    let store = ManifestStore::default()?;
    let runtime_config = runtime_config()?;
    // Dashboard 刷新不能被后台修正影响；失败时继续使用现有 manifest 记录渲染。
    let _ = adopt_pending_system_installer_apps(&store);
    let _ = repair_managed_windows_executable_records(&store, Some(&runtime_config));
    let manifest = store.load()?;
    let tracked_store = TrackedRepoStore::default()?;
    tracked_store.seed_if_missing(&[DEFAULT_TRACKED_REPO_ID])?;
    let tracked_repos = tracked_store.load_for_dashboard()?.repos;
    let recent_activities = Arc::new(group_recent_activities(&manifest.lifecycle_events));
    let releasedock_core::manifest::Manifest { apps, .. } = manifest;
    let installed_ids: HashSet<String> = apps.iter().map(|app| app.id.clone()).collect();
    let language = ui_language(&runtime_config);
    let client = release_client(Some(&runtime_config))?;
    let work_items = build_dashboard_work_items(apps, tracked_repos, installed_ids);
    if work_items.is_empty() {
        return Ok(Vec::new());
    }

    let total = work_items.len();
    let mut pending = work_items.into_iter();
    let mut tasks = JoinSet::new();
    for _ in 0..DASHBOARD_CONCURRENCY {
        if let Some(work_item) = pending.next() {
            spawn_dashboard_task(
                &mut tasks,
                &client,
                work_item,
                language,
                Arc::clone(&recent_activities),
            );
        }
    }

    let mut dashboard = vec![None; total];
    let mut completed = 0usize;
    while let Some(join_result) = tasks.join_next().await {
        let (index, item) = join_result
            .map_err(|error| anyhow::anyhow!("failed to join dashboard refresh task: {error}"))?;
        completed += 1;
        dashboard[index] = Some(item.clone());
        let _ = app.emit(
            DASHBOARD_ITEM_EVENT,
            DashboardItemEvent {
                refresh_id,
                index,
                total,
                item,
            },
        );
        let _ = app.emit(
            DASHBOARD_PROGRESS_EVENT,
            DashboardProgressEvent {
                refresh_id,
                total,
                completed,
            },
        );

        if let Some(work_item) = pending.next() {
            spawn_dashboard_task(
                &mut tasks,
                &client,
                work_item,
                language,
                Arc::clone(&recent_activities),
            );
        }
    }

    Ok(dashboard
        .into_iter()
        .map(|item| item.expect("dashboard item should be populated"))
        .collect())
}

fn build_local_dashboard(language: Language) -> Result<Vec<ManagedAppView>> {
    let store = ManifestStore::default()?;
    let tracked_store = TrackedRepoStore::default()?;
    tracked_store.seed_if_missing(&[DEFAULT_TRACKED_REPO_ID])?;
    let tracked_load = tracked_store.load_for_dashboard()?;
    let tracked_repos = tracked_load.repos;
    let manifest = store.load()?;
    let installed_ids: HashSet<String> = manifest.apps.iter().map(|app| app.id.clone()).collect();
    let recent_activities = group_recent_activities(&manifest.lifecycle_events);
    let work_items = build_dashboard_work_items(manifest.apps, tracked_repos, installed_ids);
    let reason = tracked_load.error.unwrap_or_else(|| {
        tr_owned(
            language,
            "GitHub release data is unavailable. Fix the connection settings and retry.",
            "GitHub release 数据暂时不可用，请修复连接设置后重试。",
        )
    });

    Ok(build_local_dashboard_views(
        work_items,
        language,
        &reason,
        &recent_activities,
    ))
}

fn build_local_dashboard_views(
    work_items: Vec<DashboardWorkItem>,
    language: Language,
    reason: &str,
    recent_activities: &HashMap<String, Vec<LifecycleEvent>>,
) -> Vec<ManagedAppView> {
    work_items
        .into_iter()
        .map(|work_item| {
            let item = match work_item {
                DashboardWorkItem::Installed { app, repo, .. } => {
                    build_failed_installed_view(app, repo, Some(reason.to_string()), language)
                }
                DashboardWorkItem::Tracked { repo, .. } => {
                    let mut view = build_failed_view(
                        repo.id(),
                        repo.name.clone(),
                        Some(repo.github_url()),
                        Some(reason.to_string()),
                        language,
                    );
                    view.current_version = tr_owned(language, "Not installed", "未安装");
                    view
                }
            };
            attach_recent_activity(item, recent_activities)
        })
        .collect()
}

fn spawn_dashboard_task(
    tasks: &mut JoinSet<(usize, ManagedAppView)>,
    client: &ReleaseClient,
    work_item: DashboardWorkItem,
    language: Language,
    recent_activities: Arc<HashMap<String, Vec<LifecycleEvent>>>,
) {
    let client = client.clone();
    tasks.spawn(async move {
        resolve_dashboard_item(client, work_item, language, recent_activities).await
    });
}

#[derive(Debug)]
enum DashboardWorkItem {
    Installed {
        index: usize,
        app: releasedock_core::manifest::InstalledApp,
        repo: RepoRef,
    },
    Tracked {
        index: usize,
        repo: RepoRef,
    },
}

fn build_dashboard_work_items(
    installed_apps: Vec<releasedock_core::manifest::InstalledApp>,
    tracked_repos: Vec<TrackedRepo>,
    installed_ids: HashSet<String>,
) -> Vec<DashboardWorkItem> {
    let mut work_items = Vec::with_capacity(installed_apps.len() + tracked_repos.len());

    for app in installed_apps {
        if let Ok(repo) = RepoRef::parse(&app.repo_url) {
            let index = work_items.len();
            work_items.push(DashboardWorkItem::Installed { index, app, repo });
        }
    }

    for tracked_repo in tracked_repos {
        let Ok(repo) = RepoRef::parse(tracked_repo.repo_id.as_str()) else {
            continue;
        };

        if installed_ids.contains(&repo.id()) {
            continue;
        }

        let index = work_items.len();
        work_items.push(DashboardWorkItem::Tracked { index, repo });
    }

    work_items
}

async fn resolve_dashboard_item(
    client: ReleaseClient,
    work_item: DashboardWorkItem,
    language: Language,
    recent_activities: Arc<HashMap<String, Vec<LifecycleEvent>>>,
) -> (usize, ManagedAppView) {
    match work_item {
        DashboardWorkItem::Installed { index, app, repo } => {
            let item = match load_release_catalog_for_selection(
                &client,
                &repo,
                Some(&app),
                None,
                ReleaseChannel::Stable,
            )
            .await
            {
                Ok(releases) if releases.is_empty() => {
                    build_no_release_installed_view(app, repo, language)
                }
                Ok(releases) => match ReleaseSelector::select(
                    &releases,
                    &app.release_policy,
                    Some(&app.installed_version),
                    None,
                ) {
                    Ok(selection) => {
                        render_app(app, repo, selection.release, selection.direction, language)
                    }
                    Err(error) => {
                        build_failed_installed_view(app, repo, Some(error.to_string()), language)
                    }
                },
                Err(error) => {
                    build_failed_installed_view(app, repo, Some(error.to_string()), language)
                }
            };
            (index, attach_recent_activity(item, &recent_activities))
        }
        DashboardWorkItem::Tracked { index, repo } => {
            let item = match load_release_catalog_for_selection(
                &client,
                &repo,
                None,
                None,
                ReleaseChannel::Stable,
            )
            .await
            {
                Ok(releases) if releases.is_empty() => {
                    build_no_release_tracked_view(repo, language)
                }
                Ok(releases) => match select_tracked_release(&releases) {
                    Ok(selection) => render_tracked_repo(repo, selection.release, language),
                    Err(error) => build_failed_view(
                        repo.id(),
                        repo.name.clone(),
                        Some(repo.github_url()),
                        Some(error.to_string()),
                        language,
                    ),
                },
                Err(error) => build_failed_view(
                    repo.id(),
                    repo.name.clone(),
                    Some(repo.github_url()),
                    Some(error.to_string()),
                    language,
                ),
            };
            (index, attach_recent_activity(item, &recent_activities))
        }
    }
}

fn attach_recent_activity(
    mut item: ManagedAppView,
    recent_activities: &HashMap<String, Vec<LifecycleEvent>>,
) -> ManagedAppView {
    item.recent_activities = recent_activities.get(&item.id).cloned().unwrap_or_default();
    item
}

fn group_recent_activities(events: &[LifecycleEvent]) -> HashMap<String, Vec<LifecycleEvent>> {
    let mut grouped = events.iter().fold(HashMap::new(), |mut map, event| {
        map.entry(event.repo_id.clone())
            .or_insert_with(Vec::new)
            .push(event.clone());
        map
    });

    for activities in grouped.values_mut() {
        activities.reverse();
    }

    grouped
}

fn render_app(
    app: releasedock_core::manifest::InstalledApp,
    repo: RepoRef,
    release: Release,
    direction: ReleaseDirection,
    language: Language,
) -> ManagedAppView {
    let is_current = app.installed_version == release.tag_name;
    let asset_name = if is_current {
        app.asset_name.clone()
    } else {
        match AssetMatcher::current().select_best(&release) {
            Ok(matched) => matched.asset.name.clone(),
            Err(error) => {
                return build_failed_installed_view(app, repo, Some(error.to_string()), language);
            }
        }
    };
    let management_kind = management_kind_for_app(&app);
    let launch_path = resolve_launch_path(&app).map(|value| value.display().to_string());
    let status = if is_current {
        AppStatus::Current
    } else if direction == ReleaseDirection::Downgrade {
        AppStatus::DowngradeAvailable
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
            .or_else(|| {
                Some(tr_owned(
                    language,
                    "This release does not include a release note.",
                    "这个 release 没有填写 release note。",
                ))
            }),
        release_url: release.html_url.clone().or_else(|| Some(repo.github_url())),
        published_at: release
            .published_at
            .as_ref()
            .map(|value| value.to_rfc3339()),
        asset_name: Some(asset_name),
        launch_path,
        installer_path: app
            .installer_path
            .as_ref()
            .map(|value| value.display().to_string()),
        system_package_name: app.system_package_name.clone(),
        system_package_manager: app.system_package_manager,
        management_kind: Some(management_kind),
        install_path: app.install_path.display().to_string(),
        install_type: format!("{:?}", app.install_type),
        install_path_kind: app.install_path_kind,
        uninstall_supported: app.uninstall_supported,
        release_policy: app.release_policy,
        artifact_sha256: app.artifact_sha256,
        integrity_status: app.integrity_status,
        checksum_asset_name: app.checksum_asset_name,
        rollback: app.rollback.as_ref().map(RollbackSnapshotView::from),
        release_direction: direction,
        recent_activities: Vec::new(),
    }
}

fn build_failed_installed_view(
    app: InstalledApp,
    repo: RepoRef,
    reason: Option<String>,
    language: Language,
) -> ManagedAppView {
    let management_kind = management_kind_for_app(&app);
    let launch_path = resolve_launch_path(&app).map(|value| value.display().to_string());
    ManagedAppView {
        id: app.id,
        name: app.name,
        current_version: app.installed_version,
        latest_version: tr_owned(language, "Unknown", "未知"),
        status: AppStatus::Failed,
        source: "GitHub".to_string(),
        release_title: Some(tr_owned(
            language,
            "Unable to load release",
            "无法加载 release",
        )),
        release_note: reason,
        release_url: Some(repo.github_url()),
        published_at: None,
        asset_name: None,
        launch_path,
        installer_path: app
            .installer_path
            .as_ref()
            .map(|value| value.display().to_string()),
        system_package_name: app.system_package_name.clone(),
        system_package_manager: app.system_package_manager,
        management_kind: Some(management_kind),
        install_path: app.install_path.display().to_string(),
        install_type: format!("{:?}", app.install_type),
        install_path_kind: app.install_path_kind,
        uninstall_supported: app.uninstall_supported,
        release_policy: app.release_policy,
        artifact_sha256: app.artifact_sha256,
        integrity_status: app.integrity_status,
        checksum_asset_name: app.checksum_asset_name,
        rollback: app.rollback.as_ref().map(RollbackSnapshotView::from),
        release_direction: ReleaseDirection::Unknown,
        recent_activities: Vec::new(),
    }
}

fn build_no_release_installed_view(
    app: releasedock_core::manifest::InstalledApp,
    repo: RepoRef,
    language: Language,
) -> ManagedAppView {
    let management_kind = management_kind_for_app(&app);
    let launch_path = resolve_launch_path(&app).map(|value| value.display().to_string());
    ManagedAppView {
        id: app.id,
        name: app.name,
        current_version: app.installed_version,
        latest_version: tr_owned(language, "No release", "暂无 release"),
        status: AppStatus::NoRelease,
        source: "GitHub".to_string(),
        release_title: Some(tr_owned(language, "No release published", "暂无 release")),
        release_note: Some(tr_owned(
            language,
            "This repository has no release yet.",
            "这个仓库还没有发布 release。",
        )),
        release_url: Some(repo.github_url()),
        published_at: None,
        asset_name: None,
        launch_path,
        installer_path: app
            .installer_path
            .as_ref()
            .map(|value| value.display().to_string()),
        system_package_name: app.system_package_name.clone(),
        system_package_manager: app.system_package_manager,
        management_kind: Some(management_kind),
        install_path: app.install_path.display().to_string(),
        install_type: format!("{:?}", app.install_type),
        install_path_kind: app.install_path_kind,
        uninstall_supported: app.uninstall_supported,
        release_policy: app.release_policy,
        artifact_sha256: app.artifact_sha256,
        integrity_status: app.integrity_status,
        checksum_asset_name: app.checksum_asset_name,
        rollback: app.rollback.as_ref().map(RollbackSnapshotView::from),
        release_direction: ReleaseDirection::Unknown,
        recent_activities: Vec::new(),
    }
}

fn render_tracked_repo(repo: RepoRef, release: Release, language: Language) -> ManagedAppView {
    let matcher = AssetMatcher::current();
    let matched = matcher.select_best(&release).ok();
    let install_path = default_install_path(&repo);
    ManagedAppView {
        id: repo.id(),
        name: repo.name.clone(),
        current_version: tr_owned(language, "Not installed", "未安装"),
        latest_version: release.tag_name.clone(),
        status: AppStatus::NeedsChoice,
        source: "GitHub".to_string(),
        release_title: release.name.clone(),
        release_note: release
            .release_note()
            .map(|note| note.to_string())
            .or_else(|| {
                Some(tr_owned(
                    language,
                    "This release does not include a release note.",
                    "这个 release 没有填写 release note。",
                ))
            }),
        release_url: release.html_url.clone().or_else(|| Some(repo.github_url())),
        published_at: release
            .published_at
            .as_ref()
            .map(|value| value.to_rfc3339()),
        asset_name: matched.map(|asset| asset.asset.name),
        launch_path: None,
        installer_path: None,
        system_package_name: None,
        system_package_manager: None,
        management_kind: None,
        install_path: install_path.display().to_string(),
        install_type: "Unknown".to_string(),
        install_path_kind: InstallPathKind::Unknown,
        uninstall_supported: false,
        release_policy: ReleasePolicy::default(),
        artifact_sha256: None,
        integrity_status: None,
        checksum_asset_name: None,
        rollback: None,
        release_direction: ReleaseDirection::Unknown,
        recent_activities: Vec::new(),
    }
}

fn build_no_release_tracked_view(repo: RepoRef, language: Language) -> ManagedAppView {
    let install_path = default_install_path(&repo);
    ManagedAppView {
        id: repo.id(),
        name: repo.name.clone(),
        current_version: tr_owned(language, "Not installed", "未安装"),
        latest_version: tr_owned(language, "No release", "暂无 release"),
        status: AppStatus::NoRelease,
        source: "GitHub".to_string(),
        release_title: Some(tr_owned(language, "No release published", "暂无 release")),
        release_note: Some(tr_owned(
            language,
            "This repository has no release yet.",
            "这个仓库还没有发布 release。",
        )),
        release_url: Some(repo.github_url()),
        published_at: None,
        asset_name: None,
        launch_path: None,
        installer_path: None,
        system_package_name: None,
        system_package_manager: None,
        management_kind: None,
        install_path: install_path.display().to_string(),
        install_type: "Unknown".to_string(),
        install_path_kind: InstallPathKind::Unknown,
        uninstall_supported: false,
        release_policy: ReleasePolicy::default(),
        artifact_sha256: None,
        integrity_status: None,
        checksum_asset_name: None,
        rollback: None,
        release_direction: ReleaseDirection::Unknown,
        recent_activities: Vec::new(),
    }
}

fn build_failed_view(
    id: String,
    name: String,
    release_url: Option<String>,
    reason: Option<String>,
    language: Language,
) -> ManagedAppView {
    ManagedAppView {
        id,
        name,
        current_version: tr_owned(language, "Unknown", "未知"),
        latest_version: tr_owned(language, "Unknown", "未知"),
        status: AppStatus::Failed,
        source: "GitHub".to_string(),
        release_title: Some(tr_owned(
            language,
            "Unable to load release",
            "无法加载 release",
        )),
        release_note: reason,
        release_url,
        published_at: None,
        asset_name: None,
        launch_path: None,
        installer_path: None,
        system_package_name: None,
        system_package_manager: None,
        management_kind: None,
        install_path: "unknown".to_string(),
        install_type: "Unknown".to_string(),
        install_path_kind: InstallPathKind::Unknown,
        uninstall_supported: false,
        release_policy: ReleasePolicy::default(),
        artifact_sha256: None,
        integrity_status: None,
        checksum_asset_name: None,
        rollback: None,
        release_direction: ReleaseDirection::Unknown,
        recent_activities: Vec::new(),
    }
}

async fn add_repo_to_tracking(
    app: &tauri::AppHandle,
    repo_input: &str,
) -> Result<Vec<ManagedAppView>> {
    let repo = RepoRef::parse(repo_input)?;
    let store = TrackedRepoStore::default()?;
    store.upsert(&repo.id())?;

    build_dashboard(app, 0).await
}

async fn search_github_repos_for_discovery(query: &str) -> Result<Vec<RepositoryDiscoveryView>> {
    let runtime_config = runtime_config()?;
    let client = release_client(Some(&runtime_config))?;
    let matcher = AssetMatcher::current();
    let repositories = client.search_repositories(query, 8).await?;
    let mut views = Vec::with_capacity(repositories.len());

    for repository in repositories {
        // 发现页的主价值是给用户候选仓库；单个仓库 latest release 失败不应清空整批结果。
        let view =
            match build_repository_discovery_view(&client, &matcher, repository.clone()).await {
                Ok(view) => view,
                Err(_) => RepositoryDiscoveryView {
                    repo_id: repository.repo_id,
                    name: repository.name,
                    description: repository.description,
                    stars: repository.stars,
                    latest_tag: None,
                    latest_release_name: None,
                    has_installable_asset: false,
                    installable_asset_name: None,
                    html_url: repository.html_url,
                },
            };
        views.push(view);
    }

    Ok(views)
}

async fn build_repository_discovery_view(
    client: &ReleaseClient,
    matcher: &AssetMatcher,
    repository: RepositorySearchResult,
) -> Result<RepositoryDiscoveryView> {
    let repo = RepoRef::parse(&repository.repo_id)?;
    let latest_release = client.latest_release_optional(&repo).await?;
    let (latest_tag, latest_release_name, installable_asset_name) =
        if let Some(release) = latest_release {
            let matched_asset = matcher.select_best(&release).ok();
            (
                Some(release.tag_name),
                release.name,
                matched_asset.map(|matched| matched.asset.name),
            )
        } else {
            (None, None, None)
        };

    Ok(RepositoryDiscoveryView {
        repo_id: repository.repo_id,
        name: repository.name,
        description: repository.description,
        stars: repository.stars,
        latest_tag,
        latest_release_name,
        has_installable_asset: installable_asset_name.is_some(),
        installable_asset_name,
        html_url: repository.html_url,
    })
}

async fn install_repo_to_tracking(
    app: &tauri::AppHandle,
    preview: &InstallPlanView,
) -> Result<Vec<ManagedAppView>> {
    let repo = RepoRef::parse(&preview.repo_id)?;
    let runtime_config = runtime_config()?;
    let language = ui_language(&runtime_config);
    let store = ManifestStore::default()?;
    let current_app = store
        .load()?
        .apps
        .into_iter()
        .find(|installed| installed.id == repo.id());
    let selection_guard = preview
        .selection_guard
        .as_ref()
        .context("stale install preview: selection guard is missing")?;
    selection_guard.validate(current_app.as_ref())?;
    let target_channel = preview
        .target_policy
        .as_ref()
        .map(|policy| policy.channel)
        .unwrap_or_default();

    // Rebuild every release-derived field after confirmation. The frontend
    // returns the path-free preview as identity, never executable URLs or paths.
    let plan = build_install_plan(
        &repo.id(),
        Some(&preview.version),
        target_channel,
        None,
        None,
        None,
    )
    .await?;
    let rebuilt = InstallPlanView::from(&plan);
    ensure_install_preview_matches(preview, &rebuilt)?;
    let reporter = task_progress_reporter(app);
    install_from_plan(
        &plan,
        &store,
        None,
        Some(&runtime_config),
        language,
        reporter,
    )
    .await?;

    build_dashboard(app, 0).await
}

async fn mutate_release_policy_and_reload(
    app: &tauri::AppHandle,
    repo_input: &str,
    mutation: PolicyMutation,
) -> Result<Vec<ManagedAppView>> {
    let repo = RepoRef::parse(repo_input)?;
    ManifestStore::default()?.mutate_release_policy(&repo.id(), mutation)?;
    build_dashboard(app, 0).await
}

fn build_rollback_preview(repo_input: &str) -> Result<RollbackPreview> {
    let repo = RepoRef::parse(repo_input)?;
    let manifest = ManifestStore::default()?.load()?;
    let app = manifest
        .apps
        .iter()
        .find(|installed| installed.id == repo.id())
        .with_context(|| format!("no managed app matched {}", repo.id()))?;
    if !matches!(app.install_path_kind, InstallPathKind::ManagedPath) {
        anyhow::bail!("only managed-path installs can be rolled back: {}", app.id);
    }
    let snapshot = app
        .rollback
        .as_ref()
        .with_context(|| format!("{} does not have a rollback snapshot", app.id))?;
    Ok(RollbackPreview {
        repo_id: app.id.clone(),
        active_version: app.installed_version.clone(),
        snapshot_version: snapshot.version.clone(),
        snapshot_path: snapshot.snapshot_path.clone(),
    })
}

async fn rollback_repo_from_preview(
    app_handle: &tauri::AppHandle,
    preview: &RollbackPreview,
) -> Result<Vec<ManagedAppView>> {
    let repo = RepoRef::parse(&preview.repo_id)?;
    let store = ManifestStore::default()?;
    let manifest = store.load()?;
    let app = manifest
        .apps
        .iter()
        .find(|installed| installed.id == repo.id())
        .with_context(|| format!("no managed app matched {}", repo.id()))?;
    // Client-provided path data is used only for equality against the current
    // manifest. Core receives a guard constructed from that trusted record.
    let guard = rollback_guard_from_preview(app, preview)?;
    let runtime_config = runtime_config()?;
    let language = ui_language(&runtime_config);
    let reporter = task_progress_reporter(app_handle);
    core_rollback_repo_guarded(&store, &repo.id(), &guard, language, reporter)?
        .with_context(|| format!("no managed app matched {}", repo.id()))?;
    build_dashboard(app_handle, 0).await
}

async fn uninstall_repo_from_tracking(
    app: &tauri::AppHandle,
    repo_input: &str,
) -> Result<Vec<ManagedAppView>> {
    let repo = RepoRef::parse(repo_input)?;
    let runtime_config = runtime_config()?;
    let language = ui_language(&runtime_config);
    let store = ManifestStore::default()?;
    let reporter = task_progress_reporter(app);
    let removed = core_uninstall_repo(&store, &repo.id(), language, reporter)?;
    if removed.is_none() {
        anyhow::bail!("no managed app matched {}", repo.id());
    }

    build_dashboard(app, 0).await
}

async fn remove_tracked_repo_from_tracking(
    app: &tauri::AppHandle,
    repo_input: &str,
) -> Result<Vec<ManagedAppView>> {
    let repo = RepoRef::parse(repo_input)?;
    let store = TrackedRepoStore::default()?;
    let removed = store.remove(&repo.id())?;
    if !removed {
        anyhow::bail!("no tracked repo matched {}", repo.id());
    }

    build_dashboard(app, 0).await
}

#[tauri::command]
async fn adopt_system_install(
    app: tauri::AppHandle,
    repo_input: String,
) -> Result<Vec<ManagedAppView>, String> {
    adopt_system_install_in_tracking(&app, &repo_input)
        .await
        .map_err(format_error)
}

async fn bulk_remove_tracked_repos_from_tracking(
    app: &tauri::AppHandle,
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

    let apps = build_dashboard(app, 0).await?;
    Ok(BulkRemoveResultView {
        apps,
        removed_count: removed_ids.len(),
    })
}

async fn adopt_system_install_in_tracking(
    app: &tauri::AppHandle,
    repo_input: &str,
) -> Result<Vec<ManagedAppView>> {
    let repo = RepoRef::parse(repo_input)?;
    let store = ManifestStore::default()?;
    let _adopted = adopt_system_installer_app(&store, &repo)?;
    build_dashboard(app, 0).await
}

fn task_progress_reporter(app: &tauri::AppHandle) -> Option<ProgressReporter> {
    let app = app.clone();
    Some(Arc::new(move |progress: TaskProgress| {
        let _ = app.emit(TASK_PROGRESS_EVENT, progress);
    }))
}

fn ui_language(config: &Config) -> Language {
    match config.language.as_deref() {
        Some("zh-CN") => Language::ZhCn,
        _ => Language::En,
    }
}

fn open_url_in_system(url: &str) -> Result<()> {
    let parsed = url::Url::parse(url).context("failed to parse URL")?;
    validate_github_url(&parsed)?;
    open_target_with_platform(url)
}

fn open_path_in_system(path: &str) -> Result<()> {
    open_target_with_platform(path)
}

fn open_install_location_in_system(path: &str, install_path_kind: InstallPathKind) -> Result<()> {
    let target = resolve_open_install_location_target(Path::new(path), install_path_kind)?;
    let target = target.to_string_lossy().into_owned();
    open_target_with_platform(&target)
}

fn open_installer_folder_in_system(path: &str) -> Result<()> {
    let target = resolve_installer_folder_target(Path::new(path))?;
    let target = target.to_string_lossy().into_owned();
    open_target_with_platform(&target)
}

fn resolve_open_install_location_target(
    path: &Path,
    install_path_kind: InstallPathKind,
) -> Result<PathBuf> {
    match install_path_kind {
        InstallPathKind::ManagedPath => {
            if path.is_dir() {
                Ok(path.to_path_buf())
            } else {
                path.parent()
                    .map(Path::to_path_buf)
                    .ok_or_else(|| anyhow::anyhow!("install path {} has no parent", path.display()))
            }
        }
        InstallPathKind::SystemInstaller => Ok(path.to_path_buf()),
        InstallPathKind::Unknown => anyhow::bail!("install path kind is unknown"),
    }
}

fn resolve_installer_folder_target(path: &Path) -> Result<PathBuf> {
    path.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow::anyhow!("installer path {} has no parent", path.display()))
}

fn open_app_in_system(repo_input: &str) -> Result<()> {
    let repo = RepoRef::parse(repo_input)?;
    let store = ManifestStore::default()?;
    let manifest = store.load()?;
    let Some(app) = manifest.apps.into_iter().find(|app| app.id == repo.id()) else {
        anyhow::bail!("no managed app matched {}", repo.id());
    };

    let Some(launch_path) = resolve_launch_path(&app) else {
        anyhow::bail!("no launch target available for {}", app.id);
    };

    launch_target_with_platform(&launch_path)
}

fn open_system_uninstall_settings_in_system() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        open_with_windows_shell("ms-settings:appsfeatures")?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        anyhow::bail!("system uninstall settings are only available on Windows");
    }
}

fn open_notification_settings_in_system() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        open_with_windows_shell("ms-settings:notifications")?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        anyhow::bail!("notification settings are only available on Windows");
    }
}

const MAX_RELEASE_PAGES: u32 = 20;
const MAX_RELEASES: usize = 2_000;

/// Loads a bounded catalog while letting the caller define when enough release
/// metadata has arrived. The same helper powers version listing and policy
/// selection so draft-heavy pages and pins beyond page one behave consistently.
async fn load_release_catalog_with<F, Fut, S>(
    mut fetch_page: F,
    mut should_stop: S,
) -> Result<Vec<Release>>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<ReleasePage>>,
    S: FnMut(&[Release]) -> bool,
{
    let mut page_number = 1;
    let mut releases = Vec::new();
    let mut page_tag_sets = HashSet::new();

    loop {
        let page = fetch_page(page_number).await?;
        if page.releases.is_empty() {
            break;
        }

        let mut tag_set = page
            .releases
            .iter()
            .map(|release| release.tag_name.clone())
            .collect::<Vec<_>>();
        tag_set.sort();
        tag_set.dedup();
        if !page_tag_sets.insert(tag_set) {
            anyhow::bail!("repeated release page tag set at page {page_number}");
        }
        if releases.len() + page.releases.len() > MAX_RELEASES {
            anyhow::bail!("release catalog exceeded maximum {MAX_RELEASES} releases");
        }

        let has_next_page = page.has_next_page;
        releases.extend(page.releases);
        if should_stop(&releases) || !has_next_page {
            break;
        }
        if page_number >= MAX_RELEASE_PAGES {
            anyhow::bail!("release catalog exceeded maximum {MAX_RELEASE_PAGES} pages");
        }
        page_number += 1;
    }

    Ok(releases)
}

fn release_catalog_complete_for_selection(
    releases: &[Release],
    installed: Option<&InstalledApp>,
    manual_version: Option<&str>,
    new_install_channel: ReleaseChannel,
) -> bool {
    let new_install_policy = ReleasePolicy {
        channel: new_install_channel,
        ..ReleasePolicy::default()
    };
    let policy = installed
        .map(|app| &app.release_policy)
        .unwrap_or(&new_install_policy);
    let current_version = installed.map(|app| app.installed_version.as_str());
    let Ok(selection) = ReleaseSelector::select(releases, policy, current_version, manual_version)
    else {
        return false;
    };

    installed.is_none_or(|app| {
        app.installed_version == selection.release.tag_name
            || releases
                .iter()
                .any(|release| release.tag_name == app.installed_version)
    })
}

fn select_tracked_release(releases: &[Release]) -> Result<ReleaseSelection> {
    Ok(ReleaseSelector::select(
        releases,
        &ReleasePolicy::default(),
        None,
        None,
    )?)
}

async fn load_release_catalog_for_selection(
    client: &ReleaseClient,
    repo: &RepoRef,
    installed: Option<&InstalledApp>,
    manual_version: Option<&str>,
    new_install_channel: ReleaseChannel,
) -> Result<Vec<Release>> {
    load_release_catalog_with(
        |page| client.releases_page(repo, page, 100),
        |releases| {
            release_catalog_complete_for_selection(
                releases,
                installed,
                manual_version,
                new_install_channel,
            )
        },
    )
    .await
}

fn release_versions_from_catalog(releases: &[Release]) -> Vec<ReleaseVersionView> {
    releases
        .iter()
        .filter(|release| !release.draft)
        .take(100)
        .map(|release| ReleaseVersionView {
            tag_name: release.tag_name.clone(),
            name: release.name.clone(),
            prerelease: release.prerelease,
            published_at: release
                .published_at
                .as_ref()
                .map(|published_at| published_at.to_rfc3339()),
        })
        .collect()
}

fn build_plan_from_releases(
    repo: &RepoRef,
    releases: &[Release],
    installed: Option<&InstalledApp>,
    manual_version: Option<&str>,
    new_install_channel: ReleaseChannel,
    matcher: &AssetMatcher,
    integrity: IntegrityPlan,
    language: Language,
) -> Result<InstallPlan> {
    let policy = installed
        .map(|app| app.release_policy.clone())
        .unwrap_or_else(|| ReleasePolicy {
            channel: new_install_channel,
            ..ReleasePolicy::default()
        });
    let selection = ReleaseSelector::select(
        releases,
        &policy,
        installed.map(|app| app.installed_version.as_str()),
        manual_version,
    )?;
    let matched = matcher.select_best(&selection.release)?;
    let mut plan = InstallPlan::from_match(repo, &selection.release, &matched, language)
        .with_release_direction(selection.direction)
        .with_integrity(integrity);
    if let Some(app) = installed {
        plan = plan.with_selection_guard(InstallSelectionGuard::from_app(app));
    } else {
        plan = plan
            .with_selection_guard(InstallSelectionGuard::ExpectedAbsent)
            .with_target_policy(policy);
    }
    Ok(plan)
}

fn rollback_guard_from_preview(
    app: &InstalledApp,
    preview: &RollbackPreview,
) -> Result<RollbackGuard> {
    let snapshot = app
        .rollback
        .as_ref()
        .with_context(|| format!("{} does not have a rollback snapshot", app.id))?;
    if app.id != preview.repo_id
        || app.installed_version != preview.active_version
        || snapshot.version != preview.snapshot_version
        || snapshot.snapshot_path != preview.snapshot_path
    {
        anyhow::bail!("stale rollback preview for {}", app.id);
    }
    Ok(RollbackGuard::from_app(app))
}

async fn build_install_plan(
    repo_input: &str,
    manual_version: Option<&str>,
    new_install_channel: ReleaseChannel,
    release_fixture: Option<PathBuf>,
    os: Option<UiOs>,
    arch: Option<UiArch>,
) -> Result<InstallPlan> {
    let repo = RepoRef::parse(repo_input)?;
    let runtime_config = runtime_config()?;
    let language = ui_language(&runtime_config);
    let manifest = ManifestStore::default()?.load()?;
    let installed = manifest.apps.iter().find(|app| app.id == repo.id());
    let (releases, client) = match release_fixture {
        Some(path) => (read_fixture_releases(&path)?, None),
        None => {
            let client = release_client(Some(&runtime_config))?;
            let releases = load_release_catalog_for_selection(
                &client,
                &repo,
                installed,
                manual_version,
                new_install_channel,
            )
            .await
            .with_context(|| format!("failed to fetch releases for {}", repo.id()))?;
            (releases, Some(client))
        }
    };

    let matcher = match (os, arch) {
        (Some(os), Some(arch)) => AssetMatcher::new(os.into(), arch.into()),
        _ => AssetMatcher::current(),
    };
    let mut plan = build_plan_from_releases(
        &repo,
        &releases,
        installed,
        manual_version,
        new_install_channel,
        &matcher,
        IntegrityPlan::default(),
        language,
    )?;
    if let Some(client) = client.as_ref() {
        let release = releases
            .iter()
            .find(|release| release.tag_name == plan.version)
            .context("selected release disappeared from the catalog")?;
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == plan.asset_name)
            .context("selected release asset disappeared from the catalog")?;
        plan.integrity = IntegrityVerifier::discover(client, release, asset).await?;
    }
    if plan.integrity.expected_sha256.is_none() {
        plan.requires_user_confirmation = true;
        plan.notes.push(tr_owned(
            language,
            "No upstream SHA-256 checksum was found; verify the artifact source before continuing.",
            "未找到上游 SHA-256 校验值；继续前请确认安装文件来源。",
        ));
    }
    Ok(plan)
}

fn default_install_path(repo: &RepoRef) -> PathBuf {
    let runtime_config = runtime_config().ok();
    let base_dir = effective_install_root(runtime_config.as_ref(), install_root_fallback());
    base_dir
        .join("apps")
        .join(format!("{}-{}", repo.owner, repo.name))
}

fn config_store() -> Result<ConfigStore> {
    ConfigStore::from_env_or_default()
}

pub(crate) fn runtime_config() -> Result<Config> {
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

fn non_empty_config_value(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn github_connectivity_result(
    ok: bool,
    message: String,
    problem: &str,
    used_token: bool,
    used_proxy: bool,
) -> GithubConnectivityTestResult {
    GithubConnectivityTestResult {
        ok,
        message,
        problem: problem.to_string(),
        used_token,
        used_proxy,
    }
}

fn classify_connectivity_problem(message: &str, used_proxy: bool) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("proxy") {
        return "proxy";
    }
    if lower.contains("401 unauthorized")
        || lower.contains("bad credentials")
        || lower.contains("authentication")
        || lower.contains("requires authentication")
    {
        return "auth";
    }
    if is_rate_limit_connectivity_message(&lower) {
        return "rateLimit";
    }
    if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("failed to connect")
        || lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("dns")
        || lower.contains("failed to request")
    {
        return if used_proxy { "proxy" } else { "network" };
    }
    "unknown"
}

fn is_rate_limit_connectivity_message(lower_message: &str) -> bool {
    lower_message.contains("api rate limit exceeded")
        || lower_message.contains("secondary rate limit")
        || lower_message.contains("rate limit remaining 0")
        || lower_message.contains("x-ratelimit-remaining: 0")
}

fn sanitize_connectivity_message(
    message: &str,
    github_token: Option<&str>,
    proxy_url: Option<&str>,
) -> String {
    let mut sanitized = message.replace('\n', " ");

    if let Some(token) = github_token.filter(|value| !value.trim().is_empty()) {
        sanitized = sanitized.replace(token, "[token]");
    }

    if let Some(proxy) = proxy_url.filter(|value| !value.trim().is_empty()) {
        sanitized = sanitized.replace(proxy, "[proxy]");

        // reqwest/url 错误有时只包含代理的 host 或 host:port；这里也做替换，避免泄漏本地代理地址。
        if let Ok(parsed) = url::Url::parse(proxy) {
            if let Some(host) = parsed.host_str() {
                if let Some(port) = parsed.port() {
                    sanitized = sanitized.replace(&format!("{host}:{port}"), "[proxy]");
                }
                sanitized = sanitized.replace(host, "[proxy]");
            }
            if !parsed.username().is_empty() {
                sanitized = sanitized.replace(parsed.username(), "[proxy-user]");
            }
            if let Some(password) = parsed.password() {
                sanitized = sanitized.replace(password, "[proxy-password]");
            }
        }
    }

    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        "GitHub connectivity check failed".to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopConfig {
    github_token: Option<String>,
    proxy_url: Option<String>,
    install_root: Option<PathBuf>,
    effective_install_root: Option<PathBuf>,
    language: Option<String>,
    theme_mode: Option<String>,
    background_check_enabled: Option<bool>,
    check_interval_minutes: Option<u32>,
    tray_hint_shown: Option<bool>,
    download_acceleration_enabled: Option<bool>,
    download_max_connections: Option<u8>,
    autostart_enabled: Option<bool>,
}

impl From<DesktopConfig> for Config {
    fn from(value: DesktopConfig) -> Self {
        Self {
            github_token: value.github_token,
            proxy_url: value.proxy_url,
            install_root: value.install_root,
            language: value.language,
            theme_mode: value.theme_mode,
            background_check_enabled: value.background_check_enabled,
            check_interval_minutes: value.check_interval_minutes,
            tray_hint_shown: value.tray_hint_shown,
            download_acceleration_enabled: value.download_acceleration_enabled,
            download_max_connections: value.download_max_connections,
            autostart_enabled: value.autostart_enabled,
        }
    }
}

fn desktop_config_from_runtime(value: Config) -> DesktopConfig {
    let effective_install_root = effective_install_root(Some(&value), install_root_fallback());
    let download_acceleration_enabled = download_acceleration_enabled(Some(&value));
    let download_max_connections = download_max_connections(Some(&value));
    DesktopConfig {
        github_token: value.github_token,
        proxy_url: value.proxy_url,
        install_root: value.install_root,
        effective_install_root: Some(effective_install_root),
        language: value.language,
        theme_mode: value.theme_mode,
        background_check_enabled: value.background_check_enabled,
        check_interval_minutes: value.check_interval_minutes,
        tray_hint_shown: value.tray_hint_shown,
        download_acceleration_enabled: Some(download_acceleration_enabled),
        download_max_connections: Some(download_max_connections),
        autostart_enabled: value.autostart_enabled,
    }
}

/// 重启后台检查任务（abort 旧任务 → 读最新 config → 决定是否 spawn 新任务）
async fn restart_background_checker(app: tauri::AppHandle) {
    // abort 旧任务
    {
        let mut guard = BACKGROUND_TASK.lock().await;
        if let Some(handle) = guard.take() {
            handle.abort();
        }
    }

    // 读最新 config 决定是否 spawn 新任务
    let runtime_config = runtime_config().ok();
    let bg_enabled = background_check_enabled(runtime_config.as_ref());
    let interval = check_interval_minutes(runtime_config.as_ref()) as u64;

    if bg_enabled && interval > 0 {
        let handle = background_check::spawn_background_checker(app, interval);
        let mut guard = BACKGROUND_TASK.lock().await;
        *guard = Some(handle);
    }
}

fn install_root_fallback() -> Option<PathBuf> {
    ManifestStore::default_path()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
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

pub(crate) fn tr_owned(language: Language, english: &'static str, chinese: &'static str) -> String {
    match language {
        Language::En => english.to_string(),
        Language::ZhCn => chinese.to_string(),
    }
}

fn open_target_with_platform(target: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        open_with_windows_shell(target).with_context(|| format!("failed to open {target}"))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(target)
            .spawn()
            .with_context(|| format!("failed to open {target}"))?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(target)
            .spawn()
            .with_context(|| format!("failed to open {target}"))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    {
        anyhow::bail!("opening URLs is not supported on this platform");
    }
}

fn launch_target_with_platform(target: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        open_with_windows_shell(target.as_os_str())
            .with_context(|| format!("failed to launch {}", target.display()))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        if target.is_dir()
            && target
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("app"))
                .unwrap_or(false)
        {
            Command::new("open")
                .arg(target)
                .spawn()
                .with_context(|| format!("failed to launch {}", target.display()))?;
            return Ok(());
        }

        Command::new(target)
            .spawn()
            .with_context(|| format!("failed to launch {}", target.display()))?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new(target)
            .spawn()
            .with_context(|| format!("failed to launch {}", target.display()))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    {
        anyhow::bail!("launching apps is not supported on this platform");
    }
}

#[cfg(target_os = "windows")]
fn open_with_windows_shell(target: impl AsRef<std::ffi::OsStr>) -> Result<()> {
    let wide_target = os_str_to_wide_null(target.as_ref());
    let result = unsafe {
        ShellExecuteW(
            None,
            None,
            PCWSTR::from_raw(wide_target.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };

    // Windows documents ShellExecuteW return values <= 32 as failure codes.
    let result_code = result.0 as usize;
    if result_code <= 32 {
        anyhow::bail!(
            "Windows shell failed to open {}",
            target.as_ref().to_string_lossy()
        );
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn os_str_to_wide_null(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn resolve_launch_path(app: &releasedock_core::manifest::InstalledApp) -> Option<PathBuf> {
    if let Some(launch_path) = &app.launch_path {
        if launch_path.exists() {
            return Some(launch_path.clone());
        }
    }

    infer_launch_target(
        &app.install_path,
        app.install_type,
        &app.name,
        &app.asset_name,
    )
}

fn management_kind_for_app(
    app: &releasedock_core::manifest::InstalledApp,
) -> InstallManagementKind {
    match app.install_type {
        InstallType::AppImage
        | InstallType::PortableArchive
        | InstallType::Archive
        | InstallType::Executable => InstallManagementKind::ManagedLocal,
        InstallType::LinuxPackage => InstallManagementKind::SystemPackage,
        InstallType::WindowsInstaller => InstallManagementKind::ExternalInstaller,
        InstallType::Unknown => {
            if matches!(app.install_path_kind, InstallPathKind::ManagedPath) {
                InstallManagementKind::ManagedLocal
            } else {
                InstallManagementKind::ExternalInstaller
            }
        }
    }
}

fn read_fixture_releases(path: &PathBuf) -> Result<Vec<Release>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read release fixture {}", path.display()))?;
    let fixture: DesktopReleaseFixture = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse release fixture {}", path.display()))?;
    Ok(match fixture {
        DesktopReleaseFixture::Many(releases) => releases,
        DesktopReleaseFixture::One(release) => vec![release],
    })
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DesktopReleaseFixture {
    Many(Vec<Release>),
    One(Release),
}

fn format_error(error: anyhow::Error) -> String {
    error
        .chain()
        .map(|cause| cause.to_string().replace('\n', " ").trim().to_string())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(": ")
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::VecDeque, future::ready, path::PathBuf, rc::Rc};

    use releasedock_core::{
        asset_matcher::{Architecture, AssetMatcher, InstallType, OperatingSystem},
        config::Language,
        install_plan::InstallSelectionGuard,
        integrity::{IntegrityPlan, IntegrityStatus},
        manifest::{InstallPathKind, InstalledApp, RollbackSnapshot},
        release::{Release, ReleaseAsset, ReleasePage},
        release_policy::{ReleaseChannel, ReleaseDirection, ReleasePolicy, ReleaseSelector},
        repo::RepoRef,
    };

    use super::{
        InstallPlanView, RollbackPreview, background_start_from_args, build_dashboard_work_items,
        build_local_dashboard_views, build_plan_from_releases, classify_connectivity_problem,
        ensure_install_preview_matches, load_release_catalog_with, management_kind_for_app,
        preview_install, release_catalog_complete_for_selection, release_versions_from_catalog,
        render_app, resolve_installer_folder_target, resolve_open_install_location_target,
        rollback_guard_from_preview, sanitize_connectivity_message, select_tracked_release,
        validate_github_url,
    };

    #[test]
    fn single_instance_plugin_is_registered_before_other_plugins() {
        let source = include_str!("main.rs");
        let single_instance_index = source
            .find(".plugin(tauri_plugin_single_instance::init")
            .expect("desktop startup should register the single-instance plugin");
        let notification_index = source
            .find(".plugin(tauri_plugin_notification::init")
            .expect("desktop startup should keep the notification plugin");

        assert!(
            single_instance_index < notification_index,
            "single-instance must be registered before other plugins so duplicate launches are intercepted"
        );
    }

    #[test]
    fn background_start_is_detected_from_autostart_arguments() {
        assert!(background_start_from_args([
            "releasedock.exe",
            "--background",
        ]));
        assert!(!background_start_from_args(["releasedock.exe", "--gui"]));
    }

    #[test]
    fn desktop_startup_registers_basic_windows_lifecycle_plugins() {
        let source = include_str!("main.rs");
        let builder_start = source
            .find("tauri::Builder::default()")
            .expect("desktop startup should create a Tauri builder");
        let builder_end = source
            .find(".run(tauri::generate_context!())")
            .expect("desktop startup should run the Tauri builder");
        let builder_source = &source[builder_start..builder_end];
        assert!(
            builder_source.contains("tauri_plugin_window_state::Builder::default().build()"),
            "desktop startup should persist and restore the main window size and position"
        );
        assert!(
            builder_source.contains("tauri_plugin_autostart::init"),
            "desktop startup should register the optional autostart plugin"
        );
        assert!(
            builder_source.contains("is_background_start")
                && builder_source.contains("is_main_window_visible"),
            "desktop startup should expose hidden-start and restore-race queries"
        );
        assert!(
            builder_source.contains("MacosLauncher::LaunchAgent"),
            "autostart should use the explicit LaunchAgent strategy on macOS"
        );
    }

    #[test]
    fn restoring_main_window_also_unminimizes_it() {
        let source = include_str!("main.rs");
        let restore_function_index = source
            .find("fn restore_main_window")
            .expect("desktop startup should centralize main window restoration");
        let restore_function_end = source[restore_function_index..]
            .find("fn should_run_cli")
            .map(|offset| restore_function_index + offset)
            .expect("restore_main_window should stay before should_run_cli");
        let restore_function = &source[restore_function_index..restore_function_end];
        assert!(
            restore_function.contains(".show()")
                && restore_function.contains(".unminimize()")
                && restore_function.contains(".set_focus()")
                && restore_function.contains("restore-main-window"),
            "restoring the main window should show, unminimize, focus it, and notify the frontend"
        );
    }

    #[test]
    fn tray_restore_uses_shared_main_window_restore_helper() {
        let tray_source = include_str!("tray.rs");
        assert!(
            tray_source.contains("crate::restore_main_window(app)")
                && tray_source.contains("crate::restore_main_window(tray.app_handle())"),
            "tray menu and left-click restore should use the same unminimize-aware helper as duplicate launches"
        );
        assert!(
            !tray_source.contains("fn show_window"),
            "tray should not keep a second restore implementation that can drift from duplicate-launch behavior"
        );
    }

    #[test]
    fn notification_permission_commands_are_registered_and_open_windows_settings() {
        let source = include_str!("main.rs");
        let handler_start = source
            .find("tauri::generate_handler![")
            .expect("desktop startup should register invoke handlers");
        let handler_end = source[handler_start..]
            .find("])")
            .map(|offset| handler_start + offset)
            .expect("invoke handler list should close");
        let handlers = &source[handler_start..handler_end];
        assert!(
            handlers.contains("request_notification_permission")
                && handlers.contains("open_notification_settings"),
            "notification permission request and settings commands should be exposed to the frontend"
        );
        assert!(
            handlers.contains("load_local_dashboard"),
            "startup should expose a local-only dashboard fallback"
        );
        assert!(
            handlers.contains("search_github_repos"),
            "dashboard should expose GitHub repository discovery"
        );
        assert!(
            source.contains("ms-settings:notifications"),
            "Windows notification settings should open the OS notification settings page"
        );
    }

    #[test]
    fn local_dashboard_keeps_manifest_and_tracking_records_without_release_data() {
        let installed = InstalledApp::new(
            "owner/installed",
            "Installed",
            "v1.2.3",
            "installed.AppImage",
            PathBuf::from("/managed/installed.AppImage"),
        );
        let work_items = build_dashboard_work_items(
            vec![installed],
            vec![super::TrackedRepo {
                repo_id: "owner/tracked".to_string(),
            }],
            ["owner/installed".to_string()].into_iter().collect(),
        );

        let views = build_local_dashboard_views(
            work_items,
            Language::ZhCn,
            "GitHub 暂时不可用，请修复连接后重试。",
            &std::collections::HashMap::new(),
        );

        assert_eq!(views.len(), 2);
        assert_eq!(views[0].id, "owner/installed");
        assert_eq!(views[0].current_version, "v1.2.3");
        assert!(matches!(views[0].status, super::AppStatus::Failed));
        assert_eq!(views[1].id, "owner/tracked");
        assert_eq!(views[1].current_version, "未安装");
        assert_eq!(
            views[1].release_note.as_deref(),
            Some("GitHub 暂时不可用，请修复连接后重试。")
        );
    }

    #[test]
    fn explicit_version_plan_carries_core_direction_integrity_and_selection_guard() {
        let repo = RepoRef::parse("owner/project").unwrap();
        let releases = vec![
            release_with_asset("v3.0.0", false),
            release_with_asset("v2.0.0", false),
            release_with_asset("v1.0.0", false),
        ];
        let installed = InstalledApp::new(
            "owner/project",
            "project",
            "v2.0.0",
            "project-linux-x86_64.AppImage",
            PathBuf::from("/managed/project.AppImage"),
        );
        let integrity = IntegrityPlan {
            expected_sha256: Some("a".repeat(64)),
            checksum_asset_name: Some("SHA256SUMS".to_string()),
            status: IntegrityStatus::RecordedOnly,
        };

        let plan = build_plan_from_releases(
            &repo,
            &releases,
            Some(&installed),
            Some("v1.0.0"),
            ReleaseChannel::Stable,
            &AssetMatcher::new(OperatingSystem::Linux, Architecture::X64),
            integrity.clone(),
            Language::En,
        )
        .unwrap();

        assert_eq!(plan.version, "v1.0.0");
        assert_eq!(plan.release_direction, ReleaseDirection::Downgrade);
        assert_eq!(plan.integrity, integrity);
        assert!(plan.selection_guard.is_some());
        assert!(plan.target_policy.is_none());
    }

    #[test]
    fn new_install_plan_carries_selected_channel_as_target_policy() {
        let repo = RepoRef::parse("owner/project").unwrap();
        let releases = vec![release_with_asset("v2.0.0-beta.1", true)];

        let plan = build_plan_from_releases(
            &repo,
            &releases,
            None,
            Some("v2.0.0-beta.1"),
            ReleaseChannel::Prerelease,
            &AssetMatcher::new(OperatingSystem::Linux, Architecture::X64),
            IntegrityPlan::default(),
            Language::En,
        )
        .unwrap();

        assert_eq!(
            plan.target_policy.as_ref().unwrap().channel,
            ReleaseChannel::Prerelease
        );
        assert_eq!(
            plan.selection_guard,
            Some(InstallSelectionGuard::ExpectedAbsent)
        );
    }

    #[test]
    fn desktop_install_preview_omits_download_and_install_paths() {
        let repo = RepoRef::parse("owner/project").unwrap();
        let releases = vec![release_with_asset("v2.0.0", false)];
        let plan = build_plan_from_releases(
            &repo,
            &releases,
            None,
            Some("v2.0.0"),
            ReleaseChannel::Stable,
            &AssetMatcher::new(OperatingSystem::Linux, Architecture::X64),
            IntegrityPlan::default(),
            Language::En,
        )
        .unwrap();

        let serialized = serde_json::to_value(InstallPlanView::from(&plan)).unwrap();

        assert!(serialized.get("download_url").is_none());
        assert!(serialized.get("repo_url").is_none());
        assert!(serialized.get("install_path").is_none());
        assert_eq!(serialized["version"], "v2.0.0");
    }

    #[test]
    fn install_preview_round_trips_and_rejects_every_security_relevant_change() {
        let repo = RepoRef::parse("owner/project").unwrap();
        let releases = vec![release_with_asset("v2.0.0", false)];
        let plan = build_plan_from_releases(
            &repo,
            &releases,
            None,
            Some("v2.0.0"),
            ReleaseChannel::Stable,
            &AssetMatcher::new(OperatingSystem::Linux, Architecture::X64),
            IntegrityPlan::default(),
            Language::En,
        )
        .unwrap();
        let preview = InstallPlanView::from(&plan);
        let restored: InstallPlanView =
            serde_json::from_value(serde_json::to_value(&preview).unwrap()).unwrap();
        assert_eq!(restored, preview);

        let mut notes_only = preview.clone();
        notes_only.notes.push("server copy may change".to_string());
        assert!(ensure_install_preview_matches(&preview, &notes_only).is_ok());

        let mut changed = Vec::new();
        let mut value = preview.clone();
        value.repo_id = "other/project".to_string();
        changed.push(value);
        let mut value = preview.clone();
        value.version = "v3.0.0".to_string();
        changed.push(value);
        let mut value = preview.clone();
        value.asset_name = "other.AppImage".to_string();
        changed.push(value);
        let mut value = preview.clone();
        value.install_type = InstallType::Archive;
        changed.push(value);
        let mut value = preview.clone();
        value.management_kind = super::InstallManagementKind::SystemPackage;
        changed.push(value);
        let mut value = preview.clone();
        value.system_package_manager = Some(super::SystemPackageManager::Debian);
        changed.push(value);
        let mut value = preview.clone();
        value.requires_user_confirmation = !value.requires_user_confirmation;
        changed.push(value);
        let mut value = preview.clone();
        value.integrity.expected_sha256 = Some("a".repeat(64));
        changed.push(value);
        let mut value = preview.clone();
        value.release_direction = ReleaseDirection::Downgrade;
        changed.push(value);
        let mut value = preview.clone();
        value.selection_guard = None;
        changed.push(value);
        let mut value = preview.clone();
        value.target_policy = Some(ReleasePolicy {
            channel: ReleaseChannel::Prerelease,
            ..ReleasePolicy::default()
        });
        changed.push(value);

        for rebuilt in changed {
            let error = ensure_install_preview_matches(&preview, &rebuilt).unwrap_err();
            assert!(error.to_string().contains("stale install preview"));
        }
    }

    #[test]
    fn production_preview_command_accepts_only_selection_inputs() {
        drop(preview_install(
            "owner/project".to_string(),
            Some("v2.0.0".to_string()),
            Some(ReleaseChannel::Stable),
        ));
    }

    #[tokio::test]
    async fn release_catalog_loader_collects_one_hundred_non_draft_across_pages() {
        let mut first = (0..75)
            .map(|index| release_with_asset(&format!("v1.{index}.0"), false))
            .collect::<Vec<_>>();
        first.extend((0..25).map(|index| {
            let mut release = release_with_asset(&format!("draft-{index}"), false);
            release.draft = true;
            release
        }));
        let second = (0..40)
            .map(|index| release_with_asset(&format!("v0.{index}.0"), false))
            .collect::<Vec<_>>();
        let pages = Rc::new(RefCell::new(VecDeque::from([
            ReleasePage {
                releases: first,
                has_next_page: true,
            },
            ReleasePage {
                releases: second,
                has_next_page: false,
            },
        ])));
        let fetch_count = Rc::new(RefCell::new(0));

        let catalog = load_release_catalog_with(
            {
                let pages = pages.clone();
                let fetch_count = fetch_count.clone();
                move |_| {
                    *fetch_count.borrow_mut() += 1;
                    ready(Ok::<_, anyhow::Error>(
                        pages.borrow_mut().pop_front().unwrap(),
                    ))
                }
            },
            |releases| releases.iter().filter(|release| !release.draft).count() >= 100,
        )
        .await
        .unwrap();

        assert_eq!(*fetch_count.borrow(), 2);
        assert_eq!(release_versions_from_catalog(&catalog).len(), 100);
    }

    #[tokio::test]
    async fn release_catalog_loader_rejects_repeated_pages() {
        let page = ReleasePage {
            releases: vec![release_with_asset("v1.0.0", false)],
            has_next_page: true,
        };
        let pages = Rc::new(RefCell::new(VecDeque::from([page.clone(), page])));

        let error = load_release_catalog_with(
            {
                let pages = pages.clone();
                move |_| {
                    ready(Ok::<_, anyhow::Error>(
                        pages.borrow_mut().pop_front().unwrap(),
                    ))
                }
            },
            |_| false,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("repeated release page"));
    }

    #[tokio::test]
    async fn release_catalog_loader_is_bounded_to_twenty_pages() {
        let pages = Rc::new(RefCell::new(VecDeque::from(
            (1..=20)
                .map(|page| ReleasePage {
                    releases: vec![release_with_asset(&format!("v{page}.0.0"), false)],
                    has_next_page: true,
                })
                .collect::<Vec<_>>(),
        )));

        let error = load_release_catalog_with(
            {
                let pages = pages.clone();
                move |_| {
                    ready(Ok::<_, anyhow::Error>(
                        pages.borrow_mut().pop_front().unwrap(),
                    ))
                }
            },
            |_| false,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("maximum 20 pages"));
    }

    #[tokio::test]
    async fn policy_catalog_loader_reaches_a_pin_beyond_the_first_page() {
        let mut installed = InstalledApp::new(
            "owner/project",
            "project",
            "v3.0.0",
            "project-linux-x86_64.AppImage",
            PathBuf::from("/managed/project.AppImage"),
        );
        installed.release_policy.pinned_version = Some("v1.0.0".to_string());
        let pages = Rc::new(RefCell::new(VecDeque::from([
            ReleasePage {
                releases: vec![
                    release_with_asset("v3.0.0", false),
                    release_with_asset("v2.0.0", false),
                ],
                has_next_page: true,
            },
            ReleasePage {
                releases: vec![release_with_asset("v1.0.0", false)],
                has_next_page: false,
            },
        ])));

        let catalog = load_release_catalog_with(
            {
                let pages = pages.clone();
                move |_| {
                    ready(Ok::<_, anyhow::Error>(
                        pages.borrow_mut().pop_front().unwrap(),
                    ))
                }
            },
            |releases| {
                release_catalog_complete_for_selection(
                    releases,
                    Some(&installed),
                    None,
                    ReleaseChannel::Stable,
                )
            },
        )
        .await
        .unwrap();
        let selection = ReleaseSelector::select(
            &catalog,
            &installed.release_policy,
            Some(&installed.installed_version),
            None,
        )
        .unwrap();

        assert_eq!(selection.release.tag_name, "v1.0.0");
        assert_eq!(selection.direction, ReleaseDirection::Downgrade);
    }

    #[tokio::test]
    async fn automatic_stable_selection_continues_past_a_prerelease_only_page() {
        let installed = InstalledApp::new(
            "owner/project",
            "project",
            "v1.0.0",
            "project-linux-x86_64.AppImage",
            PathBuf::from("/managed/project.AppImage"),
        );
        let pages = Rc::new(RefCell::new(VecDeque::from([
            ReleasePage {
                releases: vec![release_with_asset("v3.0.0-beta.1", true)],
                has_next_page: true,
            },
            ReleasePage {
                releases: vec![
                    release_with_asset("v2.0.0", false),
                    release_with_asset("v1.0.0", false),
                ],
                has_next_page: false,
            },
        ])));

        let catalog = load_release_catalog_with(
            {
                let pages = pages.clone();
                move |_| {
                    ready(Ok::<_, anyhow::Error>(
                        pages.borrow_mut().pop_front().unwrap(),
                    ))
                }
            },
            |releases| {
                release_catalog_complete_for_selection(
                    releases,
                    Some(&installed),
                    None,
                    ReleaseChannel::Stable,
                )
            },
        )
        .await
        .unwrap();
        let selection = ReleaseSelector::select(
            &catalog,
            &installed.release_policy,
            Some(&installed.installed_version),
            None,
        )
        .unwrap();

        assert_eq!(selection.release.tag_name, "v2.0.0");
        assert_eq!(selection.direction, ReleaseDirection::Upgrade);
    }

    #[tokio::test]
    async fn automatic_selection_continues_when_the_first_page_is_ignored() {
        let mut installed = InstalledApp::new(
            "owner/project",
            "project",
            "v1.0.0",
            "project-linux-x86_64.AppImage",
            PathBuf::from("/managed/project.AppImage"),
        );
        installed.release_policy.ignored_versions = vec!["v3.0.0".to_string()];
        let pages = Rc::new(RefCell::new(VecDeque::from([
            ReleasePage {
                releases: vec![release_with_asset("v3.0.0", false)],
                has_next_page: true,
            },
            ReleasePage {
                releases: vec![
                    release_with_asset("v2.0.0", false),
                    release_with_asset("v1.0.0", false),
                ],
                has_next_page: false,
            },
        ])));

        let catalog = load_release_catalog_with(
            {
                let pages = pages.clone();
                move |_| {
                    ready(Ok::<_, anyhow::Error>(
                        pages.borrow_mut().pop_front().unwrap(),
                    ))
                }
            },
            |releases| {
                release_catalog_complete_for_selection(
                    releases,
                    Some(&installed),
                    None,
                    ReleaseChannel::Stable,
                )
            },
        )
        .await
        .unwrap();
        let selection = ReleaseSelector::select(
            &catalog,
            &installed.release_policy,
            Some(&installed.installed_version),
            None,
        )
        .unwrap();

        assert_eq!(selection.release.tag_name, "v2.0.0");
    }

    #[tokio::test]
    async fn tracked_selection_continues_past_draft_and_prerelease_only_pages() {
        let mut draft = release_with_asset("v4.0.0", false);
        draft.draft = true;
        let pages = Rc::new(RefCell::new(VecDeque::from([
            ReleasePage {
                releases: vec![draft, release_with_asset("v3.0.0-beta.1", true)],
                has_next_page: true,
            },
            ReleasePage {
                releases: vec![release_with_asset("v2.0.0", false)],
                has_next_page: false,
            },
        ])));

        let catalog = load_release_catalog_with(
            {
                let pages = pages.clone();
                move |_| {
                    ready(Ok::<_, anyhow::Error>(
                        pages.borrow_mut().pop_front().unwrap(),
                    ))
                }
            },
            |releases| {
                release_catalog_complete_for_selection(releases, None, None, ReleaseChannel::Stable)
            },
        )
        .await
        .unwrap();
        let selection = select_tracked_release(&catalog).unwrap();

        assert_eq!(selection.release.tag_name, "v2.0.0");
    }

    #[test]
    fn current_release_without_a_platform_asset_uses_installed_asset_metadata() {
        let mut installed = InstalledApp::new(
            "owner/project",
            "project",
            "v2.0.0",
            "installed-project.AppImage",
            PathBuf::from("/managed/project.AppImage"),
        );
        installed.install_path_kind = InstallPathKind::ManagedPath;
        let release = Release::fixture("v2.0.0", Vec::new());

        let view = render_app(
            installed,
            RepoRef::parse("owner/project").unwrap(),
            release,
            ReleaseDirection::Reinstall,
            Language::En,
        );

        assert!(matches!(view.status, super::AppStatus::Current));
        assert_eq!(
            view.asset_name.as_deref(),
            Some("installed-project.AppImage")
        );
    }

    #[test]
    fn release_version_view_excludes_drafts_and_limits_first_page() {
        let mut releases = (0..101)
            .map(|index| release_with_asset(&format!("v{index}.0.0"), false))
            .collect::<Vec<_>>();
        let mut draft = release_with_asset("v999.0.0", false);
        draft.draft = true;
        releases.insert(0, draft);

        let versions = release_versions_from_catalog(&releases);

        assert_eq!(versions.len(), 100);
        assert!(
            versions
                .iter()
                .all(|version| version.tag_name != "v999.0.0")
        );
    }

    #[test]
    fn rollback_preview_path_is_only_accepted_when_it_matches_manifest_identity() {
        let mut app = InstalledApp::new(
            "owner/project",
            "project",
            "v2.0.0",
            "project.AppImage",
            PathBuf::from("/managed/project.AppImage"),
        );
        app.install_path_kind = InstallPathKind::ManagedPath;
        app.rollback = Some(RollbackSnapshot {
            version: "v1.0.0".to_string(),
            asset_name: "project.AppImage".to_string(),
            install_path: PathBuf::from("/snapshots/one/project.AppImage"),
            launch_path: None,
            install_type: InstallType::AppImage,
            artifact_sha256: None,
            integrity_status: None,
            checksum_asset_name: None,
            snapshot_path: PathBuf::from("/snapshots/one"),
            installed_at: app.installed_at,
        });
        let tampered = RollbackPreview {
            repo_id: app.id.clone(),
            active_version: app.installed_version.clone(),
            snapshot_version: "v1.0.0".to_string(),
            snapshot_path: PathBuf::from("/arbitrary/client/path"),
        };

        let error = rollback_guard_from_preview(&app, &tampered).unwrap_err();

        assert!(error.to_string().contains("stale rollback preview"));
    }

    fn release_with_asset(tag: &str, prerelease: bool) -> Release {
        let mut release = Release::fixture(
            tag,
            vec![ReleaseAsset::fixture("project-linux-x86_64.AppImage")],
        );
        release.prerelease = prerelease;
        release
    }

    #[test]
    fn allows_github_release_pages() {
        let url = url::Url::parse("https://github.com/dongrencd/releasedock/releases/tag/v0.2.4")
            .expect("valid URL");
        assert!(validate_github_url(&url).is_ok());
    }

    #[test]
    fn rejects_non_github_urls() {
        let url = url::Url::parse("https://example.com/releases").expect("valid URL");
        assert!(validate_github_url(&url).is_err());
    }

    #[test]
    fn sanitizes_connectivity_errors_before_returning_them_to_the_ui() {
        let message =
            "failed with token ghp_secret and proxy http://user:pass@proxy.example.com:8080";

        let sanitized = sanitize_connectivity_message(
            message,
            Some("ghp_secret"),
            Some("http://user:pass@proxy.example.com:8080"),
        );

        assert!(!sanitized.contains("ghp_secret"));
        assert!(!sanitized.contains("user:pass"));
        assert!(sanitized.contains("[token]"));
        assert!(sanitized.contains("[proxy]"));
    }

    #[test]
    fn classifies_connectivity_errors_for_actionable_ui_guidance() {
        assert_eq!(
            classify_connectivity_problem("failed to configure proxy for GitHub client", true),
            "proxy"
        );
        assert_eq!(
            classify_connectivity_problem(
                "GitHub connectivity check request failed with 403 Forbidden: API rate limit exceeded",
                false,
            ),
            "rateLimit"
        );
        assert_eq!(
            classify_connectivity_problem(
                "GitHub connectivity check request failed with 403 Forbidden: Request forbidden by administrative rules. (rate limit remaining 59)",
                false,
            ),
            "unknown"
        );
        assert_eq!(
            classify_connectivity_problem(
                "GitHub connectivity check request failed with 403 Forbidden (rate limit remaining 0)",
                false,
            ),
            "rateLimit"
        );
        assert_eq!(
            classify_connectivity_problem(
                "GitHub connectivity check request failed with 401 Unauthorized: Bad credentials",
                false,
            ),
            "auth"
        );
        assert_eq!(
            classify_connectivity_problem(
                "failed to request GitHub connectivity check: operation timed out",
                false,
            ),
            "network"
        );
        assert_eq!(
            classify_connectivity_problem(
                "failed to request GitHub connectivity check: operation timed out",
                true,
            ),
            "proxy"
        );
    }

    #[test]
    fn classifies_installed_apps_by_management_kind() {
        let appimage = InstalledApp::with_install_metadata(
            "owner/appimage",
            "AppImage",
            "v1.0.0",
            "app.AppImage",
            PathBuf::from("/tmp/appimage"),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        assert!(matches!(
            management_kind_for_app(&appimage),
            super::InstallManagementKind::ManagedLocal
        ));

        let executable = InstalledApp::with_install_metadata(
            "owner/executable",
            "Executable",
            "v1.0.0",
            "releasedock-linux-x64",
            PathBuf::from("/tmp/executable"),
            InstallType::Executable,
            InstallPathKind::ManagedPath,
            true,
        );
        assert!(matches!(
            management_kind_for_app(&executable),
            super::InstallManagementKind::ManagedLocal
        ));

        let linux_package = InstalledApp::with_install_metadata(
            "owner/package",
            "Package",
            "v1.0.0",
            "package.deb",
            PathBuf::from("/tmp/package"),
            InstallType::LinuxPackage,
            InstallPathKind::SystemInstaller,
            true,
        );
        assert!(matches!(
            management_kind_for_app(&linux_package),
            super::InstallManagementKind::SystemPackage
        ));

        let windows_installer = InstalledApp::with_install_metadata(
            "owner/windows",
            "Windows",
            "v1.0.0",
            "setup.exe",
            PathBuf::from("/tmp/windows"),
            InstallType::WindowsInstaller,
            InstallPathKind::SystemInstaller,
            false,
        );
        assert!(matches!(
            management_kind_for_app(&windows_installer),
            super::InstallManagementKind::ExternalInstaller
        ));

        let unknown_managed = InstalledApp::with_install_metadata(
            "owner/legacy",
            "Legacy",
            "v1.0.0",
            "legacy.bin",
            PathBuf::from("/tmp/legacy"),
            InstallType::Unknown,
            InstallPathKind::ManagedPath,
            true,
        );
        assert!(matches!(
            management_kind_for_app(&unknown_managed),
            super::InstallManagementKind::ManagedLocal
        ));
    }

    #[test]
    fn windows_release_build_uses_gui_subsystem() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
        )
        .expect("read main.rs");
        assert!(
            source.contains(r#"cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")"#),
            "main.rs should opt into the Windows GUI subsystem for release builds"
        );
    }

    #[test]
    fn windows_open_actions_use_shell_execute_not_cmd() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
        )
        .expect("read main.rs");
        let production_source = source
            .split("#[cfg(test)]")
            .next()
            .expect("main.rs should contain a test module marker");

        assert!(
            production_source.contains("ShellExecuteW"),
            "Windows open paths should use ShellExecuteW"
        );
        assert!(
            !production_source.contains(r#"Command::new("cmd")"#),
            "Windows open paths should not spawn cmd.exe"
        );
        assert!(
            !production_source.contains("spawn_without_console"),
            "Windows open paths should not need a console-hiding helper"
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn managed_install_path_opens_parent_directory_for_files() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let executable = tempdir.path().join("releasedock-linux-x64");
        std::fs::write(&executable, b"fake binary").expect("write executable");

        let target =
            resolve_open_install_location_target(&executable, InstallPathKind::ManagedPath)
                .expect("target");
        assert_eq!(target, tempdir.path());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn managed_install_path_keeps_directories() {
        let tempdir = tempfile::tempdir().expect("tempdir");

        let target =
            resolve_open_install_location_target(tempdir.path(), InstallPathKind::ManagedPath)
                .expect("target");
        assert_eq!(target, tempdir.path());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn system_installer_keeps_file_path() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let installer = tempdir.path().join("setup.deb");
        std::fs::write(&installer, b"fake package").expect("write installer");

        let target =
            resolve_open_install_location_target(&installer, InstallPathKind::SystemInstaller)
                .expect("target");
        assert_eq!(target, installer);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn system_installer_folder_opens_parent_directory() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let installer = tempdir.path().join("setup.deb");
        std::fs::write(&installer, b"fake package").expect("write installer");

        let target = resolve_installer_folder_target(&installer).expect("target");
        assert_eq!(target, tempdir.path());
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

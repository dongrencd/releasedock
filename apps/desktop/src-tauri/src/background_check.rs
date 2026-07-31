// 后台定时检查 GitHub Release 更新模块
//
// 设计要点：
// - 每隔 N 分钟并发请求所有已跟踪仓库的最新 release
// - 只比对已安装/跟踪版本和最新版本，统计"有更新"数量
// - 有新更新时通过系统通知提醒用户
// - 托盘 badge 通过切换图标 tooltip 来跨平台传达更新计数
// - 后台检查不阻塞前端手动刷新，用独立 refreshId

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use releasedock_core::{
    config::{Config, Language},
    manifest::ManifestStore,
    release::ReleaseClient,
    repo::RepoRef,
};
use tauri::AppHandle;
use tauri::Emitter;
use tauri_plugin_notification::NotificationExt;
use tokio::task::{JoinHandle, JoinSet};

use crate::tracking::TrackedRepoStore;

const BACKGROUND_CHECK_EVENT: &str = "background-check-complete";
const BACKGROUND_CHECK_CONCURRENCY: usize = 6;

/// 后台检查结果
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundCheckStatus {
    Success,
    Partial,
    Failed,
}

/// 后台检查结果
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundCheckResult {
    pub update_count: usize,
    pub total_checked: usize,
    pub checked_at: String,
    pub status: BackgroundCheckStatus,
    pub failed_count: usize,
    pub error: Option<String>,
}

impl BackgroundCheckResult {
    fn failed(error: impl Into<String>) -> Self {
        Self {
            update_count: 0,
            total_checked: 0,
            checked_at: checked_at_now(),
            status: BackgroundCheckStatus::Failed,
            failed_count: 0,
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct RepoCheckSummary {
    update_count: usize,
    total_checked: usize,
    failed_count: usize,
    first_error: Option<String>,
}

impl RepoCheckSummary {
    fn status(&self) -> BackgroundCheckStatus {
        if self.failed_count == 0 {
            BackgroundCheckStatus::Success
        } else if self.failed_count == self.total_checked {
            BackgroundCheckStatus::Failed
        } else {
            BackgroundCheckStatus::Partial
        }
    }
}

fn summarize_repo_checks<I>(results: I) -> RepoCheckSummary
where
    I: IntoIterator<Item = std::result::Result<bool, anyhow::Error>>,
{
    let mut summary = RepoCheckSummary::default();
    for result in results {
        summary.total_checked += 1;
        match result {
            Ok(true) => summary.update_count += 1,
            Ok(false) => {}
            Err(error) => {
                summary.failed_count += 1;
                if summary.first_error.is_none() {
                    summary.first_error = Some(error.to_string());
                }
            }
        }
    }
    summary
}

/// 版本比对纯函数
/// installed 为 None 表示未安装（跟踪但未安装），此时只要有 release 就算有更新。
pub fn has_update(installed: Option<&str>, latest: &str) -> bool {
    match installed {
        Some(current) => current != latest,
        None => true,
    }
}

/// 启动后台定时检查任务
pub fn spawn_background_checker(app: AppHandle, interval_minutes: u64) -> JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_secs(interval_minutes * 60);
        let mut last_notified_update_count: Option<usize> = None;
        loop {
            // 首次启动先让前台 dashboard 完成自己的连接检查，避免同一时间重复请求 GitHub。
            tokio::time::sleep(interval).await;
            match run_background_check().await {
                Ok(result) => {
                    let _ = app.emit(BACKGROUND_CHECK_EVENT, &result);
                    // 每次迭代 re-read language，支持热切换
                    let lang = crate::runtime_config()
                        .ok()
                        .and_then(|c| c.language)
                        .map(|l| match l.as_str() {
                            "zh-CN" => Language::ZhCn,
                            _ => Language::En,
                        })
                        .unwrap_or(Language::En);
                    if result.status == BackgroundCheckStatus::Success {
                        if should_notify_updates(last_notified_update_count, result.update_count) {
                            notify_updates(&app, result.update_count, lang);
                        }
                        last_notified_update_count = Some(result.update_count);
                        crate::tray::update_tray_tooltip(&app, result.update_count, lang);
                    }
                }
                Err(error) => {
                    let result = BackgroundCheckResult::failed(sanitize_background_error(
                        &error.to_string(),
                    ));
                    let _ = app.emit(BACKGROUND_CHECK_EVENT, &result);
                }
            }
        }
    })
}

/// 执行一次后台更新检查
async fn run_background_check() -> Result<BackgroundCheckResult> {
    let store = ManifestStore::default()?;
    let manifest = store.load()?;
    let tracked_store = TrackedRepoStore::default()?;
    let tracked_repos = tracked_store.load()?;
    let installed_ids: HashSet<String> = manifest.apps.iter().map(|app| app.id.clone()).collect();

    // 一次性加载 manifest 构建版本 map（不消费 manifest.apps）
    let installed_versions: HashMap<String, String> = manifest
        .apps
        .iter()
        .map(|app| (app.id.clone(), app.installed_version.clone()))
        .collect();

    let runtime_config = runtime_config()?;
    let client = release_client(Some(&runtime_config))?;

    // 收集需要检查的仓库
    let mut repos_to_check: Vec<(RepoRef, Option<String>)> = Vec::new();
    for app in &manifest.apps {
        let Ok(repo) = RepoRef::parse(&app.repo_url) else {
            continue;
        };
        let ver = installed_versions.get(&app.id).cloned();
        repos_to_check.push((repo, ver));
    }
    for tracked in &tracked_repos {
        let Ok(repo) = RepoRef::parse(tracked.repo_id.as_str()) else {
            continue;
        };
        if installed_ids.contains(&repo.id()) {
            continue;
        }
        repos_to_check.push((repo, None));
    }

    // 并发检查，限制并发数
    let total_checked = repos_to_check.len();
    let mut check_results = Vec::with_capacity(total_checked);
    let mut tasks = JoinSet::new();
    let mut pending = repos_to_check.into_iter();
    for _ in 0..BACKGROUND_CHECK_CONCURRENCY {
        if let Some((repo, installed)) = pending.next() {
            spawn_check_task(&mut tasks, &client, repo, installed);
        }
    }
    while let Some(join_result) = tasks.join_next().await {
        check_results.push(match join_result {
            Ok(result) => result,
            Err(error) => Err(anyhow::anyhow!("background release task failed: {error}")),
        });
        if let Some((repo, installed)) = pending.next() {
            spawn_check_task(&mut tasks, &client, repo, installed);
        }
    }

    let summary = summarize_repo_checks(check_results);
    let error = summary
        .first_error
        .as_deref()
        .map(sanitize_background_error);
    Ok(BackgroundCheckResult {
        update_count: summary.update_count,
        total_checked: summary.total_checked,
        checked_at: checked_at_now(),
        status: summary.status(),
        failed_count: summary.failed_count,
        error,
    })
}

fn spawn_check_task(
    tasks: &mut JoinSet<std::result::Result<bool, anyhow::Error>>,
    client: &ReleaseClient,
    repo: RepoRef,
    installed_version: Option<String>,
) {
    let client = client.clone();
    tasks.spawn(async move {
        match client.latest_release_optional(&repo).await {
            Ok(Some(release)) => {
                // 用纯函数做版本比对
                Ok(has_update(installed_version.as_deref(), &release.tag_name))
            }
            Ok(None) => Ok(false),
            Err(error) => Err(error),
        }
    });
}

fn sanitize_background_error(message: &str) -> String {
    let config = crate::runtime_config().ok();
    let token = config
        .as_ref()
        .and_then(|value| value.github_token.as_deref())
        .map(str::to_string)
        .or_else(|| std::env::var("GITHUB_TOKEN").ok());
    let proxy = config
        .as_ref()
        .and_then(|value| value.proxy_url.as_deref())
        .map(str::to_string)
        .or_else(|| std::env::var("HTTPS_PROXY").ok());
    crate::sanitize_connectivity_message(message, token.as_deref(), proxy.as_deref())
}

fn should_notify_updates(previous_count: Option<usize>, current_count: usize) -> bool {
    current_count > 0 && previous_count != Some(current_count)
}

fn checked_at_now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

/// 发送系统通知
fn notify_updates(app: &AppHandle, count: usize, language: Language) {
    let title = match language {
        Language::En => "ReleaseDock - Updates Available".to_string(),
        Language::ZhCn => "ReleaseDock - 有可用更新".to_string(),
    };
    let body = match language {
        Language::En => {
            if count > 1 {
                format!("{count} apps have updates available")
            } else {
                format!("{count} app has updates available")
            }
        }
        Language::ZhCn => format!("{count} 个软件有可用更新"),
    };
    let _ = app
        .notification()
        .builder()
        .title(&title)
        .body(&body)
        .show();
}

fn runtime_config() -> Result<Config> {
    crate::runtime_config()
}

fn release_client(runtime_config: Option<&Config>) -> Result<ReleaseClient> {
    use std::env;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_checker_waits_for_interval_before_first_request() {
        let source = include_str!("background_check.rs");
        let first_check = source
            .find("match run_background_check().await")
            .expect("background checker should run its check in the loop");
        let first_sleep = source
            .find("tokio::time::sleep(interval).await")
            .expect("background checker should wait before its first check");

        assert!(
            first_sleep < first_check,
            "the initial background check must not compete with the foreground dashboard load"
        );
    }

    #[test]
    fn has_update_returns_false_when_versions_match() {
        assert!(!has_update(Some("v1.0.0"), "v1.0.0"));
    }

    #[test]
    fn has_update_returns_true_when_versions_differ() {
        assert!(has_update(Some("v1.0.0"), "v1.1.0"));
    }

    #[test]
    fn has_update_returns_true_when_not_installed() {
        assert!(has_update(None, "v1.0.0"));
    }

    #[test]
    fn notification_is_sent_only_when_positive_update_count_changes() {
        assert!(should_notify_updates(None, 2));
        assert!(!should_notify_updates(Some(2), 2));
        assert!(should_notify_updates(Some(2), 3));
        assert!(!should_notify_updates(Some(2), 0));
        assert!(should_notify_updates(Some(0), 1));
    }

    #[test]
    fn successful_background_result_records_status_and_check_time() {
        let result = BackgroundCheckResult {
            update_count: 2,
            total_checked: 7,
            checked_at: checked_at_now(),
            status: BackgroundCheckStatus::Success,
            failed_count: 0,
            error: None,
        };

        assert_eq!(result.update_count, 2);
        assert_eq!(result.total_checked, 7);
        assert_eq!(result.status, BackgroundCheckStatus::Success);
        assert_eq!(result.failed_count, 0);
        assert!(result.error.is_none());
        assert!(!result.checked_at.is_empty());
    }

    #[test]
    fn failed_background_result_keeps_error_summary_without_update_count() {
        let result = BackgroundCheckResult::failed("config load failed");

        assert_eq!(result.update_count, 0);
        assert_eq!(result.total_checked, 0);
        assert_eq!(result.status, BackgroundCheckStatus::Failed);
        assert_eq!(result.error.as_deref(), Some("config load failed"));
        assert!(!result.checked_at.is_empty());
    }

    #[test]
    fn repo_check_summary_tracks_partial_failures_without_counting_them_as_current() {
        let summary = summarize_repo_checks([
            Ok(true),
            Ok(false),
            Err(anyhow::anyhow!("network unavailable")),
        ]);

        assert_eq!(summary.update_count, 1);
        assert_eq!(summary.total_checked, 3);
        assert_eq!(summary.failed_count, 1);
        assert_eq!(summary.status(), BackgroundCheckStatus::Partial);
    }

    #[test]
    fn repo_check_summary_marks_all_failed_requests_as_failed() {
        let summary = summarize_repo_checks([
            Err(anyhow::anyhow!("token rejected")),
            Err(anyhow::anyhow!("network unavailable")),
        ]);

        assert_eq!(summary.update_count, 0);
        assert_eq!(summary.total_checked, 2);
        assert_eq!(summary.failed_count, 2);
        assert_eq!(summary.status(), BackgroundCheckStatus::Failed);
    }
}

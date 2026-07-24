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
    time::Duration,
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
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundCheckResult {
    pub update_count: usize,
    pub total_checked: usize,
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
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_minutes * 60));
        ticker.tick().await;
        loop {
            ticker.tick().await;
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
                    if result.update_count > 0 {
                        notify_updates(&app, result.update_count, lang);
                    }
                    crate::tray::update_tray_tooltip(&app, result.update_count, lang);
                }
                Err(_) => {}
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
    let mut update_count = 0usize;
    let mut tasks = JoinSet::new();
    let mut pending = repos_to_check.into_iter();
    for _ in 0..BACKGROUND_CHECK_CONCURRENCY {
        if let Some((repo, installed)) = pending.next() {
            spawn_check_task(&mut tasks, &client, repo, installed);
        }
    }
    while let Some(join_result) = tasks.join_next().await {
        if let Ok(Ok(true)) = join_result {
            update_count += 1;
        }
        if let Some((repo, installed)) = pending.next() {
            spawn_check_task(&mut tasks, &client, repo, installed);
        }
    }

    Ok(BackgroundCheckResult {
        update_count,
        total_checked,
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
            Err(_) => Ok(false),
        }
    });
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
    let _ = app.notification().builder().title(&title).body(&body).show();
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
}

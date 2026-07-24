use std::{
    ffi::OsString,
    fs,
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use releasedock_core::{
    asset_matcher::{Architecture, AssetMatcher, OperatingSystem},
    config::{Config, ConfigStore, Language},
    install_plan::InstallPlan,
    installer::{install_from_plan, uninstall_repo},
    manifest::ManifestStore,
    release::{Release, ReleaseClient},
    repo::RepoRef,
};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(name = "releasedock")]
#[command(about = "Manage applications installed from GitHub Releases")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Install(InstallArgs),
    List(ManifestArgs),
    Check(ManifestArgs),
    Update(UpdateArgs),
    Uninstall(UninstallArgs),
    Info(InfoArgs),
    Doctor,
    Config(ConfigArgs),
}

#[derive(Debug, Args)]
struct InstallArgs {
    repo: String,
    #[arg(long)]
    release_fixture: Option<PathBuf>,
    #[arg(long)]
    manifest: Option<PathBuf>,
    #[arg(long, hide = true)]
    artifact_fixture: Option<PathBuf>,
    #[arg(long, value_enum)]
    os: Option<CliOs>,
    #[arg(long, value_enum)]
    arch: Option<CliArch>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Args)]
struct ManifestArgs {
    #[arg(long)]
    manifest: Option<PathBuf>,
    #[arg(long)]
    json: bool,
    #[arg(long, hide = true)]
    release_fixture: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct UpdateArgs {
    repo: Option<String>,
    #[arg(long)]
    all: bool,
    #[arg(long)]
    manifest: Option<PathBuf>,
    #[arg(long, hide = true)]
    artifact_fixture: Option<PathBuf>,
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Args)]
struct UninstallArgs {
    repo: String,
    #[arg(long)]
    manifest: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct InfoArgs {
    repo: String,
    #[arg(long)]
    release_fixture: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Get,
    Set(ConfigSetArgs),
    Clear(ConfigClearArgs),
}

#[derive(Debug, Args)]
struct ConfigSetArgs {
    #[command(subcommand)]
    field: ConfigSetField,
}

#[derive(Debug, Subcommand)]
enum ConfigSetField {
    GithubToken(ConfigValueArgs),
    Proxy(ConfigValueArgs),
    InstallRoot(ConfigPathArgs),
}

#[derive(Debug, Args)]
struct ConfigValueArgs {
    value: String,
}

#[derive(Debug, Args)]
struct ConfigPathArgs {
    value: PathBuf,
}

#[derive(Debug, Args)]
struct ConfigClearArgs {
    #[command(subcommand)]
    field: ConfigClearField,
}

#[derive(Debug, Subcommand)]
enum ConfigClearField {
    GithubToken,
    Proxy,
    InstallRoot,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliOs {
    Windows,
    Linux,
    Macos,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliArch {
    X64,
    Arm64,
}

pub async fn run() -> Result<()> {
    run_from_args(std::env::args_os()).await
}

pub async fn run_from_args<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    dispatch(cli).await
}

async fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Install(args) => install(args).await,
        Commands::List(args) => list(args),
        Commands::Check(args) => check(args).await,
        Commands::Update(args) => update(args).await,
        Commands::Uninstall(args) => uninstall(args),
        Commands::Info(args) => info(args).await,
        Commands::Doctor => doctor(),
        Commands::Config(args) => config(args),
    }
}

async fn install(args: InstallArgs) -> Result<()> {
    let InstallArgs {
        repo,
        release_fixture,
        manifest,
        artifact_fixture,
        os,
        arch,
        json,
        yes,
    } = args;

    if json {
        let runtime_config = runtime_config()?;
        let plan = build_install_plan(
            &repo,
            release_fixture.clone(),
            os,
            arch,
            Some(&runtime_config),
        )
        .await?;
        println!("{}", serde_json::to_string(&plan)?);
        return Ok(());
    }

    let store = manifest_store(manifest)?;
    let runtime_config = runtime_config()?;
    let plan = build_install_plan(&repo, release_fixture, os, arch, Some(&runtime_config)).await?;

    if !json {
        confirm_execution("install", &plan, yes)?;
    }

    let outcome = install_from_plan(
        &plan,
        &store,
        artifact_fixture.as_deref(),
        Some(&runtime_config),
        Language::En,
        None,
    )
    .await?;

    println!(
        "Installed {} {} to {} [{}]",
        outcome.app.id,
        outcome.app.installed_version,
        outcome.install_path.display(),
        install_path_kind_label(outcome.install_path_kind)
    );
    println!("Manifest updated at {}", store.path().display());

    Ok(())
}

async fn build_install_plan(
    repo_input: &str,
    release_fixture: Option<PathBuf>,
    os: Option<CliOs>,
    arch: Option<CliArch>,
    runtime_config: Option<&Config>,
) -> Result<InstallPlan> {
    let repo = RepoRef::parse(repo_input)?;
    let release = match release_fixture {
        Some(path) => read_fixture_release(&path)?,
        None => {
            let client = release_client(runtime_config)?;
            client.latest_release(&repo).await?
        }
    };

    let matcher = match (os, arch) {
        (Some(os), Some(arch)) => AssetMatcher::new(os.into(), arch.into()),
        _ => AssetMatcher::current(),
    };
    let matched = matcher.select_best(&release)?;
    Ok(InstallPlan::from_match(&repo, &release, &matched, Language::En))
}

fn list(args: ManifestArgs) -> Result<()> {
    let ManifestArgs {
        manifest,
        json,
        release_fixture: _,
    } = args;
    let store = manifest_store(manifest)?;
    let manifest = store.load()?;

    if json {
        println!("{}", serde_json::to_string(&manifest)?);
        return Ok(());
    }

    if manifest.apps.is_empty() {
        println!("No managed apps");
    } else {
        for app in manifest.apps {
            println!(
                "{} {} {} [{} / {}]",
                app.id,
                app.installed_version,
                app.asset_name,
                install_path_kind_label(app.install_path_kind),
                if app.uninstall_supported {
                    "可卸载"
                } else {
                    "需系统卸载"
                }
            );
        }
    }
    Ok(())
}

async fn update(args: UpdateArgs) -> Result<()> {
    let UpdateArgs {
        repo,
        all,
        manifest,
        artifact_fixture,
        yes,
    } = args;

    let store = manifest_store(manifest)?;
    let manifest = store.load()?;
    let runtime_config = runtime_config()?;

    if all {
        if manifest.apps.is_empty() {
            println!("No managed apps");
            return Ok(());
        }

        let mut plans = Vec::with_capacity(manifest.apps.len());
        for app in manifest.apps {
            let plan =
                build_install_plan(&app.repo_url, None, None, None, Some(&runtime_config)).await?;
            plans.push((app.id.clone(), plan));
        }

        confirm_bulk_execution("update", &plans, yes)?;

        for (_app_id, plan) in plans {
            let outcome = install_from_plan(
                &plan,
                &store,
                artifact_fixture.as_deref(),
                Some(&runtime_config),
                Language::En,
                None,
            )
            .await?;
            println!(
                "Updated {} to {} at {} [{}]",
                outcome.app.id,
                outcome.app.installed_version,
                outcome.install_path.display(),
                install_path_kind_label(outcome.install_path_kind)
            );
        }

        return Ok(());
    }

    let Some(repo_input) = repo else {
        println!("provide a repo or --all");
        return Ok(());
    };

    let plan = build_install_plan(&repo_input, None, None, None, Some(&runtime_config)).await?;
    confirm_execution("update", &plan, yes)?;
    let outcome = install_from_plan(
        &plan,
        &store,
        artifact_fixture.as_deref(),
        Some(&runtime_config),
        Language::En,
        None,
    )
    .await?;
    println!(
        "Updated {} to {} at {} [{}]",
        outcome.app.id,
        outcome.app.installed_version,
        outcome.install_path.display(),
        install_path_kind_label(outcome.install_path_kind)
    );
    Ok(())
}

fn uninstall(args: UninstallArgs) -> Result<()> {
    let UninstallArgs { repo, manifest } = args;
    let store = manifest_store(manifest)?;
    let repo = RepoRef::parse(&repo)?;
    match uninstall_repo(&store, &repo.id(), Language::En, None)? {
        Some(app) => {
            println!("Uninstalled {} from {}", app.id, app.install_path.display());
        }
        None => {
            println!("No managed app matched {}", repo.id());
        }
    }
    Ok(())
}

async fn check(args: ManifestArgs) -> Result<()> {
    let ManifestArgs {
        manifest,
        json,
        release_fixture,
    } = args;
    let store = manifest_store(manifest)?;
    let manifest = store.load()?;
    let report = build_check_report(&manifest.apps, release_fixture.as_ref()).await?;

    if json {
        println!("{}", serde_json::to_string(&report)?);
        return Ok(());
    }

    if report.apps.is_empty() {
        println!("No managed apps");
    } else {
        for app in report.apps {
            println!(
                "{} [{}] {} -> {} ({})",
                app.id,
                status_label(app.status),
                app.current_version,
                app.latest_version,
                app.asset_name.as_deref().unwrap_or("no matching asset")
            );
            if let Some(note) = app.release_note.as_deref() {
                println!("  note: {note}");
            }
            if let Some(reason) = app.reason.as_deref() {
                println!("  reason: {reason}");
            }
        }
    }
    Ok(())
}

async fn info(args: InfoArgs) -> Result<()> {
    let InfoArgs {
        repo,
        release_fixture,
        json,
    } = args;

    let repo = RepoRef::parse(&repo)?;
    let runtime_config = runtime_config()?;
    let release = match release_fixture {
        Some(path) => read_fixture_release(&path)?,
        None => {
            let client = release_client(Some(&runtime_config))?;
            client.latest_release(&repo).await?
        }
    };

    if json {
        println!("{}", serde_json::to_string(&release)?);
        return Ok(());
    }

    println!("{} -> {}", repo.id(), repo.github_url());
    println!(
        "Release: {} ({})",
        release.name.as_deref().unwrap_or(&release.tag_name),
        release.tag_name
    );
    if let Some(published_at) = release.published_at {
        println!("Published: {}", published_at.to_rfc3339());
    }
    if let Some(url) = release.html_url.as_deref() {
        println!("URL: {url}");
    }
    println!();
    println!("Release note:");
    println!(
        "{}",
        release
            .release_note()
            .unwrap_or("This release does not include a release note.")
    );

    if !release.assets.is_empty() {
        println!();
        println!("Assets:");
        for asset in release.assets {
            println!("- {} ({})", asset.name, asset.browser_download_url);
        }
    }

    Ok(())
}

fn manifest_store(path: Option<PathBuf>) -> Result<ManifestStore> {
    path.map(ManifestStore::at_path)
        .map(Ok)
        .unwrap_or_else(ManifestStore::default)
}

async fn build_check_report(
    apps: &[releasedock_core::manifest::InstalledApp],
    release_fixture: Option<&PathBuf>,
) -> Result<CheckReport> {
    let matcher = AssetMatcher::current();
    let mut entries = Vec::with_capacity(apps.len());

    for app in apps {
        let repo = RepoRef::parse(&app.repo_url)?;
        match load_release(&repo, release_fixture).await {
            Ok(release) => {
                let matched = matcher.select_best(&release).ok();
                let current_version = app.installed_version.clone();
                let latest_version = release.tag_name.clone();
                let status = if current_version == latest_version {
                    CheckStatus::Current
                } else if matched.is_some() {
                    CheckStatus::UpdateAvailable
                } else {
                    CheckStatus::MissingAsset
                };

                entries.push(CheckEntry {
                    id: app.id.clone(),
                    current_version,
                    latest_version,
                    status,
                    release_title: release.name.clone(),
                    release_note: release.release_note().map(|note| note.to_string()).or_else(
                        || Some("This release does not include a release note.".to_string()),
                    ),
                    release_url: release.html_url.clone().or_else(|| Some(repo.github_url())),
                    published_at: release
                        .published_at
                        .as_ref()
                        .map(|value| value.to_rfc3339()),
                    asset_name: matched.map(|asset| asset.asset.name),
                    reason: None,
                });
            }
            Err(error) => entries.push(failed_check_entry(app, &repo, error)),
        }
    }

    Ok(CheckReport { apps: entries })
}

async fn load_release(repo: &RepoRef, release_fixture: Option<&PathBuf>) -> Result<Release> {
    match release_fixture {
        Some(path) => read_fixture_release(path),
        None => {
            let runtime_config = runtime_config()?;
            let client = release_client(Some(&runtime_config))?;
            client.latest_release(repo).await
        }
    }
}

fn config(args: ConfigArgs) -> Result<()> {
    let store = config_store()?;
    let mut current = store.load()?;

    match args.command {
        ConfigCommand::Get => {
            println!("{}", serde_json::to_string_pretty(&current)?);
        }
        ConfigCommand::Set(args) => match args.field {
            ConfigSetField::GithubToken(value) => {
                current.github_token = Some(value.value);
                store.save(&current)?;
                println!("已更新 githubToken");
            }
            ConfigSetField::Proxy(value) => {
                current.proxy_url = Some(value.value);
                store.save(&current)?;
                println!("已更新 proxyUrl");
            }
            ConfigSetField::InstallRoot(value) => {
                current.install_root = Some(value.value);
                store.save(&current)?;
                println!("已更新 installRoot");
            }
        },
        ConfigCommand::Clear(args) => match args.field {
            ConfigClearField::GithubToken => {
                current.github_token = None;
                store.save(&current)?;
                println!("已清除 githubToken");
            }
            ConfigClearField::Proxy => {
                current.proxy_url = None;
                store.save(&current)?;
                println!("已清除 proxyUrl");
            }
            ConfigClearField::InstallRoot => {
                current.install_root = None;
                store.save(&current)?;
                println!("已清除 installRoot");
            }
        },
    }

    Ok(())
}

fn doctor() -> Result<()> {
    let store = config_store()?;
    let config = store.load()?;

    println!("releasedock doctor: core CLI is available");
    println!("config path: {}", store.path().display());
    println!(
        "github token source: {}",
        config_source_label(config.github_token.as_deref(), "GITHUB_TOKEN")
    );
    println!(
        "proxy source: {}",
        config_source_label(config.proxy_url.as_deref(), "HTTPS_PROXY")
    );
    println!(
        "github token: {}",
        if config
            .github_token
            .as_deref()
            .is_some_and(|value| !value.is_empty())
            || std::env::var("GITHUB_TOKEN")
                .ok()
                .is_some_and(|value| !value.trim().is_empty())
        {
            "已配置"
        } else {
            "未配置"
        }
    );
    println!(
        "proxy: {}",
        if config
            .proxy_url
            .as_deref()
            .is_some_and(|value| !value.is_empty())
            || std::env::var("HTTPS_PROXY")
                .ok()
                .is_some_and(|value| !value.trim().is_empty())
        {
            "已配置"
        } else {
            "未配置"
        }
    );
    println!(
        "install root: {}",
        config
            .install_root
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "默认位置".to_string())
    );

    Ok(())
}

fn config_source_label(config_value: Option<&str>, env_name: &str) -> String {
    if config_value.is_some_and(|value| !value.trim().is_empty()) {
        return "config".to_string();
    }

    if std::env::var(env_name)
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return format!("env {env_name}");
    }

    "none".to_string()
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
        .or_else(|| std::env::var("GITHUB_TOKEN").ok());
    let proxy = runtime_config
        .and_then(|config| config.proxy_url.as_deref())
        .map(str::to_string)
        .or_else(|| std::env::var("HTTPS_PROXY").ok());
    ReleaseClient::new(token.as_deref(), proxy.as_deref())
}

fn confirm_execution(action: &str, plan: &InstallPlan, yes: bool) -> Result<()> {
    if yes {
        return Ok(());
    }

    if !io::stdin().is_terminal() {
        anyhow::bail!("{} 需要交互确认；请使用 --yes 或在交互式终端中运行", action);
    }

    println!("准备执行 {}：", action);
    println!("  仓库: {}", plan.repo_id);
    println!("  版本: {}", plan.version);
    println!("  资产: {}", plan.asset_name);
    for note in &plan.notes {
        println!("  提示: {}", note);
    }
    if plan.requires_user_confirmation {
        println!("  该安装包需要系统权限确认。");
    }

    print!("继续执行吗？[y/N] ");
    io::stdout()
        .flush()
        .context("failed to flush confirmation prompt")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read confirmation")?;
    let answer = input.trim().to_ascii_lowercase();
    if matches!(answer.as_str(), "y" | "yes") {
        return Ok(());
    }

    anyhow::bail!("已取消 {} 操作", action);
}

fn confirm_bulk_execution(action: &str, plans: &[(String, InstallPlan)], yes: bool) -> Result<()> {
    if yes {
        return Ok(());
    }

    if !io::stdin().is_terminal() {
        anyhow::bail!("{} 需要交互确认；请使用 --yes 或在交互式终端中运行", action);
    }

    println!("准备执行 {}：", action);
    println!("  共 {} 个项目", plans.len());
    for (index, (app_id, plan)) in plans.iter().enumerate() {
        println!(
            "  {}. {} -> {} / {}",
            index + 1,
            app_id,
            plan.version,
            plan.asset_name
        );
        for note in &plan.notes {
            println!("     提示: {}", note);
        }
    }
    if plans
        .iter()
        .any(|(_, plan)| plan.requires_user_confirmation)
    {
        println!("  其中包含系统安装器，执行时会继续触发系统权限确认。");
    }

    print!("继续执行吗？[y/N] ");
    io::stdout()
        .flush()
        .context("failed to flush confirmation prompt")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read confirmation")?;
    let answer = input.trim().to_ascii_lowercase();
    if matches!(answer.as_str(), "y" | "yes") {
        return Ok(());
    }

    anyhow::bail!("已取消 {} 操作", action);
}

fn failed_check_entry(
    app: &releasedock_core::manifest::InstalledApp,
    repo: &RepoRef,
    error: anyhow::Error,
) -> CheckEntry {
    CheckEntry {
        id: app.id.clone(),
        current_version: app.installed_version.clone(),
        latest_version: "unknown".to_string(),
        status: CheckStatus::FetchFailed,
        release_title: None,
        release_note: None,
        release_url: Some(repo.github_url()),
        published_at: None,
        asset_name: None,
        reason: Some(error.to_string()),
    }
}

#[derive(Debug, Serialize)]
struct CheckReport {
    apps: Vec<CheckEntry>,
}

#[derive(Debug, Serialize)]
struct CheckEntry {
    id: String,
    current_version: String,
    latest_version: String,
    status: CheckStatus,
    release_title: Option<String>,
    release_note: Option<String>,
    release_url: Option<String>,
    published_at: Option<String>,
    asset_name: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum CheckStatus {
    Current,
    UpdateAvailable,
    MissingAsset,
    FetchFailed,
}

fn status_label(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Current => "最新",
        CheckStatus::UpdateAvailable => "有更新",
        CheckStatus::MissingAsset => "缺少资产",
        CheckStatus::FetchFailed => "获取失败",
    }
}

fn install_path_kind_label(kind: releasedock_core::manifest::InstallPathKind) -> &'static str {
    match kind {
        releasedock_core::manifest::InstallPathKind::ManagedPath => "managedPath",
        releasedock_core::manifest::InstallPathKind::SystemInstaller => "systemInstaller",
        releasedock_core::manifest::InstallPathKind::Unknown => "unknown",
    }
}

fn read_fixture_release(path: &PathBuf) -> Result<Release> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read release fixture {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse release fixture {}", path.display()))
}

impl From<CliOs> for OperatingSystem {
    fn from(value: CliOs) -> Self {
        match value {
            CliOs::Windows => OperatingSystem::Windows,
            CliOs::Linux => OperatingSystem::Linux,
            CliOs::Macos => OperatingSystem::Macos,
        }
    }
}

impl From<CliArch> for Architecture {
    fn from(value: CliArch) -> Self {
        match value {
            CliArch::X64 => Architecture::X64,
            CliArch::Arm64 => Architecture::Arm64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CheckStatus, failed_check_entry, status_label};
    use releasedock_core::manifest::InstalledApp;

    #[test]
    fn formats_failed_check_entries_with_reason() {
        let repo = releasedock_core::repo::RepoRef::parse("owner/project").unwrap();
        let app = InstalledApp::new(
            "owner/project",
            "project",
            "v1.0.0",
            "project-linux-x86_64.AppImage",
            std::path::PathBuf::from("/tmp/project"),
        );

        let entry = failed_check_entry(&app, &repo, anyhow::anyhow!("network down"));
        assert_eq!(entry.status, CheckStatus::FetchFailed);
        assert_eq!(status_label(entry.status), "获取失败");
        assert!(entry.reason.as_deref().unwrap().contains("network down"));
    }
}

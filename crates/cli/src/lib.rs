use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use releasedock_core::{
    asset_matcher::{Architecture, AssetMatcher, OperatingSystem},
    config::{Config, ConfigStore, Language},
    install_plan::{InstallManagementKind, InstallPlan, InstallSelectionGuard},
    installer::{
        RollbackGuard, adopt_system_installer_app, install_from_plan, rollback_repo_guarded,
        uninstall_repo,
    },
    integrity::{IntegrityStatus, IntegrityVerifier},
    manifest::{InstalledApp, ManifestStore, SystemPackageManager},
    release::{Release, ReleaseClient},
    release_policy::{
        PolicyMutation, PolicyMutationResult, ReleaseChannel, ReleaseDirection, ReleasePolicy,
        ReleaseSelection, ReleaseSelector,
    },
    repo::RepoRef,
};
use serde::{Deserialize, Serialize};

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
    Adopt(AdoptArgs),
    Releases(ReleasesArgs),
    List(ManifestArgs),
    Check(ManifestArgs),
    Update(UpdateArgs),
    Pin(PinArgs),
    Unpin(PolicyRepoArgs),
    Ignore(PolicyVersionArgs),
    Unignore(PolicyVersionArgs),
    Channel(ChannelArgs),
    Rollback(RollbackArgs),
    Uninstall(UninstallArgs),
    Info(InfoArgs),
    Doctor,
    Config(ConfigArgs),
}

#[derive(Debug, Args)]
struct InstallArgs {
    repo: String,
    #[arg(long)]
    version: Option<String>,
    #[arg(long)]
    prerelease: bool,
    #[arg(long, hide = true)]
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
struct ReleasesArgs {
    repo: String,
    #[arg(long)]
    include_prerelease: bool,
    #[arg(long)]
    all: bool,
    #[arg(long)]
    json: bool,
    #[arg(long, hide = true)]
    release_fixture: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct AdoptArgs {
    repo: String,
    #[arg(long)]
    manifest: Option<PathBuf>,
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
#[command(group(
    ArgGroup::new("update_target")
        .required(true)
        .multiple(false)
        .args(["repo", "all"])
))]
struct UpdateArgs {
    repo: Option<String>,
    #[arg(long)]
    all: bool,
    #[arg(long, conflicts_with = "all")]
    version: Option<String>,
    #[arg(long)]
    manifest: Option<PathBuf>,
    #[arg(long, hide = true)]
    release_fixture: Option<PathBuf>,
    #[arg(long, hide = true)]
    artifact_fixture: Option<PathBuf>,
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Args)]
struct PinArgs {
    repo: String,
    version: Option<String>,
    #[arg(long)]
    manifest: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct PolicyRepoArgs {
    repo: String,
    #[arg(long)]
    manifest: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct PolicyVersionArgs {
    repo: String,
    version: String,
    #[arg(long)]
    manifest: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ChannelArgs {
    repo: String,
    #[arg(value_enum)]
    channel: CliReleaseChannel,
    #[arg(long)]
    manifest: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct RollbackArgs {
    repo: String,
    #[arg(long)]
    manifest: Option<PathBuf>,
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
    #[arg(long, hide = true)]
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliReleaseChannel {
    Stable,
    Prerelease,
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
        Commands::Adopt(args) => adopt(args),
        Commands::Releases(args) => releases(args).await,
        Commands::List(args) => list(args),
        Commands::Check(args) => check(args).await,
        Commands::Update(args) => update(args).await,
        Commands::Pin(args) => pin(args),
        Commands::Unpin(args) => unpin(args),
        Commands::Ignore(args) => ignore(args),
        Commands::Unignore(args) => unignore(args),
        Commands::Channel(args) => channel(args),
        Commands::Rollback(args) => rollback(args),
        Commands::Uninstall(args) => uninstall(args),
        Commands::Info(args) => info(args).await,
        Commands::Doctor => doctor(),
        Commands::Config(args) => config(args),
    }
}

async fn install(args: InstallArgs) -> Result<()> {
    let InstallArgs {
        repo,
        version,
        prerelease,
        release_fixture,
        manifest,
        artifact_fixture,
        os,
        arch,
        json,
        yes,
    } = args;

    let store = manifest_store(manifest)?;
    let runtime_config = runtime_config()?;
    let mut plan = build_install_plan(
        &repo,
        release_fixture,
        os,
        arch,
        Some(&runtime_config),
        version.as_deref(),
        prerelease,
    )
    .await?;
    let installed = store
        .load()?
        .apps
        .into_iter()
        .find(|app| app.id == plan.repo_id);
    plan.selection_guard = Some(
        installed
            .as_ref()
            .map(InstallSelectionGuard::from_app)
            .unwrap_or(InstallSelectionGuard::ExpectedAbsent),
    );

    if json {
        println!("{}", serde_json::to_string(&plan)?);
        return Ok(());
    }

    confirm_execution("install", &plan, yes)?;

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
        "Installed {} {} to {} [{}] {}",
        outcome.app.id,
        outcome.app.installed_version,
        outcome.install_path.display(),
        install_path_kind_label(outcome.install_path_kind),
        app_integrity_summary(&outcome.app)
    );
    println!("Manifest updated at {}", store.path().display());

    Ok(())
}

fn adopt(args: AdoptArgs) -> Result<()> {
    let repo = RepoRef::parse(&args.repo)?;
    let store = manifest_store(args.manifest)?;
    let adopted = adopt_system_installer_app(&store, &repo)?;

    println!(
        "Adopted {} to {}",
        repo.id(),
        adopted.install_path.display()
    );
    Ok(())
}

async fn build_install_plan(
    repo_input: &str,
    release_fixture: Option<PathBuf>,
    os: Option<CliOs>,
    arch: Option<CliArch>,
    runtime_config: Option<&Config>,
    version: Option<&str>,
    prerelease: bool,
) -> Result<InstallPlan> {
    let repo = RepoRef::parse(repo_input)?;
    let client = release_fixture
        .is_none()
        .then(|| release_client(runtime_config))
        .transpose()?;
    let selection = select_install_release(
        &repo,
        release_fixture.as_ref(),
        client.as_ref(),
        version,
        prerelease,
    )
    .await?;

    let matcher = match (os, arch) {
        (Some(os), Some(arch)) => AssetMatcher::new(os.into(), arch.into()),
        _ => AssetMatcher::current(),
    };
    let mut plan = build_plan_from_selection(
        &repo,
        &selection,
        &matcher,
        release_fixture.is_some(),
        client.as_ref(),
    )
    .await?;
    if prerelease {
        plan = plan.with_target_policy(ReleasePolicy {
            channel: ReleaseChannel::Prerelease,
            ..ReleasePolicy::default()
        });
    }
    Ok(plan)
}

async fn releases(args: ReleasesArgs) -> Result<()> {
    let repo = RepoRef::parse(&args.repo)?;
    let runtime_config = runtime_config()?;
    let client = args
        .release_fixture
        .is_none()
        .then(|| release_client(Some(&runtime_config)))
        .transpose()?;
    let mut catalog = load_release_catalog(
        &repo,
        args.release_fixture.as_ref(),
        client.as_ref(),
        args.all,
    )
    .await?;
    catalog.retain(|release| !release.draft && (args.include_prerelease || !release.prerelease));

    if args.json {
        println!("{}", serde_json::to_string(&catalog)?);
        return Ok(());
    }

    if catalog.is_empty() {
        println!("No matching releases");
        return Ok(());
    }
    for release in catalog {
        let channel = if release.prerelease {
            "prerelease"
        } else {
            "stable"
        };
        let published_at = release
            .published_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_else(|| "unknown".to_string());
        println!("{} {} {}", release.tag_name, channel, published_at);
    }
    Ok(())
}

fn pin(args: PinArgs) -> Result<()> {
    let repo = RepoRef::parse(&args.repo)?;
    let store = manifest_store(args.manifest)?;
    let mutation = args
        .version
        .map(PolicyMutation::PinVersion)
        .unwrap_or(PolicyMutation::PinCurrent);
    let result = store.mutate_release_policy(&repo.id(), mutation)?;
    println!(
        "Pinned {} to {}{}",
        repo.id(),
        result.policy.pinned_version.as_deref().unwrap_or("unknown"),
        no_op_suffix(&result)
    );
    Ok(())
}

fn unpin(args: PolicyRepoArgs) -> Result<()> {
    let repo = RepoRef::parse(&args.repo)?;
    let store = manifest_store(args.manifest)?;
    let result = store.mutate_release_policy(&repo.id(), PolicyMutation::Unpin)?;
    println!("Unpinned {}{}", repo.id(), no_op_suffix(&result));
    Ok(())
}

fn ignore(args: PolicyVersionArgs) -> Result<()> {
    mutate_ignored_version(args, true)
}

fn unignore(args: PolicyVersionArgs) -> Result<()> {
    mutate_ignored_version(args, false)
}

fn mutate_ignored_version(args: PolicyVersionArgs, ignore: bool) -> Result<()> {
    let repo = RepoRef::parse(&args.repo)?;
    let store = manifest_store(args.manifest)?;
    let mutation = if ignore {
        PolicyMutation::IgnoreVersion(args.version.clone())
    } else {
        PolicyMutation::UnignoreVersion(args.version.clone())
    };
    let result = store.mutate_release_policy(&repo.id(), mutation)?;
    let action = if ignore { "Ignored" } else { "Unignored" };
    println!(
        "{} {} for {}{}",
        action,
        args.version,
        repo.id(),
        no_op_suffix(&result)
    );
    Ok(())
}

fn channel(args: ChannelArgs) -> Result<()> {
    let repo = RepoRef::parse(&args.repo)?;
    let store = manifest_store(args.manifest)?;
    let channel: ReleaseChannel = args.channel.into();
    let result = store.mutate_release_policy(&repo.id(), PolicyMutation::SetChannel(channel))?;
    println!(
        "Set {} channel to {}{}",
        repo.id(),
        release_channel_label(channel),
        no_op_suffix(&result)
    );
    Ok(())
}

fn no_op_suffix(result: &PolicyMutationResult) -> &'static str {
    if result.changed { "" } else { " (unchanged)" }
}

fn rollback(args: RollbackArgs) -> Result<()> {
    rollback_with_confirmation(args, confirm_simple_execution)
}

fn rollback_with_confirmation<F>(args: RollbackArgs, confirm: F) -> Result<()>
where
    F: FnOnce(&str, bool) -> Result<()>,
{
    let repo = RepoRef::parse(&args.repo)?;
    let store = manifest_store(args.manifest)?;
    let manifest = store.load()?;
    let Some(current_app) = manifest.apps.iter().find(|app| app.id == repo.id()) else {
        println!("No managed app matched {}", repo.id());
        return Ok(());
    };

    // Capture both the current version and the presence or identity of the
    // snapshot before confirmation so every state change is detected later.
    let guard = RollbackGuard::from_app(current_app);
    if let Some(snapshot) = current_app.rollback.as_ref() {
        println!(
            "Rollback {}: {} -> {}",
            repo.id(),
            current_app.installed_version,
            snapshot.version
        );
    }

    confirm("rollback", args.yes)?;
    print_rollback_result(
        &repo.id(),
        rollback_repo_guarded(&store, &repo.id(), &guard, Language::En, None)?,
    )
}

fn print_rollback_result(repo_id: &str, result: Option<InstalledApp>) -> Result<()> {
    match result {
        Some(app) => {
            // The post-rollback snapshot is the former active version, so this
            // derives both sides from the core transaction's atomic result.
            let from_version = app
                .rollback
                .as_ref()
                .map(|snapshot| snapshot.version.as_str())
                .unwrap_or("unknown");
            println!(
                "Rolled back {} from {} to {}",
                repo_id, from_version, app.installed_version
            );
        }
        None => println!("No managed app matched {repo_id}"),
    }
    Ok(())
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
            let system_package = match (
                app.system_package_name.as_deref(),
                app.system_package_manager,
            ) {
                (Some(name), Some(manager)) => format!(" / {} ({:?})", name, manager),
                (Some(name), None) => format!(" / {}", name),
                _ => String::new(),
            };
            println!(
                "{} {} {} [{} / {}{}] {}",
                app.id,
                app.installed_version,
                app.asset_name,
                install_path_kind_label(app.install_path_kind),
                if app.uninstall_supported {
                    "可卸载"
                } else {
                    "需系统卸载"
                },
                system_package,
                app_integrity_summary(&app)
            );
        }
    }
    Ok(())
}

async fn update(args: UpdateArgs) -> Result<()> {
    let UpdateArgs {
        repo,
        all,
        version,
        manifest,
        release_fixture,
        artifact_fixture,
        yes,
    } = args;

    let store = manifest_store(manifest)?;
    let manifest = store.load()?;
    let runtime_config = runtime_config()?;
    let client = release_fixture
        .is_none()
        .then(|| release_client(Some(&runtime_config)))
        .transpose()?;

    if all {
        if manifest.apps.is_empty() {
            println!("No managed apps");
            return Ok(());
        }

        let mut plans = Vec::with_capacity(manifest.apps.len());
        for app in manifest.apps {
            let (repo, selection) =
                select_update_release(&app, release_fixture.as_ref(), client.as_ref(), None)
                    .await?;
            if selection.release.tag_name == app.installed_version {
                println!(
                    "Already at target version {} for {}",
                    selection.release.tag_name, app.id
                );
                continue;
            }
            let plan = build_plan_from_selection(
                &repo,
                &selection,
                &AssetMatcher::current(),
                release_fixture.is_some(),
                client.as_ref(),
            )
            .await?
            .with_selection_guard(InstallSelectionGuard::from_app(&app));
            plans.push((app.id.clone(), plan));
        }

        if plans.is_empty() {
            return Ok(());
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
                "Updated {} to {} at {} [{}] {}",
                outcome.app.id,
                outcome.app.installed_version,
                outcome.install_path.display(),
                install_path_kind_label(outcome.install_path_kind),
                app_integrity_summary(&outcome.app)
            );
        }

        return Ok(());
    }

    let Some(repo_input) = repo else {
        println!("provide a repo or --all");
        return Ok(());
    };

    let repo = RepoRef::parse(&repo_input)?;
    let app = manifest
        .apps
        .iter()
        .find(|app| app.id == repo.id())
        .with_context(|| format!("managed app `{}` is not installed", repo.id()))?;
    let (repo, selection) = select_update_release(
        app,
        release_fixture.as_ref(),
        client.as_ref(),
        version.as_deref(),
    )
    .await?;
    if selection.release.tag_name == app.installed_version {
        println!(
            "Already at target version {} for {}",
            selection.release.tag_name, app.id
        );
        return Ok(());
    }
    let plan = build_plan_from_selection(
        &repo,
        &selection,
        &AssetMatcher::current(),
        release_fixture.is_some(),
        client.as_ref(),
    )
    .await?
    .with_selection_guard(InstallSelectionGuard::from_app(app));
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
        "Updated {} to {} at {} [{}] {}",
        outcome.app.id,
        outcome.app.installed_version,
        outcome.install_path.display(),
        install_path_kind_label(outcome.install_path_kind),
        app_integrity_summary(&outcome.app)
    );
    Ok(())
}

async fn select_update_release(
    app: &releasedock_core::manifest::InstalledApp,
    release_fixture: Option<&PathBuf>,
    client: Option<&ReleaseClient>,
    version: Option<&str>,
) -> Result<(RepoRef, ReleaseSelection)> {
    let repo = RepoRef::parse(&app.repo_url)?;
    let selection = select_policy_release(
        &repo,
        release_fixture,
        client,
        &app.release_policy,
        Some(&app.installed_version),
        version,
    )
    .await?;
    Ok((repo, selection))
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
                "{} [{}] current {} -> target {} ({}, direction={})",
                app.id,
                status_label(app.status),
                app.current_version,
                app.latest_version,
                app.asset_name.as_deref().unwrap_or("no matching asset"),
                release_direction_label(app.direction)
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

    let recent_activities = ManifestStore::default()
        .ok()
        .and_then(|store| store.load().ok())
        .map(|manifest| {
            manifest
                .recent_lifecycle_events(&repo.id(), 5)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !recent_activities.is_empty() {
        println!();
        println!("Recent activity:");
        for event in recent_activities {
            println!("  - {}", event.summary);
            println!("    recorded at: {}", event.recorded_at.to_rfc3339());
            if let Some(version) = event.version.as_deref() {
                println!("    version: {version}");
            }
            if let Some(asset_name) = event.asset_name.as_deref() {
                println!("    asset: {asset_name}");
            }
            if let Some(error) = event.error.as_deref() {
                println!("    error: {error}");
            }
        }
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
    let client = if release_fixture.is_some() {
        None
    } else {
        let runtime_config = runtime_config()?;
        Some(release_client(Some(&runtime_config))?)
    };

    for app in apps {
        let repo = RepoRef::parse(&app.repo_url)?;
        let selection = select_policy_release(
            &repo,
            release_fixture,
            client.as_ref(),
            &app.release_policy,
            Some(&app.installed_version),
            None,
        )
        .await;
        match selection {
            Ok(selection) => {
                let direction = selection.direction;
                let release = selection.release;
                let current_version = app.installed_version.clone();
                let latest_version = release.tag_name.clone();
                let matched = matcher.select_best(&release).ok();
                let status = if current_version == latest_version {
                    CheckStatus::Current
                } else if matched.is_some() {
                    if direction == ReleaseDirection::Downgrade {
                        CheckStatus::DowngradeAvailable
                    } else {
                        CheckStatus::UpdateAvailable
                    }
                } else {
                    CheckStatus::MissingAsset
                };

                entries.push(CheckEntry {
                    id: app.id.clone(),
                    current_version,
                    latest_version,
                    direction,
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

async fn select_install_release(
    repo: &RepoRef,
    release_fixture: Option<&PathBuf>,
    client: Option<&ReleaseClient>,
    version: Option<&str>,
    prerelease: bool,
) -> Result<ReleaseSelection> {
    let policy = ReleasePolicy {
        channel: if prerelease {
            ReleaseChannel::Prerelease
        } else {
            ReleaseChannel::Stable
        },
        ..ReleasePolicy::default()
    };

    let releases = if let Some(path) = release_fixture {
        let releases = read_fixture_releases(path)?;
        if let Some(version) = version
            && !releases.iter().any(|release| release.tag_name == version)
        {
            anyhow::bail!("release fixture does not contain tag `{version}`");
        }
        releases
    } else {
        let client = client.context("release client is required for live selection")?;
        if let Some(version) = version {
            vec![
                client
                    .release_by_tag(repo, version)
                    .await?
                    .with_context(|| format!("release tag `{version}` was not found"))?,
            ]
        } else if prerelease {
            load_release_catalog(repo, None, Some(client), true).await?
        } else {
            vec![client.latest_release(repo).await?]
        }
    };

    ReleaseSelector::select(&releases, &policy, None, version).map_err(Into::into)
}

async fn select_policy_release(
    repo: &RepoRef,
    release_fixture: Option<&PathBuf>,
    client: Option<&ReleaseClient>,
    policy: &ReleasePolicy,
    current_version: Option<&str>,
    manual_version: Option<&str>,
) -> Result<ReleaseSelection> {
    let releases = load_release_catalog(repo, release_fixture, client, true).await?;
    if let (Some(path), Some(version)) = (release_fixture, manual_version)
        && !releases.iter().any(|release| release.tag_name == version)
    {
        anyhow::bail!(
            "release fixture {} does not contain tag `{version}`",
            path.display()
        );
    }
    ReleaseSelector::select(&releases, policy, current_version, manual_version).map_err(Into::into)
}

async fn load_release_catalog(
    repo: &RepoRef,
    release_fixture: Option<&PathBuf>,
    client: Option<&ReleaseClient>,
    all_pages: bool,
) -> Result<Vec<Release>> {
    const MAX_RELEASE_PAGES: u32 = 20;
    const MAX_RELEASES: usize = 2_000;

    if let Some(path) = release_fixture {
        let mut releases = read_fixture_releases(path)?;
        if !all_pages {
            releases.truncate(100);
        }
        if releases.len() > MAX_RELEASES {
            anyhow::bail!("release catalog exceeded maximum {MAX_RELEASES} releases");
        }
        return Ok(releases);
    }

    let client = client.context("release client is required for live catalog requests")?;
    let mut page_number = 1;
    let mut releases = Vec::new();
    let mut page_tag_sets = HashSet::new();
    loop {
        let page = client.releases_page(repo, page_number, 100).await?;
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
        releases.extend(page.releases);
        if !all_pages || !page.has_next_page {
            break;
        }
        if page_number >= MAX_RELEASE_PAGES {
            anyhow::bail!("release catalog exceeded maximum {MAX_RELEASE_PAGES} pages");
        }
        page_number += 1;
    }
    Ok(releases)
}

async fn build_plan_from_selection(
    repo: &RepoRef,
    selection: &ReleaseSelection,
    matcher: &AssetMatcher,
    is_fixture: bool,
    client: Option<&ReleaseClient>,
) -> Result<InstallPlan> {
    let matched = matcher.select_best(&selection.release)?;
    let integrity = if is_fixture {
        Default::default()
    } else {
        IntegrityVerifier::discover(
            client.context("release client is required for checksum discovery")?,
            &selection.release,
            &matched.asset,
        )
        .await?
    };
    let mut plan = InstallPlan::from_match(repo, &selection.release, &matched, Language::En)
        .with_integrity(integrity)
        .with_release_direction(selection.direction);

    if let Some(source) = plan.integrity.checksum_asset_name.as_deref() {
        plan.notes
            .push(format!("SHA-256 will be verified using `{source}`."));
    } else {
        plan.requires_user_confirmation = true;
        plan.notes.push(
            "No upstream SHA-256 checksum was found; verify the artifact source before continuing."
                .to_string(),
        );
    }
    Ok(plan)
}

fn config(args: ConfigArgs) -> Result<()> {
    let store = config_store()?;
    let mut current = store.load()?;

    match args.command {
        ConfigCommand::Get => {
            let mut display = current.clone();
            if display.github_token.is_some() {
                display.github_token = Some("***redacted***".to_string());
            }
            println!("{}", serde_json::to_string_pretty(&display)?);
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
    print_plan_preview(action, plan);
    if yes {
        return Ok(());
    }

    if !io::stdin().is_terminal() {
        anyhow::bail!("{} 需要交互确认；请使用 --yes 或在交互式终端中运行", action);
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

/// Plan details are output independently from prompt handling so `--yes` only
/// skips reading stdin; it never hides integrity or direction information.
fn print_plan_preview(action: &str, plan: &InstallPlan) {
    println!("准备执行 {}：", action);
    println!("  仓库: {}", plan.repo_id);
    println!("  版本: {}", plan.version);
    println!("  资产: {}", plan.asset_name);
    println!("  管理: {}", plan_management_label(plan));
    println!(
        "  Release direction: {}",
        release_direction_label(plan.release_direction)
    );
    match (
        plan.integrity.expected_sha256.as_deref(),
        plan.integrity.checksum_asset_name.as_deref(),
    ) {
        (Some(expected), Some(source)) => {
            println!(
                "  SHA-256: pending verification from {source} (expected {})",
                digest_summary(expected)
            );
        }
        _ => println!("  SHA-256: unverified; no upstream checksum was found"),
    }
    for note in &plan.notes {
        println!("  提示: {}", note);
    }
    if matches!(
        plan.management_kind,
        InstallManagementKind::SystemPackage | InstallManagementKind::ExternalInstaller
    ) {
        println!("  该安装包需要系统权限确认。");
    }
}

fn confirm_simple_execution(action: &str, yes: bool) -> Result<()> {
    if yes {
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        anyhow::bail!("{action} requires confirmation; use --yes in non-interactive mode");
    }

    print!("Continue with {action}? [y/N] ");
    io::stdout()
        .flush()
        .context("failed to flush confirmation prompt")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read confirmation")?;
    if matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        return Ok(());
    }
    anyhow::bail!("cancelled {action}")
}

fn confirm_bulk_execution(action: &str, plans: &[(String, InstallPlan)], yes: bool) -> Result<()> {
    print_bulk_plan_preview(action, plans);
    if yes {
        return Ok(());
    }

    if !io::stdin().is_terminal() {
        anyhow::bail!("{} 需要交互确认；请使用 --yes 或在交互式终端中运行", action);
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

fn print_bulk_plan_preview(action: &str, plans: &[(String, InstallPlan)]) {
    println!("准备执行 {}：", action);
    println!("  共 {} 个项目", plans.len());
    println!("  管理方式: {}", plan_management_summary(plans));
    for (index, (app_id, plan)) in plans.iter().enumerate() {
        println!(
            "  {}. {} -> {} / {} [{}]",
            index + 1,
            app_id,
            plan.version,
            plan.asset_name,
            plan_management_label(plan)
        );
        for note in &plan.notes {
            println!("     提示: {}", note);
        }
        println!(
            "     Release direction: {}",
            release_direction_label(plan.release_direction)
        );
        match (
            plan.integrity.expected_sha256.as_deref(),
            plan.integrity.checksum_asset_name.as_deref(),
        ) {
            (Some(expected), Some(source)) => println!(
                "     SHA-256: pending verification from {source} (expected {})",
                digest_summary(expected)
            ),
            _ => println!("     SHA-256: unverified; no upstream checksum was found"),
        }
    }
    if plans.iter().any(|(_, plan)| {
        matches!(
            plan.management_kind,
            InstallManagementKind::SystemPackage | InstallManagementKind::ExternalInstaller
        )
    }) {
        println!("  其中包含系统包或外部安装器，执行时会继续触发系统权限确认。");
    }
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
        direction: ReleaseDirection::Unknown,
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
    direction: ReleaseDirection,
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
    DowngradeAvailable,
    MissingAsset,
    FetchFailed,
}

fn status_label(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Current => "最新",
        CheckStatus::UpdateAvailable => "有更新",
        CheckStatus::DowngradeAvailable => "可降级",
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

fn plan_management_label(plan: &InstallPlan) -> String {
    match plan.system_package_manager {
        Some(manager) => format!(
            "{} ({})",
            management_kind_label(plan.management_kind),
            package_manager_label(manager)
        ),
        None => management_kind_label(plan.management_kind).to_string(),
    }
}

fn plan_management_summary(plans: &[(String, InstallPlan)]) -> String {
    let managed_local = plans
        .iter()
        .filter(|(_, plan)| matches!(plan.management_kind, InstallManagementKind::ManagedLocal))
        .count();
    let system_package = plans
        .iter()
        .filter(|(_, plan)| matches!(plan.management_kind, InstallManagementKind::SystemPackage))
        .count();
    let external_installer = plans
        .iter()
        .filter(|(_, plan)| {
            matches!(
                plan.management_kind,
                InstallManagementKind::ExternalInstaller
            )
        })
        .count();

    format!(
        "本地托管 {} 个，系统包 {} 个，外部安装器 {} 个",
        managed_local, system_package, external_installer
    )
}

fn management_kind_label(kind: InstallManagementKind) -> &'static str {
    match kind {
        InstallManagementKind::ManagedLocal => "本地托管",
        InstallManagementKind::SystemPackage => "系统包",
        InstallManagementKind::ExternalInstaller => "外部安装器",
    }
}

fn package_manager_label(manager: SystemPackageManager) -> &'static str {
    match manager {
        SystemPackageManager::Debian => "apt",
        SystemPackageManager::Rpm => "dnf",
        SystemPackageManager::Pacman => "pacman",
    }
}

fn release_direction_label(direction: ReleaseDirection) -> &'static str {
    match direction {
        ReleaseDirection::Upgrade => "upgrade",
        ReleaseDirection::Downgrade => "downgrade",
        ReleaseDirection::Reinstall => "reinstall",
        ReleaseDirection::Unknown => "unknown",
    }
}

fn release_channel_label(channel: ReleaseChannel) -> &'static str {
    match channel {
        ReleaseChannel::Stable => "stable",
        ReleaseChannel::Prerelease => "prerelease",
    }
}

fn app_integrity_summary(app: &InstalledApp) -> String {
    let status = match app.integrity_status {
        Some(IntegrityStatus::VerifiedChecksum) => "verifiedChecksum",
        Some(IntegrityStatus::RecordedOnly) => "recordedOnly",
        None => "unknown",
    };
    let digest = app
        .artifact_sha256
        .as_deref()
        .map(digest_summary)
        .unwrap_or_else(|| "none".to_string());
    let source = app.checksum_asset_name.as_deref().unwrap_or("none");
    format!("integrity={status} sha256={digest} source={source}")
}

fn digest_summary(digest: &str) -> String {
    digest.chars().take(12).collect()
}

fn read_fixture_release(path: &PathBuf) -> Result<Release> {
    read_fixture_releases(path)?
        .into_iter()
        .next()
        .with_context(|| format!("release fixture {} is empty", path.display()))
}

fn read_fixture_releases(path: &PathBuf) -> Result<Vec<Release>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read release fixture {}", path.display()))?;
    let fixture: ReleaseFixture = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse release fixture {}", path.display()))?;
    Ok(match fixture {
        ReleaseFixture::Many(releases) => releases,
        ReleaseFixture::One(release) => vec![release],
    })
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ReleaseFixture {
    Many(Vec<Release>),
    One(Release),
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

impl From<CliReleaseChannel> for ReleaseChannel {
    fn from(value: CliReleaseChannel) -> Self {
        match value {
            CliReleaseChannel::Stable => ReleaseChannel::Stable,
            CliReleaseChannel::Prerelease => ReleaseChannel::Prerelease,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CheckStatus, RollbackArgs, failed_check_entry, management_kind_label,
        package_manager_label, plan_management_summary, rollback_with_confirmation, status_label,
    };
    use releasedock_core::{
        asset_matcher::InstallType,
        install_plan::{InstallManagementKind, InstallPlan},
        manifest::{
            InstallPathKind, InstalledApp, ManifestStore, RollbackSnapshot, SystemPackageManager,
        },
    };

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

    #[test]
    fn labels_install_management_for_confirmation_output() {
        assert_eq!(
            management_kind_label(InstallManagementKind::ManagedLocal),
            "本地托管"
        );
        assert_eq!(
            management_kind_label(InstallManagementKind::SystemPackage),
            "系统包"
        );
        assert_eq!(
            management_kind_label(InstallManagementKind::ExternalInstaller),
            "外部安装器"
        );
        assert_eq!(
            package_manager_label(SystemPackageManager::Pacman),
            "pacman"
        );
    }

    #[test]
    fn summarizes_bulk_plan_management_kinds() {
        let plans = vec![
            (
                "owner/local".to_string(),
                sample_plan("owner/local", InstallManagementKind::ManagedLocal, None),
            ),
            (
                "owner/pkg".to_string(),
                sample_plan(
                    "owner/pkg",
                    InstallManagementKind::SystemPackage,
                    Some(SystemPackageManager::Pacman),
                ),
            ),
            (
                "owner/installer".to_string(),
                sample_plan(
                    "owner/installer",
                    InstallManagementKind::ExternalInstaller,
                    None,
                ),
            ),
        ];

        let summary = plan_management_summary(&plans);

        assert!(summary.contains("本地托管 1 个"));
        assert!(summary.contains("系统包 1 个"));
        assert!(summary.contains("外部安装器 1 个"));
    }

    #[test]
    fn rollback_confirmation_guard_rejects_snapshot_created_while_waiting() {
        let temp = tempfile::tempdir().unwrap();
        let manifest_path = temp.path().join("apps.json");
        let store = ManifestStore::at_path(manifest_path.clone());
        let active_dir = temp.path().join("apps/owner-project");
        let active_path = active_dir.join("demo.AppImage");
        let snapshot_dir = temp
            .path()
            .join("rollbacks/owner-project/created-during-confirm");
        let snapshot_path = snapshot_dir.join("demo.AppImage");
        std::fs::create_dir_all(&active_dir).unwrap();
        std::fs::create_dir_all(&snapshot_dir).unwrap();
        std::fs::write(&active_path, b"active version").unwrap();
        std::fs::write(&snapshot_path, b"concurrent snapshot").unwrap();
        let mut app = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v2.0.0",
            "demo.AppImage",
            active_path.clone(),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        app.managed_root = Some(temp.path().to_path_buf());
        store.save_apps(&[app]).unwrap();

        let confirm_manifest_path = manifest_path.clone();
        let snapshot_dir_for_confirm = snapshot_dir.clone();
        let snapshot_path_for_confirm = snapshot_path.clone();
        let error = rollback_with_confirmation(
            RollbackArgs {
                repo: "owner/project".to_string(),
                manifest: Some(manifest_path),
                yes: false,
            },
            move |action, yes| {
                assert_eq!(action, "rollback");
                assert!(!yes);
                let confirm_store = ManifestStore::at_path(confirm_manifest_path);
                let mut manifest = confirm_store.load().unwrap();
                let app = &mut manifest.apps[0];
                app.rollback = Some(RollbackSnapshot {
                    version: "v1.0.0".to_string(),
                    asset_name: "demo.AppImage".to_string(),
                    install_path: snapshot_path_for_confirm,
                    launch_path: None,
                    install_type: InstallType::AppImage,
                    artifact_sha256: None,
                    integrity_status: None,
                    checksum_asset_name: None,
                    snapshot_path: snapshot_dir_for_confirm,
                    installed_at: app.installed_at,
                });
                confirm_store.save(&manifest).unwrap();
                Ok(())
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("stale rollback plan"));
        assert_eq!(std::fs::read(active_path).unwrap(), b"active version");
        assert_eq!(
            std::fs::read(snapshot_dir.join("demo.AppImage")).unwrap(),
            b"concurrent snapshot"
        );
    }

    fn sample_plan(
        repo_id: &str,
        management_kind: InstallManagementKind,
        system_package_manager: Option<SystemPackageManager>,
    ) -> InstallPlan {
        InstallPlan {
            repo_id: repo_id.to_string(),
            repo_url: format!("https://github.com/{repo_id}"),
            version: "v1.0.0".to_string(),
            asset_name: "demo.bin".to_string(),
            download_url: "https://example.invalid/demo.bin".to_string(),
            install_type: InstallType::Unknown,
            management_kind,
            system_package_manager,
            requires_user_confirmation: management_kind != InstallManagementKind::ManagedLocal,
            integrity: releasedock_core::integrity::IntegrityPlan::default(),
            release_direction: Default::default(),
            selection_guard: None,
            target_policy: None,
            notes: Vec::new(),
        }
    }
}

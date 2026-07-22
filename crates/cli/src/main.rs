use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use ghrm_core::{
    asset_matcher::{Architecture, AssetMatcher, OperatingSystem},
    install_plan::InstallPlan,
    manifest::ManifestStore,
    release::{Release, ReleaseClient},
    repo::RepoRef,
};

#[derive(Debug, Parser)]
#[command(name = "ghrm")]
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
    Config,
}

#[derive(Debug, Args)]
struct InstallArgs {
    repo: String,
    #[arg(long)]
    release_fixture: Option<PathBuf>,
    #[arg(long, value_enum)]
    os: Option<CliOs>,
    #[arg(long, value_enum)]
    arch: Option<CliArch>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ManifestArgs {
    #[arg(long)]
    manifest: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct UpdateArgs {
    repo: Option<String>,
    #[arg(long)]
    all: bool,
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliOs {
    Windows,
    Linux,
    Macos,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliArch {
    X64,
    Arm64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install(args) => install(args).await,
        Commands::List(args) => list(args),
        Commands::Check(_) => {
            println!("check is planned for the next implementation slice");
            Ok(())
        }
        Commands::Update(args) => {
            if args.all {
                println!("update --all is planned for the next implementation slice");
            } else if let Some(repo) = args.repo {
                println!("update {repo} is planned for the next implementation slice");
            } else {
                println!("provide a repo or --all");
            }
            Ok(())
        }
        Commands::Uninstall(args) => {
            println!(
                "uninstall {} is planned; manifest override: {}",
                args.repo,
                args.manifest
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "default".to_string())
            );
            Ok(())
        }
        Commands::Info(args) => info(args).await,
        Commands::Doctor => {
            println!("ghrm doctor: core CLI is available");
            Ok(())
        }
        Commands::Config => {
            println!("config is planned for the next implementation slice");
            Ok(())
        }
    }
}

async fn install(args: InstallArgs) -> Result<()> {
    let repo = RepoRef::parse(&args.repo)?;
    let release = match args.release_fixture {
        Some(path) => read_fixture_release(&path)?,
        None => {
            let token = std::env::var("GITHUB_TOKEN").ok();
            ReleaseClient::new(token.as_deref())?
                .latest_release(&repo)
                .await?
        }
    };

    let matcher = match (args.os, args.arch) {
        (Some(os), Some(arch)) => AssetMatcher::new(os.into(), arch.into()),
        _ => AssetMatcher::current(),
    };
    let matched = matcher.select_best(&release)?;
    let plan = InstallPlan::from_match(&repo, &release, &matched);

    if args.json {
        println!("{}", serde_json::to_string(&plan)?);
    } else {
        println!(
            "Install plan: {} {} using {}",
            plan.repo_id, plan.version, plan.asset_name
        );
        if plan.requires_user_confirmation {
            println!("This asset requires user confirmation before execution.");
        }
    }

    Ok(())
}

fn list(args: ManifestArgs) -> Result<()> {
    let store = manifest_store(args.manifest)?;
    let manifest = store.load()?;

    if args.json {
        println!("{}", serde_json::to_string(&manifest)?);
        return Ok(());
    }

    if manifest.apps.is_empty() {
        println!("No managed apps");
    } else {
        for app in manifest.apps {
            println!("{} {} {}", app.id, app.installed_version, app.asset_name);
        }
    }
    Ok(())
}

async fn info(args: InfoArgs) -> Result<()> {
    let repo = RepoRef::parse(&args.repo)?;
    let release = match args.release_fixture {
        Some(path) => read_fixture_release(&path)?,
        None => {
            let token = std::env::var("GITHUB_TOKEN").ok();
            ReleaseClient::new(token.as_deref())?
                .latest_release(&repo)
                .await?
        }
    };

    if args.json {
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

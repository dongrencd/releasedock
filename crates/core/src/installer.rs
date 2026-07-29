use std::{
    fs, io,
    path::{Component, Path, PathBuf},
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
    install_plan::{InstallManagementKind, InstallPlan, InstallSelectionGuard},
    integrity::{IntegrityStatus, sha256_file, verify_file_sha256},
    manifest::{
        InstallPathKind, InstalledApp, LifecycleAction, LifecycleEvent, ManagedTransactionJournal,
        ManagedTransactionMove, ManagedTransactionOperation, Manifest, ManifestStore,
        RollbackSnapshot, SystemPackageManager,
    },
    release::ReleaseClient,
    release_policy::ReleaseDirection,
    repo::RepoRef,
    windows_install_registry::discover_installation,
};

pub type ProgressReporter = Arc<dyn Fn(TaskProgress) + Send + Sync>;

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    if let Err(record_error) = manifest_store.append_lifecycle_event(event) {
        eprintln!("failed to record failed lifecycle event: {record_error:#}");
    }
}

#[allow(clippy::too_many_arguments)]
fn record_lifecycle_failure_unlocked(
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
    let result = manifest_store.load_unlocked().and_then(|mut manifest| {
        manifest.append_lifecycle_event(event);
        manifest_store.save_unlocked(&manifest)
    });
    if let Err(record_error) = result {
        eprintln!("failed to record failed lifecycle event: {record_error:#}");
    }
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

/// 重新探测系统安装器的真实安装位置，并把 manifest 中的记录切到可打开的应用路径。
///
/// 这一步只更新仓库记录，不会重新运行安装器，也不会执行卸载命令。
pub fn adopt_system_installer_app(
    manifest_store: &ManifestStore,
    repo: &RepoRef,
) -> Result<InstalledApp> {
    adopt_system_installer_app_with(manifest_store, repo, discover_installation)
}

fn load_adoptable_system_installer(
    manifest_store: &ManifestStore,
    repo: &RepoRef,
) -> Result<(crate::manifest::Manifest, usize, String)> {
    let manifest = manifest_store.load()?;
    let repo_id = repo.id();
    let Some(app_index) = manifest.apps.iter().position(|app| app.id == repo_id) else {
        anyhow::bail!("no managed app matched {}", repo_id);
    };
    if !matches!(
        manifest.apps[app_index].install_path_kind,
        InstallPathKind::SystemInstaller
    ) {
        anyhow::bail!("only system installer entries can be adopted");
    }

    Ok((manifest, app_index, repo_id))
}

/// 可注入 discovery 的接管入口，便于单测验证 manifest 更新行为。
#[cfg(target_os = "windows")]
pub fn adopt_system_installer_app_with<F>(
    manifest_store: &ManifestStore,
    repo: &RepoRef,
    discover: F,
) -> Result<InstalledApp>
where
    F: FnOnce(
        &[&str],
        &[&str],
    ) -> Result<Option<crate::windows_install_registry::WindowsInstallDiscovery>>,
{
    let (mut manifest, app_index, repo_id) = load_adoptable_system_installer(manifest_store, repo)?;
    let app = &manifest.apps[app_index];
    let installer_path = app
        .installer_path
        .clone()
        .unwrap_or_else(|| app.install_path.clone());
    let repo_tail = repo_id.rsplit('/').next().unwrap_or_default().to_string();
    let candidate_names = [
        repo.name.as_str(),
        repo_tail.as_str(),
        app.asset_name.as_str(),
    ];
    let candidate_versions = [app.installed_version.as_str()];
    let Some(discovery) = discover(&candidate_names, &candidate_versions)? else {
        anyhow::bail!("no matching Windows installation was found for {}", repo_id);
    };

    let app = &mut manifest.apps[app_index];
    app.install_path = discovery.install_path;
    app.launch_path = discovery.launch_path;
    app.installer_path = Some(installer_path);
    let adopted_app = app.clone();
    manifest_store.save(&manifest)?;

    Ok(adopted_app)
}

/// 非 Windows 平台保留同签名入口，让 CLI/Tauri 调用得到明确的平台错误。
#[cfg(not(target_os = "windows"))]
pub fn adopt_system_installer_app_with<F>(
    manifest_store: &ManifestStore,
    repo: &RepoRef,
    _discover: F,
) -> Result<InstalledApp>
where
    F: FnOnce(
        &[&str],
        &[&str],
    ) -> Result<Option<crate::windows_install_registry::WindowsInstallDiscovery>>,
{
    let _ = load_adoptable_system_installer(manifest_store, repo)?;
    anyhow::bail!("system installer adoption is only available on Windows");
}

/// 托管安装以整个仓库 active 目录为边界，文件型资产和解压目录共享同一提交协议。
struct ManagedInstallTransaction {
    active_dir: PathBuf,
    staged_dir: PathBuf,
    rollback_dir: PathBuf,
    previous_app: Option<InstalledApp>,
}

#[derive(Debug)]
struct ManagedTombstone {
    original: PathBuf,
    pending: PathBuf,
    committed: PathBuf,
}

impl ManagedTombstone {
    fn new(original: &Path, purpose: &str) -> Self {
        Self {
            original: original.to_path_buf(),
            pending: staging_path(original, &format!("releasedock-pending-{purpose}")),
            committed: staging_path(original, &format!("releasedock-committed-{purpose}")),
        }
    }
}

impl ManagedInstallTransaction {
    fn new(
        active_dir: PathBuf,
        rollback_dir: PathBuf,
        previous_app: Option<InstalledApp>,
    ) -> Result<Self> {
        let parent = active_dir.parent().ok_or_else(|| {
            anyhow::anyhow!("managed active path {} has no parent", active_dir.display())
        })?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create install root {}", parent.display()))?;
        cleanup_managed_gc_tombstones(parent);
        if rollback_dir.exists() {
            cleanup_managed_gc_tombstones(&rollback_dir);
        }
        let staged_dir = staging_path(&active_dir, "staging");
        if staged_dir.exists() {
            remove_path(&staged_dir)?;
        }
        fs::create_dir_all(&staged_dir).with_context(|| {
            format!(
                "failed to create managed staging directory {}",
                staged_dir.display()
            )
        })?;

        Ok(Self {
            active_dir,
            staged_dir,
            rollback_dir,
            previous_app,
        })
    }

    fn install_path(&self, install_type: InstallType, asset_name: &str) -> PathBuf {
        match install_type {
            InstallType::AppImage | InstallType::Executable => self.active_dir.join(asset_name),
            InstallType::PortableArchive | InstallType::Archive => self.active_dir.clone(),
            _ => self.active_dir.join(asset_name),
        }
    }

    fn commit_with<P, R, D>(
        self,
        manifest_store: &ManifestStore,
        manifest: &Manifest,
        app: &mut InstalledApp,
        success_event: LifecycleEvent,
        mut persist: P,
        mut rename: R,
        mut cleanup: D,
    ) -> Result<()>
    where
        P: FnMut(&Manifest) -> Result<()>,
        R: FnMut(&Path, &Path) -> Result<()>,
        D: FnMut(&Path) -> Result<()>,
    {
        let previous_managed = self
            .previous_app
            .as_ref()
            .filter(|previous| matches!(previous.install_path_kind, InstallPathKind::ManagedPath));
        let previous_rollback = previous_managed.and_then(|previous| previous.rollback.clone());
        let mut moves = Vec::new();

        if let Some(previous_rollback) = previous_rollback.as_ref() {
            let tombstone = ManagedTombstone::new(&previous_rollback.snapshot_path, "stale");
            moves.push(ManagedTransactionMove {
                from: tombstone.original,
                to: tombstone.pending,
                discard_path: Some(tombstone.committed),
            });
        }

        if let Some(previous) = previous_managed {
            if !self.active_dir.exists() {
                anyhow::bail!(
                    "managed active path {} is missing for {}",
                    self.active_dir.display(),
                    previous.id
                );
            }
            prepare_managed_rollback_directory(app, &self.rollback_dir)?;
            let snapshot_path = self.rollback_dir.join(unique_staging_token());
            app.rollback = Some(rollback_snapshot(previous, snapshot_path.clone()));
            moves.push(ManagedTransactionMove {
                from: self.active_dir.clone(),
                to: snapshot_path,
                discard_path: None,
            });
        }
        moves.push(ManagedTransactionMove {
            from: self.staged_dir.clone(),
            to: self.active_dir.clone(),
            discard_path: None,
        });

        let trusted_root = app
            .managed_root
            .clone()
            .ok_or_else(|| anyhow::anyhow!("managed install is missing its trusted root"))?;
        let mut journal = ManagedTransactionJournal {
            repo_id: app.id.clone(),
            operation: ManagedTransactionOperation::Install,
            trusted_root,
            before_app: previous_managed.cloned(),
            after_app: Some(app.clone()),
            moves,
            completed_moves: 0,
            manifest_committed: false,
        };
        manifest_store.save_transaction_journal_unlocked(&journal)?;

        for move_index in 0..journal.moves.len() {
            if let Err(move_error) =
                execute_managed_journal_move(manifest_store, &mut journal, move_index, &mut rename)
            {
                if let Err(restore_error) =
                    restore_managed_transaction_moves_with(&journal, &mut rename)
                {
                    return Err(move_error.context(format!(
                        "also failed to restore managed install moves: {restore_error:#}"
                    )));
                }
                manifest_store.remove_transaction_journal_unlocked()?;
                if self.staged_dir.exists() {
                    remove_path(&self.staged_dir)?;
                }
                if previous_managed.is_some() {
                    prune_empty_dir(&self.rollback_dir)?;
                }
                return Err(move_error);
            }
        }

        let next_manifest = manifest_with_app_and_event(manifest, app.clone(), success_event);
        if let Err(persist_error) = persist(&next_manifest) {
            if let Err(restore_error) =
                restore_managed_transaction_moves_with(&journal, &mut rename)
            {
                return Err(persist_error.context(format!(
                    "also failed to restore managed install moves: {restore_error:#}"
                )));
            }
            manifest_store.remove_transaction_journal_unlocked()?;
            if self.staged_dir.exists() {
                remove_path(&self.staged_dir)?;
            }
            if previous_managed.is_some() {
                prune_empty_dir(&self.rollback_dir)?;
            }
            return Err(persist_error);
        }

        journal.manifest_committed = true;
        if let Err(error) = manifest_store.save_transaction_journal_unlocked(&journal) {
            eprintln!(
                "managed install committed but journal mark failed for {}: {error:#}",
                app.id
            );
            return Ok(());
        }
        if let Err(error) =
            finalize_managed_transaction_discards_with(&journal, &mut rename, &mut cleanup)
        {
            eprintln!(
                "managed install committed but discard finalization failed for {}: {error:#}",
                app.id
            );
            return Ok(());
        }
        if let Err(error) = manifest_store.remove_transaction_journal_unlocked() {
            eprintln!(
                "managed install committed but journal cleanup failed for {}: {error:#}",
                app.id
            );
        }
        Ok(())
    }
}

#[cfg(test)]
fn commit_managed_tombstone<R, D>(
    rename: &mut R,
    cleanup: &mut D,
    tombstone: &ManagedTombstone,
    operation: &str,
) where
    R: FnMut(&Path, &Path) -> Result<()>,
    D: FnMut(&Path) -> Result<()>,
{
    // 只有 manifest 已提交的数据才能进入 GC 可识别状态。状态切换或清理失败
    // 不得反向报告事务失败，因为此时磁盘和 manifest 的业务状态已经提交。
    if let Err(error) = rename(&tombstone.pending, &tombstone.committed) {
        eprintln!(
            "{operation} committed but tombstone state transition failed for {}: {error:#}",
            tombstone.pending.display()
        );
        return;
    }
    if let Err(error) = cleanup(&tombstone.committed) {
        eprintln!(
            "{operation} committed but tombstone cleanup failed for {}: {error:#}",
            tombstone.committed.display()
        );
    }
}

fn rollback_snapshot(previous: &InstalledApp, snapshot_path: PathBuf) -> RollbackSnapshot {
    RollbackSnapshot {
        version: previous.installed_version.clone(),
        asset_name: previous.asset_name.clone(),
        install_path: previous.install_path.clone(),
        launch_path: previous.launch_path.clone(),
        install_type: previous.install_type,
        artifact_sha256: previous.artifact_sha256.clone(),
        integrity_status: previous.integrity_status,
        checksum_asset_name: previous.checksum_asset_name.clone(),
        snapshot_path,
        installed_at: previous.installed_at,
    }
}

fn manifest_with_app_and_event(
    manifest: &Manifest,
    app: InstalledApp,
    event: LifecycleEvent,
) -> Manifest {
    let mut next = manifest.clone();
    next.apps.retain(|existing| existing.id != app.id);
    next.apps.push(app);
    next.append_lifecycle_event(event);
    next
}

fn rename_managed_path(source: &Path, target: &Path) -> Result<()> {
    fs::rename(source, target).with_context(|| {
        format!(
            "failed to move managed path {} to {}",
            source.display(),
            target.display()
        )
    })
}

fn is_managed_gc_tombstone(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.') && name.contains(".releasedock-committed-"))
}

fn cleanup_managed_gc_tombstones(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_managed_gc_tombstone(&path) {
            continue;
        }
        if let Err(error) = remove_path(&path) {
            eprintln!(
                "failed to clean managed transaction tombstone {}: {error:#}",
                path.display()
            );
        }
    }
}

fn recover_managed_transaction_unlocked(manifest_store: &ManifestStore) -> Result<()> {
    let Some(mut journal) = manifest_store.load_transaction_journal_unlocked()? else {
        return Ok(());
    };
    validate_managed_transaction_journal(manifest_store, &journal)?;
    reconcile_completed_moves(&mut journal)?;
    let manifest = manifest_store.load_unlocked()?;
    let current_app = manifest
        .apps
        .iter()
        .find(|app| app.id == journal.repo_id)
        .cloned();

    if current_app == journal.before_app {
        restore_managed_transaction_moves(&journal)?;
        manifest_store.remove_transaction_journal_unlocked()?;
        return Ok(());
    }
    if current_app == journal.after_app {
        if journal.completed_moves != journal.moves.len() {
            anyhow::bail!(
                "managed transaction journal for {} is committed but only {}/{} moves are complete",
                journal.repo_id,
                journal.completed_moves,
                journal.moves.len()
            );
        }
        finalize_managed_transaction_discards(&journal)?;
        manifest_store.remove_transaction_journal_unlocked()?;
        return Ok(());
    }

    anyhow::bail!(
        "managed transaction journal conflict for {}: manifest matches neither before nor after state",
        journal.repo_id
    )
}

fn validate_managed_transaction_journal(
    _manifest_store: &ManifestStore,
    journal: &ManagedTransactionJournal,
) -> Result<()> {
    if journal.completed_moves > journal.moves.len() {
        anyhow::bail!(
            "managed transaction journal for {} has invalid completed move count",
            journal.repo_id
        );
    }
    let shape_is_valid = match journal.operation {
        ManagedTransactionOperation::Install => journal.after_app.is_some(),
        ManagedTransactionOperation::Rollback => {
            journal.before_app.is_some() && journal.after_app.is_some()
        }
        ManagedTransactionOperation::Uninstall => {
            journal.before_app.is_some() && journal.after_app.is_none()
        }
    };
    if !shape_is_valid {
        anyhow::bail!(
            "managed transaction journal for {} has invalid before/after state",
            journal.repo_id
        );
    }
    ensure_real_directory(&journal.trusted_root, "journal managed root", false)?;
    let canonical_root = fs::canonicalize(&journal.trusted_root)?;
    if canonical_root != journal.trusted_root {
        anyhow::bail!(
            "managed transaction journal root {} is not canonical",
            journal.trusted_root.display()
        );
    }
    let app_roots = [journal.before_app.as_ref(), journal.after_app.as_ref()]
        .into_iter()
        .flatten()
        .filter_map(|app| app.managed_root.as_ref())
        .collect::<Vec<_>>();
    for app_root in app_roots {
        if fs::canonicalize(app_root)? != canonical_root {
            anyhow::bail!(
                "managed transaction journal root does not match app managed root for {}",
                journal.repo_id
            );
        }
    }
    for app in [journal.before_app.as_ref(), journal.after_app.as_ref()]
        .into_iter()
        .flatten()
    {
        if app.id != journal.repo_id {
            anyhow::bail!("managed transaction journal app identity mismatch");
        }
    }
    for managed_move in &journal.moves {
        validate_journal_managed_path(&canonical_root, &managed_move.from)?;
        validate_journal_managed_path(&canonical_root, &managed_move.to)?;
        if let Some(discard_path) = managed_move.discard_path.as_ref() {
            validate_journal_managed_path(&canonical_root, discard_path)?;
        }
    }
    validate_managed_transaction_move_template(journal, &canonical_root)?;
    Ok(())
}

fn validate_managed_transaction_move_template(
    journal: &ManagedTransactionJournal,
    trusted_root: &Path,
) -> Result<()> {
    let repo = RepoRef::parse(&journal.repo_id)?;
    let repo_dir_name = format!("{}-{}", repo.owner, repo.name);
    let active_dir = trusted_root.join("apps").join(&repo_dir_name);
    let rollback_dir = trusted_root.join("rollbacks").join(&repo_dir_name);
    for app in [journal.before_app.as_ref(), journal.after_app.as_ref()]
        .into_iter()
        .flatten()
    {
        let app_repo = RepoRef::parse(&app.repo_url)?;
        if app.id != journal.repo_id || app_repo.id() != journal.repo_id {
            anyhow::bail!("managed transaction move template has mismatched app identity");
        }
        validate_journal_app_paths(app, trusted_root, &active_dir)?;
    }

    match journal.operation {
        ManagedTransactionOperation::Install => {
            validate_install_move_template(journal, &active_dir, &rollback_dir)
        }
        ManagedTransactionOperation::Rollback => {
            validate_rollback_move_template(journal, &active_dir, &rollback_dir)
        }
        ManagedTransactionOperation::Uninstall => {
            validate_uninstall_move_template(journal, &active_dir, &rollback_dir)
        }
    }
    .with_context(|| {
        format!(
            "managed transaction move template is invalid for {}",
            journal.repo_id
        )
    })
}

fn validate_journal_app_paths(
    app: &InstalledApp,
    trusted_root: &Path,
    active_dir: &Path,
) -> Result<()> {
    if let Some(managed_root) = app.managed_root.as_ref() {
        if managed_root != trusted_root {
            anyhow::bail!("journal app managed root does not match trusted root");
        }
    } else if infer_legacy_managed_root_from_metadata(app)? != trusted_root {
        anyhow::bail!("legacy journal app does not derive the trusted root");
    }
    let app_active = managed_active_dir(app)?;
    if app_active != active_dir {
        anyhow::bail!("journal app active path does not match repository active path");
    }
    match app.install_type {
        InstallType::AppImage | InstallType::Executable
            if app.install_path.parent() != Some(active_dir) =>
        {
            anyhow::bail!("journal managed file is not a direct active child")
        }
        InstallType::PortableArchive | InstallType::Archive if app.install_path != active_dir => {
            anyhow::bail!("journal managed archive does not equal active path")
        }
        _ => {}
    }
    Ok(())
}

fn validate_install_move_template(
    journal: &ManagedTransactionJournal,
    active_dir: &Path,
    rollback_dir: &Path,
) -> Result<()> {
    let after = journal
        .after_app
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("install journal has no after app"))?;
    if journal.before_app.is_none() {
        if after.rollback.is_some() {
            anyhow::bail!("first install journal must not contain a rollback snapshot");
        }
        if journal.moves.len() != 1 {
            anyhow::bail!("first install journal must have exactly one move");
        }
        return validate_staged_promotion(&journal.moves[0], active_dir);
    }

    let before = journal.before_app.as_ref().expect("checked above");
    let after_snapshot = after
        .rollback
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("managed update journal has no after snapshot"))?;
    ensure_lexical_direct_child(
        rollback_dir,
        &after_snapshot.snapshot_path,
        "update snapshot",
    )?;
    let mut move_index = 0;
    if let Some(old_snapshot) = before.rollback.as_ref() {
        ensure_lexical_direct_child(
            rollback_dir,
            &old_snapshot.snapshot_path,
            "previous update snapshot",
        )?;
        if journal.moves.len() != 3 {
            anyhow::bail!("update with old snapshot must have exactly three moves");
        }
        let stale_move = &journal.moves[move_index];
        if stale_move.from != old_snapshot.snapshot_path {
            anyhow::bail!("stale snapshot move source does not match before app");
        }
        validate_tombstone_move(
            stale_move,
            "releasedock-pending-stale",
            "releasedock-committed-stale",
        )?;
        move_index += 1;
    } else if journal.moves.len() != 2 {
        anyhow::bail!("update without old snapshot must have exactly two moves");
    }
    let snapshot_move = &journal.moves[move_index];
    if snapshot_move.from != active_dir
        || snapshot_move.to != after_snapshot.snapshot_path
        || snapshot_move.discard_path.is_some()
    {
        anyhow::bail!("active-to-snapshot move does not match update metadata");
    }
    validate_staged_promotion(&journal.moves[move_index + 1], active_dir)
}

fn validate_rollback_move_template(
    journal: &ManagedTransactionJournal,
    active_dir: &Path,
    rollback_dir: &Path,
) -> Result<()> {
    if journal.moves.len() != 3 {
        anyhow::bail!("rollback journal must have exactly three moves");
    }
    let before_snapshot = journal
        .before_app
        .as_ref()
        .and_then(|app| app.rollback.as_ref())
        .ok_or_else(|| anyhow::anyhow!("rollback journal has no before snapshot"))?;
    let after_snapshot = journal
        .after_app
        .as_ref()
        .and_then(|app| app.rollback.as_ref())
        .ok_or_else(|| anyhow::anyhow!("rollback journal has no after snapshot"))?;
    if after_snapshot.snapshot_path != before_snapshot.snapshot_path {
        anyhow::bail!("rollback before and after snapshots do not share the swap path");
    }
    ensure_lexical_direct_child(
        rollback_dir,
        &before_snapshot.snapshot_path,
        "rollback snapshot",
    )?;
    let temporary = &journal.moves[0].to;
    if journal.moves[0].from != active_dir
        || journal.moves[0].discard_path.is_some()
        || !matches_staging_path(temporary, active_dir, "rollback-swap")
    {
        anyhow::bail!("rollback active-to-temporary move is invalid");
    }
    if journal.moves[1].from != before_snapshot.snapshot_path
        || journal.moves[1].to != active_dir
        || journal.moves[1].discard_path.is_some()
    {
        anyhow::bail!("rollback snapshot-to-active move is invalid");
    }
    if journal.moves[2].from != *temporary
        || journal.moves[2].to != before_snapshot.snapshot_path
        || journal.moves[2].discard_path.is_some()
    {
        anyhow::bail!("rollback temporary-to-snapshot move is invalid");
    }
    Ok(())
}

fn validate_uninstall_move_template(
    journal: &ManagedTransactionJournal,
    active_dir: &Path,
    rollback_dir: &Path,
) -> Result<()> {
    let before = journal
        .before_app
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("uninstall journal has no before app"))?;
    let expected_moves = if before.rollback.is_some() { 2 } else { 1 };
    if journal.moves.len() != expected_moves {
        anyhow::bail!("uninstall journal has an unexpected move count");
    }
    if journal.moves[0].from != active_dir {
        anyhow::bail!("uninstall active move source is invalid");
    }
    validate_tombstone_move(
        &journal.moves[0],
        "releasedock-pending-uninstall",
        "releasedock-committed-uninstall",
    )?;
    if let Some(snapshot) = before.rollback.as_ref() {
        ensure_lexical_direct_child(rollback_dir, &snapshot.snapshot_path, "uninstall snapshot")?;
        if journal.moves[1].from != snapshot.snapshot_path {
            anyhow::bail!("uninstall snapshot move source is invalid");
        }
        validate_tombstone_move(
            &journal.moves[1],
            "releasedock-pending-uninstall",
            "releasedock-committed-uninstall",
        )?;
    }
    Ok(())
}

fn validate_staged_promotion(
    managed_move: &ManagedTransactionMove,
    active_dir: &Path,
) -> Result<()> {
    if managed_move.to != active_dir
        || managed_move.discard_path.is_some()
        || !matches_staging_path(&managed_move.from, active_dir, "staging")
    {
        anyhow::bail!("staged promotion move is invalid");
    }
    Ok(())
}

fn validate_tombstone_move(
    managed_move: &ManagedTransactionMove,
    pending_label: &str,
    committed_label: &str,
) -> Result<()> {
    let discard = managed_move
        .discard_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("discard move has no committed tombstone"))?;
    if !matches_staging_path(&managed_move.to, &managed_move.from, pending_label)
        || !matches_staging_path(discard, &managed_move.from, committed_label)
    {
        anyhow::bail!("discard tombstone name or parent is invalid");
    }
    Ok(())
}

fn matches_staging_path(path: &Path, base: &Path, label: &str) -> bool {
    if path.parent() != base.parent() {
        return false;
    }
    let Some(base_name) = base.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let prefix = format!(".{base_name}.{label}.");
    file_name
        .strip_prefix(&prefix)
        .is_some_and(is_unique_staging_token)
}

fn is_unique_staging_token(value: &str) -> bool {
    let mut parts = value.split('-');
    let valid = parts
        .by_ref()
        .take(3)
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
    valid && parts.next().is_none() && value.split('-').count() == 3
}

fn ensure_lexical_direct_child(parent: &Path, child: &Path, description: &str) -> Result<()> {
    if child.parent() != Some(parent) || child.file_name().is_none() {
        anyhow::bail!(
            "{description} {} is not a direct child of {}",
            child.display(),
            parent.display()
        );
    }
    Ok(())
}

fn validate_journal_managed_path(root: &Path, path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        anyhow::bail!("journal managed path {} is not normalized", path.display());
    }
    let relative = path.strip_prefix(root).with_context(|| {
        format!(
            "journal managed path {} is outside trusted root {}",
            path.display(),
            root.display()
        )
    })?;
    let Some(Component::Normal(area)) = relative.components().next() else {
        anyhow::bail!(
            "journal managed path {} has no managed area",
            path.display()
        );
    };
    if area != "apps" && area != "rollbacks" {
        anyhow::bail!(
            "journal managed path {} is outside apps and rollbacks",
            path.display()
        );
    }

    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        cursor.push(component.as_os_str());
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                anyhow::bail!("journal managed path {} crosses a symlink", path.display())
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", cursor.display()));
            }
        }
    }
    Ok(())
}

fn reconcile_completed_moves(journal: &mut ManagedTransactionJournal) -> Result<()> {
    if journal.completed_moves == journal.moves.len() {
        return Ok(());
    }
    let next = &journal.moves[journal.completed_moves];
    let from_exists = next.from.exists();
    let to_exists = next.to.exists();
    if !from_exists && to_exists {
        journal.completed_moves += 1;
    } else if !from_exists && !to_exists {
        anyhow::bail!(
            "managed transaction move state is missing both {} and {}",
            next.from.display(),
            next.to.display()
        );
    }
    Ok(())
}

fn restore_managed_transaction_moves(journal: &ManagedTransactionJournal) -> Result<()> {
    for managed_move in journal.moves[..journal.completed_moves].iter().rev() {
        let from_exists = managed_move.from.exists();
        let to_exists = managed_move.to.exists();
        match (from_exists, to_exists) {
            (false, true) => rename_managed_path(&managed_move.to, &managed_move.from)?,
            (true, false) => {}
            _ => anyhow::bail!(
                "cannot restore managed transaction move {} -> {}",
                managed_move.from.display(),
                managed_move.to.display()
            ),
        }
    }
    Ok(())
}

fn finalize_managed_transaction_discards(journal: &ManagedTransactionJournal) -> Result<()> {
    for managed_move in &journal.moves {
        let Some(discard_path) = managed_move.discard_path.as_ref() else {
            continue;
        };
        match (managed_move.to.exists(), discard_path.exists()) {
            (true, false) => rename_managed_path(&managed_move.to, discard_path)?,
            (false, true) | (false, false) => {}
            (true, true) => {
                anyhow::bail!("managed transaction has both pending and committed discard paths")
            }
        }
        if discard_path.exists() {
            remove_path(discard_path)?;
        }
    }
    Ok(())
}

fn execute_managed_journal_move<R>(
    manifest_store: &ManifestStore,
    journal: &mut ManagedTransactionJournal,
    move_index: usize,
    rename: &mut R,
) -> Result<()>
where
    R: FnMut(&Path, &Path) -> Result<()>,
{
    let managed_move = journal
        .moves
        .get(move_index)
        .ok_or_else(|| anyhow::anyhow!("managed transaction move index is out of bounds"))?;
    rename(&managed_move.from, &managed_move.to)?;
    journal.completed_moves = move_index + 1;
    manifest_store.save_transaction_journal_unlocked(journal)
}

fn restore_managed_transaction_moves_with<R>(
    journal: &ManagedTransactionJournal,
    rename: &mut R,
) -> Result<()>
where
    R: FnMut(&Path, &Path) -> Result<()>,
{
    for managed_move in journal.moves[..journal.completed_moves].iter().rev() {
        let from_exists = managed_move.from.exists();
        let to_exists = managed_move.to.exists();
        match (from_exists, to_exists) {
            (false, true) => rename(&managed_move.to, &managed_move.from)?,
            (true, false) => {}
            _ => anyhow::bail!(
                "cannot restore managed transaction move {} -> {}",
                managed_move.from.display(),
                managed_move.to.display()
            ),
        }
    }
    Ok(())
}

fn finalize_managed_transaction_discards_with<R, D>(
    journal: &ManagedTransactionJournal,
    rename: &mut R,
    cleanup: &mut D,
) -> Result<()>
where
    R: FnMut(&Path, &Path) -> Result<()>,
    D: FnMut(&Path) -> Result<()>,
{
    for managed_move in &journal.moves {
        let Some(discard_path) = managed_move.discard_path.as_ref() else {
            continue;
        };
        match (managed_move.to.exists(), discard_path.exists()) {
            (true, false) => rename(&managed_move.to, discard_path)?,
            (false, true) | (false, false) => {}
            (true, true) => {
                anyhow::bail!("managed transaction has both pending and committed discard paths")
            }
        }
        if discard_path.exists() {
            cleanup(discard_path)?;
        }
    }
    Ok(())
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

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
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
    Rollback,
    Uninstall,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStage {
    Preparing,
    Downloading,
    CopyingAsset,
    VerifyingArtifact,
    ExtractingArchive,
    CreatingRollback,
    RestoringRollback,
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
    install_from_plan_with_persist(
        plan,
        manifest_store,
        asset_fixture,
        runtime_config,
        language,
        progress,
        |manifest| manifest_store.save_unlocked(manifest),
    )
    .await
}

async fn install_from_plan_with_persist<P>(
    plan: &InstallPlan,
    manifest_store: &ManifestStore,
    asset_fixture: Option<&Path>,
    runtime_config: Option<&Config>,
    language: Language,
    progress: Option<ProgressReporter>,
    mut persist: P,
) -> Result<InstallOutcome>
where
    P: FnMut(&Manifest) -> Result<()>,
{
    let repo = RepoRef::parse(&plan.repo_url)?;
    let baseline_manifest = manifest_store.load()?;
    let baseline_app = baseline_manifest
        .apps
        .iter()
        .find(|app| app.id == repo.id())
        .cloned();
    let baseline_action = install_lifecycle_action(baseline_app.as_ref(), plan);
    let baseline_failure_summary = install_failure_summary(language, &repo, plan, &baseline_action);
    if let Err(error) = ensure_selection_guard(baseline_app.as_ref(), plan.selection_guard.as_ref())
    {
        record_lifecycle_failure(
            manifest_store,
            &repo,
            baseline_action,
            baseline_failure_summary,
            error.to_string(),
            Some(plan.version.clone()),
            Some(plan.asset_name.clone()),
            None,
            None,
        );
        return Err(error);
    }
    if let Err(error) = ensure_management_kind_compatible(baseline_app.as_ref(), plan) {
        record_lifecycle_failure(
            manifest_store,
            &repo,
            baseline_action,
            baseline_failure_summary,
            error.to_string(),
            Some(plan.version.clone()),
            Some(plan.asset_name.clone()),
            None,
            None,
        );
        return Err(error);
    }
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
            baseline_action.clone(),
            baseline_failure_summary.clone(),
            error.to_string(),
            Some(plan.version.clone()),
            Some(plan.asset_name.clone()),
            None,
            None,
        );
        error
    })?;
    report_progress(
        progress.as_ref(),
        TaskProgress {
            repo_id: repo.id(),
            action: TaskAction::Install,
            stage: TaskStage::VerifyingArtifact,
            message: format!(
                "{} {}",
                tr(language, "Verifying", "正在校验"),
                plan.asset_name
            ),
            percent: Some(60),
        },
    );
    let (artifact_sha256, integrity_status) = match plan.integrity.expected_sha256.as_deref() {
        Some(expected) => verify_file_sha256(&download_path, expected)
            .map(|digest| (digest, IntegrityStatus::VerifiedChecksum)),
        None => sha256_file(&download_path).map(|digest| (digest, IntegrityStatus::RecordedOnly)),
    }
    .map_err(|error| {
        record_lifecycle_failure(
            manifest_store,
            &repo,
            baseline_action.clone(),
            baseline_failure_summary.clone(),
            error.to_string(),
            Some(plan.version.clone()),
            Some(plan.asset_name.clone()),
            None,
            None,
        );
        error
    })?;

    // 下载和摘要计算不占用清单锁；从这里开始，磁盘切换、清单提交和失败恢复
    // 必须处在同一临界区内，避免多个 ReleaseDock 进程互相覆盖状态。
    let manifest_lock = manifest_store.lock_exclusive()?;
    recover_managed_transaction_unlocked(manifest_store)?;
    let manifest = manifest_store.load_unlocked()?;
    let previous_app = manifest
        .apps
        .iter()
        .find(|app| app.id == repo.id())
        .cloned();
    if previous_app != baseline_app {
        drop(manifest_lock);
        let error = anyhow::anyhow!(
            "manifest conflict for {}: target app changed while the artifact was downloading",
            repo.id()
        );
        record_lifecycle_failure(
            manifest_store,
            &repo,
            baseline_action,
            baseline_failure_summary,
            error.to_string(),
            Some(plan.version.clone()),
            Some(plan.asset_name.clone()),
            None,
            None,
        );
        return Err(error);
    }
    let lifecycle_action = install_lifecycle_action(previous_app.as_ref(), plan);
    let lifecycle_summary = install_lifecycle_summary(language, &repo, plan, &lifecycle_action);
    let lifecycle_failure_summary =
        install_failure_summary(language, &repo, plan, &lifecycle_action);

    let install_result = if matches!(
        plan.install_type,
        InstallType::AppImage
            | InstallType::Executable
            | InstallType::PortableArchive
            | InstallType::Archive
    ) {
        install_managed_from_download(
            plan,
            &repo,
            &manifest,
            previous_app.clone(),
            &download_path,
            manifest_store,
            runtime_config,
            language,
            progress.as_ref(),
            artifact_sha256,
            integrity_status,
            lifecycle_action.clone(),
            lifecycle_summary,
            &mut persist,
        )
    } else {
        install_external_from_download(
            plan,
            &repo,
            &manifest,
            previous_app.as_ref(),
            &download_path,
            manifest_store,
            runtime_config,
            language,
            progress.as_ref(),
            artifact_sha256,
            integrity_status,
            lifecycle_action.clone(),
            lifecycle_summary,
            &mut persist,
        )
    };

    // 失败事件通过 ManifestStore 的公开写接口记录，因此需先释放当前事务锁。
    drop(manifest_lock);
    let (app, install_path, install_path_kind, uninstall_supported) = match install_result {
        Ok(outcome) => outcome,
        Err(error) => {
            record_lifecycle_failure(
                manifest_store,
                &repo,
                lifecycle_action,
                lifecycle_failure_summary,
                error.to_string(),
                Some(plan.version.clone()),
                Some(plan.asset_name.clone()),
                None,
                Some(
                    if matches!(
                        plan.install_type,
                        InstallType::AppImage
                            | InstallType::Executable
                            | InstallType::PortableArchive
                            | InstallType::Archive
                    ) {
                        InstallPathKind::ManagedPath
                    } else {
                        InstallPathKind::SystemInstaller
                    },
                ),
            );
            return Err(error);
        }
    };

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
    if let Err(error) = cleanup_download_cache(&download_path) {
        eprintln!(
            "installed {} but failed to clean download cache {}: {error:#}",
            repo.id(),
            download_path.display()
        );
    }

    Ok(InstallOutcome {
        app,
        download_path,
        install_path,
        install_type: plan.install_type,
        install_path_kind,
        uninstall_supported,
    })
}

#[allow(clippy::too_many_arguments)]
fn install_managed_from_download<P>(
    plan: &InstallPlan,
    repo: &RepoRef,
    manifest: &Manifest,
    previous_app: Option<InstalledApp>,
    downloaded: &Path,
    manifest_store: &ManifestStore,
    runtime_config: Option<&Config>,
    language: Language,
    progress: Option<&ProgressReporter>,
    artifact_sha256: String,
    integrity_status: IntegrityStatus,
    lifecycle_action: LifecycleAction,
    lifecycle_summary: String,
    persist: &mut P,
) -> Result<(InstalledApp, PathBuf, InstallPathKind, bool)>
where
    P: FnMut(&Manifest) -> Result<()>,
{
    // 已安装应用的 manifest 路径是磁盘事实；用户后来修改 install_root 只影响新安装。
    let layout = if let Some(previous) = previous_app.as_ref() {
        validate_managed_layout(previous, manifest_store, runtime_config)?
    } else {
        prepare_new_managed_layout(manifest_store, repo, runtime_config)?
    };
    if let Some(previous) = previous_app.as_ref()
        && let Some(snapshot) = previous.rollback.as_ref()
    {
        validate_rollback_snapshot(previous, snapshot, &layout)?;
    }
    let transaction = ManagedInstallTransaction::new(
        layout.active_dir.clone(),
        layout.rollback_dir.clone(),
        previous_app.clone(),
    )?;

    match plan.install_type {
        InstallType::AppImage | InstallType::Executable => prepare_managed_executable(
            downloaded,
            &transaction.staged_dir,
            repo,
            &plan.asset_name,
            language,
            progress,
        )?,
        InstallType::PortableArchive | InstallType::Archive => prepare_managed_archive(
            downloaded,
            &transaction.staged_dir,
            repo,
            &plan.asset_name,
            language,
            progress,
        )?,
        _ => anyhow::bail!("install type {:?} is not managed-local", plan.install_type),
    }

    let install_path = transaction.install_path(plan.install_type, &plan.asset_name);
    let staged_install_path = match plan.install_type {
        InstallType::AppImage | InstallType::Executable => {
            transaction.staged_dir.join(&plan.asset_name)
        }
        _ => transaction.staged_dir.clone(),
    };
    let launch_path = infer_launch_target(
        &staged_install_path,
        plan.install_type,
        &repo.name,
        &plan.asset_name,
    )
    .and_then(|path| {
        path.strip_prefix(&transaction.staged_dir)
            .ok()
            .map(|relative| transaction.active_dir.join(relative))
    });

    let mut app = InstalledApp::with_install_metadata(
        repo.id(),
        repo.name.clone(),
        plan.version.clone(),
        plan.asset_name.clone(),
        install_path.clone(),
        plan.install_type,
        InstallPathKind::ManagedPath,
        true,
    );
    app.managed_root = Some(layout.trusted_root.clone());
    app.launch_path = launch_path;
    app.artifact_sha256 = Some(artifact_sha256);
    app.integrity_status = Some(integrity_status);
    app.checksum_asset_name = plan
        .integrity
        .expected_sha256
        .as_ref()
        .and(plan.integrity.checksum_asset_name.clone());
    if let Some(previous_app) = previous_app.as_ref() {
        app.release_policy = previous_app.release_policy.clone();
    } else if let Some(target_policy) = plan.target_policy.as_ref() {
        app.release_policy = target_policy.clone();
    }

    if previous_app
        .as_ref()
        .is_some_and(|previous| matches!(previous.install_path_kind, InstallPathKind::ManagedPath))
    {
        report_progress(
            progress,
            TaskProgress {
                repo_id: repo.id(),
                action: TaskAction::Install,
                stage: TaskStage::CreatingRollback,
                message: format!(
                    "{} {}",
                    tr(
                        language,
                        "Creating rollback snapshot for",
                        "正在创建回滚快照："
                    ),
                    repo.name
                ),
                percent: Some(85),
            },
        );
    }

    let success_event = LifecycleEvent::succeeded(
        repo.id(),
        repo.name.clone(),
        lifecycle_action,
        lifecycle_summary,
        Some(plan.version.clone()),
        Some(plan.asset_name.clone()),
        Some(install_path.clone()),
        Some(InstallPathKind::ManagedPath),
    );
    transaction.commit_with(
        manifest_store,
        manifest,
        &mut app,
        success_event,
        |next| persist(next),
        rename_managed_path,
        remove_path,
    )?;

    Ok((app, install_path, InstallPathKind::ManagedPath, true))
}

#[allow(clippy::too_many_arguments)]
fn install_external_from_download<P>(
    plan: &InstallPlan,
    repo: &RepoRef,
    manifest: &Manifest,
    previous_app: Option<&InstalledApp>,
    downloaded: &Path,
    manifest_store: &ManifestStore,
    runtime_config: Option<&Config>,
    language: Language,
    progress: Option<&ProgressReporter>,
    artifact_sha256: String,
    integrity_status: IntegrityStatus,
    lifecycle_action: LifecycleAction,
    lifecycle_summary: String,
    persist: &mut P,
) -> Result<(InstalledApp, PathBuf, InstallPathKind, bool)>
where
    P: FnMut(&Manifest) -> Result<()>,
{
    match plan.install_type {
        InstallType::WindowsInstaller => {
            let installer_path = install_windows_installer(
                downloaded,
                repo,
                manifest_store,
                &plan.asset_name,
                runtime_config,
                language,
                progress,
            )?;
            let repo_id = repo.id();
            let repo_tail = repo_id.rsplit('/').next().unwrap_or_default().to_string();
            let candidate_names = [
                repo.name.as_str(),
                repo_tail.as_str(),
                plan.asset_name.as_str(),
            ];
            let candidate_versions = [plan.version.as_str()];
            let adopted_installation =
                discover_installation(&candidate_names, &candidate_versions)?;
            let (install_path, launch_path, installer_path_record) =
                if let Some(adopted) = adopted_installation {
                    (
                        adopted.install_path,
                        adopted.launch_path,
                        Some(installer_path.clone()),
                    )
                } else {
                    (installer_path.clone(), None, None)
                };

            let mut app = InstalledApp::with_install_metadata(
                repo.id(),
                repo.name.clone(),
                plan.version.clone(),
                plan.asset_name.clone(),
                install_path.clone(),
                plan.install_type,
                InstallPathKind::SystemInstaller,
                false,
            )
            .with_installer_path(installer_path_record);
            app.launch_path = launch_path;
            app.artifact_sha256 = Some(artifact_sha256);
            app.integrity_status = Some(integrity_status);
            app.checksum_asset_name = plan
                .integrity
                .expected_sha256
                .as_ref()
                .and(plan.integrity.checksum_asset_name.clone());
            if let Some(previous_app) = previous_app {
                app.release_policy = previous_app.release_policy.clone();
            } else if let Some(target_policy) = plan.target_policy.as_ref() {
                app.release_policy = target_policy.clone();
            }

            let event = LifecycleEvent::succeeded(
                repo.id(),
                repo.name.clone(),
                lifecycle_action,
                lifecycle_summary,
                Some(plan.version.clone()),
                Some(plan.asset_name.clone()),
                Some(install_path.clone()),
                Some(InstallPathKind::SystemInstaller),
            );
            persist(&manifest_with_app_and_event(manifest, app.clone(), event))?;
            Ok((app, install_path, InstallPathKind::SystemInstaller, false))
        }
        InstallType::LinuxPackage => {
            let (install_path, metadata) = install_linux_package(
                downloaded,
                repo,
                manifest_store,
                &plan.asset_name,
                runtime_config,
                language,
                progress,
            )?;

            let mut app = InstalledApp::with_install_metadata(
                repo.id(),
                repo.name.clone(),
                plan.version.clone(),
                plan.asset_name.clone(),
                install_path.clone(),
                plan.install_type,
                InstallPathKind::SystemInstaller,
                true,
            );
            app.artifact_sha256 = Some(artifact_sha256);
            app.integrity_status = Some(integrity_status);
            app.checksum_asset_name = plan
                .integrity
                .expected_sha256
                .as_ref()
                .and(plan.integrity.checksum_asset_name.clone());
            if let Some(previous_app) = previous_app {
                app.release_policy = previous_app.release_policy.clone();
            } else if let Some(target_policy) = plan.target_policy.as_ref() {
                app.release_policy = target_policy.clone();
            }
            app.system_package_name = Some(metadata.package_name);
            app.system_package_manager = Some(metadata.manager);

            let event = LifecycleEvent::succeeded(
                repo.id(),
                repo.name.clone(),
                lifecycle_action,
                lifecycle_summary,
                Some(plan.version.clone()),
                Some(plan.asset_name.clone()),
                Some(install_path.clone()),
                Some(InstallPathKind::SystemInstaller),
            );
            persist(&manifest_with_app_and_event(manifest, app.clone(), event))?;
            Ok((app, install_path, InstallPathKind::SystemInstaller, true))
        }
        InstallType::Unknown => anyhow::bail!(
            "installing {:?} assets is not implemented yet; use the preview path instead",
            plan.install_type
        ),
        _ => anyhow::bail!("install type {:?} is not external", plan.install_type),
    }
}

fn install_lifecycle_action(
    previous_app: Option<&InstalledApp>,
    plan: &InstallPlan,
) -> LifecycleAction {
    let Some(previous_app) = previous_app else {
        return LifecycleAction::Install;
    };
    if matches!(plan.release_direction, ReleaseDirection::Downgrade) {
        return LifecycleAction::Downgrade;
    }
    if previous_app.installed_version == plan.version
        || matches!(plan.release_direction, ReleaseDirection::Reinstall)
    {
        LifecycleAction::Install
    } else {
        LifecycleAction::Update
    }
}

fn ensure_selection_guard(
    installed: Option<&InstalledApp>,
    guard: Option<&InstallSelectionGuard>,
) -> Result<()> {
    let Some(guard) = guard else {
        // Plans serialized before selection guards were introduced remain
        // readable. Every newly generated install or update plan has a guard.
        return Ok(());
    };
    guard.validate(installed)
}

fn ensure_management_kind_compatible(
    previous_app: Option<&InstalledApp>,
    plan: &InstallPlan,
) -> Result<()> {
    let Some(previous_app) = previous_app else {
        return Ok(());
    };
    let installed_kind = match previous_app.install_path_kind {
        InstallPathKind::ManagedPath => InstallManagementKind::ManagedLocal,
        InstallPathKind::SystemInstaller | InstallPathKind::Unknown => {
            if matches!(previous_app.install_type, InstallType::LinuxPackage) {
                InstallManagementKind::SystemPackage
            } else {
                InstallManagementKind::ExternalInstaller
            }
        }
    };
    if installed_kind != plan.management_kind {
        anyhow::bail!(
            "management kind change for {} from {:?} to {:?} is not supported",
            previous_app.id,
            installed_kind,
            plan.management_kind
        );
    }
    Ok(())
}

fn install_lifecycle_summary(
    language: Language,
    repo: &RepoRef,
    plan: &InstallPlan,
    action: &LifecycleAction,
) -> String {
    let verb = match action {
        LifecycleAction::Downgrade => tr(language, "Downgraded", "已降级"),
        LifecycleAction::Update => tr(language, "Updated", "已更新"),
        _ => tr(language, "Installed", "已安装"),
    };
    format!("{verb} {} {}", repo.name, plan.version)
}

fn install_failure_summary(
    language: Language,
    repo: &RepoRef,
    plan: &InstallPlan,
    action: &LifecycleAction,
) -> String {
    let verb = match action {
        LifecycleAction::Downgrade => tr(language, "Failed to downgrade", "降级失败"),
        LifecycleAction::Update => tr(language, "Failed to update", "更新失败"),
        _ => tr(language, "Failed to install", "安装失败"),
    };
    format!("{verb} {} {}", repo.name, plan.version)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackGuard {
    installed_version: String,
    expected_snapshot: Option<RollbackSnapshotIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RollbackSnapshotIdentity {
    version: String,
    path: PathBuf,
}

impl From<&RollbackSnapshot> for RollbackSnapshotIdentity {
    fn from(snapshot: &RollbackSnapshot) -> Self {
        Self {
            version: snapshot.version.clone(),
            path: snapshot.snapshot_path.clone(),
        }
    }
}

impl RollbackGuard {
    pub fn from_app(app: &InstalledApp) -> Self {
        Self {
            installed_version: app.installed_version.clone(),
            expected_snapshot: app.rollback.as_ref().map(RollbackSnapshotIdentity::from),
        }
    }
}

pub fn rollback_repo(
    manifest_store: &ManifestStore,
    repo_id: &str,
    language: Language,
    progress: Option<ProgressReporter>,
) -> Result<Option<InstalledApp>> {
    rollback_repo_with_persist(manifest_store, repo_id, language, progress, |manifest| {
        manifest_store.save_unlocked(manifest)
    })
}

pub fn rollback_repo_guarded(
    manifest_store: &ManifestStore,
    repo_id: &str,
    guard: &RollbackGuard,
    language: Language,
    progress: Option<ProgressReporter>,
) -> Result<Option<InstalledApp>> {
    rollback_repo_with_ops_guarded(
        manifest_store,
        repo_id,
        Some(guard),
        language,
        progress,
        |manifest| manifest_store.save_unlocked(manifest),
        rename_managed_path,
    )
}

fn rollback_repo_with_persist<P>(
    manifest_store: &ManifestStore,
    repo_id: &str,
    language: Language,
    progress: Option<ProgressReporter>,
    persist: P,
) -> Result<Option<InstalledApp>>
where
    P: FnMut(&Manifest) -> Result<()>,
{
    rollback_repo_with_ops(
        manifest_store,
        repo_id,
        language,
        progress,
        persist,
        rename_managed_path,
    )
}

fn rollback_repo_with_ops<P, R>(
    manifest_store: &ManifestStore,
    repo_id: &str,
    language: Language,
    progress: Option<ProgressReporter>,
    persist: P,
    rename: R,
) -> Result<Option<InstalledApp>>
where
    P: FnMut(&Manifest) -> Result<()>,
    R: FnMut(&Path, &Path) -> Result<()>,
{
    rollback_repo_with_ops_guarded(
        manifest_store,
        repo_id,
        None,
        language,
        progress,
        persist,
        rename,
    )
}

fn rollback_repo_with_ops_guarded<P, R>(
    manifest_store: &ManifestStore,
    repo_id: &str,
    guard: Option<&RollbackGuard>,
    language: Language,
    progress: Option<ProgressReporter>,
    mut persist: P,
    mut rename: R,
) -> Result<Option<InstalledApp>>
where
    P: FnMut(&Manifest) -> Result<()>,
    R: FnMut(&Path, &Path) -> Result<()>,
{
    let _manifest_lock = manifest_store.lock_exclusive()?;
    recover_managed_transaction_unlocked(manifest_store)?;
    let manifest = manifest_store.load_unlocked()?;
    let Some(app) = manifest.apps.iter().find(|app| app.id == repo_id).cloned() else {
        return Ok(None);
    };
    let repo = RepoRef::parse(&app.repo_url)?;

    if let Some(guard) = guard {
        let current_snapshot = app.rollback.as_ref().map(RollbackSnapshotIdentity::from);
        let matches_guard = app.installed_version == guard.installed_version
            && current_snapshot == guard.expected_snapshot;
        if !matches_guard {
            let error = anyhow::anyhow!(
                "stale rollback plan for {}: installed version or snapshot changed",
                app.id
            );
            record_rollback_failure_unlocked(manifest_store, &repo, language, &app, &error);
            return Err(error);
        }
    }

    report_progress(
        progress.as_ref(),
        TaskProgress {
            repo_id: app.id.clone(),
            action: TaskAction::Rollback,
            stage: TaskStage::LocatingRecord,
            message: format!(
                "{} {}",
                tr(
                    language,
                    "Locating rollback snapshot for",
                    "正在定位回滚快照："
                ),
                app.name
            ),
            percent: Some(10),
        },
    );

    if !matches!(app.install_path_kind, InstallPathKind::ManagedPath) {
        let error = anyhow::anyhow!("only managed-path installs can be rolled back: {}", app.id);
        record_rollback_failure_unlocked(manifest_store, &repo, language, &app, &error);
        return Err(error);
    }
    let Some(snapshot) = app.rollback.clone() else {
        let error = anyhow::anyhow!("{} does not have a rollback snapshot", app.id);
        record_rollback_failure_unlocked(manifest_store, &repo, language, &app, &error);
        return Err(error);
    };
    let layout = validate_managed_layout(&app, manifest_store, None).map_err(|error| {
        record_rollback_failure_unlocked(manifest_store, &repo, language, &app, &error);
        error
    })?;
    validate_rollback_snapshot(&app, &snapshot, &layout).map_err(|error| {
        record_rollback_failure_unlocked(manifest_store, &repo, language, &app, &error);
        error
    })?;
    let active_dir = layout.active_dir;

    // 交换元数据时只替换版本内容，仓库身份和当前 release policy 始终保留。
    let mut restored = app.clone();
    restored.installed_version = snapshot.version.clone();
    restored.installed_at = snapshot.installed_at;
    restored.asset_name = snapshot.asset_name.clone();
    restored.install_path = snapshot.install_path.clone();
    restored.launch_path = snapshot.launch_path.clone();
    restored.install_type = snapshot.install_type;
    restored.install_path_kind = InstallPathKind::ManagedPath;
    restored.uninstall_supported = true;
    restored.system_package_name = None;
    restored.system_package_manager = None;
    restored.artifact_sha256 = snapshot.artifact_sha256.clone();
    restored.integrity_status = snapshot.integrity_status;
    restored.checksum_asset_name = snapshot.checksum_asset_name.clone();
    restored.rollback = Some(rollback_snapshot(&app, snapshot.snapshot_path.clone()));

    let temporary = staging_path(&active_dir, "rollback-swap");
    let mut journal = ManagedTransactionJournal {
        repo_id: app.id.clone(),
        operation: ManagedTransactionOperation::Rollback,
        trusted_root: layout.trusted_root,
        before_app: Some(app.clone()),
        after_app: Some(restored.clone()),
        moves: vec![
            ManagedTransactionMove {
                from: active_dir.clone(),
                to: temporary.clone(),
                discard_path: None,
            },
            ManagedTransactionMove {
                from: snapshot.snapshot_path.clone(),
                to: active_dir.clone(),
                discard_path: None,
            },
            ManagedTransactionMove {
                from: temporary,
                to: snapshot.snapshot_path.clone(),
                discard_path: None,
            },
        ],
        completed_moves: 0,
        manifest_committed: false,
    };
    manifest_store.save_transaction_journal_unlocked(&journal)?;

    report_progress(
        progress.as_ref(),
        TaskProgress {
            repo_id: app.id.clone(),
            action: TaskAction::Rollback,
            stage: TaskStage::RestoringRollback,
            message: format!(
                "{} {}",
                tr(
                    language,
                    "Restoring rollback snapshot for",
                    "正在恢复回滚快照："
                ),
                app.name
            ),
            percent: Some(55),
        },
    );

    for move_index in 0..journal.moves.len() {
        if let Err(move_error) =
            execute_managed_journal_move(manifest_store, &mut journal, move_index, &mut rename)
        {
            if let Err(restore_error) =
                restore_managed_transaction_moves_with(&journal, &mut rename)
            {
                return Err(move_error.context(format!(
                    "also failed to restore rollback moves: {restore_error:#}"
                )));
            }
            manifest_store.remove_transaction_journal_unlocked()?;
            record_rollback_failure_unlocked(manifest_store, &repo, language, &app, &move_error);
            return Err(move_error);
        }
    }

    let event = LifecycleEvent::succeeded(
        app.id.clone(),
        app.name.clone(),
        LifecycleAction::Rollback,
        format!(
            "{} {} {}",
            tr(language, "Rolled back", "已回滚"),
            app.name,
            restored.installed_version
        ),
        Some(restored.installed_version.clone()),
        Some(restored.asset_name.clone()),
        Some(restored.install_path.clone()),
        Some(InstallPathKind::ManagedPath),
    );
    let next_manifest = manifest_with_app_and_event(&manifest, restored.clone(), event);
    if let Err(persist_error) = persist(&next_manifest) {
        if let Err(restore_error) = restore_managed_transaction_moves_with(&journal, &mut rename) {
            return Err(persist_error.context(format!(
                "also failed to reverse rollback file swap: {restore_error:#}"
            )));
        }
        manifest_store.remove_transaction_journal_unlocked()?;
        record_rollback_failure_unlocked(manifest_store, &repo, language, &app, &persist_error);
        return Err(persist_error);
    }
    journal.manifest_committed = true;
    if let Err(error) = manifest_store.save_transaction_journal_unlocked(&journal) {
        eprintln!(
            "rollback committed but journal mark failed for {}: {error:#}",
            app.id
        );
    } else if let Err(error) = manifest_store.remove_transaction_journal_unlocked() {
        eprintln!(
            "rollback committed but journal cleanup failed for {}: {error:#}",
            app.id
        );
    }

    report_progress(
        progress.as_ref(),
        TaskProgress {
            repo_id: app.id,
            action: TaskAction::Rollback,
            stage: TaskStage::Finished,
            message: format!(
                "{} {}",
                tr(language, "Finished rollback for", "已完成回滚："),
                restored.name
            ),
            percent: Some(100),
        },
    );
    Ok(Some(restored))
}

fn managed_active_dir(app: &InstalledApp) -> Result<PathBuf> {
    match app.install_type {
        InstallType::AppImage | InstallType::Executable => app
            .install_path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "managed install path {} has no active directory",
                    app.install_path.display()
                )
            }),
        InstallType::PortableArchive | InstallType::Archive => Ok(app.install_path.clone()),
        _ => anyhow::bail!(
            "managed rollback metadata has unsupported install type {:?}",
            app.install_type
        ),
    }
}

#[derive(Debug, Clone)]
struct ManagedLayout {
    trusted_root: PathBuf,
    active_dir: PathBuf,
    rollback_dir: PathBuf,
}

fn prepare_new_managed_layout(
    manifest_store: &ManifestStore,
    repo: &RepoRef,
    runtime_config: Option<&Config>,
) -> Result<ManagedLayout> {
    let trusted_root = cache_root(manifest_store, runtime_config);
    ensure_real_directory(&trusted_root, "managed root", true)?;
    let trusted_root = fs::canonicalize(&trusted_root)
        .with_context(|| format!("failed to resolve managed root {}", trusted_root.display()))?;
    let apps_dir = trusted_root.join("apps");
    ensure_real_directory(&apps_dir, "managed apps directory", true)?;
    ensure_direct_child(&trusted_root, &apps_dir, "apps", "managed apps directory")?;
    Ok(ManagedLayout {
        active_dir: apps_dir.join(format!("{}-{}", repo.owner, repo.name)),
        rollback_dir: trusted_root
            .join("rollbacks")
            .join(format!("{}-{}", repo.owner, repo.name)),
        trusted_root,
    })
}

fn validate_managed_layout(
    app: &InstalledApp,
    _manifest_store: &ManifestStore,
    _runtime_config: Option<&Config>,
) -> Result<ManagedLayout> {
    if !matches!(app.install_path_kind, InstallPathKind::ManagedPath) {
        anyhow::bail!("{} is not a managed-path install", app.id);
    }
    let repo = RepoRef::parse(&app.repo_url)?;
    let repo_dir_name = format!("{}-{}", repo.owner, repo.name);
    let mut candidates = if let Some(managed_root) = app.managed_root.as_ref() {
        vec![managed_root.clone()]
    } else {
        // Schema v3 manifests did not persist managed_root. The installed path is
        // the durable disk fact, while the runtime install_root may have changed.
        vec![infer_legacy_managed_root_from_metadata(app)?]
    };
    let mut last_error = None;
    for trusted_root in candidates.drain(..) {
        match validate_managed_layout_at_root(app, &repo_dir_name, &trusted_root) {
            Ok(layout) => return Ok(layout),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("no trusted managed root candidates")))
}

fn infer_legacy_managed_root_from_metadata(app: &InstalledApp) -> Result<PathBuf> {
    if !matches!(app.install_path_kind, InstallPathKind::ManagedPath) {
        anyhow::bail!("{} is not a managed-path install", app.id);
    }
    let repo = RepoRef::parse(&app.repo_url)?;
    if repo.id() != app.id {
        anyhow::bail!("legacy managed app identity does not match repository URL");
    }
    let repo_dir_name = format!("{}-{}", repo.owner, repo.name);
    let active_dir = managed_active_dir(app)?;
    if active_dir.file_name().and_then(|name| name.to_str()) != Some(&repo_dir_name) {
        anyhow::bail!(
            "legacy managed active path {} does not use repository directory {}",
            active_dir.display(),
            repo_dir_name
        );
    }
    let apps_dir = active_dir.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "legacy managed active path {} has no apps parent",
            active_dir.display()
        )
    })?;
    if apps_dir.file_name().and_then(|name| name.to_str()) != Some("apps") {
        anyhow::bail!(
            "legacy managed active path {} is not under an apps directory",
            active_dir.display()
        );
    }
    let trusted_root = apps_dir.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "legacy managed apps path {} has no managed root",
            apps_dir.display()
        )
    })?;
    ensure_real_directory(trusted_root, "legacy managed root", false)?;
    let canonical_root = fs::canonicalize(trusted_root).with_context(|| {
        format!(
            "failed to resolve legacy managed root {}",
            trusted_root.display()
        )
    })?;
    if canonical_root != trusted_root {
        anyhow::bail!(
            "legacy managed root {} is not canonical",
            trusted_root.display()
        );
    }
    ensure_real_directory(apps_dir, "legacy managed apps directory", false)?;
    ensure_direct_child(
        &canonical_root,
        apps_dir,
        "apps",
        "legacy managed apps directory",
    )?;
    let expected_active = canonical_root.join("apps").join(&repo_dir_name);
    if active_dir != expected_active {
        anyhow::bail!(
            "legacy managed active path {} is not the canonical repository path {}",
            active_dir.display(),
            expected_active.display()
        );
    }
    match app.install_type {
        InstallType::AppImage | InstallType::Executable
            if app.install_path.parent() != Some(active_dir.as_path())
                || app.install_path.file_name().is_none() =>
        {
            anyhow::bail!("legacy managed file is not a direct active child")
        }
        InstallType::PortableArchive | InstallType::Archive if app.install_path != active_dir => {
            anyhow::bail!("legacy managed archive does not equal its active path")
        }
        InstallType::AppImage
        | InstallType::Executable
        | InstallType::PortableArchive
        | InstallType::Archive => {}
        _ => anyhow::bail!(
            "legacy managed metadata has unsupported install type {:?}",
            app.install_type
        ),
    }
    if active_dir.exists() {
        ensure_real_directory(&active_dir, "legacy managed active path", false)?;
        ensure_direct_child(
            apps_dir,
            &active_dir,
            &repo_dir_name,
            "legacy managed active path",
        )?;
    }
    Ok(canonical_root)
}

fn validate_managed_layout_at_root(
    app: &InstalledApp,
    repo_dir_name: &str,
    trusted_root: &Path,
) -> Result<ManagedLayout> {
    ensure_real_directory(trusted_root, "managed root", false)?;
    let canonical_root = fs::canonicalize(trusted_root)
        .with_context(|| format!("failed to resolve managed root {}", trusted_root.display()))?;
    let apps_dir = trusted_root.join("apps");
    ensure_real_directory(&apps_dir, "managed apps directory", false)?;
    ensure_direct_child(&canonical_root, &apps_dir, "apps", "managed apps directory")?;

    let active_dir = managed_active_dir(app)?;
    if active_dir.file_name().and_then(|name| name.to_str()) != Some(repo_dir_name) {
        anyhow::bail!(
            "managed active path {} does not use repository directory {}",
            active_dir.display(),
            repo_dir_name
        );
    }
    ensure_real_directory(&active_dir, "managed active path", false)?;
    ensure_direct_child(
        &apps_dir,
        &active_dir,
        &repo_dir_name,
        "managed active path",
    )?;
    validate_managed_install_path(app, &active_dir)?;

    Ok(ManagedLayout {
        trusted_root: canonical_root.clone(),
        rollback_dir: canonical_root.join("rollbacks").join(repo_dir_name),
        active_dir: fs::canonicalize(&active_dir).with_context(|| {
            format!(
                "failed to resolve managed active path {}",
                active_dir.display()
            )
        })?,
    })
}

fn prepare_managed_rollback_directory(app: &InstalledApp, rollback_dir: &Path) -> Result<()> {
    let trusted_root = app
        .managed_root
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("managed install is missing its trusted root"))?;
    ensure_real_directory(trusted_root, "managed root", false)?;
    let rollback_root = trusted_root.join("rollbacks");
    ensure_real_directory(&rollback_root, "managed rollback root", true)?;
    ensure_direct_child(
        trusted_root,
        &rollback_root,
        "rollbacks",
        "managed rollback root",
    )?;
    ensure_real_directory(rollback_dir, "managed repository rollback directory", true)?;
    let repo_dir_name = rollback_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("managed rollback directory has no repository name"))?;
    ensure_direct_child(
        &rollback_root,
        rollback_dir,
        repo_dir_name,
        "managed repository rollback directory",
    )
}

fn ensure_real_directory(path: &Path, description: &str, create: bool) -> Result<()> {
    if create && !path.exists() {
        fs::create_dir_all(path)
            .with_context(|| format!("failed to create {description} {}", path.display()))?;
    }
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {description} {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        anyhow::bail!("{description} {} must not be a symlink", path.display());
    }
    if !metadata.is_dir() {
        anyhow::bail!("{description} {} is not a directory", path.display());
    }
    Ok(())
}

fn ensure_direct_child(
    parent: &Path,
    child: &Path,
    expected_name: &str,
    description: &str,
) -> Result<()> {
    let canonical_parent = fs::canonicalize(parent)
        .with_context(|| format!("failed to resolve {}", parent.display()))?;
    let canonical_child = fs::canonicalize(child)
        .with_context(|| format!("failed to resolve {description} {}", child.display()))?;
    if canonical_child.parent() != Some(canonical_parent.as_path())
        || canonical_child.file_name().and_then(|name| name.to_str()) != Some(expected_name)
    {
        anyhow::bail!(
            "{description} {} is not the expected direct child of {}",
            child.display(),
            parent.display()
        );
    }
    Ok(())
}

fn validate_managed_install_path(app: &InstalledApp, active_dir: &Path) -> Result<()> {
    match app.install_type {
        InstallType::AppImage | InstallType::Executable => {
            let metadata = fs::symlink_metadata(&app.install_path).with_context(|| {
                format!(
                    "failed to inspect managed install path {}",
                    app.install_path.display()
                )
            })?;
            if metadata.file_type().is_symlink() {
                anyhow::bail!(
                    "managed install path {} must not be a symlink",
                    app.install_path.display()
                );
            }
            if !metadata.is_file() {
                anyhow::bail!(
                    "managed install path {} is not a file",
                    app.install_path.display()
                );
            }
            let canonical_install = fs::canonicalize(&app.install_path)?;
            let canonical_active = fs::canonicalize(active_dir)?;
            if canonical_install.parent() != Some(canonical_active.as_path()) {
                anyhow::bail!(
                    "managed install path {} is not a direct file child of managed active path {}",
                    app.install_path.display(),
                    active_dir.display()
                );
            }
        }
        InstallType::PortableArchive | InstallType::Archive => {
            if fs::canonicalize(&app.install_path)? != fs::canonicalize(active_dir)? {
                anyhow::bail!(
                    "managed archive install path {} does not equal managed active path {}",
                    app.install_path.display(),
                    active_dir.display()
                );
            }
        }
        _ => anyhow::bail!(
            "managed install metadata has unsupported install type {:?}",
            app.install_type
        ),
    }
    Ok(())
}

fn validate_rollback_snapshot(
    app: &InstalledApp,
    snapshot: &RollbackSnapshot,
    layout: &ManagedLayout,
) -> Result<()> {
    let active_dir = &layout.active_dir;
    if !snapshot.snapshot_path.is_dir() {
        anyhow::bail!(
            "rollback snapshot path {} does not exist or is not a directory",
            snapshot.snapshot_path.display()
        );
    }
    if !matches!(
        snapshot.install_type,
        InstallType::AppImage
            | InstallType::Executable
            | InstallType::PortableArchive
            | InstallType::Archive
    ) {
        anyhow::bail!(
            "rollback snapshot has unsupported install type {:?}",
            snapshot.install_type
        );
    }
    match snapshot.install_type {
        InstallType::AppImage | InstallType::Executable
            if snapshot.install_path.parent() != Some(active_dir.as_path()) =>
        {
            anyhow::bail!(
                "rollback install path {} is not a direct file child of active directory {}",
                snapshot.install_path.display(),
                active_dir.display()
            );
        }
        InstallType::PortableArchive | InstallType::Archive
            if snapshot.install_path != *active_dir =>
        {
            anyhow::bail!(
                "rollback archive path {} does not equal active directory {}",
                snapshot.install_path.display(),
                active_dir.display()
            );
        }
        _ => {}
    }
    let rollback_root = layout.trusted_root.join("rollbacks");
    ensure_real_directory(&rollback_root, "managed rollback root", false)?;
    ensure_direct_child(
        &layout.trusted_root,
        &rollback_root,
        "rollbacks",
        "managed rollback root",
    )?;
    let expected_rollback_dir = &layout.rollback_dir;
    ensure_real_directory(
        expected_rollback_dir,
        "managed repository rollback directory",
        false,
    )?;
    let expected_repo_name = active_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("managed active path has no repository name"))?;
    ensure_direct_child(
        &rollback_root,
        expected_rollback_dir,
        expected_repo_name,
        "managed repository rollback directory",
    )?;
    let snapshot_metadata = fs::symlink_metadata(&snapshot.snapshot_path).with_context(|| {
        format!(
            "failed to inspect rollback snapshot path {}",
            snapshot.snapshot_path.display()
        )
    })?;
    if snapshot_metadata.file_type().is_symlink() {
        anyhow::bail!(
            "rollback snapshot path {} must not be a symlink",
            snapshot.snapshot_path.display()
        );
    }
    let canonical_expected = fs::canonicalize(&expected_rollback_dir).with_context(|| {
        format!(
            "failed to resolve rollback directory {}",
            expected_rollback_dir.display()
        )
    })?;
    let canonical_snapshot = fs::canonicalize(&snapshot.snapshot_path).with_context(|| {
        format!(
            "failed to resolve rollback snapshot path {}",
            snapshot.snapshot_path.display()
        )
    })?;
    if canonical_snapshot.parent() != Some(canonical_expected.as_path()) {
        anyhow::bail!(
            "rollback snapshot path {} is outside {}",
            snapshot.snapshot_path.display(),
            expected_rollback_dir.display()
        );
    }

    if matches!(
        snapshot.install_type,
        InstallType::AppImage | InstallType::Executable
    ) {
        let relative_path = snapshot
            .install_path
            .strip_prefix(active_dir)
            .with_context(|| {
                format!(
                    "failed to resolve rollback artifact {} relative to {}",
                    snapshot.install_path.display(),
                    active_dir.display()
                )
            })?;
        let artifact_path = snapshot.snapshot_path.join(relative_path);
        if !artifact_path.is_file() {
            anyhow::bail!(
                "rollback artifact {} does not exist or is not a file",
                artifact_path.display()
            );
        }
        if let Some(expected) = snapshot.artifact_sha256.as_deref() {
            verify_file_sha256(&artifact_path, expected)?;
        }
    }

    if app.id.is_empty() || snapshot.version.is_empty() || snapshot.asset_name.is_empty() {
        anyhow::bail!("rollback snapshot metadata is incomplete for {}", app.id);
    }
    Ok(())
}

fn record_rollback_failure_unlocked(
    manifest_store: &ManifestStore,
    repo: &RepoRef,
    language: Language,
    app: &InstalledApp,
    error: &anyhow::Error,
) {
    record_lifecycle_failure_unlocked(
        manifest_store,
        repo,
        LifecycleAction::Rollback,
        format!(
            "{} {}",
            tr(language, "Failed to roll back", "回滚失败："),
            app.name
        ),
        error.to_string(),
        Some(app.installed_version.clone()),
        Some(app.asset_name.clone()),
        Some(app.install_path.clone()),
        Some(app.install_path_kind),
    );
}

fn uninstall_managed_repo(
    manifest_store: &ManifestStore,
    manifest: Manifest,
    app: InstalledApp,
    repo: &RepoRef,
    language: Language,
    progress: Option<ProgressReporter>,
) -> Result<Option<InstalledApp>> {
    uninstall_managed_repo_with_ops(
        manifest_store,
        manifest,
        app,
        repo,
        language,
        progress,
        |next| manifest_store.save_unlocked(next),
        rename_managed_path,
        remove_path,
    )
}

#[allow(clippy::too_many_arguments)]
fn uninstall_managed_repo_with_ops<P, R, D>(
    manifest_store: &ManifestStore,
    manifest: Manifest,
    app: InstalledApp,
    repo: &RepoRef,
    language: Language,
    progress: Option<ProgressReporter>,
    mut persist: P,
    mut rename: R,
    mut cleanup: D,
) -> Result<Option<InstalledApp>>
where
    P: FnMut(&Manifest) -> Result<()>,
    R: FnMut(&Path, &Path) -> Result<()>,
    D: FnMut(&Path) -> Result<()>,
{
    let layout = validate_managed_layout(&app, manifest_store, None)?;
    let active_dir = layout.active_dir.clone();
    if let Some(snapshot) = app.rollback.as_ref() {
        validate_rollback_snapshot(&app, snapshot, &layout)?;
    }
    if let Some(parent) = active_dir.parent() {
        cleanup_managed_gc_tombstones(parent);
    }
    let active_tombstone = ManagedTombstone::new(&active_dir, "uninstall");
    let snapshot_path = app
        .rollback
        .as_ref()
        .map(|snapshot| snapshot.snapshot_path.clone());
    let snapshot_tombstone = snapshot_path
        .as_ref()
        .map(|path| ManagedTombstone::new(path, "uninstall"));
    if let Some(rollback_dir) = snapshot_path.as_ref().and_then(|path| path.parent()) {
        cleanup_managed_gc_tombstones(rollback_dir);
    }
    let mut moves = vec![ManagedTransactionMove {
        from: active_tombstone.original.clone(),
        to: active_tombstone.pending.clone(),
        discard_path: Some(active_tombstone.committed.clone()),
    }];
    if let Some(snapshot_tombstone) = snapshot_tombstone.as_ref() {
        moves.push(ManagedTransactionMove {
            from: snapshot_tombstone.original.clone(),
            to: snapshot_tombstone.pending.clone(),
            discard_path: Some(snapshot_tombstone.committed.clone()),
        });
    }
    let mut journal = ManagedTransactionJournal {
        repo_id: app.id.clone(),
        operation: ManagedTransactionOperation::Uninstall,
        trusted_root: layout.trusted_root,
        before_app: Some(app.clone()),
        after_app: None,
        moves,
        completed_moves: 0,
        manifest_committed: false,
    };
    manifest_store.save_transaction_journal_unlocked(&journal)?;

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

    for move_index in 0..journal.moves.len() {
        if let Err(move_error) =
            execute_managed_journal_move(manifest_store, &mut journal, move_index, &mut rename)
        {
            if let Err(restore_error) =
                restore_managed_transaction_moves_with(&journal, &mut rename)
            {
                return Err(move_error.context(format!(
                    "also failed to restore managed uninstall moves: {restore_error:#}"
                )));
            }
            manifest_store.remove_transaction_journal_unlocked()?;
            record_lifecycle_failure_unlocked(
                manifest_store,
                repo,
                LifecycleAction::Uninstall,
                format!(
                    "{} {}",
                    tr(language, "Failed to uninstall", "卸载失败："),
                    app.name
                ),
                move_error.to_string(),
                Some(app.installed_version.clone()),
                Some(app.asset_name.clone()),
                Some(app.install_path.clone()),
                Some(app.install_path_kind),
            );
            return Err(move_error);
        }
    }

    let event = LifecycleEvent::succeeded(
        app.id.clone(),
        app.name.clone(),
        LifecycleAction::Uninstall,
        format!("{} {}", tr(language, "Uninstalled", "已卸载"), app.name),
        Some(app.installed_version.clone()),
        Some(app.asset_name.clone()),
        Some(app.install_path.clone()),
        Some(app.install_path_kind),
    );
    let mut next_manifest = manifest;
    next_manifest.apps.retain(|existing| existing.id != app.id);
    next_manifest.append_lifecycle_event(event);
    if let Err(persist_error) = persist(&next_manifest) {
        if let Err(restore_error) = restore_managed_transaction_moves_with(&journal, &mut rename) {
            return Err(persist_error.context(format!(
                "also failed to restore managed uninstall paths: {restore_error:#}"
            )));
        }
        manifest_store.remove_transaction_journal_unlocked()?;
        record_lifecycle_failure_unlocked(
            manifest_store,
            repo,
            LifecycleAction::Uninstall,
            format!(
                "{} {}",
                tr(language, "Failed to uninstall", "卸载失败："),
                app.name
            ),
            persist_error.to_string(),
            Some(app.installed_version.clone()),
            Some(app.asset_name.clone()),
            Some(app.install_path.clone()),
            Some(app.install_path_kind),
        );
        return Err(persist_error);
    }

    journal.manifest_committed = true;
    match manifest_store.save_transaction_journal_unlocked(&journal) {
        Err(error) => eprintln!(
            "managed uninstall committed but journal mark failed for {}: {error:#}",
            app.id
        ),
        Ok(()) => {
            match finalize_managed_transaction_discards_with(&journal, &mut rename, &mut cleanup) {
                Err(error) => eprintln!(
                    "managed uninstall committed but discard finalization failed for {}: {error:#}",
                    app.id
                ),
                Ok(()) => {
                    if let Err(error) = manifest_store.remove_transaction_journal_unlocked() {
                        eprintln!(
                            "managed uninstall committed but journal cleanup failed for {}: {error:#}",
                            app.id
                        );
                    }
                }
            }
        }
    }
    if let Some(snapshot_path) = snapshot_path.as_ref()
        && let Some(rollback_dir) = snapshot_path.parent()
    {
        if let Err(error) = prune_empty_dir(rollback_dir) {
            eprintln!(
                "managed uninstall committed but rollback directory cleanup failed for {}: {error:#}",
                rollback_dir.display()
            );
        }
    }

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

pub fn uninstall_repo(
    manifest_store: &ManifestStore,
    repo_id: &str,
    language: Language,
    progress: Option<ProgressReporter>,
) -> Result<Option<InstalledApp>> {
    let _manifest_lock = manifest_store.lock_exclusive()?;
    recover_managed_transaction_unlocked(manifest_store)?;
    let manifest = manifest_store.load_unlocked()?;
    let Some(app) = manifest.apps.iter().find(|app| app.id == repo_id).cloned() else {
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
        record_lifecycle_failure_unlocked(
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

    if matches!(app.install_path_kind, InstallPathKind::ManagedPath) {
        return uninstall_managed_repo(manifest_store, manifest, app, &repo, language, progress);
    }

    if !matches!(app.install_type, InstallType::LinuxPackage) {
        anyhow::bail!(
            "unsupported uninstall target {:?} for {}",
            app.install_type,
            app.id
        );
    }

    {
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
            record_lifecycle_failure_unlocked(
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
            record_lifecycle_failure_unlocked(
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
    report_progress(
        progress.as_ref(),
        TaskProgress {
            repo_id: app.id.clone(),
            action: TaskAction::Uninstall,
            stage: TaskStage::UpdatingManifest,
            message: format!(
                "{} {}",
                tr(
                    language,
                    "Updating install record for",
                    "正在更新安装记录："
                ),
                app.name
            ),
            percent: Some(90),
        },
    );
    let event = LifecycleEvent::succeeded(
        repo.id(),
        repo.name.clone(),
        LifecycleAction::Uninstall,
        format!("{} {}", tr(language, "Uninstalled", "已卸载"), app.name),
        Some(app.installed_version.clone()),
        Some(app.asset_name.clone()),
        Some(app.install_path.clone()),
        Some(app.install_path_kind),
    );
    let mut next_manifest = manifest;
    next_manifest.apps.retain(|existing| existing.id != repo_id);
    next_manifest.append_lifecycle_event(event);
    manifest_store.save_unlocked(&next_manifest)?;
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

fn prepare_managed_executable(
    downloaded: &Path,
    staged_dir: &Path,
    repo: &RepoRef,
    asset_name: &str,
    language: Language,
    progress: Option<&ProgressReporter>,
) -> Result<()> {
    let staged_path = staged_dir.join(asset_name);
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
    Ok(())
}

fn prepare_managed_archive(
    downloaded: &Path,
    staged_dir: &Path,
    repo: &RepoRef,
    asset_name: &str,
    language: Language,
    progress: Option<&ProgressReporter>,
) -> Result<()> {
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
        extract_zip(downloaded, staged_dir)
    } else if asset_name.ends_with(".tar.gz") || asset_name.ends_with(".tgz") {
        extract_tar_archive(GzDecoder::new(open_archive(downloaded)?), staged_dir)
    } else if asset_name.ends_with(".tar.xz") {
        extract_tar_archive(XzDecoder::new(open_archive(downloaded)?), staged_dir)
    } else {
        anyhow::bail!("archive format for {} is not supported yet", asset_name);
    };

    if let Err(error) = extract_result {
        if let Err(cleanup_error) = remove_path(staged_dir) {
            return Err(error.context(format!(
                "also failed to remove managed staging directory: {cleanup_error:#}"
            )));
        }
        return Err(error);
    }
    Ok(())
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
    #[cfg(not(target_os = "linux"))]
    let _ = manager;

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
    #[cfg(not(target_os = "linux"))]
    let _ = manager;

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
    #[cfg(not(target_os = "linux"))]
    let _ = manager;

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
    if let Some(container_dir) = dir.parent() {
        if container_dir
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| matches!(name, "downloads" | "rollbacks"))
            && fs::read_dir(container_dir)
                .with_context(|| {
                    format!(
                        "failed to inspect cache directory {}",
                        container_dir.display()
                    )
                })?
                .next()
                .is_none()
        {
            match fs::remove_dir(container_dir) {
                Ok(()) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
                    ) => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "failed to remove empty cache directory {}",
                            container_dir.display()
                        )
                    });
                }
            }
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
    #[cfg(not(unix))]
    let _ = path;

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
        install_plan::{InstallManagementKind, InstallSelectionGuard},
        integrity::{IntegrityPlan, IntegrityStatus, sha256_file},
        release::{Release, ReleaseAsset},
        release_policy::{ReleaseChannel, ReleasePolicy},
    };
    use flate2::{Compression, write::GzEncoder};
    use std::{
        env,
        sync::{Mutex, OnceLock, atomic::AtomicBool},
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
            integrity: crate::integrity::IntegrityPlan::default(),
            release_direction: crate::release_policy::ReleaseDirection::Unknown,
            selection_guard: None,
            target_policy: None,
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
        assert!(stored.apps[0].rollback.is_none());

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
    async fn install_records_verified_or_recorded_artifact_digest() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"verified payload").unwrap();
        let digest = sha256_file(&fixture).unwrap();

        let verified_plan =
            sample_plan(InstallType::AppImage, "verified.AppImage").with_integrity(IntegrityPlan {
                expected_sha256: Some(digest.clone()),
                checksum_asset_name: Some("SHA256SUMS".to_string()),
                status: IntegrityStatus::RecordedOnly,
            });
        let verified = install_from_plan(
            &verified_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            verified.app.artifact_sha256.as_deref(),
            Some(digest.as_str())
        );
        assert_eq!(
            verified.app.integrity_status,
            Some(IntegrityStatus::VerifiedChecksum)
        );
        assert_eq!(
            verified.app.checksum_asset_name.as_deref(),
            Some("SHA256SUMS")
        );

        let recorded_plan = sample_plan(InstallType::AppImage, "recorded.AppImage");
        let recorded = install_from_plan(
            &recorded_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            recorded.app.artifact_sha256.as_deref(),
            Some(digest.as_str())
        );
        assert_eq!(
            recorded.app.integrity_status,
            Some(IntegrityStatus::RecordedOnly)
        );
        assert_eq!(recorded.app.checksum_asset_name, None);
    }

    #[tokio::test]
    async fn checksum_mismatch_keeps_previous_install_manifest_and_download_cache() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let old_fixture = temp.path().join("old.AppImage");
        let new_fixture = temp.path().join("new.AppImage");
        fs::write(&old_fixture, b"old payload").unwrap();
        fs::write(&new_fixture, b"new payload").unwrap();

        let first_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        let first = install_from_plan(
            &first_plan,
            &manifest,
            Some(&old_fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        let old_manifest_app = manifest.load().unwrap().apps[0].clone();

        let mut mismatch_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        mismatch_plan.version = "v2.0.0".to_string();
        mismatch_plan.integrity.expected_sha256 = Some("0".repeat(64));
        mismatch_plan.integrity.checksum_asset_name = Some("demo.AppImage.sha256".to_string());
        let error = install_from_plan(
            &mismatch_plan,
            &manifest,
            Some(&new_fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("SHA-256 mismatch"));
        assert_eq!(fs::read(&first.install_path).unwrap(), b"old payload");
        assert_eq!(manifest.load().unwrap().apps[0], old_manifest_app);
        let failed_event = manifest
            .load()
            .unwrap()
            .latest_lifecycle_event("owner/project")
            .unwrap()
            .clone();
        assert_eq!(failed_event.action, LifecycleAction::Update);
        assert_eq!(
            failed_event.outcome,
            crate::manifest::LifecycleOutcome::Failed
        );
        assert!(
            failed_event
                .error
                .as_deref()
                .unwrap()
                .contains("SHA-256 mismatch")
        );
        assert!(
            temp.path()
                .join("downloads/owner_project/demo.AppImage")
                .exists()
        );
    }

    #[tokio::test]
    async fn install_reloads_locked_manifest_and_preserves_concurrent_other_records() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"payload").unwrap();
        let manifest_path = manifest.path().to_path_buf();
        let injected = Arc::new(AtomicBool::new(false));
        let injected_for_progress = injected.clone();
        let progress: ProgressReporter = Arc::new(move |event| {
            if !matches!(event.stage, TaskStage::VerifyingArtifact)
                || injected_for_progress.swap(true, Ordering::SeqCst)
            {
                return;
            }
            let store = ManifestStore::at_path(manifest_path.clone());
            store
                .upsert_app(InstalledApp::new(
                    "other/repo",
                    "other",
                    "v9.0.0",
                    "other.AppImage",
                    temp_path_for_manifest(&store).join("other.AppImage"),
                ))
                .unwrap();
            store
                .append_lifecycle_event(LifecycleEvent::succeeded(
                    "other/repo",
                    "other",
                    LifecycleAction::Install,
                    "other installed",
                    Some("v9.0.0".to_string()),
                    None,
                    None,
                    None,
                ))
                .unwrap();
        });

        install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            Some(progress),
        )
        .await
        .unwrap();

        let stored = manifest.load().unwrap();
        assert_eq!(stored.apps.len(), 2);
        assert!(stored.apps.iter().any(|app| app.id == "other/repo"));
        assert!(stored.latest_lifecycle_event("other/repo").is_some());
    }

    #[tokio::test]
    async fn install_rejects_target_changed_since_download_baseline_before_move() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"version one").unwrap();
        let plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        let first = install_from_plan(&plan, &manifest, Some(&fixture), None, Language::En, None)
            .await
            .unwrap();
        let manifest_path = manifest.path().to_path_buf();
        let injected = Arc::new(AtomicBool::new(false));
        let injected_for_progress = injected.clone();
        let progress: ProgressReporter = Arc::new(move |event| {
            if !matches!(event.stage, TaskStage::VerifyingArtifact)
                || injected_for_progress.swap(true, Ordering::SeqCst)
            {
                return;
            }
            let store = ManifestStore::at_path(manifest_path.clone());
            let mut app = store.load().unwrap().apps[0].clone();
            app.installed_version = "v-concurrent".to_string();
            store.upsert_app(app).unwrap();
        });
        fs::write(&fixture, b"version two").unwrap();
        let mut update = sample_plan(InstallType::AppImage, "demo.AppImage");
        update.version = "v2.0.0".to_string();

        let error = install_from_plan(
            &update,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            Some(progress),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("manifest conflict"));
        assert_eq!(fs::read(first.install_path).unwrap(), b"version one");
        assert_eq!(
            manifest.load().unwrap().apps[0].installed_version,
            "v-concurrent"
        );
    }

    #[tokio::test]
    async fn update_selection_guard_rejects_policy_change_before_reading_artifact() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"version one").unwrap();
        install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();

        let selected_app = manifest.load().unwrap().apps[0].clone();
        let mut update = sample_plan(InstallType::AppImage, "demo.AppImage");
        update.version = "v2.0.0".to_string();
        update.selection_guard = Some(InstallSelectionGuard::from_app(&selected_app));
        manifest
            .mutate_release_policy(
                "owner/project",
                crate::release_policy::PolicyMutation::SetChannel(ReleaseChannel::Prerelease),
            )
            .unwrap();

        let missing_fixture = temp.path().join("must-not-be-read.AppImage");
        let error = install_from_plan(
            &update,
            &manifest,
            Some(&missing_fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("stale install plan"));
        assert!(error.to_string().contains("release policy changed"));
    }

    #[tokio::test]
    async fn expected_absent_guard_rejects_an_existing_app_before_reading_artifact() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"version one").unwrap();
        install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();

        let mut plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        plan.selection_guard = Some(InstallSelectionGuard::ExpectedAbsent);
        let missing_fixture = temp.path().join("must-not-be-read.AppImage");

        let error = install_from_plan(
            &plan,
            &manifest,
            Some(&missing_fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("stale install plan"));
        assert!(error.to_string().contains("expected no installed app"));
    }

    #[tokio::test]
    async fn update_rejects_policy_change_after_initial_selection_guard() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"version one").unwrap();
        let first = install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();

        let selected_app = manifest.load().unwrap().apps[0].clone();
        let manifest_path = manifest.path().to_path_buf();
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = changed.clone();
        let progress: ProgressReporter = Arc::new(move |event| {
            if !matches!(event.stage, TaskStage::VerifyingArtifact)
                || changed_for_progress.swap(true, Ordering::SeqCst)
            {
                return;
            }
            ManifestStore::at_path(manifest_path.clone())
                .mutate_release_policy(
                    "owner/project",
                    crate::release_policy::PolicyMutation::SetChannel(ReleaseChannel::Prerelease),
                )
                .unwrap();
        });
        fs::write(&fixture, b"version two").unwrap();
        let mut update = sample_plan(InstallType::AppImage, "demo.AppImage");
        update.version = "v2.0.0".to_string();
        update.selection_guard = Some(InstallSelectionGuard::from_app(&selected_app));

        let error = install_from_plan(
            &update,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            Some(progress),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("manifest conflict"));
        assert_eq!(fs::read(first.install_path).unwrap(), b"version one");
        let stored = manifest.load().unwrap();
        assert_eq!(stored.apps[0].installed_version, "v1.2.3");
        assert_eq!(
            stored.apps[0].release_policy.channel,
            ReleaseChannel::Prerelease
        );
    }

    #[tokio::test]
    async fn new_install_commits_target_policy_without_policy_change_event() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"prerelease payload").unwrap();
        let mut plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        plan.target_policy = Some(ReleasePolicy {
            channel: ReleaseChannel::Prerelease,
            ..ReleasePolicy::default()
        });

        let outcome = install_from_plan(&plan, &manifest, Some(&fixture), None, Language::En, None)
            .await
            .unwrap();

        assert_eq!(
            outcome.app.release_policy.channel,
            ReleaseChannel::Prerelease
        );
        let stored = manifest.load().unwrap();
        assert_eq!(
            stored.apps[0].release_policy.channel,
            ReleaseChannel::Prerelease
        );
        assert_eq!(stored.lifecycle_events.len(), 1);
        assert_eq!(stored.lifecycle_events[0].action, LifecycleAction::Install);
    }

    #[tokio::test]
    async fn target_policy_save_failure_leaves_no_app_or_active_install() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"prerelease payload").unwrap();
        let mut plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        plan.target_policy = Some(ReleasePolicy {
            channel: ReleaseChannel::Prerelease,
            ..ReleasePolicy::default()
        });

        let error = install_from_plan_with_persist(
            &plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
            |_| anyhow::bail!("injected target policy save failure"),
        )
        .await
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("injected target policy save failure")
        );
        assert!(manifest.load().unwrap().apps.is_empty());
        assert!(!temp.path().join("apps/owner-project").exists());
    }

    #[tokio::test]
    async fn update_keeps_existing_managed_root_when_runtime_config_changes() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("state/apps.json"));
        let first_root = temp.path().join("first-root");
        let second_root = temp.path().join("second-root");
        let first_config = Config {
            install_root: Some(first_root.clone()),
            ..Config::default()
        };
        let second_config = Config {
            install_root: Some(second_root.clone()),
            ..Config::default()
        };
        let first_fixture = temp.path().join("first.AppImage");
        let second_fixture = temp.path().join("second.AppImage");
        fs::write(&first_fixture, b"first").unwrap();
        fs::write(&second_fixture, b"second").unwrap();

        install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&first_fixture),
            Some(&first_config),
            Language::En,
            None,
        )
        .await
        .unwrap();
        let mut legacy_app = manifest.load().unwrap().apps[0].clone();
        legacy_app.managed_root = None;
        manifest.upsert_app(legacy_app).unwrap();
        let mut update = sample_plan(InstallType::AppImage, "demo.AppImage");
        update.version = "v2.0.0".to_string();
        let outcome = install_from_plan(
            &update,
            &manifest,
            Some(&second_fixture),
            Some(&second_config),
            Language::En,
            None,
        )
        .await
        .unwrap();

        assert!(outcome.install_path.starts_with(&first_root));
        assert_eq!(fs::read(&outcome.install_path).unwrap(), b"second");
        assert!(!second_root.join("apps/owner-project").exists());
        let rollback = outcome.app.rollback.unwrap();
        assert!(
            rollback
                .snapshot_path
                .starts_with(first_root.join("rollbacks"))
        );
        assert_eq!(
            outcome.app.managed_root,
            Some(first_root.canonicalize().unwrap())
        );
    }

    #[tokio::test]
    async fn legacy_custom_root_supports_rollback_and_uninstall_after_config_changes() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("state/apps.json"));
        let custom_root = temp.path().join("legacy-custom-root");
        let config = Config {
            install_root: Some(custom_root.clone()),
            ..Config::default()
        };
        let fixture = temp.path().join("payload.AppImage");

        fs::write(&fixture, b"version one").unwrap();
        install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&fixture),
            Some(&config),
            Language::En,
            None,
        )
        .await
        .unwrap();

        fs::write(&fixture, b"version two").unwrap();
        let mut update = sample_plan(InstallType::AppImage, "demo.AppImage");
        update.version = "v2.0.0".to_string();
        install_from_plan(
            &update,
            &manifest,
            Some(&fixture),
            Some(&config),
            Language::En,
            None,
        )
        .await
        .unwrap();

        // Simulate a schema v3 record after the user changed the runtime root.
        let mut legacy_app = manifest.load().unwrap().apps[0].clone();
        legacy_app.managed_root = None;
        manifest.upsert_app(legacy_app).unwrap();

        let restored = rollback_repo(&manifest, "owner/project", Language::En, None)
            .unwrap()
            .unwrap();
        assert_eq!(restored.installed_version, "v1.2.3");
        assert_eq!(fs::read(&restored.install_path).unwrap(), b"version one");
        assert!(restored.install_path.starts_with(&custom_root));

        let removed = uninstall_repo(&manifest, "owner/project", Language::En, None)
            .unwrap()
            .unwrap();
        assert_eq!(removed.id, "owner/project");
        assert!(manifest.load().unwrap().apps.is_empty());
        assert!(!custom_root.join("apps/owner-project").exists());
        assert!(!custom_root.join("rollbacks/owner-project").exists());
    }

    #[tokio::test]
    async fn update_rejects_management_kind_change_before_downloading() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"managed payload").unwrap();
        let installed = install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        let external_plan = sample_plan(InstallType::WindowsInstaller, "setup.exe");
        let missing_fixture = temp.path().join("does-not-exist.exe");

        let error = install_from_plan(
            &external_plan,
            &manifest,
            Some(&missing_fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("management kind change"));
        assert_eq!(
            fs::read(installed.install_path).unwrap(),
            b"managed payload"
        );
    }

    fn temp_path_for_manifest(store: &ManifestStore) -> PathBuf {
        store.path().parent().unwrap().join("apps/other-repo")
    }

    #[tokio::test]
    async fn appimage_updates_move_previous_asset_into_rollback_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let first_fixture = temp.path().join("payload-v1.AppImage");
        let second_fixture = temp.path().join("payload-v2.AppImage");
        fs::write(&first_fixture, b"first appimage payload").unwrap();
        fs::write(&second_fixture, b"second appimage payload").unwrap();

        let first_plan = sample_plan(InstallType::AppImage, "demo-v1.AppImage");
        install_from_plan(
            &first_plan,
            &manifest,
            Some(&first_fixture),
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

        let mut second_plan = sample_plan(InstallType::AppImage, "demo-v2.AppImage");
        second_plan.version = "v2.0.0".to_string();
        let outcome = install_from_plan(
            &second_plan,
            &manifest,
            Some(&second_fixture),
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
        let rollback = stored.apps[0]
            .rollback
            .as_ref()
            .expect("update should keep rollback snapshot");
        assert_eq!(rollback.version, "v1.2.3");
        assert_eq!(rollback.asset_name, "demo-v1.AppImage");
        assert_eq!(rollback.install_path, first_install_path);
        assert_eq!(
            fs::read(rollback.snapshot_path.join("demo-v1.AppImage")).unwrap(),
            b"first appimage payload"
        );
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

        let mut second_plan = sample_plan(InstallType::Archive, "fixture-v2.tar.gz");
        second_plan.version = "v2.0.0".to_string();
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
        let rollback = stored.apps[0]
            .rollback
            .as_ref()
            .expect("archive update should keep rollback snapshot");
        assert!(rollback.snapshot_path.join("bundle/first.txt").exists());
        assert!(!rollback.snapshot_path.join("bundle/second.txt").exists());
    }

    #[tokio::test]
    async fn second_managed_update_rotates_previous_rollback_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");

        fs::write(&fixture, b"version one").unwrap();
        let first_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        install_from_plan(
            &first_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();

        fs::write(&fixture, b"version two").unwrap();
        let mut second_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        second_plan.version = "v2.0.0".to_string();
        install_from_plan(
            &second_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        let first_snapshot = manifest.load().unwrap().apps[0]
            .rollback
            .as_ref()
            .unwrap()
            .snapshot_path
            .clone();

        fs::write(&fixture, b"version three").unwrap();
        let mut third_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        third_plan.version = "v3.0.0".to_string();
        install_from_plan(
            &third_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();

        let stored = manifest.load().unwrap();
        let rollback = stored.apps[0].rollback.as_ref().unwrap();
        assert_eq!(rollback.version, "v2.0.0");
        assert_ne!(rollback.snapshot_path, first_snapshot);
        assert!(!first_snapshot.exists());
        assert_eq!(
            fs::read(rollback.snapshot_path.join("demo.AppImage")).unwrap(),
            b"version two"
        );
    }

    #[tokio::test]
    async fn managed_install_persists_app_and_success_event_once() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"atomic manifest payload").unwrap();
        let plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        let mut save_count = 0;

        install_from_plan_with_persist(
            &plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
            |next| {
                save_count += 1;
                assert_eq!(next.apps.len(), 1);
                assert_eq!(next.lifecycle_events.len(), 1);
                manifest.save_unlocked(next)
            },
        )
        .await
        .unwrap();

        assert_eq!(save_count, 1);
    }

    #[tokio::test]
    async fn managed_install_keeps_after_state_when_parent_sync_fails_after_manifest_rename() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");

        fs::write(&fixture, b"version one").unwrap();
        install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();

        fs::write(&fixture, b"version two").unwrap();
        let mut update = sample_plan(InstallType::AppImage, "demo.AppImage");
        update.version = "v2.0.0".to_string();
        let outcome = install_from_plan_with_persist(
            &update,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
            |next| {
                manifest.save_unlocked_with_parent_sync(next, |_, _| {
                    anyhow::bail!("injected parent directory sync failure")
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(fs::read(&outcome.install_path).unwrap(), b"version two");
        let stored = manifest.load().unwrap().apps[0].clone();
        assert_eq!(stored.installed_version, "v2.0.0");
        assert_eq!(
            fs::read(
                stored
                    .rollback
                    .as_ref()
                    .unwrap()
                    .snapshot_path
                    .join("demo.AppImage")
            )
            .unwrap(),
            b"version one"
        );
        assert!(!manifest.transaction_journal_path().exists());
    }

    #[test]
    fn managed_transaction_restores_active_when_promotion_fails() {
        let temp = tempfile::tempdir().unwrap();
        let active_dir = temp.path().join("apps/owner-project");
        let rollback_dir = temp.path().join("rollbacks/owner-project");
        fs::create_dir_all(&active_dir).unwrap();
        fs::write(active_dir.join("demo.AppImage"), b"old active").unwrap();
        let mut previous = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v1.0.0",
            "demo.AppImage",
            active_dir.join("demo.AppImage"),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        previous.managed_root = Some(temp.path().canonicalize().unwrap());
        let transaction = ManagedInstallTransaction::new(
            active_dir.clone(),
            rollback_dir,
            Some(previous.clone()),
        )
        .unwrap();
        fs::write(transaction.staged_dir.join("demo.AppImage"), b"new active").unwrap();
        let staged_dir = transaction.staged_dir.clone();
        let mut next = previous.clone();
        next.installed_version = "v2.0.0".to_string();
        let event = LifecycleEvent::succeeded(
            "owner/project",
            "project",
            LifecycleAction::Update,
            "updated",
            Some("v2.0.0".to_string()),
            Some("demo.AppImage".to_string()),
            Some(active_dir.join("demo.AppImage")),
            Some(InstallPathKind::ManagedPath),
        );
        let mut rename_count = 0;

        let error = transaction
            .commit_with(
                &ManifestStore::at_path(temp.path().join("apps.json")),
                &Manifest {
                    schema_version: 4,
                    apps: vec![previous],
                    lifecycle_events: Vec::new(),
                },
                &mut next,
                event,
                |_| panic!("manifest persistence must not run after failed promotion"),
                |source, target| {
                    rename_count += 1;
                    if rename_count == 2 {
                        anyhow::bail!("injected promotion failure");
                    }
                    rename_managed_path(source, target)
                },
                remove_path,
            )
            .unwrap_err();

        assert!(error.to_string().contains("injected promotion failure"));
        assert_eq!(
            fs::read(active_dir.join("demo.AppImage")).unwrap(),
            b"old active"
        );
        assert!(!staged_dir.exists());
        assert!(!temp.path().join("rollbacks/owner-project").exists());
    }

    #[tokio::test]
    async fn managed_manifest_save_failure_restores_active_and_previous_rollback() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");

        fs::write(&fixture, b"version one").unwrap();
        let first_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        install_from_plan(
            &first_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        fs::write(&fixture, b"version two").unwrap();
        let mut second_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        second_plan.version = "v2.0.0".to_string();
        install_from_plan(
            &second_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        let previous_app = manifest.load().unwrap().apps[0].clone();
        let previous_snapshot = previous_app
            .rollback
            .as_ref()
            .unwrap()
            .snapshot_path
            .clone();

        fs::write(&fixture, b"version three").unwrap();
        let mut third_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        third_plan.version = "v3.0.0".to_string();
        let error = install_from_plan_with_persist(
            &third_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
            |_| anyhow::bail!("injected manifest save failure"),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("injected manifest save failure"));
        assert_eq!(
            fs::read(previous_app.install_path.clone()).unwrap(),
            b"version two"
        );
        assert_eq!(manifest.load().unwrap().apps[0], previous_app);
        assert!(previous_snapshot.exists());
        assert_eq!(
            fs::read(previous_snapshot.join("demo.AppImage")).unwrap(),
            b"version one"
        );
        assert_eq!(
            fs::read_dir(previous_snapshot.parent().unwrap())
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn managed_post_commit_cleanup_failure_keeps_successful_manifest_and_unreferenced_stale() {
        let temp = tempfile::tempdir().unwrap();
        let manifest_store = ManifestStore::at_path(temp.path().join("apps.json"));
        let active_dir = temp.path().join("apps/owner-project");
        let rollback_dir = temp.path().join("rollbacks/owner-project");
        let previous_snapshot_path = rollback_dir.join("previous-snapshot");
        fs::create_dir_all(&active_dir).unwrap();
        fs::create_dir_all(&previous_snapshot_path).unwrap();
        fs::write(active_dir.join("demo.AppImage"), b"version two").unwrap();
        fs::write(previous_snapshot_path.join("demo.AppImage"), b"version one").unwrap();

        let mut previous = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v2.0.0",
            "demo.AppImage",
            active_dir.join("demo.AppImage"),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        previous.managed_root = Some(temp.path().canonicalize().unwrap());
        previous.rollback = Some(rollback_snapshot(
            &InstalledApp::with_install_metadata(
                "owner/project",
                "project",
                "v1.0.0",
                "demo.AppImage",
                active_dir.join("demo.AppImage"),
                InstallType::AppImage,
                InstallPathKind::ManagedPath,
                true,
            ),
            previous_snapshot_path.clone(),
        ));
        let original_manifest = Manifest {
            schema_version: 4,
            apps: vec![previous.clone()],
            lifecycle_events: Vec::new(),
        };
        manifest_store.save(&original_manifest).unwrap();
        let transaction = ManagedInstallTransaction::new(
            active_dir.clone(),
            rollback_dir.clone(),
            Some(previous),
        )
        .unwrap();
        fs::write(
            transaction.staged_dir.join("demo.AppImage"),
            b"version three",
        )
        .unwrap();
        let mut next = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v3.0.0",
            "demo.AppImage",
            active_dir.join("demo.AppImage"),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        next.managed_root = Some(temp.path().canonicalize().unwrap());
        let event = LifecycleEvent::succeeded(
            "owner/project",
            "project",
            LifecycleAction::Update,
            "updated",
            Some("v3.0.0".to_string()),
            Some("demo.AppImage".to_string()),
            Some(active_dir.join("demo.AppImage")),
            Some(InstallPathKind::ManagedPath),
        );

        transaction
            .commit_with(
                &manifest_store,
                &original_manifest,
                &mut next,
                event,
                |manifest| manifest_store.save(manifest),
                rename_managed_path,
                |_| anyhow::bail!("injected post-commit cleanup failure"),
            )
            .unwrap();

        let stored = manifest_store.load().unwrap();
        assert_eq!(stored.apps[0].installed_version, "v3.0.0");
        assert_eq!(
            stored
                .latest_lifecycle_event("owner/project")
                .unwrap()
                .outcome,
            crate::manifest::LifecycleOutcome::Succeeded
        );
        let referenced_snapshot = stored.apps[0]
            .rollback
            .as_ref()
            .unwrap()
            .snapshot_path
            .clone();
        assert!(referenced_snapshot.exists());
        assert_ne!(referenced_snapshot, previous_snapshot_path);
        assert!(!previous_snapshot_path.exists());
        let stale_paths = fs::read_dir(&rollback_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path != &referenced_snapshot)
            .collect::<Vec<_>>();
        assert_eq!(stale_paths.len(), 1);
        assert!(is_managed_gc_tombstone(&stale_paths[0]));

        let next_transaction =
            ManagedInstallTransaction::new(active_dir, rollback_dir, Some(stored.apps[0].clone()))
                .unwrap();
        assert!(!stale_paths[0].exists());
        remove_path(&next_transaction.staged_dir).unwrap();
    }

    #[test]
    fn managed_gc_removes_only_committed_tombstones() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("demo");
        let pending = staging_path(&base, "releasedock-pending-uninstall");
        let committed = staging_path(&base, "releasedock-committed-uninstall");
        let unrelated = staging_path(&base, "committed-user-data");
        fs::create_dir_all(&pending).unwrap();
        fs::create_dir_all(&committed).unwrap();
        fs::create_dir_all(&unrelated).unwrap();

        cleanup_managed_gc_tombstones(temp.path());

        assert!(pending.exists());
        assert!(!committed.exists());
        assert!(unrelated.exists());
        assert!(!is_managed_gc_tombstone(&pending));
        assert!(is_managed_gc_tombstone(&committed));
    }

    #[test]
    fn managed_post_commit_state_transition_failure_leaves_non_gc_pending_data() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("snapshot");
        fs::create_dir_all(&original).unwrap();
        fs::write(original.join("sentinel"), b"keep").unwrap();
        let tombstone = ManagedTombstone::new(&original, "stale");
        rename_managed_path(&tombstone.original, &tombstone.pending).unwrap();
        let mut cleanup_called = false;

        commit_managed_tombstone(
            &mut |_, _| anyhow::bail!("injected state transition failure"),
            &mut |_| {
                cleanup_called = true;
                Ok(())
            },
            &tombstone,
            "test transaction",
        );

        assert!(!cleanup_called);
        assert!(tombstone.pending.exists());
        assert_eq!(
            fs::read(tombstone.pending.join("sentinel")).unwrap(),
            b"keep"
        );
        assert!(!is_managed_gc_tombstone(&tombstone.pending));
        cleanup_managed_gc_tombstones(temp.path());
        assert!(tombstone.pending.exists());
    }

    #[test]
    fn update_journal_recovers_every_precommit_move_prefix() {
        for completed_moves in 1..=3 {
            let temp = tempfile::tempdir().unwrap();
            let store = ManifestStore::at_path(temp.path().join("apps.json"));
            let root = temp.path().canonicalize().unwrap();
            let active = root.join("apps/owner-project");
            let rollback_dir = root.join("rollbacks/owner-project");
            let old_snapshot = rollback_dir.join("old");
            let pending = staging_path(&old_snapshot, "releasedock-pending-stale");
            let committed = staging_path(&old_snapshot, "releasedock-committed-stale");
            let new_snapshot = rollback_dir.join("new");
            let staged = staging_path(&active, "staging");
            fs::create_dir_all(&active).unwrap();
            fs::create_dir_all(&old_snapshot).unwrap();
            fs::create_dir_all(&staged).unwrap();
            fs::write(active.join("demo.AppImage"), b"v2").unwrap();
            fs::write(old_snapshot.join("demo.AppImage"), b"v1").unwrap();
            fs::write(staged.join("demo.AppImage"), b"v3").unwrap();
            let mut before = InstalledApp::with_install_metadata(
                "owner/project",
                "project",
                "v2",
                "demo.AppImage",
                active.join("demo.AppImage"),
                InstallType::AppImage,
                InstallPathKind::ManagedPath,
                true,
            );
            before.managed_root = Some(root.clone());
            before.launch_path = Some(active.join("demo.AppImage"));
            let previous = InstalledApp::with_install_metadata(
                "owner/project",
                "project",
                "v1",
                "demo.AppImage",
                active.join("demo.AppImage"),
                InstallType::AppImage,
                InstallPathKind::ManagedPath,
                true,
            );
            before.rollback = Some(rollback_snapshot(&previous, old_snapshot.clone()));
            let mut after = before.clone();
            after.installed_version = "v3".to_string();
            after.rollback = Some(rollback_snapshot(&before, new_snapshot.clone()));
            store.save_apps(&[before.clone()]).unwrap();
            let moves = vec![
                ManagedTransactionMove {
                    from: old_snapshot.clone(),
                    to: pending.clone(),
                    discard_path: Some(committed),
                },
                ManagedTransactionMove {
                    from: active.clone(),
                    to: new_snapshot.clone(),
                    discard_path: None,
                },
                ManagedTransactionMove {
                    from: staged.clone(),
                    to: active.clone(),
                    discard_path: None,
                },
            ];
            let journal = ManagedTransactionJournal {
                repo_id: "owner/project".to_string(),
                operation: ManagedTransactionOperation::Install,
                trusted_root: root,
                before_app: Some(before.clone()),
                after_app: Some(after),
                moves: moves.clone(),
                completed_moves,
                manifest_committed: false,
            };
            store.save_transaction_journal_unlocked(&journal).unwrap();
            for managed_move in moves.iter().take(completed_moves) {
                rename_managed_path(&managed_move.from, &managed_move.to).unwrap();
            }

            let _lock = store.lock_exclusive().unwrap();
            recover_managed_transaction_unlocked(&store).unwrap();

            assert_eq!(fs::read(active.join("demo.AppImage")).unwrap(), b"v2");
            assert_eq!(fs::read(old_snapshot.join("demo.AppImage")).unwrap(), b"v1");
            assert_eq!(fs::read(staged.join("demo.AppImage")).unwrap(), b"v3");
            assert_eq!(store.load_unlocked().unwrap().apps[0], before);
            assert!(!store.transaction_journal_path().exists());
        }
    }

    #[test]
    fn update_journal_finalizes_discard_after_manifest_commit() {
        let temp = tempfile::tempdir().unwrap();
        let store = ManifestStore::at_path(temp.path().join("apps.json"));
        let root = temp.path().canonicalize().unwrap();
        let active = root.join("apps/owner-project");
        let rollback_dir = root.join("rollbacks/owner-project");
        let old_snapshot = rollback_dir.join("old");
        let pending = staging_path(&old_snapshot, "releasedock-pending-stale");
        let committed = staging_path(&old_snapshot, "releasedock-committed-stale");
        let new_snapshot = rollback_dir.join("new");
        let staged = staging_path(&active, "staging");
        fs::create_dir_all(&active).unwrap();
        fs::create_dir_all(&old_snapshot).unwrap();
        fs::create_dir_all(&staged).unwrap();
        fs::write(active.join("demo.AppImage"), b"v2").unwrap();
        fs::write(old_snapshot.join("demo.AppImage"), b"v1").unwrap();
        fs::write(staged.join("demo.AppImage"), b"v3").unwrap();
        let mut before = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v2",
            "demo.AppImage",
            active.join("demo.AppImage"),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        before.managed_root = Some(root.clone());
        before.launch_path = Some(active.join("demo.AppImage"));
        let previous = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v1",
            "demo.AppImage",
            active.join("demo.AppImage"),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        before.rollback = Some(rollback_snapshot(&previous, old_snapshot.clone()));
        let mut after = before.clone();
        after.installed_version = "v3".to_string();
        after.rollback = Some(rollback_snapshot(&before, new_snapshot.clone()));
        let moves = vec![
            ManagedTransactionMove {
                from: old_snapshot,
                to: pending.clone(),
                discard_path: Some(committed.clone()),
            },
            ManagedTransactionMove {
                from: active.clone(),
                to: new_snapshot,
                discard_path: None,
            },
            ManagedTransactionMove {
                from: staged,
                to: active.clone(),
                discard_path: None,
            },
        ];
        for managed_move in &moves {
            rename_managed_path(&managed_move.from, &managed_move.to).unwrap();
        }
        store.save_apps(&[after.clone()]).unwrap();
        store
            .save_transaction_journal_unlocked(&ManagedTransactionJournal {
                repo_id: "owner/project".to_string(),
                operation: ManagedTransactionOperation::Install,
                trusted_root: root,
                before_app: Some(before),
                after_app: Some(after),
                moves,
                completed_moves: 3,
                manifest_committed: false,
            })
            .unwrap();

        let _lock = store.lock_exclusive().unwrap();
        recover_managed_transaction_unlocked(&store).unwrap();

        assert_eq!(fs::read(active.join("demo.AppImage")).unwrap(), b"v3");
        assert!(!pending.exists());
        assert!(!committed.exists());
        assert!(!store.transaction_journal_path().exists());
    }

    #[test]
    fn rollback_journal_recovers_every_precommit_move_prefix() {
        for completed_moves in 1..=3 {
            let temp = tempfile::tempdir().unwrap();
            let store = ManifestStore::at_path(temp.path().join("apps.json"));
            let root = temp.path().canonicalize().unwrap();
            let active = root.join("apps/owner-project");
            let rollback_dir = root.join("rollbacks/owner-project");
            let snapshot = rollback_dir.join("previous");
            let temporary = staging_path(&active, "rollback-swap");
            fs::create_dir_all(&active).unwrap();
            fs::create_dir_all(&snapshot).unwrap();
            fs::write(active.join("demo.AppImage"), b"v2").unwrap();
            fs::write(snapshot.join("demo.AppImage"), b"v1").unwrap();
            let mut before = InstalledApp::with_install_metadata(
                "owner/project",
                "project",
                "v2",
                "demo.AppImage",
                active.join("demo.AppImage"),
                InstallType::AppImage,
                InstallPathKind::ManagedPath,
                true,
            );
            before.managed_root = Some(root.clone());
            before.launch_path = Some(active.join("demo.AppImage"));
            let mut previous = before.clone();
            previous.installed_version = "v1".to_string();
            before.rollback = Some(rollback_snapshot(&previous, snapshot.clone()));
            let mut after = previous;
            after.rollback = Some(rollback_snapshot(&before, snapshot.clone()));
            let moves = vec![
                ManagedTransactionMove {
                    from: active.clone(),
                    to: temporary.clone(),
                    discard_path: None,
                },
                ManagedTransactionMove {
                    from: snapshot.clone(),
                    to: active.clone(),
                    discard_path: None,
                },
                ManagedTransactionMove {
                    from: temporary.clone(),
                    to: snapshot.clone(),
                    discard_path: None,
                },
            ];
            store.save_apps(&[before.clone()]).unwrap();
            store
                .save_transaction_journal_unlocked(&ManagedTransactionJournal {
                    repo_id: "owner/project".to_string(),
                    operation: ManagedTransactionOperation::Rollback,
                    trusted_root: root,
                    before_app: Some(before.clone()),
                    after_app: Some(after.clone()),
                    moves: moves.clone(),
                    completed_moves,
                    manifest_committed: false,
                })
                .unwrap();
            for managed_move in moves.iter().take(completed_moves) {
                rename_managed_path(&managed_move.from, &managed_move.to).unwrap();
            }

            let _lock = store.lock_exclusive().unwrap();
            recover_managed_transaction_unlocked(&store).unwrap();

            assert_eq!(fs::read(active.join("demo.AppImage")).unwrap(), b"v2");
            assert_eq!(fs::read(snapshot.join("demo.AppImage")).unwrap(), b"v1");
            assert_eq!(store.load_unlocked().unwrap().apps[0], before);
            assert!(!store.transaction_journal_path().exists());

            if completed_moves == 3 {
                for managed_move in &moves {
                    rename_managed_path(&managed_move.from, &managed_move.to).unwrap();
                }
                store
                    .save_unlocked(&Manifest {
                        schema_version: 4,
                        apps: vec![after.clone()],
                        lifecycle_events: Vec::new(),
                    })
                    .unwrap();
                store
                    .save_transaction_journal_unlocked(&ManagedTransactionJournal {
                        repo_id: "owner/project".to_string(),
                        operation: ManagedTransactionOperation::Rollback,
                        trusted_root: temp.path().canonicalize().unwrap(),
                        before_app: Some(before.clone()),
                        after_app: Some(after.clone()),
                        moves: moves.clone(),
                        completed_moves: 3,
                        manifest_committed: false,
                    })
                    .unwrap();

                recover_managed_transaction_unlocked(&store).unwrap();

                assert_eq!(fs::read(active.join("demo.AppImage")).unwrap(), b"v1");
                assert_eq!(fs::read(snapshot.join("demo.AppImage")).unwrap(), b"v2");
                assert!(!store.transaction_journal_path().exists());
            }
        }
    }

    #[test]
    fn uninstall_journal_recovers_each_precommit_move_and_finalizes_after_commit() {
        for completed_moves in 1..=2 {
            let temp = tempfile::tempdir().unwrap();
            let store = ManifestStore::at_path(temp.path().join("apps.json"));
            let root = temp.path().canonicalize().unwrap();
            let active = root.join("apps/owner-project");
            let snapshot = root.join("rollbacks/owner-project/previous");
            let active_tombstone = ManagedTombstone::new(&active, "uninstall");
            let snapshot_tombstone = ManagedTombstone::new(&snapshot, "uninstall");
            let active_pending = active_tombstone.pending.clone();
            let snapshot_pending = snapshot_tombstone.pending.clone();
            fs::create_dir_all(&active).unwrap();
            fs::create_dir_all(&snapshot).unwrap();
            fs::write(active.join("demo.AppImage"), b"v2").unwrap();
            fs::write(snapshot.join("demo.AppImage"), b"v1").unwrap();
            let mut before = InstalledApp::with_install_metadata(
                "owner/project",
                "project",
                "v2",
                "demo.AppImage",
                active.join("demo.AppImage"),
                InstallType::AppImage,
                InstallPathKind::ManagedPath,
                true,
            );
            before.managed_root = Some(root.clone());
            before.launch_path = Some(active.join("demo.AppImage"));
            let mut previous = before.clone();
            previous.installed_version = "v1".to_string();
            before.rollback = Some(rollback_snapshot(&previous, snapshot.clone()));
            let moves = vec![
                ManagedTransactionMove {
                    from: active.clone(),
                    to: active_pending.clone(),
                    discard_path: Some(active_tombstone.committed),
                },
                ManagedTransactionMove {
                    from: snapshot.clone(),
                    to: snapshot_pending.clone(),
                    discard_path: Some(snapshot_tombstone.committed),
                },
            ];
            store.save_apps(&[before.clone()]).unwrap();
            store
                .save_transaction_journal_unlocked(&ManagedTransactionJournal {
                    repo_id: "owner/project".to_string(),
                    operation: ManagedTransactionOperation::Uninstall,
                    trusted_root: root,
                    before_app: Some(before.clone()),
                    after_app: None,
                    moves: moves.clone(),
                    completed_moves,
                    manifest_committed: false,
                })
                .unwrap();
            for managed_move in moves.iter().take(completed_moves) {
                rename_managed_path(&managed_move.from, &managed_move.to).unwrap();
            }

            let _lock = store.lock_exclusive().unwrap();
            recover_managed_transaction_unlocked(&store).unwrap();

            assert!(active.exists());
            assert!(snapshot.exists());
            assert_eq!(store.load_unlocked().unwrap().apps[0], before);
        }

        let temp = tempfile::tempdir().unwrap();
        let store = ManifestStore::at_path(temp.path().join("apps.json"));
        let root = temp.path().canonicalize().unwrap();
        let active = root.join("apps/owner-project");
        let tombstone = ManagedTombstone::new(&active, "uninstall");
        let pending = tombstone.pending;
        let committed = tombstone.committed;
        fs::create_dir_all(&active).unwrap();
        fs::write(active.join("demo.AppImage"), b"v2").unwrap();
        let mut before = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v2",
            "demo.AppImage",
            active.join("demo.AppImage"),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        before.managed_root = Some(root.clone());
        rename_managed_path(&active, &pending).unwrap();
        store.save_apps(&[]).unwrap();
        store
            .save_transaction_journal_unlocked(&ManagedTransactionJournal {
                repo_id: "owner/project".to_string(),
                operation: ManagedTransactionOperation::Uninstall,
                trusted_root: root,
                before_app: Some(before),
                after_app: None,
                moves: vec![ManagedTransactionMove {
                    from: active,
                    to: pending.clone(),
                    discard_path: Some(committed.clone()),
                }],
                completed_moves: 1,
                manifest_committed: false,
            })
            .unwrap();

        let _lock = store.lock_exclusive().unwrap();
        recover_managed_transaction_unlocked(&store).unwrap();

        assert!(!pending.exists());
        assert!(!committed.exists());
        assert!(!store.transaction_journal_path().exists());
    }

    #[test]
    fn journal_recovery_rejects_external_move_paths_without_touching_them() {
        let temp = tempfile::tempdir().unwrap();
        let store = ManifestStore::at_path(temp.path().join("apps.json"));
        let root = temp.path().canonicalize().unwrap();
        let external = temp.path().parent().unwrap().join(format!(
            "releasedock-journal-sentinel-{}",
            unique_staging_token()
        ));
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("sentinel"), b"keep").unwrap();
        let mut before = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v1",
            "demo.AppImage",
            root.join("apps/owner-project/demo.AppImage"),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        before.managed_root = Some(root.clone());
        store
            .save_transaction_journal_unlocked(&ManagedTransactionJournal {
                repo_id: "owner/project".to_string(),
                operation: ManagedTransactionOperation::Uninstall,
                trusted_root: root.clone(),
                before_app: Some(before),
                after_app: None,
                moves: vec![ManagedTransactionMove {
                    from: external.clone(),
                    to: root.join("apps/pending"),
                    discard_path: None,
                }],
                completed_moves: 0,
                manifest_committed: false,
            })
            .unwrap();

        let _lock = store.lock_exclusive().unwrap();
        let error = recover_managed_transaction_unlocked(&store).unwrap_err();

        assert!(error.to_string().contains("outside trusted root"));
        assert_eq!(fs::read(external.join("sentinel")).unwrap(), b"keep");
        drop(_lock);
        remove_path(&external).unwrap();
    }

    #[test]
    fn journal_recovery_rejects_other_repo_and_apps_root_move_templates() {
        for attack in ["other-repo", "apps-root"] {
            let temp = tempfile::tempdir().unwrap();
            let store = ManifestStore::at_path(temp.path().join("apps.json"));
            let root = temp.path().canonicalize().unwrap();
            let active = root.join("apps/owner-project");
            fs::create_dir_all(&active).unwrap();
            fs::write(active.join("demo.AppImage"), b"owner").unwrap();
            let mut before = InstalledApp::with_install_metadata(
                "owner/project",
                "project",
                "v1",
                "demo.AppImage",
                active.join("demo.AppImage"),
                InstallType::AppImage,
                InstallPathKind::ManagedPath,
                true,
            );
            before.managed_root = Some(root.clone());
            before.launch_path = Some(active.join("demo.AppImage"));
            let (from, pending, committed, sentinel) = if attack == "other-repo" {
                let other = root.join("apps/other-repo");
                fs::create_dir_all(&other).unwrap();
                let sentinel = other.join("sentinel");
                fs::write(&sentinel, b"keep").unwrap();
                (
                    other,
                    root.join("apps/.other-repo.releasedock-pending-uninstall.test"),
                    root.join("apps/.other-repo.releasedock-committed-uninstall.test"),
                    sentinel,
                )
            } else {
                let sentinel = active.join("demo.AppImage");
                (
                    root.join("apps"),
                    root.join("apps/.apps.releasedock-pending-uninstall.test"),
                    root.join("apps/.apps.releasedock-committed-uninstall.test"),
                    sentinel,
                )
            };
            store
                .save_transaction_journal_unlocked(&ManagedTransactionJournal {
                    repo_id: "owner/project".to_string(),
                    operation: ManagedTransactionOperation::Uninstall,
                    trusted_root: root,
                    before_app: Some(before),
                    after_app: None,
                    moves: vec![ManagedTransactionMove {
                        from,
                        to: pending,
                        discard_path: Some(committed),
                    }],
                    completed_moves: 0,
                    manifest_committed: false,
                })
                .unwrap();

            let _lock = store.lock_exclusive().unwrap();
            let error = recover_managed_transaction_unlocked(&store).unwrap_err();

            assert!(error.to_string().contains("template"));
            let expected: &[u8] = if attack == "other-repo" {
                b"keep"
            } else {
                b"owner"
            };
            assert_eq!(fs::read(sentinel).unwrap(), expected);
        }
    }

    #[test]
    fn install_journal_rejects_non_releasedock_staging_name() {
        let temp = tempfile::tempdir().unwrap();
        let store = ManifestStore::at_path(temp.path().join("apps.json"));
        let root = temp.path().canonicalize().unwrap();
        let active = root.join("apps/owner-project");
        let mut after = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v1",
            "demo.AppImage",
            active.join("demo.AppImage"),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        after.managed_root = Some(root.clone());
        after.launch_path = Some(active.join("demo.AppImage"));
        store
            .save_transaction_journal_unlocked(&ManagedTransactionJournal {
                repo_id: "owner/project".to_string(),
                operation: ManagedTransactionOperation::Install,
                trusted_root: root.clone(),
                before_app: None,
                after_app: Some(after),
                moves: vec![ManagedTransactionMove {
                    from: root.join("apps/user-controlled-staging"),
                    to: active,
                    discard_path: None,
                }],
                completed_moves: 0,
                manifest_committed: false,
            })
            .unwrap();

        let _lock = store.lock_exclusive().unwrap();
        let error = recover_managed_transaction_unlocked(&store).unwrap_err();

        assert!(error.to_string().contains("template"));
    }

    #[tokio::test]
    async fn rollback_swaps_active_and_snapshot_and_can_swap_back() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");

        fs::write(&fixture, b"version one").unwrap();
        let first_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        install_from_plan(
            &first_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        let mut stored = manifest.load().unwrap();
        stored.apps[0].release_policy.pinned_version = Some("keep-policy".to_string());
        manifest.save(&stored).unwrap();

        fs::write(&fixture, b"version two").unwrap();
        let mut second_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        second_plan.version = "v2.0.0".to_string();
        install_from_plan(
            &second_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();

        let restored = rollback_repo(&manifest, "owner/project", Language::En, None)
            .unwrap()
            .unwrap();
        assert_eq!(restored.installed_version, "v1.2.3");
        assert_eq!(fs::read(&restored.install_path).unwrap(), b"version one");
        assert_eq!(
            restored.release_policy.pinned_version.as_deref(),
            Some("keep-policy")
        );
        let current_snapshot = restored.rollback.as_ref().unwrap();
        assert_eq!(current_snapshot.version, "v2.0.0");
        assert_eq!(
            fs::read(current_snapshot.snapshot_path.join("demo.AppImage")).unwrap(),
            b"version two"
        );

        let swapped_back = rollback_repo(&manifest, "owner/project", Language::En, None)
            .unwrap()
            .unwrap();
        assert_eq!(swapped_back.installed_version, "v2.0.0");
        assert_eq!(
            fs::read(&swapped_back.install_path).unwrap(),
            b"version two"
        );
        assert_eq!(swapped_back.rollback.as_ref().unwrap().version, "v1.2.3");
    }

    #[tokio::test]
    async fn guarded_rollback_rejects_version_change_before_swapping_paths() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");

        fs::write(&fixture, b"version one").unwrap();
        let first_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        install_from_plan(
            &first_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();

        fs::write(&fixture, b"version two").unwrap();
        let mut second_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        second_plan.version = "v2.0.0".to_string();
        install_from_plan(
            &second_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();

        let before = manifest.load().unwrap().apps.remove(0);
        let guard = RollbackGuard::from_app(&before);

        // Simulate another process completing the rollback after the preview
        // was rendered but before this process acquired the manifest lock.
        rollback_repo(&manifest, "owner/project", Language::En, None).unwrap();

        let error = rollback_repo_guarded(&manifest, "owner/project", &guard, Language::En, None)
            .unwrap_err();

        assert!(error.to_string().contains("stale rollback plan"));
        let stored = manifest.load().unwrap();
        assert_eq!(stored.apps[0].installed_version, "v1.2.3");
        assert_eq!(stored.apps[0].rollback.as_ref().unwrap().version, "v2.0.0");
        assert_eq!(
            fs::read(&stored.apps[0].install_path).unwrap(),
            b"version one"
        );
    }

    #[tokio::test]
    async fn guarded_rollback_without_expected_or_current_snapshot_reports_missing_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"version one").unwrap();
        install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();

        let app = manifest.load().unwrap().apps.remove(0);
        let guard = RollbackGuard::from_app(&app);
        let error = rollback_repo_guarded(&manifest, "owner/project", &guard, Language::En, None)
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("does not have a rollback snapshot")
        );
        assert_eq!(fs::read(app.install_path).unwrap(), b"version one");
    }

    #[tokio::test]
    async fn guarded_rollback_rejects_snapshot_created_after_preview() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"version one").unwrap();
        install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();

        let previewed = manifest.load().unwrap().apps.remove(0);
        let guard = RollbackGuard::from_app(&previewed);

        // Another process creates the first rollback snapshot while this
        // process is waiting for confirmation.
        fs::write(&fixture, b"version two").unwrap();
        let mut update = sample_plan(InstallType::AppImage, "demo.AppImage");
        update.version = "v2.0.0".to_string();
        install_from_plan(&update, &manifest, Some(&fixture), None, Language::En, None)
            .await
            .unwrap();

        let error = rollback_repo_guarded(&manifest, "owner/project", &guard, Language::En, None)
            .unwrap_err();

        assert!(error.to_string().contains("stale rollback plan"));
        let stored = manifest.load().unwrap();
        let current = &stored.apps[0];
        assert_eq!(current.installed_version, "v2.0.0");
        assert_eq!(fs::read(&current.install_path).unwrap(), b"version two");
        let snapshot = current.rollback.as_ref().unwrap();
        assert_eq!(snapshot.version, "v1.2.3");
        assert_eq!(
            fs::read(snapshot.snapshot_path.join("demo.AppImage")).unwrap(),
            b"version one"
        );
    }

    #[tokio::test]
    async fn rollback_rejects_managed_app_without_snapshot_and_records_failure() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"only version").unwrap();
        let plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        install_from_plan(&plan, &manifest, Some(&fixture), None, Language::En, None)
            .await
            .unwrap();

        let error = rollback_repo(&manifest, "owner/project", Language::En, None).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("does not have a rollback snapshot")
        );
        let stored = manifest.load().unwrap();
        assert_eq!(stored.apps[0].installed_version, "v1.2.3");
        assert_eq!(
            stored
                .latest_lifecycle_event("owner/project")
                .unwrap()
                .action,
            LifecycleAction::Rollback
        );
        assert_eq!(
            stored
                .latest_lifecycle_event("owner/project")
                .unwrap()
                .outcome,
            crate::manifest::LifecycleOutcome::Failed
        );
    }

    #[test]
    fn rollback_rejects_non_managed_record() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let app = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v1.0.0",
            "demo.deb",
            temp.path().join("demo.deb"),
            InstallType::LinuxPackage,
            InstallPathKind::SystemInstaller,
            true,
        );
        manifest.save_apps(&[app]).unwrap();

        let error = rollback_repo(&manifest, "owner/project", Language::En, None).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("only managed-path installs can be rolled back")
        );
        assert_eq!(manifest.load().unwrap().apps.len(), 1);
    }

    #[tokio::test]
    async fn rollback_manifest_save_failure_restores_original_swap_state() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"version one").unwrap();
        let first_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        install_from_plan(
            &first_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        fs::write(&fixture, b"version two").unwrap();
        let mut second_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        second_plan.version = "v2.0.0".to_string();
        install_from_plan(
            &second_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        let previous = manifest.load().unwrap().apps[0].clone();
        let snapshot_path = previous.rollback.as_ref().unwrap().snapshot_path.clone();

        let error =
            rollback_repo_with_persist(&manifest, "owner/project", Language::En, None, |_| {
                anyhow::bail!("injected rollback save failure")
            })
            .unwrap_err();

        assert!(error.to_string().contains("injected rollback save failure"));
        assert_eq!(manifest.load().unwrap().apps[0], previous);
        assert_eq!(fs::read(&previous.install_path).unwrap(), b"version two");
        assert_eq!(
            fs::read(snapshot_path.join("demo.AppImage")).unwrap(),
            b"version one"
        );
    }

    #[test]
    fn rollback_second_rename_failure_restores_original_paths() {
        assert_rollback_rename_failure_restores_state(2);
    }

    #[test]
    fn rollback_third_rename_failure_restores_original_paths() {
        assert_rollback_rename_failure_restores_state(3);
    }

    fn assert_rollback_rename_failure_restores_state(failing_rename: usize) {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let active_dir = temp.path().join("apps/owner-project");
        let snapshot_path = temp
            .path()
            .join("rollbacks/owner-project/previous-snapshot");
        fs::create_dir_all(&active_dir).unwrap();
        fs::create_dir_all(&snapshot_path).unwrap();
        fs::write(active_dir.join("demo.AppImage"), b"current").unwrap();
        fs::write(snapshot_path.join("demo.AppImage"), b"previous").unwrap();
        let mut app = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v2.0.0",
            "demo.AppImage",
            active_dir.join("demo.AppImage"),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        let previous = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v1.0.0",
            "demo.AppImage",
            active_dir.join("demo.AppImage"),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        app.rollback = Some(rollback_snapshot(&previous, snapshot_path.clone()));
        manifest.save_apps(&[app.clone()]).unwrap();
        let persisted_app = manifest.load().unwrap().apps[0].clone();
        let mut rename_count = 0;

        let error = rollback_repo_with_ops(
            &manifest,
            "owner/project",
            Language::En,
            None,
            |next| manifest.save_unlocked(next),
            |source, target| {
                rename_count += 1;
                if rename_count == failing_rename {
                    anyhow::bail!("injected rollback rename failure {failing_rename}");
                }
                rename_managed_path(source, target)
            },
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("injected rollback rename failure")
        );
        assert_eq!(manifest.load().unwrap().apps[0], persisted_app);
        assert_eq!(
            fs::read(active_dir.join("demo.AppImage")).unwrap(),
            b"current"
        );
        assert_eq!(
            fs::read(snapshot_path.join("demo.AppImage")).unwrap(),
            b"previous"
        );
    }

    #[tokio::test]
    async fn managed_uninstall_removes_active_and_rollback_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"version one").unwrap();
        let first_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        install_from_plan(
            &first_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        fs::write(&fixture, b"version two").unwrap();
        let mut second_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        second_plan.version = "v2.0.0".to_string();
        install_from_plan(
            &second_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        let app = manifest.load().unwrap().apps[0].clone();
        let active_dir = managed_active_dir(&app).unwrap();
        let snapshot_path = app.rollback.as_ref().unwrap().snapshot_path.clone();
        let rollback_dir = snapshot_path.parent().unwrap().to_path_buf();

        let removed = uninstall_repo(&manifest, "owner/project", Language::En, None)
            .unwrap()
            .unwrap();

        assert_eq!(removed.id, "owner/project");
        assert!(!active_dir.exists());
        assert!(!snapshot_path.exists());
        assert!(!rollback_dir.exists());
        assert!(!temp.path().join("rollbacks").exists());
        let stored = manifest.load().unwrap();
        assert!(stored.apps.is_empty());
        assert_eq!(
            stored
                .latest_lifecycle_event("owner/project")
                .unwrap()
                .action,
            LifecycleAction::Uninstall
        );
    }

    #[tokio::test]
    async fn update_rejects_external_rollback_snapshot_before_moving_paths() {
        let (temp, manifest, mut app) = managed_app_with_rollback_fixture().await;
        let active_path = app.install_path.clone();
        let external_snapshot = temp.path().join("external-snapshot");
        fs::create_dir_all(&external_snapshot).unwrap();
        fs::write(external_snapshot.join("sentinel"), b"keep").unwrap();
        app.rollback.as_mut().unwrap().snapshot_path = external_snapshot.clone();
        manifest.upsert_app(app).unwrap();
        let fixture = temp.path().join("third.AppImage");
        fs::write(&fixture, b"version three").unwrap();
        let mut plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        plan.version = "v3.0.0".to_string();

        let error = install_from_plan(&plan, &manifest, Some(&fixture), None, Language::En, None)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("outside"));
        assert_eq!(fs::read(active_path).unwrap(), b"version two");
        assert_eq!(
            fs::read(external_snapshot.join("sentinel")).unwrap(),
            b"keep"
        );
    }

    #[tokio::test]
    async fn uninstall_rejects_external_rollback_snapshot_before_moving_paths() {
        let (temp, manifest, mut app) = managed_app_with_rollback_fixture().await;
        let active_path = app.install_path.clone();
        let external_snapshot = temp.path().join("external-snapshot");
        fs::create_dir_all(&external_snapshot).unwrap();
        fs::write(external_snapshot.join("sentinel"), b"keep").unwrap();
        app.rollback.as_mut().unwrap().snapshot_path = external_snapshot.clone();
        manifest.upsert_app(app).unwrap();

        let error = uninstall_repo(&manifest, "owner/project", Language::En, None).unwrap_err();

        assert!(error.to_string().contains("outside"));
        assert_eq!(fs::read(active_path).unwrap(), b"version two");
        assert_eq!(
            fs::read(external_snapshot.join("sentinel")).unwrap(),
            b"keep"
        );
    }

    #[tokio::test]
    async fn update_rejects_external_managed_active_before_moving_paths() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"version one").unwrap();
        install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        let external_active = temp.path().join("external-active");
        fs::create_dir_all(&external_active).unwrap();
        fs::write(external_active.join("demo.AppImage"), b"sentinel").unwrap();
        let mut app = manifest.load().unwrap().apps[0].clone();
        app.install_path = external_active.join("demo.AppImage");
        manifest.upsert_app(app).unwrap();
        fs::write(&fixture, b"version two").unwrap();
        let mut update = sample_plan(InstallType::AppImage, "demo.AppImage");
        update.version = "v2.0.0".to_string();

        let error = install_from_plan(&update, &manifest, Some(&fixture), None, Language::En, None)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("managed active"));
        assert_eq!(
            fs::read(external_active.join("demo.AppImage")).unwrap(),
            b"sentinel"
        );
    }

    #[tokio::test]
    async fn uninstall_rejects_external_managed_active_before_moving_paths() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"version one").unwrap();
        install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        let external_active = temp.path().join("external-active");
        fs::create_dir_all(&external_active).unwrap();
        fs::write(external_active.join("demo.AppImage"), b"sentinel").unwrap();
        let mut app = manifest.load().unwrap().apps[0].clone();
        app.install_path = external_active.join("demo.AppImage");
        manifest.upsert_app(app).unwrap();

        let error = uninstall_repo(&manifest, "owner/project", Language::En, None).unwrap_err();

        assert!(error.to_string().contains("managed active"));
        assert_eq!(
            fs::read(external_active.join("demo.AppImage")).unwrap(),
            b"sentinel"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn update_rejects_managed_active_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"version one").unwrap();
        let installed = install_from_plan(
            &sample_plan(InstallType::AppImage, "demo.AppImage"),
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        let active_dir = installed.install_path.parent().unwrap().to_path_buf();
        let external_active = temp.path().join("external-active");
        fs::rename(&active_dir, &external_active).unwrap();
        symlink(&external_active, &active_dir).unwrap();
        fs::write(&fixture, b"version two").unwrap();
        let mut update = sample_plan(InstallType::AppImage, "demo.AppImage");
        update.version = "v2.0.0".to_string();

        let error = install_from_plan(&update, &manifest, Some(&fixture), None, Language::En, None)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("symlink"));
        assert_eq!(
            fs::read(external_active.join("demo.AppImage")).unwrap(),
            b"version one"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rollback_rejects_repo_rollback_directory_symlink_escape() {
        use std::os::unix::fs::symlink;

        let (temp, manifest, app) = managed_app_with_rollback_fixture().await;
        let snapshot_path = app.rollback.as_ref().unwrap().snapshot_path.clone();
        let rollback_dir = snapshot_path.parent().unwrap().to_path_buf();
        let external_rollback = temp.path().join("external-rollback");
        fs::rename(&rollback_dir, &external_rollback).unwrap();
        symlink(&external_rollback, &rollback_dir).unwrap();

        let error = rollback_repo(&manifest, "owner/project", Language::En, None).unwrap_err();

        assert!(error.to_string().contains("symlink"));
        assert!(
            external_rollback
                .join(snapshot_path.file_name().unwrap())
                .exists()
        );
    }

    #[cfg(unix)]
    #[test]
    fn rollback_snapshot_validation_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let active_dir = temp.path().join("apps/owner-project");
        let rollback_dir = temp.path().join("rollbacks/owner-project");
        let external = temp.path().join("external");
        fs::create_dir_all(&active_dir).unwrap();
        fs::create_dir_all(&rollback_dir).unwrap();
        fs::create_dir_all(&external).unwrap();
        fs::write(active_dir.join("demo.AppImage"), b"active").unwrap();
        fs::write(external.join("demo.AppImage"), b"external").unwrap();
        let escaped_snapshot = rollback_dir.join("escaped");
        symlink(&external, &escaped_snapshot).unwrap();
        let app = InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v2.0.0",
            "demo.AppImage",
            active_dir.join("demo.AppImage"),
            InstallType::AppImage,
            InstallPathKind::ManagedPath,
            true,
        );
        let snapshot = rollback_snapshot(&app, escaped_snapshot);
        let store = ManifestStore::at_path(temp.path().join("apps.json"));
        let layout = validate_managed_layout(&app, &store, None).unwrap();

        let error = validate_rollback_snapshot(&app, &snapshot, &layout).unwrap_err();

        assert!(error.to_string().contains("symlink"));
        assert_eq!(
            fs::read(external.join("demo.AppImage")).unwrap(),
            b"external"
        );
    }

    #[tokio::test]
    async fn managed_uninstall_save_failure_restores_active_snapshot_and_manifest() {
        let (temp, manifest, app) = managed_app_with_rollback_fixture().await;
        let active_dir = managed_active_dir(&app).unwrap();
        let snapshot_path = app.rollback.as_ref().unwrap().snapshot_path.clone();
        let loaded = manifest.load().unwrap();

        let error = uninstall_managed_repo_with_ops(
            &manifest,
            loaded,
            app.clone(),
            &RepoRef::parse("owner/project").unwrap(),
            Language::En,
            None,
            |_| anyhow::bail!("injected uninstall save failure"),
            rename_managed_path,
            remove_path,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("injected uninstall save failure")
        );
        assert_eq!(manifest.load().unwrap().apps[0], app);
        assert!(active_dir.exists());
        assert!(snapshot_path.exists());
        drop(temp);
    }

    #[tokio::test]
    async fn managed_uninstall_move_failures_restore_or_preserve_original_state() {
        for failing_rename in [1, 2] {
            let (temp, manifest, app) = managed_app_with_rollback_fixture().await;
            let active_dir = managed_active_dir(&app).unwrap();
            let snapshot_path = app.rollback.as_ref().unwrap().snapshot_path.clone();
            let loaded = manifest.load().unwrap();
            let mut rename_count = 0;

            let error = uninstall_managed_repo_with_ops(
                &manifest,
                loaded,
                app.clone(),
                &RepoRef::parse("owner/project").unwrap(),
                Language::En,
                None,
                |next| manifest.save(next),
                |source, target| {
                    rename_count += 1;
                    if rename_count == failing_rename {
                        anyhow::bail!("injected uninstall rename failure {failing_rename}");
                    }
                    rename_managed_path(source, target)
                },
                remove_path,
            )
            .unwrap_err();

            assert!(
                error
                    .to_string()
                    .contains("injected uninstall rename failure")
            );
            assert_eq!(manifest.load().unwrap().apps[0], app);
            assert!(active_dir.exists());
            assert!(snapshot_path.exists());
            drop(temp);
        }
    }

    #[tokio::test]
    async fn managed_uninstall_cleanup_failure_is_non_fatal_after_manifest_commit() {
        let (temp, manifest, app) = managed_app_with_rollback_fixture().await;
        let active_dir = managed_active_dir(&app).unwrap();
        let snapshot_path = app.rollback.as_ref().unwrap().snapshot_path.clone();
        let loaded = manifest.load().unwrap();

        let removed = uninstall_managed_repo_with_ops(
            &manifest,
            loaded,
            app.clone(),
            &RepoRef::parse("owner/project").unwrap(),
            Language::En,
            None,
            |next| manifest.save(next),
            rename_managed_path,
            |_| anyhow::bail!("injected uninstall cleanup failure"),
        )
        .unwrap()
        .unwrap();

        assert_eq!(removed.id, app.id);
        let stored = manifest.load().unwrap();
        assert!(stored.apps.is_empty());
        assert_eq!(
            stored
                .latest_lifecycle_event("owner/project")
                .unwrap()
                .outcome,
            crate::manifest::LifecycleOutcome::Succeeded
        );
        assert!(!active_dir.exists());
        assert!(!snapshot_path.exists());
        assert!(
            fs::read_dir(temp.path().join("apps"))
                .unwrap()
                .any(|entry| is_managed_gc_tombstone(&entry.unwrap().path()))
        );
    }

    async fn managed_app_with_rollback_fixture() -> (tempfile::TempDir, ManifestStore, InstalledApp)
    {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ManifestStore::at_path(temp.path().join("apps.json"));
        let fixture = temp.path().join("payload.AppImage");
        fs::write(&fixture, b"version one").unwrap();
        let first_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        install_from_plan(
            &first_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        fs::write(&fixture, b"version two").unwrap();
        let mut second_plan = sample_plan(InstallType::AppImage, "demo.AppImage");
        second_plan.version = "v2.0.0".to_string();
        install_from_plan(
            &second_plan,
            &manifest,
            Some(&fixture),
            None,
            Language::En,
            None,
        )
        .await
        .unwrap();
        let app = manifest.load().unwrap().apps[0].clone();
        (temp, manifest, app)
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
        assert!(outcome.install_path.exists());
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
        assert!(outcome.install_path.exists());
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

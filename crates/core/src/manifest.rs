use std::{
    collections::HashMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::asset_matcher::{InstallType, is_linux_executable_asset_name};
use crate::{
    integrity::IntegrityStatus,
    release_policy::{PolicyMutation, PolicyMutationResult, ReleasePolicy},
};

static ATOMIC_WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub apps: Vec<InstalledApp>,
    #[serde(default)]
    pub lifecycle_events: Vec<LifecycleEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub repo_url: String,
    pub installed_version: String,
    pub installed_at: DateTime<Utc>,
    pub asset_name: String,
    pub install_path: PathBuf,
    /// Canonical root that owns `apps/` and `rollbacks/` for managed installs.
    #[serde(default)]
    pub managed_root: Option<PathBuf>,
    #[serde(default)]
    pub launch_path: Option<PathBuf>,
    #[serde(default)]
    pub system_package_name: Option<String>,
    #[serde(default)]
    pub system_package_manager: Option<SystemPackageManager>,
    #[serde(default = "default_install_type")]
    pub install_type: InstallType,
    #[serde(default = "default_install_path_kind")]
    pub install_path_kind: InstallPathKind,
    #[serde(default = "default_uninstall_supported")]
    pub uninstall_supported: bool,
    #[serde(default)]
    pub release_policy: ReleasePolicy,
    #[serde(default)]
    pub artifact_sha256: Option<String>,
    #[serde(default)]
    pub integrity_status: Option<IntegrityStatus>,
    #[serde(default)]
    pub checksum_asset_name: Option<String>,
    #[serde(default)]
    pub rollback: Option<RollbackSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RollbackSnapshot {
    pub version: String,
    pub asset_name: String,
    pub install_path: PathBuf,
    #[serde(default)]
    pub launch_path: Option<PathBuf>,
    pub install_type: InstallType,
    #[serde(default)]
    pub artifact_sha256: Option<String>,
    #[serde(default)]
    pub integrity_status: Option<IntegrityStatus>,
    #[serde(default)]
    pub checksum_asset_name: Option<String>,
    pub snapshot_path: PathBuf,
    pub installed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ManagedTransactionOperation {
    Install,
    Rollback,
    Uninstall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ManagedTransactionMove {
    pub from: PathBuf,
    pub to: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discard_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ManagedTransactionJournal {
    pub repo_id: String,
    pub operation: ManagedTransactionOperation,
    pub trusted_root: PathBuf,
    pub before_app: Option<InstalledApp>,
    pub after_app: Option<InstalledApp>,
    pub moves: Vec<ManagedTransactionMove>,
    pub completed_moves: usize,
    #[serde(default)]
    pub manifest_committed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LifecycleAction {
    Install,
    Update,
    Downgrade,
    Rollback,
    PolicyChange,
    Uninstall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LifecycleOutcome {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleEvent {
    pub repo_id: String,
    pub repo_name: String,
    pub action: LifecycleAction,
    pub outcome: LifecycleOutcome,
    pub recorded_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_path_kind: Option<InstallPathKind>,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl LifecycleEvent {
    pub fn succeeded(
        repo_id: impl Into<String>,
        repo_name: impl Into<String>,
        action: LifecycleAction,
        summary: impl Into<String>,
        version: Option<String>,
        asset_name: Option<String>,
        install_path: Option<PathBuf>,
        install_path_kind: Option<InstallPathKind>,
    ) -> Self {
        Self {
            repo_id: repo_id.into(),
            repo_name: repo_name.into(),
            action,
            outcome: LifecycleOutcome::Succeeded,
            recorded_at: Utc::now(),
            version,
            asset_name,
            install_path,
            install_path_kind,
            summary: summary.into(),
            error: None,
        }
    }

    pub fn failed(
        repo_id: impl Into<String>,
        repo_name: impl Into<String>,
        action: LifecycleAction,
        summary: impl Into<String>,
        error: impl Into<String>,
        version: Option<String>,
        asset_name: Option<String>,
        install_path: Option<PathBuf>,
        install_path_kind: Option<InstallPathKind>,
    ) -> Self {
        Self {
            repo_id: repo_id.into(),
            repo_name: repo_name.into(),
            action,
            outcome: LifecycleOutcome::Failed,
            recorded_at: Utc::now(),
            version,
            asset_name,
            install_path,
            install_path_kind,
            summary: summary.into(),
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InstallPathKind {
    ManagedPath,
    SystemInstaller,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SystemPackageManager {
    Debian,
    Rpm,
    Pacman,
}

pub struct ManifestStore {
    path: PathBuf,
}

pub(crate) struct ManifestLock {
    file: fs::File,
}

impl Drop for ManifestLock {
    fn drop(&mut self) {
        if let Err(error) = FileExt::unlock(&self.file) {
            eprintln!("failed to release manifest lock: {error}");
        }
    }
}

impl Manifest {
    pub fn empty() -> Self {
        Self {
            schema_version: 4,
            apps: Vec::new(),
            lifecycle_events: Vec::new(),
        }
    }

    pub fn normalize(mut self) -> Self {
        if self.schema_version < 4 {
            self.schema_version = 4;
        }

        for app in &mut self.apps {
            app.normalize_legacy();
        }

        self.retain_recent_lifecycle_events(5);

        self
    }

    pub fn latest_lifecycle_event(&self, repo_id: &str) -> Option<&LifecycleEvent> {
        self.lifecycle_events
            .iter()
            .rev()
            .find(|event| event.repo_id == repo_id)
    }

    pub fn recent_lifecycle_events(&self, repo_id: &str, limit: usize) -> Vec<&LifecycleEvent> {
        if limit == 0 {
            return Vec::new();
        }

        self.lifecycle_events
            .iter()
            .rev()
            .filter(|event| event.repo_id == repo_id)
            .take(limit)
            .collect()
    }

    pub fn append_lifecycle_event(&mut self, event: LifecycleEvent) {
        self.lifecycle_events.push(event);
        self.retain_recent_lifecycle_events(5);
    }

    fn retain_recent_lifecycle_events(&mut self, limit_per_repo: usize) {
        if limit_per_repo == 0 {
            self.lifecycle_events.clear();
            return;
        }

        let mut seen_counts: HashMap<String, usize> = HashMap::new();
        let mut retained_indices = Vec::new();

        for (index, event) in self.lifecycle_events.iter().enumerate().rev() {
            let count = seen_counts.entry(event.repo_id.clone()).or_insert(0);
            if *count >= limit_per_repo {
                continue;
            }
            *count += 1;
            retained_indices.push(index);
        }

        retained_indices.sort_unstable();
        self.lifecycle_events = retained_indices
            .into_iter()
            .map(|index| self.lifecycle_events[index].clone())
            .collect();
    }
}

impl InstalledApp {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        asset_name: impl Into<String>,
        install_path: PathBuf,
    ) -> Self {
        Self::with_install_metadata(
            id,
            name,
            version,
            asset_name,
            install_path,
            InstallType::Unknown,
            InstallPathKind::ManagedPath,
            true,
        )
    }

    pub fn with_install_metadata(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        asset_name: impl Into<String>,
        install_path: PathBuf,
        install_type: InstallType,
        install_path_kind: InstallPathKind,
        uninstall_supported: bool,
    ) -> Self {
        let id = id.into();
        Self {
            repo_url: format!("https://github.com/{id}"),
            id,
            name: name.into(),
            installed_version: version.into(),
            installed_at: Utc::now(),
            asset_name: asset_name.into(),
            install_path,
            managed_root: None,
            launch_path: None,
            system_package_name: None,
            system_package_manager: None,
            install_type,
            install_path_kind,
            uninstall_supported,
            release_policy: ReleasePolicy::default(),
            artifact_sha256: None,
            integrity_status: None,
            checksum_asset_name: None,
            rollback: None,
        }
    }

    pub fn normalize_legacy(&mut self) {
        if matches!(self.install_type, InstallType::Unknown) {
            self.install_type = infer_install_type(&self.asset_name);
        }

        if matches!(self.install_path_kind, InstallPathKind::Unknown) {
            self.install_path_kind = match self.install_type {
                InstallType::WindowsInstaller | InstallType::LinuxPackage => {
                    InstallPathKind::SystemInstaller
                }
                InstallType::Unknown => InstallPathKind::ManagedPath,
                _ => InstallPathKind::ManagedPath,
            };
        }

        if matches!(
            self.install_type,
            InstallType::WindowsInstaller | InstallType::LinuxPackage
        ) {
            self.install_path_kind = InstallPathKind::SystemInstaller;
            self.uninstall_supported = matches!(
                self.install_type,
                InstallType::LinuxPackage if self.system_package_name.is_some()
            );
        } else if matches!(self.install_path_kind, InstallPathKind::ManagedPath) {
            self.uninstall_supported = true;
        }

        if matches!(self.install_type, InstallType::Executable) {
            self.launch_path = None;
        } else if self.launch_path.is_none() && matches!(self.install_type, InstallType::AppImage) {
            self.launch_path = Some(self.install_path.clone());
        }
    }
}

impl ManifestStore {
    pub fn default_path() -> Result<PathBuf> {
        let project_dirs = ProjectDirs::from("io", "releasedock", "ReleaseDock")
            .context("failed to resolve platform data directory")?;
        Ok(project_dirs.data_local_dir().join("apps.json"))
    }

    pub fn default() -> Result<Self> {
        Ok(Self {
            path: Self::default_path()?,
        })
    }

    pub fn at_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Manifest> {
        self.load_unlocked()
    }

    pub(crate) fn load_unlocked(&self) -> Result<Manifest> {
        if !self.path.exists() {
            return Ok(Manifest::empty());
        }

        let content = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read manifest {}", self.path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse manifest {}", self.path.display()))
            .map(Manifest::normalize)
    }

    pub fn save_apps(&self, apps: &[InstalledApp]) -> Result<()> {
        self.mutate(|manifest| {
            manifest.apps = apps.to_vec();
            Ok(())
        })
    }

    pub fn upsert_app(&self, app: InstalledApp) -> Result<()> {
        self.mutate(|manifest| {
            manifest.apps.retain(|existing| existing.id != app.id);
            manifest.apps.push(app);
            Ok(())
        })
    }

    pub fn append_lifecycle_event(&self, event: LifecycleEvent) -> Result<()> {
        self.mutate(|manifest| {
            manifest.append_lifecycle_event(event);
            Ok(())
        })
    }

    /// Applies one repository policy mutation while holding the manifest lock.
    /// The app record and its lifecycle event are committed by the same atomic write.
    pub fn mutate_release_policy(
        &self,
        repo_id: &str,
        mutation: PolicyMutation,
    ) -> Result<PolicyMutationResult> {
        self.mutate(|manifest| {
            let app = manifest
                .apps
                .iter_mut()
                .find(|app| app.id == repo_id)
                .ok_or_else(|| anyhow::anyhow!("managed app `{repo_id}` is not installed"))?;
            let previous = app.release_policy.clone();
            mutation.apply(&mut app.release_policy, &app.installed_version);
            let result = PolicyMutationResult {
                policy: app.release_policy.clone(),
                changed: app.release_policy != previous,
            };

            if result.changed {
                let event = LifecycleEvent::succeeded(
                    app.id.clone(),
                    app.name.clone(),
                    LifecycleAction::PolicyChange,
                    mutation.summary(&app.release_policy),
                    Some(app.installed_version.clone()),
                    Some(app.asset_name.clone()),
                    Some(app.install_path.clone()),
                    Some(app.install_path_kind),
                );
                manifest.append_lifecycle_event(event);
            }

            Ok(result)
        })
    }

    pub fn remove_app(&self, repo_id: &str) -> Result<Option<InstalledApp>> {
        self.mutate(|manifest| {
            let index = manifest.apps.iter().position(|app| app.id == repo_id);
            let Some(index) = index else {
                return Ok(None);
            };
            Ok(Some(manifest.apps.remove(index)))
        })
    }

    pub fn save(&self, manifest: &Manifest) -> Result<()> {
        let _lock = self.lock_exclusive()?;
        self.save_unlocked(manifest)
    }

    pub(crate) fn lock_exclusive(&self) -> Result<ManifestLock> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create manifest directory {}", parent.display())
            })?;
        }
        let mut lock_name = self.path.as_os_str().to_os_string();
        lock_name.push(".lock");
        let lock_path = PathBuf::from(lock_name);
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("failed to open manifest lock {}", lock_path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("failed to lock manifest {}", lock_path.display()))?;
        Ok(ManifestLock { file })
    }

    pub(crate) fn save_unlocked(&self, manifest: &Manifest) -> Result<()> {
        self.save_unlocked_with_parent_sync_impl(manifest, sync_parent_directory)
    }

    #[cfg(test)]
    pub(crate) fn save_unlocked_with_parent_sync<S>(
        &self,
        manifest: &Manifest,
        sync_parent: S,
    ) -> Result<()>
    where
        S: FnMut(&Path, &str) -> Result<()>,
    {
        self.save_unlocked_with_parent_sync_impl(manifest, sync_parent)
    }

    fn save_unlocked_with_parent_sync_impl<S>(
        &self,
        manifest: &Manifest,
        sync_parent: S,
    ) -> Result<()>
    where
        S: FnMut(&Path, &str) -> Result<()>,
    {
        let manifest = manifest.clone().normalize();
        let content =
            serde_json::to_vec_pretty(&manifest).context("failed to serialize manifest")?;
        write_atomically_with_parent_sync(&self.path, &content, "manifest", sync_parent)
    }

    pub(crate) fn load_transaction_journal_unlocked(
        &self,
    ) -> Result<Option<ManagedTransactionJournal>> {
        let path = self.transaction_journal_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read transaction journal {}", path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse transaction journal {}", path.display()))
            .map(Some)
    }

    pub(crate) fn save_transaction_journal_unlocked(
        &self,
        journal: &ManagedTransactionJournal,
    ) -> Result<()> {
        let path = self.transaction_journal_path();
        let content = serde_json::to_vec_pretty(journal)
            .context("failed to serialize managed transaction journal")?;
        write_atomically(&path, &content, "managed transaction journal")
    }

    pub(crate) fn remove_transaction_journal_unlocked(&self) -> Result<()> {
        let path = self.transaction_journal_path();
        if path.exists() {
            fs::remove_file(&path).with_context(|| {
                format!("failed to remove transaction journal {}", path.display())
            })?;
        }
        Ok(())
    }

    pub(crate) fn transaction_journal_path(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(".transaction.json");
        PathBuf::from(name)
    }

    fn mutate<T>(&self, mutate: impl FnOnce(&mut Manifest) -> Result<T>) -> Result<T> {
        let _lock = self.lock_exclusive()?;
        let mut manifest = self.load_unlocked()?;
        let result = mutate(&mut manifest)?;
        self.save_unlocked(&manifest)?;
        Ok(result)
    }
}

fn write_atomically(path: &Path, content: &[u8], description: &str) -> Result<()> {
    write_atomically_with_parent_sync(path, content, description, sync_parent_directory)
}

fn write_atomically_with_parent_sync<S>(
    path: &Path,
    content: &[u8],
    description: &str,
    mut sync_parent: S,
) -> Result<()>
where
    S: FnMut(&Path, &str) -> Result<()>,
{
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create {description} directory {}",
            parent.display()
        )
    })?;
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("{description} path {} has no file name", path.display()))?;

    // create_new prevents an existing file or symlink from being followed. A
    // process/time/counter token also lets concurrent writers use separate files.
    for _ in 0..64 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = ATOMIC_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = OsString::from(".");
        temp_name.push(file_name);
        temp_name.push(format!(".tmp.{}-{nanos}-{sequence}", std::process::id()));
        let temp_path = parent.join(temp_name);
        let mut file = match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to create temporary {description} {}",
                        temp_path.display()
                    )
                });
            }
        };

        let result = (|| {
            file.write_all(content).with_context(|| {
                format!(
                    "failed to write temporary {description} {}",
                    temp_path.display()
                )
            })?;
            file.sync_all().with_context(|| {
                format!(
                    "failed to sync temporary {description} {}",
                    temp_path.display()
                )
            })?;
            drop(file);
            fs::rename(&temp_path, path)
                .with_context(|| format!("failed to replace {description} {}", path.display()))
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }
        // rename is the logical commit point. Directory sync only strengthens
        // crash durability; reporting it as a failed save could make callers
        // reverse file moves after the manifest already contains the after state.
        if let Err(error) = sync_parent(parent, description) {
            eprintln!(
                "{description} {} was committed, but parent directory sync failed; power-loss durability is not guaranteed: {error:#}",
                path.display()
            );
        }
        return Ok(());
    }

    anyhow::bail!(
        "failed to allocate a unique temporary {description} beside {}",
        path.display()
    )
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path, description: &str) -> Result<()> {
    fs::File::open(parent)
        .with_context(|| {
            format!(
                "failed to open {description} directory {}",
                parent.display()
            )
        })?
        .sync_all()
        .with_context(|| {
            format!(
                "failed to sync {description} directory {}",
                parent.display()
            )
        })
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path, _description: &str) -> Result<()> {
    Ok(())
}

fn default_schema_version() -> u32 {
    4
}

fn default_install_type() -> InstallType {
    InstallType::Unknown
}

fn default_install_path_kind() -> InstallPathKind {
    InstallPathKind::Unknown
}

fn default_uninstall_supported() -> bool {
    true
}

fn infer_install_type(asset_name: &str) -> InstallType {
    let lowered = asset_name.to_ascii_lowercase();
    if lowered.ends_with(".msi") || lowered.ends_with(".exe") {
        return InstallType::WindowsInstaller;
    }
    if lowered.ends_with(".deb") || lowered.ends_with(".rpm") {
        return InstallType::LinuxPackage;
    }
    if lowered.ends_with(".appimage") {
        return InstallType::AppImage;
    }
    if is_linux_executable_asset_name(&lowered) {
        return InstallType::Executable;
    }
    if lowered.ends_with(".zip") {
        return InstallType::PortableArchive;
    }
    if lowered.ends_with(".tar.gz") || lowered.ends_with(".tgz") || lowered.ends_with(".tar.xz") {
        return InstallType::Archive;
    }
    InstallType::Unknown
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use super::*;

    fn sample_journal(
        repo_id: impl Into<String>,
        trusted_root: &Path,
    ) -> ManagedTransactionJournal {
        ManagedTransactionJournal {
            repo_id: repo_id.into(),
            operation: ManagedTransactionOperation::Install,
            trusted_root: trusted_root.to_path_buf(),
            before_app: None,
            after_app: None,
            moves: Vec::new(),
            completed_moves: 0,
            manifest_committed: false,
        }
    }

    #[cfg(unix)]
    #[test]
    fn journal_write_does_not_follow_legacy_fixed_temp_symlink() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let store = ManifestStore::at_path(temp.path().join("apps.json"));
        let sentinel = temp.path().join("sentinel");
        fs::write(&sentinel, b"unchanged").unwrap();
        let legacy_temp = store
            .transaction_journal_path()
            .with_extension("transaction.json.tmp");
        symlink(&sentinel, &legacy_temp).unwrap();

        store
            .save_transaction_journal_unlocked(&sample_journal("owner/project", temp.path()))
            .unwrap();

        assert_eq!(fs::read(&sentinel).unwrap(), b"unchanged");
        assert!(
            fs::symlink_metadata(&legacy_temp)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            store
                .load_transaction_journal_unlocked()
                .unwrap()
                .unwrap()
                .repo_id,
            "owner/project"
        );
    }

    #[test]
    fn concurrent_journal_writes_do_not_conflict_with_residual_temp_file() {
        let temp = tempfile::tempdir().unwrap();
        let manifest_path = temp.path().join("apps.json");
        let store = ManifestStore::at_path(manifest_path.clone());
        let residual_temp = store
            .transaction_journal_path()
            .with_extension("transaction.json.tmp");
        fs::write(&residual_temp, b"residual").unwrap();
        let barrier = Arc::new(Barrier::new(8));
        let mut writers = Vec::new();

        for index in 0..8 {
            let manifest_path = manifest_path.clone();
            let trusted_root = temp.path().to_path_buf();
            let barrier = barrier.clone();
            writers.push(std::thread::spawn(move || {
                let store = ManifestStore::at_path(manifest_path);
                barrier.wait();
                store.save_transaction_journal_unlocked(&sample_journal(
                    format!("owner/project-{index}"),
                    &trusted_root,
                ))
            }));
        }
        for writer in writers {
            writer.join().unwrap().unwrap();
        }

        assert_eq!(fs::read(&residual_temp).unwrap(), b"residual");
        let journal = store.load_transaction_journal_unlocked().unwrap().unwrap();
        assert!(journal.repo_id.starts_with("owner/project-"));
    }
}

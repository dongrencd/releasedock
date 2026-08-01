import { invoke } from "@tauri-apps/api/core";
import type { DiscoveryResult, ManagedApp } from "./appModel";

export type ReleaseChannel = "stable" | "prerelease";
export type ReleaseDirection = "upgrade" | "downgrade" | "reinstall" | "unknown";
export type IntegrityStatus = "verifiedChecksum" | "recordedOnly";

export type ReleasePolicy = {
  channel: ReleaseChannel;
  pinnedVersion?: string | null;
  ignoredVersions: string[];
};

export type InstallSelectionGuard =
  | { state: "expectedAbsent" }
  | {
      state: "expectedInstalled";
      installedVersion: string;
      releasePolicy: ReleasePolicy;
    };

export type IntegrityPlan = {
  expectedSha256?: string | null;
  checksumAssetName?: string | null;
  status: IntegrityStatus;
};

export type InstallPlan = {
  repo_id: string;
  version: string;
  asset_name: string;
  install_type: "WindowsInstaller" | "PortableArchive" | "AppImage" | "LinuxPackage" | "Executable" | "Archive" | "Unknown";
  management_kind: "managedLocal" | "systemPackage" | "externalInstaller";
  system_package_manager?: "Debian" | "Rpm" | "Pacman" | null;
  requires_user_confirmation: boolean;
  integrity: IntegrityPlan;
  release_direction: ReleaseDirection;
  selection_guard?: InstallSelectionGuard | null;
  target_policy?: ReleasePolicy | null;
  notes: string[];
};

export type ReleaseVersion = {
  tagName: string;
  name?: string | null;
  prerelease: boolean;
  publishedAt?: string | null;
};

export type RollbackPreview = {
  repoId: string;
  activeVersion: string;
  snapshotVersion: string;
  snapshotPath: string;
};

export type InstallPathKind = "managedPath" | "systemInstaller" | "unknown";

export type DesktopConfig = {
  githubToken: string | null;
  proxyUrl: string | null;
  installRoot: string | null;
  effectiveInstallRoot: string | null;
  language: "en" | "zh-CN" | null;
  themeMode: "system" | "light" | "dark" | null;
  backgroundCheckEnabled: boolean | null;
  checkIntervalMinutes: number | null;
  downloadAccelerationEnabled: boolean | null;
  downloadMaxConnections: number | null;
  autostartEnabled: boolean | null;
};

export type BulkRemoveResult = {
  apps: ManagedApp[];
  removedCount: number;
};

export type GithubConnectivityTestResult = {
  ok: boolean;
  message: string;
  problem: "none" | "network" | "proxy" | "rateLimit" | "auth" | "unknown";
  usedToken: boolean;
  usedProxy: boolean;
};

// 后台检查完成事件
export type BackgroundCheckEvent = {
  updateCount: number;
  totalChecked: number;
  failedCount: number;
  checkedAt: string;
  status: "success" | "partial" | "failed";
  error?: string | null;
};

export type DashboardItemEvent = {
  refreshId: number;
  index: number;
  total: number;
  item: ManagedApp;
};

export type DashboardProgressEvent = {
  refreshId: number;
  total: number;
  completed: number;
};

export type TaskAction = "install" | "rollback" | "uninstall";

export type TaskStage =
  | "preparing"
  | "downloading"
  | "copyingAsset"
  | "verifyingArtifact"
  | "extractingArchive"
  | "creatingRollback"
  | "restoringRollback"
  | "runningSystemInstaller"
  | "updatingManifest"
  | "locatingRecord"
  | "removingFiles"
  | "finished";

export type TaskProgressEvent = {
  repoId: string;
  action: TaskAction;
  stage: TaskStage;
  message: string;
  percent?: number | null;
};

export async function loadDashboard(refreshId: number): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("load_dashboard", { refreshId });
}

export async function loadLocalDashboard(): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("load_local_dashboard");
}

export async function loadConfig(): Promise<DesktopConfig> {
  return invoke<DesktopConfig>("load_config");
}

export async function isBackgroundStart(): Promise<boolean> {
  return invoke<boolean>("is_background_start");
}

export async function isMainWindowVisible(): Promise<boolean> {
  return invoke<boolean>("is_main_window_visible");
}

export async function saveConfig(config: DesktopConfig): Promise<DesktopConfig> {
  return invoke<DesktopConfig>("save_config", { config });
}

export async function testGithubConnectivity(config: DesktopConfig): Promise<GithubConnectivityTestResult> {
  return invoke<GithubConnectivityTestResult>("test_github_connectivity", { config });
}

export async function notificationPermissionState(): Promise<string> {
  return invoke<string>("notification_permission_state");
}

export async function requestNotificationPermission(): Promise<string> {
  return invoke<string>("request_notification_permission");
}

export async function addRepo(repoInput: string): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("add_repo", { repoInput });
}

export async function searchGithubRepos(query: string): Promise<DiscoveryResult[]> {
  return invoke<DiscoveryResult[]>("search_github_repos", { query });
}

export async function listReleaseVersions(repoInput: string): Promise<ReleaseVersion[]> {
  return invoke<ReleaseVersion[]>("list_release_versions", { repoInput });
}

export async function previewInstall(
  repoInput: string,
  version?: string,
  targetChannel: ReleaseChannel = "stable"
): Promise<InstallPlan> {
  return invoke<InstallPlan>("preview_install", { repoInput, version: version || null, targetChannel });
}

export async function installRepo(plan: InstallPlan): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("install_repo", { preview: plan });
}

export async function setReleaseChannel(repoInput: string, channel: ReleaseChannel): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("set_release_channel", { repoInput, channel });
}

export async function setReleasePin(repoInput: string, version: string | null): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("set_release_pin", { repoInput, version });
}

export async function setReleaseIgnored(repoInput: string, version: string, ignored: boolean): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("set_release_ignored", { repoInput, version, ignored });
}

export async function previewRollback(repoInput: string): Promise<RollbackPreview> {
  return invoke<RollbackPreview>("preview_rollback", { repoInput });
}

export async function rollbackRepo(preview: RollbackPreview): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("rollback_repo", { preview });
}

export async function uninstallRepo(repoInput: string): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("uninstall_repo", { repoInput });
}

export async function removeTrackedRepo(repoInput: string): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("remove_tracked_repo", { repoInput });
}

export async function bulkRemoveTrackedRepos(repoInputs: string[]): Promise<BulkRemoveResult> {
  return invoke<BulkRemoveResult>("bulk_remove_tracked_repos", { repoInputs });
}

export async function adoptSystemInstall(repoInput: string): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("adopt_system_install", { repoInput });
}

export async function openApp(repoInput: string): Promise<void> {
  await invoke("open_app", { repoInput });
}

export async function openUrl(url: string): Promise<void> {
  await invoke("open_url", { url });
}

export async function openPath(path: string): Promise<void> {
  await invoke("open_path", { path });
}

export async function openInstallLocation(path: string, installPathKind?: InstallPathKind): Promise<void> {
  await invoke("open_install_location", { path, installPathKind: installPathKind ?? "unknown" });
}

export async function openInstallerFolder(path: string): Promise<void> {
  await invoke("open_installer_folder", { path });
}

export async function openNotificationSettings(): Promise<void> {
  await invoke("open_notification_settings");
}

export async function openSystemUninstallSettings(): Promise<void> {
  await invoke("open_system_uninstall_settings");
}

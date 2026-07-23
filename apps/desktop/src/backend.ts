import { invoke } from "@tauri-apps/api/core";
import type { ManagedApp } from "./appModel";

export const DEFAULT_TRACKED_REPO_ID = "dongrencd/gh-release-manager";

export type InstallPlan = {
  repo_id: string;
  repo_url: string;
  version: string;
  asset_name: string;
  download_url: string;
  install_type: "WindowsInstaller" | "PortableArchive" | "AppImage" | "LinuxPackage" | "Archive" | "Unknown";
  requires_user_confirmation: boolean;
  notes: string[];
};

export type InstallPathKind = "ManagedPath" | "SystemInstaller" | "Unknown";

export type DesktopConfig = {
  githubToken: string | null;
  proxyUrl: string | null;
  installRoot: string | null;
};

export type BulkRemoveResult = {
  apps: ManagedApp[];
  removedCount: number;
};

export async function loadDashboard(): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("load_dashboard");
}

export async function loadConfig(): Promise<DesktopConfig> {
  return invoke<DesktopConfig>("load_config");
}

export async function saveConfig(config: DesktopConfig): Promise<DesktopConfig> {
  return invoke<DesktopConfig>("save_config", { config });
}

export async function addRepo(repoInput: string): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("add_repo", { repoInput });
}

export async function previewInstall(repoInput: string): Promise<InstallPlan> {
  return invoke<InstallPlan>("preview_install", { repoInput });
}

export async function installRepo(repoInput: string): Promise<ManagedApp[]> {
  return invoke<ManagedApp[]>("install_repo", { repoInput });
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

export async function openUrl(url: string): Promise<void> {
  await invoke("open_url", { url });
}

export type AppStatus = "updateAvailable" | "current" | "needsChoice" | "failed";

export type ManagedApp = {
  id: string;
  name: string;
  currentVersion: string;
  latestVersion: string;
  status: AppStatus;
  source: string;
  releaseTitle?: string;
  releaseNote?: string;
  releaseUrl?: string;
  publishedAt?: string;
  assetName?: string;
  installPath: string;
  installType?: "WindowsInstaller" | "PortableArchive" | "AppImage" | "LinuxPackage" | "Archive" | "Unknown";
  installPathKind?: "ManagedPath" | "SystemInstaller" | "Unknown";
  uninstallSupported?: boolean;
};

export type InboxItem = ManagedApp & {
  actionLabel: "更新" | "查看" | "打开" | "重试";
  priority: number;
};

export type InboxFilter = "all" | "updateAvailable" | "needsChoice" | "failed" | "current";

export const inboxFilters: Array<{ id: InboxFilter; label: string }> = [
  { id: "all", label: "全部" },
  { id: "updateAvailable", label: "有更新" },
  { id: "needsChoice", label: "需确认" },
  { id: "failed", label: "失败" },
  { id: "current", label: "最新" }
];

export function buildUpdateInbox(apps: ManagedApp[]): InboxItem[] {
  return apps
    .map((app) => ({
      ...app,
      actionLabel: actionForStatus(app.status),
      priority: priorityForStatus(app.status)
    }))
    .sort((left, right) => left.priority - right.priority || left.name.localeCompare(right.name));
}

export function filterManagedApps(apps: ManagedApp[], filter: InboxFilter, query: string): ManagedApp[] {
  const needle = query.trim().toLowerCase();
  return apps.filter((app) => {
    if (filter !== "all" && app.status !== filter) {
      return false;
    }

    if (!needle) {
      return true;
    }

    const haystack = [app.id, app.name, app.source, app.releaseTitle, app.assetName, app.releaseUrl]
      .filter(Boolean)
      .join(" ")
      .toLowerCase();
    return haystack.includes(needle);
  });
}

function actionForStatus(status: AppStatus): InboxItem["actionLabel"] {
  switch (status) {
    case "updateAvailable":
      return "更新";
    case "needsChoice":
      return "查看";
    case "failed":
      return "重试";
    case "current":
      return "打开";
  }
}

function priorityForStatus(status: AppStatus): number {
  switch (status) {
    case "failed":
      return 0;
    case "needsChoice":
      return 1;
    case "updateAvailable":
      return 2;
    case "current":
      return 3;
  }
}

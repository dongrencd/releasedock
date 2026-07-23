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

export type ActionAvailability = {
  enabled: boolean;
  reason?: string;
};

export type BulkRemoveAvailability = {
  enabled: boolean;
  candidateCount: number;
  skippedCount: number;
  reason?: string;
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

export function getOpenReleaseAvailability(item: ManagedApp | null, busy: boolean): ActionAvailability {
  if (busy) {
    return { enabled: false, reason: "当前有任务在执行" };
  }

  if (!item?.releaseUrl) {
    return { enabled: false, reason: "当前没有可打开的 Release 链接" };
  }

  return { enabled: true };
}

export function getPrimaryActionAvailability(item: InboxItem | null, busy: boolean): ActionAvailability {
  if (busy) {
    return { enabled: false, reason: "当前有任务在执行" };
  }

  if (!item) {
    return { enabled: false, reason: "请先选择一个软件" };
  }

  if (item.status === "current") {
    return getOpenReleaseAvailability(item, busy);
  }

  return { enabled: true };
}

export function getConfirmInstallAvailability(item: InboxItem | null, busy: boolean): ActionAvailability {
  if (busy) {
    return { enabled: false, reason: "当前有任务在执行" };
  }

  if (!item) {
    return { enabled: false, reason: "请先选择一个软件" };
  }

  return { enabled: true };
}

export function getUninstallAvailability(item: InboxItem | null, busy: boolean): ActionAvailability {
  if (busy) {
    return { enabled: false, reason: "当前有任务在执行" };
  }

  if (!item) {
    return { enabled: false, reason: "请先选择一个软件" };
  }

  if (item.status === "needsChoice") {
    return { enabled: false, reason: "先选择资产后才能卸载" };
  }

  if (item.uninstallSupported === false) {
    return { enabled: false, reason: "需使用系统卸载" };
  }

  return { enabled: true };
}

export function getRemoveTrackedAvailability(item: InboxItem | null, busy: boolean): ActionAvailability {
  if (busy) {
    return { enabled: false, reason: "当前有任务在执行" };
  }

  if (!item) {
    return { enabled: false, reason: "请先选择一个软件" };
  }

  if (item.status !== "needsChoice") {
    return { enabled: false, reason: "只有未安装的跟踪项可以移除" };
  }

  return { enabled: true };
}

export function toggleSelection(selectedIds: string[], id: string): string[] {
  if (selectedIds.includes(id)) {
    return selectedIds.filter((selectedId) => selectedId !== id);
  }

  return [...selectedIds, id];
}

export function selectVisibleIds(apps: ManagedApp[]): string[] {
  return apps.map((app) => app.id);
}

export function pruneSelection(selectedIds: string[], apps: ManagedApp[]): string[] {
  const visibleIds = new Set(apps.map((app) => app.id));
  return selectedIds.filter((selectedId) => visibleIds.has(selectedId));
}

export function getBulkRemoveAvailability(
  apps: ManagedApp[],
  selectedIds: string[],
  busy: boolean
): BulkRemoveAvailability {
  if (busy) {
    return { enabled: false, candidateCount: 0, skippedCount: 0, reason: "当前有任务在执行" };
  }

  const selectedSet = new Set(selectedIds);
  const selectedApps = apps.filter((app) => selectedSet.has(app.id));
  const candidates = selectedApps.filter((app) => app.status === "needsChoice");
  const skippedCount = selectedApps.length - candidates.length;

  if (candidates.length === 0) {
    return {
      enabled: false,
      candidateCount: 0,
      skippedCount,
      reason: "选择至少一个未安装的跟踪项"
    };
  }

  return {
    enabled: true,
    candidateCount: candidates.length,
    skippedCount,
    reason: skippedCount > 0 ? `将跳过 ${skippedCount} 个不可移除项` : undefined
  };
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

    // 这里是“已管理软件列表”的本地筛选，不是 GitHub 全网搜索。
    // 只匹配列表和详情里直接展示的核心字段，避免隐藏 URL 让用户误判搜索范围。
    const haystack = [
      app.id,
      app.name,
      app.source,
      app.releaseTitle,
      app.assetName,
      app.currentVersion,
      app.latestVersion
    ]
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

import { createUiText, type Language } from "./i18n";

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
  actionLabel: string;
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

export function inboxFilters(language: Language): Array<{ id: InboxFilter; label: string }> {
  const ui = createUiText(language);
  return [
    { id: "all", label: ui.all },
    { id: "updateAvailable", label: ui.updateAvailable },
    { id: "needsChoice", label: ui.needsChoice },
    { id: "failed", label: ui.failed },
    { id: "current", label: ui.current }
  ];
}

export function buildUpdateInbox(apps: ManagedApp[], language: Language): InboxItem[] {
  return apps
    .map((app) => ({
      ...app,
      actionLabel: actionForStatus(app.status, language),
      priority: priorityForStatus(app.status)
    }))
    .sort((left, right) => left.priority - right.priority || left.name.localeCompare(right.name));
}

export function getOpenReleaseAvailability(
  item: ManagedApp | null,
  busy: boolean,
  language: Language
): ActionAvailability {
  const ui = createUiText(language);
  if (busy) {
    return { enabled: false, reason: ui.model.busy };
  }

  if (!item?.releaseUrl) {
    return { enabled: false, reason: ui.model.noRelease };
  }

  return { enabled: true };
}

export function getPrimaryActionAvailability(
  item: InboxItem | null,
  busy: boolean,
  language: Language
): ActionAvailability {
  const ui = createUiText(language);
  if (busy) {
    return { enabled: false, reason: ui.model.busy };
  }

  if (!item) {
    return { enabled: false, reason: ui.model.selectApp };
  }

  if (item.status === "current") {
    return getOpenReleaseAvailability(item, busy, language);
  }

  return { enabled: true };
}

export function getConfirmInstallAvailability(
  item: InboxItem | null,
  busy: boolean,
  language: Language
): ActionAvailability {
  const ui = createUiText(language);
  if (busy) {
    return { enabled: false, reason: ui.model.busy };
  }

  if (!item) {
    return { enabled: false, reason: ui.model.selectApp };
  }

  return { enabled: true };
}

export function getUninstallAvailability(
  item: InboxItem | null,
  busy: boolean,
  language: Language
): ActionAvailability {
  const ui = createUiText(language);
  if (busy) {
    return { enabled: false, reason: ui.model.busy };
  }

  if (!item) {
    return { enabled: false, reason: ui.model.selectApp };
  }

  if (item.status === "needsChoice") {
    return { enabled: false, reason: ui.model.selectAssetBeforeUninstall };
  }

  if (item.uninstallSupported === false) {
    return { enabled: false, reason: ui.model.useSystemUninstall };
  }

  return { enabled: true };
}

export function getRemoveTrackedAvailability(
  item: InboxItem | null,
  busy: boolean,
  language: Language
): ActionAvailability {
  const ui = createUiText(language);
  if (busy) {
    return { enabled: false, reason: ui.model.busy };
  }

  if (!item) {
    return { enabled: false, reason: ui.model.selectApp };
  }

  if (item.status !== "needsChoice") {
    return { enabled: false, reason: ui.model.onlyUntracked };
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
  busy: boolean,
  language: Language
): BulkRemoveAvailability {
  const ui = createUiText(language);
  if (busy) {
    return { enabled: false, candidateCount: 0, skippedCount: 0, reason: ui.model.busy };
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
      reason: ui.model.selectAtLeastOne
    };
  }

  return {
    enabled: true,
    candidateCount: candidates.length,
    skippedCount,
    reason: skippedCount > 0 ? ui.model.skippedCount(skippedCount) : undefined
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

    // Local filtering only. This is not a GitHub-wide search.
    // Match only the core fields already visible in the list and details pane.
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

function actionForStatus(status: AppStatus, language: Language): InboxItem["actionLabel"] {
  const ui = createUiText(language);
  switch (status) {
    case "updateAvailable":
      return ui.action.update;
    case "needsChoice":
      return ui.action.view;
    case "failed":
      return ui.action.retry;
    case "current":
      return ui.action.open;
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

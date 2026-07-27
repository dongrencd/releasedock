import { createUiText, type Language } from "./i18n";

export type AppStatus = "updateAvailable" | "current" | "needsChoice" | "noRelease" | "failed";

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
  launchPath?: string;
  systemPackageName?: string;
  systemPackageManager?: SystemPackageManagerName | null;
  managementKind?: InstallManagementKind | null;
  installPath: string;
  installType?: "WindowsInstaller" | "PortableArchive" | "AppImage" | "LinuxPackage" | "Executable" | "Archive" | "Unknown";
  installPathKind?: "ManagedPath" | "SystemInstaller" | "Unknown";
  uninstallSupported?: boolean;
};

export type InstallManagementKind = "managedLocal" | "systemPackage" | "externalInstaller";
export type SystemPackageManagerName = "Debian" | "Rpm" | "Pacman";

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

export type InspectorDetailItem = {
  label: string;
  value: string;
  fullWidth?: boolean;
  monospace?: boolean;
};

// 公开筛选以用户任务语义命名，而不是直接映射内部状态名。
// actionRequired 是聚合筛选：匹配 needsChoice 以及可移除的 noRelease 跟踪项。
export type InboxFilter = "all" | "updateAvailable" | "actionRequired" | "failed";

export type ReleaseNoteBlock =
  | { type: "heading"; level: 1 | 2 | 3; text: string }
  | { type: "paragraph"; text: string }
  | { type: "list"; ordered: boolean; items: string[] }
  | { type: "quote"; text: string }
  | { type: "table"; header: string[]; rows: string[][] }
  | { type: "divider" }
  | { type: "code"; text: string };

export type TaskProgressAction = "install" | "uninstall";

export type TaskProgressStage =
  | "preparing"
  | "downloading"
  | "copyingAsset"
  | "extractingArchive"
  | "runningSystemInstaller"
  | "updatingManifest"
  | "locatingRecord"
  | "removingFiles"
  | "finished"
  | "failed";

export type TaskProgressLike = {
  action: TaskProgressAction;
  stage: TaskProgressStage;
  message: string;
  percent?: number | null;
};

export type StatusDockPresentation = {
  eyebrow: string;
  headline: string;
  detail: string;
  pillLabel: string;
  failed: boolean;
  showProgress: boolean;
  progressMode: "determinate" | "indeterminate";
  progressPercent: number | null;
};

export function inboxFilters(language: Language): Array<{ id: InboxFilter; label: string }> {
  const ui = createUiText(language);
  return [
    { id: "all", label: ui.all },
    { id: "updateAvailable", label: ui.updateAvailable },
    { id: "actionRequired", label: ui.needsChoice },
    { id: "failed", label: ui.failed }
  ];
}

// 需处理：用户任务聚合，同时覆盖需要选资产的 needsChoice 状态
// 以及可移除的 noRelease 跟踪项。
// 用于公开筛选 actionRequired，以及顶部"需处理"统计。
export function isActionRequired(app: ManagedApp): boolean {
  return app.status === "needsChoice"
    || (app.status === "noRelease" && app.installPathKind === "Unknown");
}

export function hasInstallableAsset(app: ManagedApp): boolean {
  return app.status === "needsChoice" && Boolean(app.assetName?.trim());
}

export function buildUpdateInbox(apps: ManagedApp[], language: Language): InboxItem[] {
  return apps
    .map((app) => ({
      ...app,
      actionLabel: actionForApp(app, language),
      priority: priorityForStatus(app.status)
    }))
    .sort((left, right) => left.priority - right.priority || left.name.localeCompare(right.name));
}

export function parseReleaseNote(note: string): ReleaseNoteBlock[] {
  const lines = note.replace(/\r\n/g, "\n").split("\n");
  const blocks: ReleaseNoteBlock[] = [];
  let index = 0;

  const pushParagraph = (buffer: string[]) => {
    const text = buffer.join(" ").trim();
    if (text) {
      blocks.push({ type: "paragraph", text });
    }
    buffer.length = 0;
  };

  while (index < lines.length) {
    const line = lines[index];
    const trimmed = line.trim();

    if (!trimmed || isHtmlComment(trimmed)) {
      index += 1;
      continue;
    }

    if (isHorizontalRule(trimmed)) {
      blocks.push({ type: "divider" });
      index += 1;
      continue;
    }

    const headingMatch = /^(#{1,3})\s+(.*)$/.exec(line);
    if (headingMatch) {
      blocks.push({
        type: "heading",
        level: headingMatch[1].length as 1 | 2 | 3,
        text: headingMatch[2].trim()
      });
      index += 1;
      continue;
    }

    if (/^```/.test(line)) {
      const codeLines: string[] = [];
      index += 1;
      while (index < lines.length && !/^```/.test(lines[index])) {
        codeLines.push(lines[index]);
        index += 1;
      }
      if (index < lines.length) {
        index += 1;
      }
      blocks.push({ type: "code", text: codeLines.join("\n") });
      continue;
    }

    const tableHeader = parseTableCells(line);
    if (
      tableHeader.length > 1 &&
      index + 1 < lines.length &&
      isTableSeparator(lines[index + 1])
    ) {
      const rows: string[][] = [];
      index += 2;
      while (index < lines.length) {
        const rowLine = lines[index];
        const rowTrimmed = rowLine.trim();
        if (!rowTrimmed || !rowLine.includes("|")) {
          break;
        }
        if (
          isHtmlComment(rowTrimmed) ||
          isHorizontalRule(rowTrimmed) ||
          /^(#{1,3})\s+/.test(rowLine) ||
          /^```/.test(rowLine) ||
          /^(\s*[-*+]\s+)/.test(rowLine) ||
          /^(\s*\d+\.\s+)/.test(rowLine) ||
          /^>\s?/.test(rowLine)
        ) {
          break;
        }
        rows.push(normalizeTableRow(parseTableCells(rowLine), tableHeader.length));
        index += 1;
      }
      blocks.push({ type: "table", header: tableHeader, rows });
      continue;
    }

    const orderedListMatch = /^(\s*\d+\.\s+)/.test(line);
    const unorderedListMatch = /^(\s*[-*+]\s+)/.test(line);
    if (orderedListMatch || unorderedListMatch) {
      const ordered = orderedListMatch;
      const items: string[] = [];
      const itemPattern = ordered ? /^(\s*\d+\.\s+)/ : /^(\s*[-*+]\s+)/;
      while (index < lines.length && itemPattern.test(lines[index])) {
        items.push(lines[index].replace(itemPattern, "").trim());
        index += 1;
      }
      blocks.push({ type: "list", ordered, items });
      continue;
    }

    const quoteMatch = /^>\s?(.*)$/.test(line);
    if (quoteMatch) {
      const quoteLines: string[] = [];
      while (index < lines.length && /^>\s?/.test(lines[index])) {
        quoteLines.push(lines[index].replace(/^>\s?/, "").trim());
        index += 1;
      }
      blocks.push({ type: "quote", text: quoteLines.join(" ").trim() });
      continue;
    }

    const paragraphLines = [trimmed];
    index += 1;
    while (
      index < lines.length &&
      lines[index].trim() &&
      !isHtmlComment(lines[index].trim()) &&
      !isHorizontalRule(lines[index].trim()) &&
      !/^(#{1,3})\s+/.test(lines[index]) &&
      !/^```/.test(lines[index]) &&
      !/^(\s*[-*+]\s+)/.test(lines[index]) &&
      !/^(\s*\d+\.\s+)/.test(lines[index]) &&
      !/^>\s?/.test(lines[index])
    ) {
      paragraphLines.push(lines[index].trim());
      index += 1;
    }
    pushParagraph(paragraphLines);
  }

  return blocks;
}

export function taskActionLabel(action: TaskProgressAction, language: Language): string {
  const ui = createUiText(language);
  return action === "install" ? ui.task.install : ui.task.uninstall;
}

export function taskStageLabel(stage: TaskProgressStage, language: Language): string {
  const ui = createUiText(language);
  switch (stage) {
    case "preparing":
      return ui.stage.preparing;
    case "downloading":
      return ui.stage.downloading;
    case "copyingAsset":
      return ui.stage.copyingAsset;
    case "extractingArchive":
      return ui.stage.extractingArchive;
    case "runningSystemInstaller":
      return ui.stage.runningSystemInstaller;
    case "updatingManifest":
      return ui.stage.updatingManifest;
    case "locatingRecord":
      return ui.stage.locatingRecord;
    case "removingFiles":
      return ui.stage.removingFiles;
    case "finished":
      return ui.stage.finished;
    case "failed":
      return ui.stage.failed;
  }
}

export function installManagementKindLabel(value: InstallManagementKind, language: Language): string {
  const ui = createUiText(language);
  switch (value) {
    case "managedLocal":
      return ui.managementKind.managedLocal;
    case "systemPackage":
      return ui.managementKind.systemPackage;
    case "externalInstaller":
      return ui.managementKind.externalInstaller;
  }
}

export function systemPackageManagerLabel(value: SystemPackageManagerName): string {
  switch (value) {
    case "Debian":
      return "Debian";
    case "Rpm":
      return "RPM";
    case "Pacman":
      return "Pacman";
  }
}

export function buildStatusDockPresentation(
  taskProgress: TaskProgressLike | null,
  busy: boolean,
  taskStatus: string,
  language: Language
): StatusDockPresentation {
  const ui = createUiText(language);
  if (!taskProgress) {
    return {
      eyebrow: ui.statusBar,
      headline: taskStatus,
      detail: busy ? taskStatus : "",
      pillLabel: busy ? ui.processing : taskStatus,
      failed: false,
      showProgress: busy,
      progressMode: "indeterminate",
      progressPercent: null
    };
  }

  const normalizedPercent = normalizeProgressPercent(taskProgress.percent);
  const progressPercent = taskProgress.stage === "finished" && normalizedPercent == null ? 100 : normalizedPercent;
  const failed = taskProgress.stage === "failed";

  return {
    eyebrow: taskActionLabel(taskProgress.action, language),
    headline: taskStageLabel(taskProgress.stage, language),
    detail: taskProgress.message,
    pillLabel: failed ? ui.status.failed : progressPercent == null ? ui.processing : `${progressPercent}%`,
    failed,
    showProgress: true,
    progressMode: progressPercent == null ? "indeterminate" : "determinate",
    progressPercent
  };
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

export function getOpenAppAvailability(
  item: ManagedApp | null,
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
    return { enabled: false, reason: ui.model.noLaunchTarget };
  }

  if (item.installPathKind !== "ManagedPath") {
    return { enabled: false, reason: ui.model.noLaunchTarget };
  }

  if (!item.launchPath) {
    return { enabled: false, reason: ui.model.noLaunchTarget };
  }

  return { enabled: true };
}

// 次级 Release 动作只在主按钮不是 Release 时出现，避免重复出口。
export function shouldShowOpenReleaseSecondary(item: InboxItem | null, language: Language): boolean {
  if (!item) {
    return false;
  }

  return item.actionLabel !== createUiText(language).openRelease;
}

// 次级打开软件动作只在可更新且存在可执行目标时出现，避免和主按钮重复。
export function shouldShowOpenAppSecondary(item: InboxItem | null): boolean {
  if (!item) {
    return false;
  }

  return item.status === "updateAvailable" && item.installPathKind === "ManagedPath" && Boolean(item.launchPath);
}

export function getDetailPathLabel(item: ManagedApp | null, language: Language): string {
  const ui = createUiText(language);

  if (!item) {
    return ui.installPath;
  }

  if (item.status === "needsChoice" && !hasInstallableAsset(item)) {
    return ui.defaultInstallPath;
  }

  return item.installPathKind === "SystemInstaller" ? ui.installerFile : ui.installPath;
}

export function getInspectorDetailItems(item: ManagedApp | null, language: Language): InspectorDetailItem[] {
  const ui = createUiText(language);

  if (!item) {
    return [];
  }

  const items: InspectorDetailItem[] = [
    {
      label: ui.assetFile,
      value: item.assetName?.trim() || ui.model.noInstallableAsset,
      fullWidth: true,
      monospace: true
    },
    {
      label: getDetailPathLabel(item, language),
      value: item.installPath,
      fullWidth: true,
      monospace: true
    }
  ];

  if (item.systemPackageName) {
    items.push({
      label: ui.systemPackage,
      value: item.systemPackageName,
      monospace: true
    });
  }

  if (item.systemPackageManager) {
    items.push({
      label: ui.systemPackageManager,
      value: systemPackageManagerLabel(item.systemPackageManager)
    });
  }

  if (item.managementKind) {
    items.push({
      label: ui.installManagement,
      value: installManagementKindLabel(item.managementKind, language)
    });
  }

  return items;
}

export function hasSecondaryInspectorActions(item: InboxItem | null, language: Language): boolean {
  if (!item) {
    return false;
  }

  return shouldShowOpenAppSecondary(item)
    || shouldShowOpenReleaseSecondary(item, language)
    || (item.status !== "needsChoice" && item.installPathKind !== "Unknown");
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

  if (item.status === "current" || item.status === "noRelease") {
    if (item.launchPath && item.installPathKind === "ManagedPath") {
      return getOpenAppAvailability(item, busy, language);
    }

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

  if (item.status !== "needsChoice" && !(item.status === "noRelease" && item.installPathKind === "Unknown")) {
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
  const candidates = selectedApps.filter(
    (app) => app.status === "needsChoice" || (app.status === "noRelease" && app.installPathKind === "Unknown")
  );
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
    // actionRequired 是聚合筛选，不能直接用 status 比较
    if (filter === "actionRequired") {
      if (!isActionRequired(app)) {
        return false;
      }
    } else if (filter !== "all" && app.status !== filter) {
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

function actionForApp(app: ManagedApp, language: Language): InboxItem["actionLabel"] {
  const ui = createUiText(language);
  switch (app.status) {
    case "updateAvailable":
      return ui.action.update;
    case "needsChoice":
      return hasInstallableAsset(app) ? ui.action.install : ui.openRelease;
    case "noRelease":
      return app.installPathKind === "ManagedPath" && app.launchPath ? ui.action.openApp : ui.action.open;
    case "failed":
      return ui.action.retry;
    case "current":
      return app.installPathKind === "ManagedPath" && app.launchPath ? ui.action.openApp : ui.action.open;
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
    case "noRelease":
      return 3;
    case "current":
      return 4;
  }
}

function isHtmlComment(line: string) {
  return /^<!--[\s\S]*-->$/.test(line);
}

function isHorizontalRule(line: string) {
  return /^(?:-{3,}|\*{3,}|_{3,})$/.test(line);
}

function normalizeProgressPercent(percent: number | null | undefined) {
  if (percent == null || Number.isNaN(percent)) {
    return null;
  }

  return Math.max(0, Math.min(100, Math.round(percent)));
}

function parseTableCells(line: string) {
  return line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((cell) => cell.trim());
}

function normalizeTableRow(cells: string[], width: number) {
  const row = cells.slice(0, width);
  while (row.length < width) {
    row.push("");
  }
  return row;
}

function isTableSeparator(line: string) {
  const cells = parseTableCells(line);
  return cells.length > 1 && cells.every((cell) => /^:?-{3,}:?$/.test(cell));
}

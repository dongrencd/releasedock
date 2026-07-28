import { createUiText, formatRecordedAt, type Language } from "./i18n";
import type {
  IntegrityPlan,
  IntegrityStatus,
  ReleaseChannel,
  ReleaseDirection,
  ReleasePolicy
} from "./backend";

export type AppStatus = "updateAvailable" | "downgradeAvailable" | "current" | "needsChoice" | "noRelease" | "failed";

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
  installPathKind?: "managedPath" | "systemInstaller" | "unknown";
  uninstallSupported?: boolean;
  releasePolicy?: ReleasePolicy;
  artifactSha256?: string | null;
  integrityStatus?: IntegrityStatus | null;
  checksumAssetName?: string | null;
  rollback?: { version: string; assetName: string } | null;
  releaseDirection?: ReleaseDirection;
  recentActivities?: LifecycleActivity[] | null;
};

export type LifecycleActivity = {
  repoId: string;
  repoName: string;
  action: "install" | "update" | "downgrade" | "rollback" | "policyChange" | "uninstall";
  outcome: "succeeded" | "failed";
  recordedAt: string;
  version?: string | null;
  assetName?: string | null;
  installPath?: string | null;
  installPathKind?: ManagedApp["installPathKind"] | null;
  summary: string;
  error?: string | null;
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

export type SelectionActionAvailability = BulkRemoveAvailability & {
  kind: "remove" | "uninstall" | "mixed";
  label: string;
  uninstallTargetId?: string;
};

export type InspectorDetailItem = {
  label: string;
  value: string;
  fullWidth?: boolean;
  monospace?: boolean;
};

export type LifecycleHistoryEntry = {
  summary: string;
  recordedAt: string;
  failed: boolean;
  error?: string | null;
};

export type ReleaseActionGuidanceKind = "docker" | "source" | "manual";

export type ReleaseActionGuidance = {
  kind: ReleaseActionGuidanceKind;
  title: string;
  summary: string;
  bullets: string[];
};

export type InspectorStatusSummary = {
  label: string;
  detail: string;
  tone: "neutral" | "success" | "warning" | "danger";
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

export type TaskProgressAction = "install" | "rollback" | "uninstall";

export type TaskProgressStage =
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
  | "finished"
  | "failed";

export type TaskProgressLike = {
  repoId: string;
  action: TaskProgressAction;
  stage: TaskProgressStage;
  message: string;
  percent?: number | null;
};

export type PrimaryActionKind =
  | "install"
  | "update"
  | "openApp"
  | "openRelease"
  | "openInstallLocation"
  | "openInstallerFile"
  | "retry";

export type StatusDockPresentation = {
  eyebrow: string;
  headline: string;
  detail: string;
  pillLabel: string;
  failed: boolean;
  // Idle states hide the duplicate right-side pill; task states keep it for progress or failure labels.
  showPill: boolean;
  showProgress: boolean;
  progressMode: "determinate" | "indeterminate";
  progressPercent: number | null;
};

export type ConfigConnectivityWarning = {
  label: string;
  detail: string;
};

export type ConfigConnectivityInput = {
  githubToken: string;
  proxyUrl: string;
};

export type GithubConnectivityProblem = "none" | "unchecked" | "network" | "proxy" | "rateLimit" | "auth" | "unknown";

export type NetworkConfigHealth = {
  tokenConfigured: boolean;
  proxyConfigured: boolean;
  tokenLabel: string;
  proxyLabel: string;
  formatExample: string;
  warning: ConfigConnectivityWarning | null;
};

export type ConnectivityTestViewState = {
  status: "idle" | "testing" | "success" | "failed" | "stale";
  message?: string;
  problem?: GithubConnectivityProblem;
  configKey?: string;
};

export type GithubConnectivityResultLike = {
  ok: boolean;
  message: string;
  problem: GithubConnectivityProblem;
  usedToken?: boolean;
  usedProxy?: boolean;
};

export type ConnectivityTestStatus = {
  label: string;
  detail: string;
  tone: "neutral" | "busy" | "success" | "danger" | "warning";
};

export function isFailedInstallProgress(taskProgress: TaskProgressLike | null, repoId: string | null): boolean {
  return Boolean(
    taskProgress &&
      repoId &&
      taskProgress.action === "install" &&
      taskProgress.stage === "failed" &&
      taskProgress.repoId === repoId
  );
}

export function isManagedPathKind(kind?: ManagedApp["installPathKind"]): boolean {
  return kind === "managedPath";
}

export function isSystemInstallerKind(kind?: ManagedApp["installPathKind"]): boolean {
  return kind === "systemInstaller";
}

export function isUnknownInstallPathKind(kind?: ManagedApp["installPathKind"]): boolean {
  return !kind || kind === "unknown";
}

export function buildConfigConnectivityWarning(
  config: ConfigConnectivityInput,
  language: Language,
  state: ConnectivityTestViewState = { status: "idle" }
): ConfigConnectivityWarning | null {
  if (isStaleConnectivityResult(state, config) || state.status !== "failed") {
    return null;
  }

  const ui = createUiText(language);
  switch (state.problem) {
    case "network":
    case "proxy":
      return {
        label: ui.configProxyWarning,
        detail: ui.configProxyWarningHelp
      };
    case "rateLimit":
    case "auth":
      return {
        label: ui.configTokenWarning,
        detail: ui.configTokenWarningHelp
      };
    case "unknown":
      return {
        label: ui.configUnknownConnectivityWarning,
        detail: ui.configUnknownConnectivityWarningHelp
      };
    case "none":
    case "unchecked":
    case undefined:
      return null;
  }
}

export function buildNetworkConfigHealth(
  config: ConfigConnectivityInput,
  language: Language,
  state: ConnectivityTestViewState = { status: "idle" }
): NetworkConfigHealth {
  const ui = createUiText(language);
  const tokenConfigured = config.githubToken.trim().length > 0;
  const proxyConfigured = config.proxyUrl.trim().length > 0;

  return {
    tokenConfigured,
    proxyConfigured,
    tokenLabel: tokenConfigured ? ui.githubTokenConfigured : ui.githubTokenOptional,
    proxyLabel: proxyConfigured ? ui.proxyConfigured : ui.proxyNotConfigured,
    formatExample: ui.proxyUrlPlaceholder,
    warning: buildConfigConnectivityWarning(config, language, state)
  };
}

export function buildConnectivityTestStatus(
  state: ConnectivityTestViewState,
  language: Language,
  currentConfig?: ConfigConnectivityInput
): ConnectivityTestStatus {
  const ui = createUiText(language);

  if (isStaleConnectivityResult(state, currentConfig)) {
    return {
      label: ui.connectivityTestStale,
      detail: ui.connectivityTestStaleHelp,
      tone: "warning"
    };
  }

  if (state.status === "testing") {
    return {
      label: ui.connectivityTestTesting,
      detail: ui.connectivityTestHelp,
      tone: "busy"
    };
  }

  if (state.status === "success") {
    return {
      label: ui.connectivityTestSuccess,
      detail: state.message?.trim() || ui.connectivityTestSuccessHelp,
      tone: "success"
    };
  }

  if (state.status === "failed") {
    return {
      label: ui.connectivityTestFailed,
      detail: joinConnectivityDetail(state.message, connectivityProblemHelp(state.problem, ui)),
      tone: "danger"
    };
  }

  return {
    label: ui.connectivityTestIdle,
    detail: ui.connectivityTestHelp,
    tone: "neutral"
  };
}

export function getNetworkConfigKey(config: ConfigConnectivityInput): string {
  return JSON.stringify({
    githubToken: config.githubToken.trim(),
    proxyUrl: config.proxyUrl.trim()
  });
}

export function shouldRunAutoConnectivityCheck(config: ConfigConnectivityInput, lastCheckedKey: string | null): boolean {
  return getNetworkConfigKey(config) !== lastCheckedKey;
}

export function buildConnectivityTestViewState(
  result: GithubConnectivityResultLike,
  config: ConfigConnectivityInput
): ConnectivityTestViewState {
  return {
    status: result.ok ? "success" : "failed",
    message: result.message,
    problem: result.problem,
    configKey: getNetworkConfigKey(config)
  };
}

function isStaleConnectivityResult(
  state: ConnectivityTestViewState,
  currentConfig?: ConfigConnectivityInput
): boolean {
  return Boolean(
    currentConfig &&
      state.configKey &&
      (state.status === "success" || state.status === "failed") &&
      state.configKey !== getNetworkConfigKey(currentConfig)
  ) || state.status === "stale";
}

function connectivityProblemHelp(problem: GithubConnectivityProblem | undefined, ui: ReturnType<typeof createUiText>): string {
  switch (problem) {
    case "network":
      return ui.connectivityNetworkFailureHelp;
    case "proxy":
      return ui.connectivityProxyFailureHelp;
    case "rateLimit":
      return ui.connectivityRateLimitHelp;
    case "auth":
      return ui.connectivityAuthFailureHelp;
    case "unknown":
    case "unchecked":
    case "none":
    case undefined:
      return ui.connectivityProxyFailureHelp;
  }
}

function joinConnectivityDetail(message: string | undefined, help: string): string {
  const trimmed = message?.trim();
  if (!trimmed) {
    return help;
  }
  return `${trimmed} ${help}`;
}

function detectReleaseActionGuidanceKind(releaseText: string): ReleaseActionGuidanceKind {
  if (hasDockerSignal(releaseText)) {
    return "docker";
  }

  if (hasSourceSignal(releaseText)) {
    return "source";
  }

  return "manual";
}

function hasDockerSignal(releaseText: string): boolean {
  return [
    "docker compose",
    "docker-compose",
    "docker run",
    "dockerfile",
    "container image",
    "compose.yaml",
    "compose.yml",
    "podman run"
  ].some((signal) => releaseText.includes(signal));
}

function hasSourceSignal(releaseText: string): boolean {
  return [
    "build from source",
    "source build",
    "compile from source",
    "cargo build",
    "cmake",
    "gradle",
    "make ",
    "meson",
    "mvn",
    "npm install",
    "pip install",
    "pipx install",
    "pnpm install",
    "yarn install",
    "go build"
  ].some((signal) => releaseText.includes(signal) || releaseText === signal.trim());
}

export function shouldShowInstallLocationAction(item: ManagedApp | null): boolean {
  if (!item) {
    return false;
  }

  return item.status !== "needsChoice" && !isUnknownInstallPathKind(item.installPathKind);
}

export function shouldShowInstallLocationSecondary(item: ManagedApp | null): boolean {
  if (!item) {
    return false;
  }

  if (item.status === "needsChoice" || isUnknownInstallPathKind(item.installPathKind)) {
    return false;
  }

  const primaryActionKind = resolvePrimaryActionKind(item);
  return primaryActionKind !== "openInstallLocation" && primaryActionKind !== "openInstallerFile";
}

export function shouldShowInstallerFolderSecondary(item: ManagedApp | null): boolean {
  if (!item) {
    return false;
  }

  return item.status !== "needsChoice" && isSystemInstallerKind(item.installPathKind);
}

export function isRemovableNoRelease(item: ManagedApp): boolean {
  return item.status === "noRelease" && isUnknownInstallPathKind(item.installPathKind);
}

// 只要还是跟踪中的仓库，但当前状态已经无法继续走安装/升级链路，
// 就允许把这条跟踪移除掉。failed 状态只在没有可识别安装位置时开放，
// 避免把一个仍然有本地安装痕迹的条目误当成普通跟踪项处理。
export function isRemovableTrackedItem(item: ManagedApp): boolean {
  return item.status === "needsChoice"
    || isRemovableNoRelease(item)
    || (item.status === "failed" && isUnknownInstallPathKind(item.installPathKind));
}

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
  return isRemovableTrackedItem(app);
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
  switch (action) {
    case "install":
      return ui.task.install;
    case "rollback":
      return ui.task.rollback;
    case "uninstall":
      return ui.task.uninstall;
  }
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
    case "verifyingArtifact":
      return ui.stage.verifyingArtifact;
    case "extractingArchive":
      return ui.stage.extractingArchive;
    case "creatingRollback":
      return ui.stage.creatingRollback;
    case "restoringRollback":
      return ui.stage.restoringRollback;
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

export function releaseDirectionLabel(value: ReleaseDirection, language: Language): string {
  const ui = createUiText(language);
  return ui.releaseDirection[value];
}

export function integrityStatusLabel(value: IntegrityStatus, language: Language): string {
  const ui = createUiText(language);
  return ui.integrityStatus[value];
}

export function installPreviewIntegrityLabel(integrity: IntegrityPlan, language: Language): string {
  const ui = createUiText(language);
  return integrity.expectedSha256
    ? ui.pendingSha256Verification
    : ui.integrityStatus.recordedOnly;
}

export function resolveLifecycleSelection(
  item: ManagedApp | null,
  availableVersions: string[]
): { selectedVersion: string; channel: ReleaseChannel } {
  const target = item?.releasePolicy?.pinnedVersion ?? item?.latestVersion ?? "";
  const selectedVersion = target && (availableVersions.length === 0 || availableVersions.includes(target))
    ? target
    : availableVersions[0] ?? "";
  return {
    selectedVersion,
    channel: item?.releasePolicy?.channel ?? "stable"
  };
}

export function releaseChannelForVersion(version: { prerelease: boolean } | null | undefined): ReleaseChannel {
  return version?.prerelease ? "prerelease" : "stable";
}

export function isPreviewResponseCurrent(
  requestId: number,
  currentRequestId: number,
  requestedRepoId: string,
  selectedRepoId: string | null,
  responseRepoId: string
): boolean {
  return isPreviewRequestCurrent(requestId, currentRequestId, requestedRepoId, selectedRepoId)
    && responseRepoId === requestedRepoId;
}

export function isPreviewRequestCurrent(
  requestId: number,
  currentRequestId: number,
  requestedRepoId: string,
  selectedRepoId: string | null
): boolean {
  return requestId === currentRequestId && requestedRepoId === selectedRepoId;
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
      showPill: busy,
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
    showPill: true,
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

export function buildReleaseActionGuidance(
  item: ManagedApp | null,
  language: Language
): ReleaseActionGuidance | null {
  if (!item || item.status !== "needsChoice" || hasInstallableAsset(item)) {
    return null;
  }

  const ui = createUiText(language);
  const releaseText = `${item.releaseTitle ?? ""}\n${item.releaseNote ?? ""}`.toLowerCase();
  const kind = detectReleaseActionGuidanceKind(releaseText);

  if (kind === "docker") {
    return {
      kind,
      title: ui.releaseGuidanceDockerTitle,
      summary: ui.releaseGuidanceDockerSummary,
      bullets: [ui.releaseGuidanceScopeNote, ui.releaseGuidanceOpenRelease]
    };
  }

  if (kind === "source") {
    return {
      kind,
      title: ui.releaseGuidanceSourceTitle,
      summary: ui.releaseGuidanceSourceSummary,
      bullets: [ui.releaseGuidanceScopeNote, ui.releaseGuidanceOpenRelease]
    };
  }

  return {
    kind,
    title: ui.releaseGuidanceManualTitle,
    summary: ui.releaseGuidanceManualSummary,
    bullets: [ui.releaseGuidanceScopeNote, ui.releaseGuidanceManualFallback]
  };
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

  if (!isManagedPathKind(item.installPathKind)) {
    return { enabled: false, reason: ui.model.noLaunchTarget };
  }

  if (!item.launchPath) {
    return { enabled: false, reason: ui.model.noLaunchTarget };
  }

  return { enabled: true };
}

export function resolvePrimaryActionKind(item: ManagedApp | null): PrimaryActionKind | null {
  if (!item) {
    return null;
  }

  switch (item.status) {
    case "updateAvailable":
    case "downgradeAvailable":
      return "update";
    case "needsChoice":
      return hasInstallableAsset(item) ? "install" : "openRelease";
    case "noRelease":
    case "current":
      if (isManagedPathKind(item.installPathKind) && item.launchPath) {
        return "openApp";
      }

      if (shouldShowInstallLocationAction(item)) {
        return isSystemInstallerKind(item.installPathKind)
          ? "openInstallerFile"
          : "openInstallLocation";
      }

      return "openRelease";
    case "failed":
      return "retry";
  }

  return null;
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

  return (item.status === "updateAvailable" || item.status === "downgradeAvailable")
    && isManagedPathKind(item.installPathKind)
    && Boolean(item.launchPath);
}

export function shouldShowLifecyclePreviewAction(item: ManagedApp | null): boolean {
  if (!item) {
    return false;
  }

  // 失败态已经有更直接的重试主动作，避免在同一面板里再放一个同级别的预览入口。
  return item.status !== "failed";
}

export function buildInspectorStatusSummary(
  item: ManagedApp | null,
  selectedVersion: string,
  installRetrying: boolean,
  language: Language
): InspectorStatusSummary | null {
  if (!item) {
    return null;
  }

  const ui = createUiText(language);
  if (installRetrying) {
    return {
      label: ui.status.failed,
      detail: ui.installRetryHint,
      tone: "danger"
    };
  }

  switch (item.status) {
    case "failed":
      return {
        label: ui.status.failed,
        detail: item.latestVersion,
        tone: "danger"
      };
    case "needsChoice":
      if (hasInstallableAsset(item)) {
        return {
          label: ui.needsChoice,
          detail: `${ui.releaseTarget}: ${selectedVersion || item.latestVersion}`,
          tone: "warning"
        };
      }

      return {
        label: ui.model.noInstallableAsset,
        detail: item.releaseTitle ?? item.latestVersion,
        tone: "neutral"
      };
    case "updateAvailable":
      return {
        label: ui.status.updateAvailable,
        detail: `${item.currentVersion} → ${item.latestVersion}`,
        tone: "warning"
      };
    case "downgradeAvailable":
      return {
        label: ui.status.downgradeAvailable,
        detail: `${item.currentVersion} → ${item.latestVersion}`,
        tone: "warning"
      };
    case "current":
      return {
        label: ui.status.current,
        detail: item.currentVersion,
        tone: "success"
      };
    case "noRelease":
      return {
        label: ui.status.noRelease,
        detail: item.releaseTitle ?? ui.model.noRelease,
        tone: "neutral"
      };
  }
}

export function getDetailPathLabel(item: ManagedApp | null, language: Language): string {
  const ui = createUiText(language);

  if (!item) {
    return ui.installPath;
  }

  if (item.status === "needsChoice" && !hasInstallableAsset(item)) {
    return ui.defaultInstallPath;
  }

  return isSystemInstallerKind(item.installPathKind) ? ui.installerFile : ui.installPath;
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

  return items;
}

export function getLifecycleHistoryEntries(
  item: ManagedApp | null,
  language: Language
): LifecycleHistoryEntry[] {
  if (!item?.recentActivities?.length) {
    return [];
  }

  return item.recentActivities.map((activity) => ({
    summary: activity.summary,
    recordedAt: formatRecordedAt(activity.recordedAt, language),
    failed: activity.outcome === "failed",
    error: activity.error
  }));
}

export function hasSecondaryInspectorActions(item: InboxItem | null, language: Language): boolean {
  if (!item) {
    return false;
  }

  return shouldShowOpenAppSecondary(item)
    || shouldShowOpenReleaseSecondary(item, language)
    || shouldShowInstallerFolderSecondary(item)
    || shouldShowInstallLocationSecondary(item);
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

  const actionKind = resolvePrimaryActionKind(item);
  if (actionKind === "openApp") {
    return getOpenAppAvailability(item, busy, language);
  }

  if (actionKind === "openRelease") {
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

export function getRollbackAvailability(
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
  if (!isManagedPathKind(item.installPathKind)) {
    return { enabled: false, reason: ui.rollbackManagedOnly };
  }
  if (!item.rollback) {
    return { enabled: false, reason: ui.noRollbackSnapshot };
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

  if (!isRemovableTrackedItem(item)) {
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
  const candidates = selectedApps.filter((app) => isRemovableTrackedItem(app));
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

export function getSelectionActionAvailability(
  apps: ManagedApp[],
  selectedIds: string[],
  busy: boolean,
  language: Language
): SelectionActionAvailability {
  const ui = createUiText(language);
  const bulkRemove = getBulkRemoveAvailability(apps, selectedIds, busy, language);
  if (busy) {
    return {
      ...bulkRemove,
      kind: "remove",
      label: ui.remove
    };
  }

  const selectedSet = new Set(selectedIds);
  const selectedApps = apps.filter((app) => selectedSet.has(app.id));
  const uninstallableApps = selectedApps.filter(
    (app) => !isRemovableTrackedItem(app) && app.status !== "needsChoice" && app.uninstallSupported !== false
  );

  // 单个已安装软件时，列表危险动作进入现有卸载确认流；移除跟踪只保留给未安装项。
  if (uninstallableApps.length === 1 && selectedApps.length === 1) {
    return {
      enabled: true,
      kind: "uninstall",
      label: ui.uninstallAbility,
      uninstallTargetId: uninstallableApps[0].id,
      candidateCount: 0,
      skippedCount: 0
    };
  }

  if (uninstallableApps.length > 0) {
    return {
      ...bulkRemove,
      enabled: false,
      kind: "mixed",
      label: ui.remove,
      reason: ui.model.selectInstalledSeparately
    };
  }

  return {
    ...bulkRemove,
    kind: "remove",
    label: ui.remove
  };
}

export function getSelectionSummary(
  apps: ManagedApp[],
  selectedIds: string[],
  availability: SelectionActionAvailability,
  language: Language
): string {
  const ui = createUiText(language);
  const selectedSet = new Set(selectedIds);
  const selectedCount = apps.filter((app) => selectedSet.has(app.id)).length;
  if (selectedCount === 0) {
    return ui.selectionNone;
  }
  if (availability.kind === "mixed") {
    return ui.mixedSelection;
  }
  return ui.selectionCount(selectedCount);
}

export function filterManagedApps(apps: ManagedApp[], filter: InboxFilter, query: string): ManagedApp[] {
  const needle = query.trim().toLowerCase();
  return apps.filter((app) => {
    // actionRequired 是聚合筛选，不能直接用 status 比较
    if (filter === "actionRequired") {
      if (!isActionRequired(app)) {
        return false;
      }
    } else if (
      filter !== "all"
      && !(filter === "updateAvailable" && app.status === "downgradeAvailable")
      && app.status !== filter
    ) {
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
  if (app.status === "downgradeAvailable") {
    return ui.downgradeAvailable;
  }
  switch (resolvePrimaryActionKind(app)) {
    case "install":
      return ui.action.install;
    case "update":
      return ui.action.update;
    case "openApp":
      return ui.action.openApp;
    case "openRelease":
      return ui.openRelease;
    case "openInstallLocation":
      return ui.openInstallLocation;
    case "openInstallerFile":
      return ui.openInstallerFile;
    case "retry":
      return ui.action.retry;
  }

  return ui.action.update;
}

function priorityForStatus(status: AppStatus): number {
  switch (status) {
    case "failed":
      return 0;
    case "needsChoice":
      return 1;
    case "updateAvailable":
    case "downgradeAvailable":
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

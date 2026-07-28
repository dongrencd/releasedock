import { useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  CheckCircle2,
  Clipboard,
  CircleAlert,
  Download,
  ExternalLink,
  FolderOpen,
  Layers3,
  Pin,
  Play,
  Plus,
  Eye,
  EyeOff,
  RefreshCw,
  RotateCcw,
  Search,
  Settings2,
  ShieldAlert,
  Trash2
} from "lucide-react";
import {
  addRepo,
  type DashboardItemEvent,
  type DashboardProgressEvent,
  bulkRemoveTrackedRepos,
  installRepo,
  listReleaseVersions,
  loadConfig,
  loadDashboard,
  openApp,
  openInstallerFolder,
  openInstallLocation,
  openUrl,
  openPath,
  previewInstall,
  previewRollback,
  removeTrackedRepo,
  saveConfig,
  setReleaseIgnored,
  setReleasePin,
  testGithubConnectivity,
  uninstallRepo,
  rollbackRepo,
  openSystemUninstallSettings
} from "./backend";
import {
  buildConfigConnectivityWarning,
  buildConnectivityTestStatus,
  buildConnectivityTestViewState,
  buildNetworkConfigHealth,
  getNetworkConfigKey,
  shouldRunAutoConnectivityCheck,
  buildStatusDockPresentation,
  buildUpdateInbox,
  getConfirmInstallAvailability,
  getPrimaryActionAvailability,
  getRollbackAvailability,
  getInspectorDetailItems,
  getLifecycleHistoryEntries,
  getSelectionActionAvailability,
  getSelectionSummary,
  buildReleaseActionGuidance,
  buildInspectorStatusSummary,
  hasInstallableAsset,
  isManagedPathKind,
  isSystemInstallerKind,
  isRemovableNoRelease,
  isRemovableTrackedItem,
  isFailedInstallProgress,
  parseReleaseNote,
  pruneSelection,
  getUninstallAvailability,
  filterManagedApps,
  installManagementKindLabel,
  installPreviewIntegrityLabel,
  inboxFilters,
  isPreviewRequestCurrent,
  isPreviewResponseCurrent,
  isActionRequired,
  resolvePrimaryActionKind,
  releaseChannelForVersion,
  resolveLifecycleSelection,
  selectVisibleIds,
  systemPackageManagerLabel,
  shouldShowLifecyclePreviewAction,
  shouldShowInstallLocationAction,
  toggleSelection,
  type ConnectivityTestViewState,
  type InboxFilter,
  type InboxItem,
  type ManagedApp
} from "./appModel";
import {
  createTaskStatusText,
  createUiText,
  formatPublishedAt,
  isWindowsPlatform,
  languageOptions,
  normalizeLanguage,
  type Language
} from "./i18n";
import type {
  BackgroundCheckEvent,
  DesktopConfig,
  InstallPlan,
  ReleaseVersion,
  RollbackPreview,
  TaskProgressEvent
} from "./backend";

type ConfigDraft = {
  githubToken: string;
  proxyUrl: string;
  installRoot: string;
  effectiveInstallRoot: string;
  language: Language;
  backgroundCheckEnabled: boolean;
  checkIntervalMinutes: number;
};

type TaskProgressView = Omit<TaskProgressEvent, "stage"> & {
  stage: TaskProgressEvent["stage"] | "failed";
};

type TaskProgressContext = {
  repoId: string;
  action: TaskProgressView["action"];
} | null;

export function App() {
  const [apps, setApps] = useState<ManagedApp[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [activeView, setActiveView] = useState<"dashboard" | "settings">("dashboard");
  const [filter, setFilter] = useState<InboxFilter>("all");
  const [searchQuery, setSearchQuery] = useState("");
  const [repoInput, setRepoInput] = useState("");
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [pendingInstall, setPendingInstall] = useState<InstallPlan | null>(null);
  const [pendingRollback, setPendingRollback] = useState<RollbackPreview | null>(null);
  const [pendingUninstall, setPendingUninstall] = useState<InboxItem | null>(null);
  const [releaseVersions, setReleaseVersions] = useState<ReleaseVersion[]>([]);
  const [versionsLoading, setVersionsLoading] = useState(false);
  const [selectedVersion, setSelectedVersion] = useState("");
  const [taskProgress, setTaskProgress] = useState<TaskProgressView | null>(null);
  const [configDraft, setConfigDraft] = useState<ConfigDraft>({
    githubToken: "",
    proxyUrl: "",
    installRoot: "",
    effectiveInstallRoot: "",
    language: "en",
    backgroundCheckEnabled: true,
    checkIntervalMinutes: 30
  });
  const currentConfigKey = useRef(configDraftKey(configDraft));
  const currentNetworkConfigKey = useRef(getNetworkConfigKey(configDraft));
  const connectivityCheckId = useRef(0);
  const lastAutoConnectivityKey = useRef<string | null>(null);
  const lastSavedConfigKey = useRef("");
  const pendingConfigSaves = useRef(0);
  const [showGithubToken, setShowGithubToken] = useState(false);
  const [configLoaded, setConfigLoaded] = useState(false);
  const [configSaving, setConfigSaving] = useState(false);
  const [connectivityTesting, setConnectivityTesting] = useState(false);
  const [connectivityTest, setConnectivityTest] = useState<ConnectivityTestViewState>({ status: "idle" });
  const [networkFieldsAttention, setNetworkFieldsAttention] = useState(false);
  const [taskStatus, setTaskStatus] = useState(createTaskStatusText("en").loadingDashboard);
  const [error, setError] = useState<string | null>(null);
  // 后台检查发现的更新数（来自托盘后台检查）
  const [backgroundUpdateCount, setBackgroundUpdateCount] = useState(0);
  const activeTaskProgress = useRef<TaskProgressContext>(null);
  const dashboardRefreshId = useRef(0);
  const dashboardOrder = useRef<Map<string, number>>(new Map());
  const previewRequestId = useRef(0);
  const selectedRepoIdRef = useRef<string | null>(selectedId);
  const githubTokenInput = useRef<HTMLInputElement>(null);
  const proxyUrlInput = useRef<HTMLInputElement>(null);

  const language = normalizeLanguage(configDraft.language);
  const languageRef = useRef(language);
  const ui = createUiText(language);
  const taskText = createTaskStatusText(language);
  const visibleApps = filterManagedApps(apps, filter, searchQuery);
  const inbox = buildUpdateInbox(visibleApps, language);
  const selected = inbox.find((item) => item.id === selectedId) ?? inbox[0] ?? null;
  const isEmptyDashboard = apps.length === 0;
  const pendingInstallRepoId = pendingInstall?.repo_id ?? null;
  const installRetrying = isFailedInstallProgress(taskProgress, pendingInstallRepoId);
  const hasGithubToken = configDraft.githubToken.trim().length > 0;
  const configConnectivityWarning = buildConfigConnectivityWarning(configDraft, language, connectivityTest);
  const networkConfigHealth = buildNetworkConfigHealth(configDraft, language, connectivityTest);
  const connectivityTestStatus = buildConnectivityTestStatus(connectivityTest, language, configDraft);
  const installRoot = configDraft.installRoot.trim();
  const effectiveInstallRoot = configDraft.effectiveInstallRoot.trim();
  const displayInstallRoot = installRoot || effectiveInstallRoot;
  const usingDefaultInstallRoot = installRoot.length === 0 && effectiveInstallRoot.length > 0;
  const selectionActionAvailability = getSelectionActionAvailability(apps, selectedIds, busy, language);
  const selectionSummary = getSelectionSummary(apps, selectedIds, selectionActionAvailability, language);

  useEffect(() => {
    languageRef.current = language;
  }, [language]);

  useEffect(() => {
    if (!networkFieldsAttention) {
      return;
    }

    const timer = window.setTimeout(() => {
      setNetworkFieldsAttention(false);
    }, 3200);

    return () => {
      window.clearTimeout(timer);
    };
  }, [networkFieldsAttention]);

  function sortAppsByDashboardOrder(nextApps: ManagedApp[]) {
    const order = dashboardOrder.current;
    return [...nextApps].sort((left, right) => {
      const leftIndex = order.get(left.id) ?? Number.MAX_SAFE_INTEGER;
      const rightIndex = order.get(right.id) ?? Number.MAX_SAFE_INTEGER;
      return leftIndex - rightIndex || left.name.localeCompare(right.name);
    });
  }

  function selectRepo(repoId: string) {
    if (selectedRepoIdRef.current !== repoId) {
      selectedRepoIdRef.current = repoId;
      previewRequestId.current += 1;
    }
    setSelectedId(repoId);
  }

  function beginPreviewRequest(repoId: string) {
    selectRepo(repoId);
    previewRequestId.current += 1;
    return previewRequestId.current;
  }

  function applyLifecycleDashboard(nextApps: ManagedApp[], repoId: string) {
    setApps(nextApps);
    const updated = nextApps.find((app) => app.id === repoId) ?? null;
    const nextSelection = resolveLifecycleSelection(
      updated,
      releaseVersions.map((version) => version.tagName)
    );
    setSelectedVersion(nextSelection.selectedVersion);
  }

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void refreshWorkspace();
    }, 0);

    return () => {
      window.clearTimeout(timer);
    };
  }, []);

  useEffect(() => {
    const selectedRepoId = selected?.id ?? null;
    if (selectedRepoIdRef.current !== selectedRepoId) {
      selectedRepoIdRef.current = selectedRepoId;
      previewRequestId.current += 1;
    }
    setPendingInstall(null);
    setPendingRollback(null);
    setPendingUninstall(null);
    setReleaseVersions([]);
    const initialSelection = resolveLifecycleSelection(selected, []);
    setSelectedVersion(initialSelection.selectedVersion);
    if (!selected?.id) {
      return;
    }

    let cancelled = false;
    setVersionsLoading(true);
    void listReleaseVersions(selected.id)
      .then((versions) => {
        if (cancelled) {
          return;
        }
        setReleaseVersions(versions);
        const nextSelection = resolveLifecycleSelection(
          selected,
          versions.map((version) => version.tagName)
        );
        setSelectedVersion(nextSelection.selectedVersion);
      })
      .catch(() => {
        if (!cancelled) {
          setReleaseVersions([]);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setVersionsLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [selected?.id]);

  useEffect(() => {
    const nextSelection = resolveLifecycleSelection(
      selected,
      releaseVersions.map((version) => version.tagName)
    );
    setSelectedVersion(nextSelection.selectedVersion);
  }, [
    selected?.latestVersion,
    selected?.releasePolicy?.channel,
    selected?.releasePolicy?.pinnedVersion,
    releaseVersions
  ]);

  useEffect(() => {
    if (loading) {
      return;
    }

    setSelectedIds((current) => pruneSelection(current, apps));
  }, [apps, loading]);

  useEffect(() => {
    currentConfigKey.current = configDraftKey(configDraft);
    currentNetworkConfigKey.current = getNetworkConfigKey(configDraft);
  }, [configDraft]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<TaskProgressEvent>("task-progress", (event) => {
      const currentTask = activeTaskProgress.current;
      if (!currentTask) {
        return;
      }
      if (currentTask.repoId !== event.payload.repoId || currentTask.action !== event.payload.action) {
        return;
      }
      setTaskProgress(event.payload);
    }).then((dispose) => {
      unlisten = dispose;
    });

    return () => {
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let unlistenItem: (() => void) | undefined;
    let unlistenProgress: (() => void) | undefined;
    let unlistenBackground: (() => void) | undefined;
    let unlistenTrayCheck: (() => void) | undefined;

    // 后台检查完成事件 — 更新 badge 计数
    void listen<BackgroundCheckEvent>("background-check-complete", (event) => {
      setBackgroundUpdateCount(event.payload.updateCount);
    }).then((dispose) => {
      unlistenBackground = dispose;
    });

    // 托盘"检查更新"菜单 — 触发前端刷新
    void listen<void>("tray-check-updates", () => {
      void refreshDashboard();
    }).then((dispose) => {
      unlistenTrayCheck = dispose;
    });

    void listen<DashboardItemEvent>("dashboard-item-updated", (event) => {
      if (event.payload.refreshId !== dashboardRefreshId.current) {
        return;
      }

      dashboardOrder.current.set(event.payload.item.id, event.payload.index);
      setApps((current) => {
        const next = current.some((app) => app.id === event.payload.item.id)
          ? current.map((app) => (app.id === event.payload.item.id ? event.payload.item : app))
          : [...current, event.payload.item];
        return sortAppsByDashboardOrder(next);
      });
      setSelectedId((current) => current ?? event.payload.item.id);
    }).then((dispose) => {
      unlistenItem = dispose;
    });

    void listen<DashboardProgressEvent>("dashboard-progress", (event) => {
      if (event.payload.refreshId !== dashboardRefreshId.current || activeTaskProgress.current) {
        return;
      }

      const statusText = createTaskStatusText(languageRef.current);
      setTaskStatus(statusText.checkingLatestReleaseProgress(event.payload.completed, event.payload.total));
    }).then((dispose) => {
      unlistenProgress = dispose;
    });

    return () => {
      unlistenItem?.();
      unlistenProgress?.();
      unlistenBackground?.();
      unlistenTrayCheck?.();
    };
  }, []);

  useEffect(() => {
    if (!taskProgress || taskProgress.stage !== "finished") {
      return;
    }

    const timer = window.setTimeout(() => {
      setTaskProgress((current) => {
        if (current?.repoId === taskProgress.repoId && current.stage === "finished") {
          activeTaskProgress.current = null;
          return null;
        }
        return current;
      });
    }, 1400);

    return () => {
      window.clearTimeout(timer);
    };
  }, [taskProgress]);

  useEffect(() => {
    if (!taskProgress || taskProgress.stage !== "failed") {
      return;
    }

    const timer = window.setTimeout(() => {
      setTaskProgress((current) => {
        if (current?.repoId === taskProgress.repoId && current.stage === "failed") {
          activeTaskProgress.current = null;
          return null;
        }
        return current;
      });
    }, 4000);

    return () => {
      window.clearTimeout(timer);
    };
  }, [taskProgress]);

  useEffect(() => {
    if (!configLoaded) {
      return;
    }

    const currentKey = configDraftKey(configDraft);
    if (currentKey === lastSavedConfigKey.current) {
      return;
    }

    // Debounce disk writes so typing a token or proxy URL saves once after input settles.
    const timer = window.setTimeout(() => {
      void handleSaveConfig(configDraft, "auto");
    }, 650);

    return () => {
      window.clearTimeout(timer);
    };
  }, [configDraft, configLoaded]);

  function clearTaskProgress() {
    activeTaskProgress.current = null;
    setTaskProgress(null);
  }

  async function refreshDashboard(statusLanguage: Language = languageRef.current) {
    const statusText = createTaskStatusText(statusLanguage);
    clearTaskProgress();
    const refreshId = dashboardRefreshId.current + 1;
    dashboardRefreshId.current = refreshId;
    dashboardOrder.current = new Map(apps.map((app, index) => [app.id, index]));
    setLoading(true);
    setError(null);
    setPendingInstall(null);
    setTaskStatus(statusText.checkingLatestRelease);
    try {
      const data = await loadDashboard(refreshId);
      if (dashboardRefreshId.current !== refreshId) {
        return;
      }
      dashboardOrder.current = new Map(data.map((app, index) => [app.id, index]));
      setApps(data);
      setSelectedId((current) => (current && data.some((item) => item.id === current) ? current : data[0]?.id ?? null));
      setTaskStatus(data.length > 0 ? statusText.loadedApps(data.length) : statusText.noApps);
    } catch (caught) {
      if (dashboardRefreshId.current !== refreshId) {
        return;
      }
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(statusText.refreshFailed);
    } finally {
      if (dashboardRefreshId.current === refreshId) {
        setLoading(false);
      }
    }
  }

  async function refreshConfig() {
    clearTaskProgress();
    setConfigLoaded(false);
    try {
      const data = await loadConfig();
      const draft = {
        githubToken: data.githubToken ?? "",
        proxyUrl: data.proxyUrl ?? "",
        installRoot: data.installRoot ?? "",
        effectiveInstallRoot: data.effectiveInstallRoot ?? data.installRoot ?? "",
        language: normalizeLanguage(data.language),
        backgroundCheckEnabled: data.backgroundCheckEnabled ?? true,
        checkIntervalMinutes: data.checkIntervalMinutes ?? 30
      };
      lastSavedConfigKey.current = configDraftKey(draft);
      currentConfigKey.current = configDraftKey(draft);
      currentNetworkConfigKey.current = getNetworkConfigKey(draft);
      setConfigDraft(draft);
      setConfigLoaded(true);
      languageRef.current = draft.language;
      return draft;
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.failedToLoadSettings);
      return null;
    }
  }

  async function refreshWorkspace() {
    const draft = await refreshConfig();
    if (draft && shouldRunAutoConnectivityCheck(draft, lastAutoConnectivityKey.current)) {
      lastAutoConnectivityKey.current = getNetworkConfigKey(draft);
      void runGithubConnectivityCheck(draft, "auto");
    }
    await refreshDashboard(draft?.language ?? languageRef.current);
  }

  async function handleAddRepo() {
    const trimmed = repoInput.trim();
    if (!trimmed) {
      clearTaskProgress();
      setError(taskText.enterRepo);
      setTaskStatus(taskText.addRepoFailed);
      return;
    }

    clearTaskProgress();
    setBusy(true);
    setError(null);
    setPendingInstall(null);
    setTaskStatus(taskText.addingRepo(trimmed));
    try {
      const data = await addRepo(trimmed);
      setApps(data);
      setSelectedId(normalizeRepoId(trimmed));
      setRepoInput("");
      setTaskStatus(taskText.addedRepo(trimmed));
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.addRepoFailed);
    } finally {
      setBusy(false);
    }
  }

  async function handleSaveConfig(draft: ConfigDraft = configDraft, mode: "auto" | "manual" = "manual") {
    const draftKey = configDraftKey(draft);
    if (draftKey === lastSavedConfigKey.current) {
      return;
    }

    clearTaskProgress();
    pendingConfigSaves.current += 1;
    setConfigSaving(true);
    setError(null);
    setTaskStatus(mode === "auto" ? taskText.autoSavingSettings : taskText.savingSettings);
    try {
      const saved = await saveConfig(desktopConfigFromDraft(draft));
      const savedDraft = {
        githubToken: saved.githubToken ?? "",
        proxyUrl: saved.proxyUrl ?? "",
        installRoot: saved.installRoot ?? "",
        effectiveInstallRoot: saved.effectiveInstallRoot ?? saved.installRoot ?? "",
        language: normalizeLanguage(saved.language),
        backgroundCheckEnabled: saved.backgroundCheckEnabled ?? true,
        checkIntervalMinutes: saved.checkIntervalMinutes ?? 30
      };
      lastSavedConfigKey.current = configDraftKey(savedDraft);
      if (currentConfigKey.current === draftKey) {
        currentNetworkConfigKey.current = getNetworkConfigKey(savedDraft);
        setConfigDraft(savedDraft);
        setTaskStatus(taskText.settingsSaved);
      }
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.failedToSaveSettings);
    } finally {
      pendingConfigSaves.current = Math.max(0, pendingConfigSaves.current - 1);
      if (pendingConfigSaves.current === 0) {
        setConfigSaving(false);
      }
    }
  }

  async function handlePrimaryAction(item: InboxItem) {
    switch (resolvePrimaryActionKind(item)) {
      case "openApp":
        await handleOpenApp(item);
        return;
      case "openRelease":
        await handleOpenRelease(item);
        return;
      case "openInstallLocation":
      case "openInstallerFile":
        await handleOpenInstallPath(item);
        return;
      case "retry":
        await refreshDashboard();
        return;
      case "install":
      case "update":
        const requestId = beginPreviewRequest(item.id);
        clearTaskProgress();
        setBusy(true);
        setError(null);
        setTaskStatus(taskText.generatingInstallPreview(item.name));
        try {
          const version = item.id === selected?.id ? selectedVersion || item.latestVersion : item.latestVersion;
          const releaseVersion = item.id === selected?.id
            ? releaseVersions.find((candidate) => candidate.tagName === version)
            : null;
          const channel = item.id === selected?.id
            ? releaseChannelForVersion(releaseVersion)
            : item.releasePolicy?.channel ?? "stable";
          const plan = await previewInstall(item.id, version, channel);
          if (!isPreviewResponseCurrent(
            requestId,
            previewRequestId.current,
            item.id,
            selectedRepoIdRef.current,
            plan.repo_id
          )) {
            return;
          }
          setPendingInstall(plan);
          setPendingRollback(null);
          setTaskStatus(taskText.generatedInstallPreview(item.name));
        } catch (caught) {
          if (!isPreviewRequestCurrent(
            requestId,
            previewRequestId.current,
            item.id,
            selectedRepoIdRef.current
          )) {
            return;
          }
          const message = caught instanceof Error ? caught.message : String(caught);
          setError(message);
          setTaskStatus(taskText.failedToBuildInstallPreview);
        } finally {
          setBusy(false);
        }
        return;
      default:
        return;
    }
  }

  async function handleConfirmInstall(item: InboxItem) {
    if (!pendingInstall || pendingInstall.repo_id !== item.id) {
      setPendingInstall(null);
      setError(taskText.failedToBuildInstallPreview);
      return;
    }
    activeTaskProgress.current = { repoId: item.id, action: "install" };
    setBusy(true);
    setError(null);
    setTaskStatus(taskText.installing(item.name));
    setTaskProgress({
      repoId: item.id,
      action: "install",
      stage: "preparing",
      message: taskText.preparingInstall(item.name),
      percent: 0
    });
    try {
      const data = await installRepo(pendingInstall);
      setApps(data);
      setSelectedId(item.id);
      setPendingInstall(null);
      setTaskProgress({
        repoId: item.id,
        action: "install",
        stage: "finished",
        message: taskText.finishedInstalling(item.name),
        percent: 100
      });
      setTaskStatus(taskText.installedOrUpdated(item.name));
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.installFailed);
      setTaskProgress((current) =>
        current && current.repoId === item.id
          ? {
              ...current,
              stage: "failed",
              message
            }
          : current
      );
    } finally {
      setBusy(false);
    }
  }

  async function handlePreviewSelectedVersion(item: InboxItem | null) {
    if (!item || !selectedVersion) {
      return;
    }
    const requestId = beginPreviewRequest(item.id);
    clearTaskProgress();
    setBusy(true);
    setError(null);
    setTaskStatus(taskText.generatingInstallPreview(item.name));
    try {
      const selectedRelease = releaseVersions.find((version) => version.tagName === selectedVersion);
      const plan = await previewInstall(item.id, selectedVersion, releaseChannelForVersion(selectedRelease));
      if (!isPreviewResponseCurrent(
        requestId,
        previewRequestId.current,
        item.id,
        selectedRepoIdRef.current,
        plan.repo_id
      )) {
        return;
      }
      setPendingInstall(plan);
      setPendingRollback(null);
      setTaskStatus(taskText.generatedInstallPreview(item.name));
    } catch (caught) {
      if (!isPreviewRequestCurrent(
        requestId,
        previewRequestId.current,
        item.id,
        selectedRepoIdRef.current
      )) {
        return;
      }
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.failedToBuildInstallPreview);
    } finally {
      setBusy(false);
    }
  }

  function isInstalledLifecycleItem(item: InboxItem): boolean {
    return isManagedPathKind(item.installPathKind) || isSystemInstallerKind(item.installPathKind);
  }

  async function handleSetPinned(item: InboxItem | null, pinned: boolean) {
    if (!item || !isInstalledLifecycleItem(item) || (pinned && !selectedVersion)) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const data = await setReleasePin(item.id, pinned ? selectedVersion : null);
      applyLifecycleDashboard(data, item.id);
      setPendingInstall(null);
      setTaskStatus(taskText.releasePolicyUpdated);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.releasePolicyFailed);
    } finally {
      setBusy(false);
    }
  }

  async function handleToggleIgnored(item: InboxItem | null) {
    if (!item || !isInstalledLifecycleItem(item) || !selectedVersion) {
      return;
    }
    const ignored = !(item.releasePolicy?.ignoredVersions ?? []).includes(selectedVersion);
    setBusy(true);
    setError(null);
    try {
      const data = await setReleaseIgnored(item.id, selectedVersion, ignored);
      applyLifecycleDashboard(data, item.id);
      setPendingInstall(null);
      setTaskStatus(taskText.releasePolicyUpdated);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.releasePolicyFailed);
    } finally {
      setBusy(false);
    }
  }

  async function handlePreviewRollback(item: InboxItem | null) {
    if (!item || !getRollbackAvailability(item, busy, language).enabled) {
      return;
    }
    const requestId = beginPreviewRequest(item.id);
    clearTaskProgress();
    setBusy(true);
    setError(null);
    setTaskStatus(taskText.preparingRollback(item.name));
    try {
      const preview = await previewRollback(item.id);
      if (!isPreviewResponseCurrent(
        requestId,
        previewRequestId.current,
        item.id,
        selectedRepoIdRef.current,
        preview.repoId
      )) {
        return;
      }
      setPendingRollback(preview);
      setPendingInstall(null);
      setTaskStatus(taskText.preparingRollback(item.name));
    } catch (caught) {
      if (!isPreviewRequestCurrent(
        requestId,
        previewRequestId.current,
        item.id,
        selectedRepoIdRef.current
      )) {
        return;
      }
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.rollbackFailed);
    } finally {
      setBusy(false);
    }
  }

  async function handleConfirmRollback(item: InboxItem | null) {
    if (!item || !pendingRollback) {
      return;
    }
    if (pendingRollback.repoId !== item.id) {
      setPendingRollback(null);
      setError(taskText.rollbackFailed);
      return;
    }
    activeTaskProgress.current = { repoId: item.id, action: "rollback" };
    setBusy(true);
    setError(null);
    setTaskStatus(taskText.rollingBack(item.name));
    setTaskProgress({
      repoId: item.id,
      action: "rollback",
      stage: "locatingRecord",
      message: taskText.rollingBack(item.name),
      percent: 0
    });
    try {
      const data = await rollbackRepo(pendingRollback);
      setApps(data);
      setPendingRollback(null);
      setTaskProgress({
        repoId: item.id,
        action: "rollback",
        stage: "finished",
        message: taskText.rolledBack(item.name),
        percent: 100
      });
      setTaskStatus(taskText.rolledBack(item.name));
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.rollbackFailed);
      setTaskProgress((current) => current ? { ...current, stage: "failed", message } : current);
    } finally {
      setBusy(false);
    }
  }

  function requestUninstall(item: InboxItem | null) {
    if (!item || item.status === "needsChoice" || item.uninstallSupported === false) {
      return;
    }

    setPendingInstall(null);
    setPendingRollback(null);
    setPendingUninstall(item);
  }

  async function handleConfirmUninstall(item: InboxItem | null) {
    if (!item || item.status === "needsChoice" || item.uninstallSupported === false) {
      return;
    }

    activeTaskProgress.current = { repoId: item.id, action: "uninstall" };
    setBusy(true);
    setError(null);
    setTaskProgress({
      repoId: item.id,
      action: "uninstall",
      stage: "locatingRecord",
      message: taskText.uninstalling(item.name),
      percent: 0
    });
    try {
      const data = await uninstallRepo(item.id);
      setApps(data);
      setSelectedId(data.find((app) => app.id === item.id)?.id ?? data[0]?.id ?? null);
      setPendingInstall(null);
      setPendingRollback(null);
      setPendingUninstall(null);
      setTaskProgress({
        repoId: item.id,
        action: "uninstall",
        stage: "finished",
        message: taskText.finishedUninstalling(item.name),
        percent: 100
      });
      setTaskStatus(taskText.uninstalled(item.name));
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.uninstallFailed);
      setTaskProgress((current) =>
        current && current.repoId === item.id
          ? {
              ...current,
              stage: "failed",
              message
            }
          : current
      );
    } finally {
      setBusy(false);
    }
  }

  async function handleRemoveTracked(item: InboxItem | null) {
    if (!item || !isRemovableTrackedItem(item)) {
      clearTaskProgress();
      return;
    }

    clearTaskProgress();
    setBusy(true);
    setError(null);
    try {
      const data = await removeTrackedRepo(item.id);
      setApps(data);
      setSelectedId(data.find((app) => app.id === item.id)?.id ?? data[0]?.id ?? null);
      setPendingInstall(null);
      setTaskStatus(taskText.stoppedTracking(item.name));
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.removeTrackingFailed);
    } finally {
      setBusy(false);
    }
  }

  async function handleSelectionAction() {
    if (!selectionActionAvailability.enabled) {
      clearTaskProgress();
      setError(selectionActionAvailability.reason ?? taskText.selectAtLeastOneRemovableItem);
      setTaskStatus(taskText.bulkRemoveFailed);
      return;
    }

    if (selectionActionAvailability.kind === "uninstall") {
      requestUninstall(inbox.find((item) => item.id === selectionActionAvailability.uninstallTargetId) ?? null);
      return;
    }

    const targets = apps.filter(
      (app) => selectedIds.includes(app.id) && isRemovableTrackedItem(app)
    );
    if (targets.length === 0) {
      clearTaskProgress();
      setError(taskText.selectAtLeastOneUninstalledTrackedItem);
      setTaskStatus(taskText.bulkRemoveFailed);
      return;
    }

    clearTaskProgress();
    setBusy(true);
    setError(null);
    setPendingInstall(null);
    setTaskStatus(taskText.removingTracked(targets.length));

    try {
      const result = await bulkRemoveTrackedRepos(targets.map((target) => target.id));
      setApps(result.apps);
      setTaskStatus(taskText.removedTracked(result.removedCount, targets.length));
      setSelectedIds((current) => pruneSelection(current, result.apps));
      setSelectedId((current) => {
        if (current && result.apps.some((app) => app.id === current)) {
          return current;
        }
        return result.apps[0]?.id ?? null;
      });
      setPendingInstall(null);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.bulkRemoveFailed);
    } finally {
      setBusy(false);
    }
  }

  async function handleOpenRelease(item: InboxItem | null) {
    if (!item?.releaseUrl) {
      clearTaskProgress();
      setTaskStatus(taskText.noReleaseLinkAvailable);
      return;
    }
    clearTaskProgress();
    try {
      await openUrl(item.releaseUrl);
      setTaskStatus(taskText.openedReleasePage(item.name));
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.openFailed);
    }
  }

  async function handleOpenApp(item: InboxItem | null) {
    if (!item || !isManagedPathKind(item.installPathKind) || !item.launchPath) {
      clearTaskProgress();
      setTaskStatus(ui.model.noLaunchTarget);
      return;
    }

    clearTaskProgress();
    try {
      await openApp(item.id);
      setTaskStatus(taskText.openedApp(item.name));
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.openFailed);
    }
  }

  async function handleOpenInstallPath(item: InboxItem | null) {
    if (!item?.installPath || item.installPath === "unknown" || item.status === "needsChoice" || !shouldShowInstallLocationAction(item)) {
      clearTaskProgress();
      setTaskStatus(taskText.noInstallPathAvailable);
      return;
    }

    clearTaskProgress();
    try {
      await openInstallLocation(item.installPath, item.installPathKind);
      setTaskStatus(
        isSystemInstallerKind(item.installPathKind)
          ? taskText.openedInstallerFile(item.name)
          : taskText.openedInstallLocation(item.name)
      );
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.openFolderFailed);
    }
  }

  async function handleOpenInstallerFolder(item: InboxItem | null) {
    if (!item?.installPath || item.installPath === "unknown" || !isSystemInstallerKind(item.installPathKind)) {
      clearTaskProgress();
      setTaskStatus(taskText.noInstallPathAvailable);
      return;
    }

    clearTaskProgress();
    try {
      await openInstallerFolder(item.installPath);
      setTaskStatus(taskText.openedInstallerFolder(item.name));
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.openFolderFailed);
    }
  }

  async function handleOpenInstallRoot() {
    if (!displayInstallRoot) {
      clearTaskProgress();
      setTaskStatus(taskText.noInstallRootSelected);
      return;
    }

    clearTaskProgress();
    try {
      await openPath(displayInstallRoot);
      setTaskStatus(taskText.openedInstallRoot);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus(taskText.openFolderFailed);
    }
  }

  function handleCopyReleaseNote(note?: string) {
    if (!note || !navigator.clipboard) {
      clearTaskProgress();
      return;
    }
    clearTaskProgress();
    void navigator.clipboard.writeText(note);
    setTaskStatus(taskText.releaseNoteCopied);
  }

  function handleCopyValue(label: string, value: string) {
    if (!value || !navigator.clipboard) {
      clearTaskProgress();
      return;
    }
    clearTaskProgress();
    void navigator.clipboard.writeText(value);
    setTaskStatus(taskText.copiedValue(label));
  }

  function handleFocusNetworkConfig() {
    clearTaskProgress();
    setActiveView("settings");
    setNetworkFieldsAttention(true);
    setTaskStatus(configConnectivityWarning?.detail ?? ui.connectivityTestHelp);

    window.setTimeout(() => {
      const hasProxyUrl = configDraft.proxyUrl.trim().length > 0;
      const target = !hasProxyUrl
        ? proxyUrlInput.current
        : !hasGithubToken
          ? githubTokenInput.current
          : proxyUrlInput.current;
      target?.focus();
    }, 0);
  }

  async function runGithubConnectivityCheck(draft: ConfigDraft, mode: "auto" | "manual") {
    const testedConfigKey = getNetworkConfigKey(draft);
    const checkId = connectivityCheckId.current + 1;
    connectivityCheckId.current = checkId;

    if (mode === "manual") {
      clearTaskProgress();
      setConnectivityTesting(true);
      setError(null);
      setTaskStatus(taskText.testingGithubConnectivity);
    }
    setConnectivityTest({ status: "testing", configKey: testedConfigKey });

    try {
      const result = await testGithubConnectivity(desktopConfigFromDraft(draft));
      if (connectivityCheckId.current !== checkId) {
        return;
      }
      if (currentNetworkConfigKey.current !== testedConfigKey) {
        setConnectivityTest({ status: "stale", configKey: testedConfigKey });
        return;
      }
      setConnectivityTest(buildConnectivityTestViewState(result, draft));
      if (mode === "manual") {
        setTaskStatus(result.ok ? taskText.githubConnectivitySucceeded : taskText.githubConnectivityFailed);
      }
    } catch (caught) {
      if (connectivityCheckId.current !== checkId) {
        return;
      }
      const message = caught instanceof Error ? caught.message : String(caught);
      setConnectivityTest({ status: "failed", message, problem: "unknown", configKey: testedConfigKey });
      if (mode === "manual") {
        setError(message);
        setTaskStatus(taskText.githubConnectivityFailed);
      }
    } finally {
      if (mode === "manual") {
        setConnectivityTesting(false);
      }
    }
  }

  async function handleTestGithubConnectivity() {
    await runGithubConnectivityCheck(configDraft, "manual");
  }

  return (
    <div className="shell">
      <aside className="sidebar" aria-label={ui.navUpdates}>
        <div className="brandBlock">
          <button className="brand brandButton" type="button" aria-label={ui.appName}>
            <div className="brandMark">RD</div>
          </button>
          <div className="brandCopy">
            <strong>{ui.appName}</strong>
            <span>{ui.appSubtitle}</span>
          </div>
        </div>

        <nav className="navList">
          <NavItem
            icon={<Download size={18} />}
            label={ui.navUpdates}
            active={activeView === "dashboard"}
            onClick={() => setActiveView("dashboard")}
          />
          <NavItem
            icon={<Settings2 size={18} />}
            label={ui.navSettings}
            active={activeView === "settings"}
            onClick={() => setActiveView("settings")}
          />
        </nav>

        <div className="sidebarFooter" aria-label={`${ui.appName} ${ui.appSubtitle}`}>
          <span>{ui.appName}</span>
          <span>{ui.appSubtitle}</span>
        </div>
      </aside>

      <main className="workspace">
        <header className="topbar">
          <div className="topbarCopy">
            <p className="eyebrow">{activeView === "dashboard" ? ui.updatesEyebrow : ui.settingsEyebrow}</p>
            <h1>{activeView === "dashboard" ? ui.updatesTitle : ui.settingsTitle}</h1>
          </div>
          <div className="topbarRight">
            {activeView === "dashboard" ? (
              <>
                <div className="topbarMeta">
                  <span className={hasGithubToken ? "statePill success" : "statePill"}>{hasGithubToken ? ui.configReady : ui.configPublic}</span>
                  {configConnectivityWarning ? (
                    <button
                      className="statePill warning statePillButton"
                      type="button"
                      onClick={handleFocusNetworkConfig}
                      title={configConnectivityWarning.detail}
                      aria-label={ui.openNetworkSettings}
                    >
                      {configConnectivityWarning.label}
                    </button>
                  ) : null}
                  {backgroundUpdateCount > 0 ? (
                    <span className="statePill subtle" title={ui.trayBadge(backgroundUpdateCount)}>
                      {ui.trayBadge(backgroundUpdateCount)}
                    </span>
                  ) : null}
                </div>
                <TooltipButton label={ui.checkUpdates} onClick={() => void refreshDashboard()} disabled={busy || loading} className="ghostButton topbarButton">
                  <RefreshCw size={17} />
                  <span>{ui.checkUpdates}</span>
                </TooltipButton>
              </>
            ) : null}
          </div>
        </header>

        {error ? <div className="errorBanner">{error}</div> : null}

        {activeView === "dashboard" ? (
          <section className="dashboardView">
            <section className={isEmptyDashboard ? "contentGrid emptyDashboardGrid" : "contentGrid"}>
              <section className="inboxPanel" aria-label={ui.managedAppsTitle}>
                <div className="sectionHeader workbenchHeader">
                  <div className="sectionTitle">
                    <div className="sectionGlyph">
                      <Layers3 size={16} />
                    </div>
                    <div>
                      <p className="eyebrow">{ui.managedAppsTitle}</p>
                      <h2>{ui.managedAppsCount(inbox.length)}</h2>
                    </div>
                  </div>
                  <div className="workbenchHeaderActions">
                    <div className="sectionMeta">
                      <span className="statePill subtle">{ui.managedAppsPending(inbox.filter((item) => isActionRequired(item) || item.status === "failed").length)}</span>
                      <span className="statePill subtle">{ui.filterPrefix}{filterLabel(filter, language)}</span>
                    </div>
                    <div className="repoControl">
                      <div className="repoBox">
                        <Plus size={17} />
                        <input
                          placeholder={ui.addRepoPlaceholder}
                          aria-label={ui.addRepoEyebrow}
                          value={repoInput}
                          onChange={(event) => setRepoInput(event.target.value)}
                          onKeyDown={(event) => {
                            if (event.key === "Enter") {
                              void handleAddRepo();
                            }
                          }}
                        />
                      </div>
                      <TooltipButton label={ui.addRepoButton} onClick={() => void handleAddRepo()} disabled={busy} className="primaryButton addRepoButton">
                        <Plus size={17} />
                        <span>{ui.addRepoButton}</span>
                      </TooltipButton>
                    </div>
                  </div>
                </div>

                <div className="listTools">
                  <div className="listToolsPrimary">
                    <div className="searchBox">
                      <Search size={17} />
                      <input
                        placeholder={ui.searchPlaceholder}
                        aria-label={ui.searchPlaceholder}
                        value={searchQuery}
                        onChange={(event) => setSearchQuery(event.target.value)}
                      />
                    </div>
                  </div>
                  <div className="listToolsActions">
                    <div className="filterRow" aria-label={ui.filterPrefix}>
                      {inboxFilters(language).map((item) => (
                        <TooltipButton
                          key={item.id}
                          label={item.label}
                          onClick={() => setFilter(item.id)}
                          active={filter === item.id}
                          className={filter === item.id ? "filterPill active" : "filterPill"}
                        >
                          <FilterIcon status={item.id} />
                          <span>{item.label}</span>
                        </TooltipButton>
                      ))}
                    </div>
                    <span className={`selectionSummary ${selectionActionAvailability.kind === "mixed" ? "warning" : ""}`}>
                      {selectionSummary}
                    </span>
                    <div className="bulkActions">
                      <TooltipButton
                        type="button"
                        label={ui.selectAll}
                        onClick={() => setSelectedIds(selectVisibleIds(inbox))}
                        disabled={visibleApps.length === 0 || busy}
                        className="ghostButton bulkButton bulkIconButton"
                      >
                        <CheckCircle2 size={17} />
                      </TooltipButton>
                      <TooltipButton
                        type="button"
                        label={ui.clearSelection}
                        onClick={() => setSelectedIds([])}
                        disabled={selectedIds.length === 0 || busy}
                        className="ghostButton bulkButton bulkIconButton"
                      >
                        <RotateCcw size={17} />
                      </TooltipButton>
                      <TooltipButton
                        type="button"
                        label={selectionActionAvailability.reason ?? selectionActionAvailability.label}
                        onClick={() => void handleSelectionAction()}
                        disabled={!selectionActionAvailability.enabled}
                        className="dangerButton bulkButton"
                      >
                        <Trash2 size={17} />
                        <span>{selectionActionAvailability.label}</span>
                      </TooltipButton>
                    </div>
                  </div>
                </div>

                <div className="appTable" role="table" aria-label={ui.updatesTitle}>
                  {loading && apps.length === 0 ? (
                    <div className="emptyState">{ui.loadingDashboard}</div>
                  ) : apps.length === 0 ? (
                    <div className="emptyState">{ui.noApps}</div>
                  ) : inbox.length === 0 ? (
                    <div className="emptyState">{ui.noMatch}</div>
                  ) : (
                    inbox.map((item) => (
                    <InboxRow
                        key={item.id}
                        item={item}
                        language={language}
                        busy={busy}
                        selected={item.id === selected?.id}
                        checked={selectedIds.includes(item.id)}
                        pendingInstallRepoId={pendingInstallRepoId}
                        installRetrying={installRetrying}
                        onSelect={() => selectRepo(item.id)}
                        onToggleSelection={() => {
                          setSelectedIds((current) => toggleSelection(current, item.id));
                        }}
                        onPrimaryAction={() => {
                          selectRepo(item.id);
                          if (pendingInstallRepoId === item.id) {
                            void handleConfirmInstall(item);
                            return;
                          }
                          void handlePrimaryAction(item);
                        }}
                      />
                    ))
                  )}
                </div>
              </section>

              {!isEmptyDashboard ? (
                <Inspector
                  item={selected}
                  busy={busy}
                  language={language}
                  onOpenInstallPath={() => {
                    void handleOpenInstallPath(selected);
                  }}
                  onOpenInstallerFolder={() => {
                    void handleOpenInstallerFolder(selected);
                  }}
                  onOpenApp={() => {
                    void handleOpenApp(selected);
                  }}
                  onCopyReleaseNote={handleCopyReleaseNote}
                  onCopyValue={handleCopyValue}
                  onOpenRelease={() => {
                    void handleOpenRelease(selected);
                  }}
                  onPrimaryAction={() => {
                    if (selected) {
                      void handlePrimaryAction(selected);
                    }
                  }}
                  onConfirmInstall={() => {
                    if (selected) {
                      void handleConfirmInstall(selected);
                    }
                  }}
                  releaseVersions={releaseVersions}
                  versionsLoading={versionsLoading}
                  selectedVersion={selectedVersion}
                  onVersionChange={setSelectedVersion}
                  onPreviewSelectedVersion={() => {
                    void handlePreviewSelectedVersion(selected);
                  }}
                  onPinnedChange={(pinned) => {
                    void handleSetPinned(selected, pinned);
                  }}
                  onToggleIgnored={() => {
                    void handleToggleIgnored(selected);
                  }}
                  pendingRollback={pendingRollback}
                  onPreviewRollback={() => {
                    void handlePreviewRollback(selected);
                  }}
                  onConfirmRollback={() => {
                    void handleConfirmRollback(selected);
                  }}
                  onCancelRollback={() => setPendingRollback(null)}
                  onRequestUninstall={() => requestUninstall(selected)}
                  onConfirmUninstall={() => {
                    void handleConfirmUninstall(pendingUninstall);
                  }}
                  onCancelUninstall={() => setPendingUninstall(null)}
                  onRemoveTracked={() => {
                    void handleRemoveTracked(selected);
                  }}
                  pendingInstall={pendingInstall}
                  installRetrying={installRetrying}
                  onCancelInstall={() => setPendingInstall(null)}
                  pendingUninstall={pendingUninstall}
                />
              ) : null}
            </section>
          </section>
        ) : (
          <section className="settingsPanel" aria-label={ui.navSettings}>
            <div className="settingsLayout">
              <div className="settingsMain">
                <div className="settingsForm">
                  <label className="fieldRow wide primaryField">
                    <span>{ui.installRoot}</span>
                    <input
                      value={configDraft.installRoot}
                      onChange={(event) => setConfigDraft((current) => ({ ...current, installRoot: event.target.value }))}
                      placeholder={configDraft.effectiveInstallRoot || ui.openInstallRoot}
                      autoComplete="off"
                    />
                    <small>{usingDefaultInstallRoot ? `${ui.usingDefaultInstallRoot}: ${displayInstallRoot}` : ui.installRootHelp}</small>
                    <div className="fieldActions">
                      <TooltipButton
                        label={ui.restoreDefault}
                        onClick={() => setConfigDraft((current) => ({ ...current, installRoot: "" }))}
                        disabled={configSaving || installRoot.length === 0}
                        className="ghostButton fieldActionButton"
                      >
                        <RotateCcw size={16} />
                        <span>{ui.restoreDefault}</span>
                      </TooltipButton>
                      <TooltipButton
                        label={ui.openInstallRoot}
                        onClick={() => void handleOpenInstallRoot()}
                        disabled={configSaving || displayInstallRoot.length === 0}
                        className="ghostButton fieldActionButton"
                      >
                        <FolderOpen size={16} />
                        <span>{ui.openInstallRoot}</span>
                      </TooltipButton>
                    </div>
                  </label>

                  <label className="fieldRow">
                    <span>{ui.language}</span>
                    <div className="languageSwitch" role="group" aria-label={ui.language}>
                      {languageOptions(language).map((option) => (
                        <TooltipButton
                          key={option.value}
                          label={option.label}
                          onClick={() => setConfigDraft((current) => ({ ...current, language: option.value }))}
                          active={configDraft.language === option.value}
                          className={configDraft.language === option.value ? "languagePill active" : "languagePill"}
                        >
                          <span>{option.label}</span>
                        </TooltipButton>
                      ))}
                    </div>
                  </label>

                  <label className={networkFieldsAttention ? "fieldRow attention" : "fieldRow"}>
                    <span>{ui.githubToken}</span>
                    <div className="fieldInputRow">
                      <input
                        ref={githubTokenInput}
                        type={showGithubToken ? "text" : "password"}
                        value={configDraft.githubToken}
                        onChange={(event) => setConfigDraft((current) => ({ ...current, githubToken: event.target.value }))}
                        placeholder="token"
                        autoComplete="off"
                      />
                      <TooltipButton
                        label={showGithubToken ? ui.hideToken : ui.showToken}
                        onClick={() => setShowGithubToken((current) => !current)}
                        className="ghostButton tokenToggle"
                      >
                        {showGithubToken ? <EyeOff size={16} /> : <Eye size={16} />}
                      </TooltipButton>
                    </div>
                    <small>{ui.githubTokenHelp}</small>
                  </label>

                  <label className={networkFieldsAttention ? "fieldRow attention" : "fieldRow"}>
                    <span>{ui.proxyUrl}</span>
                    <input
                      ref={proxyUrlInput}
                      value={configDraft.proxyUrl}
                      onChange={(event) => setConfigDraft((current) => ({ ...current, proxyUrl: event.target.value }))}
                      placeholder={ui.proxyUrlPlaceholder}
                      autoComplete="off"
                    />
                    <small>{ui.proxyUrlHelp}</small>
                  </label>

                  <div className="fieldRow backgroundCheckRow">
                    <div className="backgroundCheckHeader">
                      <span>{ui.backgroundCheck}</span>
                      <label className="toggleRow">
                        <input
                          type="checkbox"
                          checked={configDraft.backgroundCheckEnabled}
                          onChange={(event) => setConfigDraft((current) => ({ ...current, backgroundCheckEnabled: event.target.checked }))}
                        />
                        <span>{configDraft.backgroundCheckEnabled ? ui.backgroundCheckEnabled : ui.backgroundCheckDisabled}</span>
                      </label>
                    </div>
                    <div className="backgroundCheckControls">
                      <label className="intervalField">
                        <span>{ui.checkInterval}</span>
                        <input
                          type="number"
                          min={1}
                          value={configDraft.checkIntervalMinutes}
                          onChange={(event) => {
                            const value = Number.parseInt(event.target.value, 10);
                            setConfigDraft((current) => ({
                              ...current,
                              checkIntervalMinutes: Number.isNaN(value) || value < 1 ? 1 : value
                            }));
                          }}
                        />
                        <span className="intervalUnit">{ui.checkIntervalUnit}</span>
                      </label>
                    </div>
                    <small>{ui.backgroundCheckHelp} {ui.checkIntervalHelp}</small>
                  </div>

                </div>
              </div>

              <aside className="settingsSidebar" aria-label={ui.networkConfigHealth}>
                <section className="settingsCard networkConfigCard">
                  <div className="settingsCardHeader">
                    <span className="settingsCardEyebrow">{ui.networkConfigHealth}</span>
                    <p>{ui.networkConfigHealthHelp}</p>
                  </div>
                  <div className="networkConfigStatusRow">
                    <span className={networkConfigHealth.tokenConfigured ? "statePill success" : "statePill"}>
                      {networkConfigHealth.tokenLabel}
                    </span>
                    <span className={networkConfigHealth.proxyConfigured ? "statePill success" : "statePill"}>
                      {networkConfigHealth.proxyLabel}
                    </span>
                  </div>
                  <p className="networkConfigGuide">{ui.networkProxyFormat}</p>
                  {networkConfigHealth.warning ? (
                    <p className="connectivityResult danger">{networkConfigHealth.warning.detail}</p>
                  ) : null}
                  <div className="networkConfigActions">
                    <TooltipButton
                      label={ui.testGithubConnectivity}
                      onClick={() => void handleTestGithubConnectivity()}
                      disabled={connectivityTesting}
                      className="ghostButton"
                    >
                      <Play size={16} />
                      <span>{ui.testGithubConnectivity}</span>
                    </TooltipButton>
                    <p className={`connectivityResult ${connectivityTestStatus.tone}`}>
                      <strong>{connectivityTestStatus.label}</strong> · {connectivityTestStatus.detail}
                    </p>
                  </div>
                </section>
              </aside>
            </div>
          </section>
        )}

        <StatusDock taskStatus={taskStatus} taskProgress={taskProgress} busy={busy || loading || configSaving || connectivityTesting} language={language} />
      </main>
    </div>
  );
}

function NavItem({
  icon,
  label,
  active = false,
  onClick
}: {
  icon: ReactNode;
  label: string;
  active?: boolean;
  onClick: () => void;
}) {
  return (
    <TooltipButton label={label} onClick={onClick} active={active} className={active ? "navItem active" : "navItem"}>
      {icon}
      <span>{label}</span>
    </TooltipButton>
  );
}

function InboxRow({
  item,
  language,
  busy,
  selected,
  checked,
  pendingInstallRepoId,
  installRetrying,
  onSelect,
  onToggleSelection,
  onPrimaryAction
}: {
  item: InboxItem;
  language: Language;
  busy: boolean;
  selected: boolean;
  checked: boolean;
  pendingInstallRepoId: string | null;
  installRetrying: boolean;
  onSelect: () => void;
  onToggleSelection: () => void;
  onPrimaryAction: () => void;
}) {
  const ui = createUiText(language);
  const primaryActionAvailability = getPrimaryActionAvailability(item, busy, language);
  const confirmInstallAvailability = getConfirmInstallAvailability(item, busy, language);
  const confirmingInstall = pendingInstallRepoId === item.id;
  const retryLabel = installRetrying ? ui.retryInstall : ui.confirmInstall;
  const actionLabel = confirmingInstall ? retryLabel : item.actionLabel;
  const actionReason = confirmingInstall ? retryLabel : primaryActionAvailability.reason ?? item.actionLabel;

  return (
    <div
      className={selected ? "tableRow selected" : "tableRow"}
      role="row"
      aria-selected={selected}
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect();
        }
      }}
    >
      <label className="rowCheckbox" onClick={(event) => event.stopPropagation()}>
        <input
          type="checkbox"
          checked={checked}
          aria-label={`Select ${item.name}`}
          onClick={(event) => event.stopPropagation()}
          onChange={onToggleSelection}
        />
      </label>
      <span className="appName">
        <StatusIcon status={item.status} />
        <span className="appNameCopy">
          <strong title={item.id}>{item.name}</strong>
          <span className="appNameMeta">{item.id}</span>
        </span>
      </span>
      <span className="mono">{item.currentVersion}</span>
      <span className="mono">{item.latestVersion}</span>
      <span className={`statusBadge ${item.status}`} aria-label={statusLabel(item.status, language)}>
        {statusLabel(item.status, language)}
      </span>
      <button
        type="button"
        className={confirmingInstall ? "rowAction confirming" : "rowAction"}
        aria-label={actionReason}
        title={actionReason}
        disabled={confirmingInstall ? !confirmInstallAvailability.enabled : !primaryActionAvailability.enabled}
        onClick={(event) => {
          event.stopPropagation();
          onSelect();
          onPrimaryAction();
        }}
      >
        {actionLabel}
      </button>
    </div>
  );
}

function Inspector({
  item,
  busy,
  language,
  onOpenApp,
  onOpenInstallPath,
  onOpenInstallerFolder,
  onOpenRelease,
  onCopyReleaseNote,
  onCopyValue,
  onPrimaryAction,
  onConfirmInstall,
  onRequestUninstall,
  onConfirmUninstall,
  onCancelUninstall,
  onRemoveTracked,
  onCancelInstall,
  pendingInstall,
  installRetrying,
  releaseVersions,
  versionsLoading,
  selectedVersion,
  onVersionChange,
  onPreviewSelectedVersion,
  onPinnedChange,
  onToggleIgnored,
  pendingRollback,
  onPreviewRollback,
  onConfirmRollback,
  onCancelRollback,
  pendingUninstall
}: {
  item: InboxItem | null;
  busy: boolean;
  language: Language;
  onOpenApp: () => void;
  onOpenInstallPath: () => void;
  onOpenInstallerFolder: () => void;
  onOpenRelease: () => void;
  onCopyReleaseNote: (note?: string) => void;
  onCopyValue: (label: string, value: string) => void;
  onPrimaryAction: () => void;
  onConfirmInstall: () => void;
  onRequestUninstall: () => void;
  onConfirmUninstall: () => void;
  onCancelUninstall: () => void;
  onRemoveTracked: () => void;
  onCancelInstall: () => void;
  pendingInstall: InstallPlan | null;
  installRetrying: boolean;
  releaseVersions: ReleaseVersion[];
  versionsLoading: boolean;
  selectedVersion: string;
  onVersionChange: (version: string) => void;
  onPreviewSelectedVersion: () => void;
  onPinnedChange: (pinned: boolean) => void;
  onToggleIgnored: () => void;
  pendingRollback: RollbackPreview | null;
  onPreviewRollback: () => void;
  onConfirmRollback: () => void;
  onCancelRollback: () => void;
  pendingUninstall: InboxItem | null;
}) {
  const ui = createUiText(language);

  if (!item) {
    return (
      <aside className="inspector" aria-label={ui.noSelection}>
        <div className="emptyInspector">
          <strong>{ui.noSelection}</strong>
          <span>{ui.addRepoTitle}</span>
        </div>
      </aside>
    );
  }

  const primaryActionAvailability = getPrimaryActionAvailability(item, busy, language);
  const confirmInstallAvailability = getConfirmInstallAvailability(item, busy, language);
  const uninstallAvailability = getUninstallAvailability(item, busy, language);
  const primaryActionKind = resolvePrimaryActionKind(item);
  const detailItems = getInspectorDetailItems(item, language);
  const lifecycleHistory = getLifecycleHistoryEntries(item, language);
  const releaseGuidance = buildReleaseActionGuidance(item, language);
  const showPrimaryInspectorAction = !(item.status === "needsChoice" && hasInstallableAsset(item));
  const installedLifecycleItem = isManagedPathKind(item.installPathKind) || isSystemInstallerKind(item.installPathKind);
  const showDangerInspectorActions = item.status !== "needsChoice" && installedLifecycleItem;
  const showInspectorActions =
    pendingInstall == null &&
    pendingRollback == null &&
    pendingUninstall == null &&
    (showPrimaryInspectorAction || showDangerInspectorActions);
  const inspectorSummary = buildInspectorStatusSummary(item, selectedVersion, installRetrying, language);
  const selectedReleaseVersion = releaseVersions.find((version) => version.tagName === selectedVersion) ?? null;
  const selectedReleaseTitle =
    selectedReleaseVersion?.name?.trim() || selectedVersion || item.releaseTitle || item.latestVersion || ui.noVersions;
  const decisionHeaderValue = installedLifecycleItem ? item.currentVersion : selectedReleaseTitle;
  const decisionStateLabel = installedLifecycleItem
    ? inspectorSummary?.label ?? ui.installedState
    : hasInstallableAsset(item)
      ? ui.installableState
      : null;
  const selectedReleasePublishedAt = formatPublishedAt(selectedReleaseVersion?.publishedAt ?? item.publishedAt, language);
  const inspectorSummaryDetail =
    inspectorSummary && item.status === "needsChoice" && selectedReleaseVersion
      ? `${inspectorSummary.detail} · ${selectedReleasePublishedAt}`
      : inspectorSummary?.detail ?? null;
  const showLifecyclePreviewAction = shouldShowLifecyclePreviewAction(item) && !installedLifecycleItem;
  const pendingInstallSafetyText = pendingInstall
    ? [
        pendingInstall.integrity.checksumAssetName ?? ui.installPreviewNoChecksumHint,
        pendingInstall.requires_user_confirmation ? ui.installPreviewSystemConfirmationHint : ""
      ]
        .filter((text) => text.length > 0)
        .join(" · ")
    : "";
  const inspectorActionSection = (
    <div className="inspectorActions" aria-label={ui.managedAppsTitle}>
      {/* 主动作独占第一组：安装 / 更新 / 打开 / 重试 */}
      {showPrimaryInspectorAction ? (
        <div className="inspectorActionsGroup primaryActionGroup">
          <button
            type="button"
            className="primaryButton actionButton wide inspectorPrimaryAction"
            onClick={onPrimaryAction}
            disabled={!primaryActionAvailability.enabled}
            aria-label={primaryActionAvailability.reason ?? item.actionLabel}
          >
            {primaryActionKind === "openApp" ? (
              <Play size={16} />
            ) : primaryActionKind === "openRelease" ? (
              <ExternalLink size={16} />
            ) : primaryActionKind === "openInstallLocation" || primaryActionKind === "openInstallerFile" ? (
              <FolderOpen size={16} />
            ) : (
              <Download size={16} />
            )}
            <span>{item.actionLabel}</span>
          </button>
        </div>
      ) : null}

      {/* 已安装的软件才露出卸载入口；未安装的跟踪项只保留版本预览路径。 */}
      {showDangerInspectorActions ? (
        <div className="inspectorActionsGroup dangerActionGroup">
          {item.uninstallSupported === false ? (
            isWindowsPlatform() ? (
              <TooltipButton
                label={ui.openSystemUninstall}
                onClick={() => void openSystemUninstallSettings()}
                className="dangerButton actionButton wide inspectorDangerAction"
              >
                <Trash2 size={16} />
                <span>{ui.openSystemUninstall}</span>
              </TooltipButton>
            ) : (
              <button
                type="button"
                className="ghostButton actionButton wide inspectorDangerAction"
                disabled
                aria-label={uninstallAvailability.reason ?? ui.model.useSystemUninstall}
              >
                <Trash2 size={16} />
                <span>{ui.model.useSystemUninstall}</span>
              </button>
            )
          ) : (
            <button
              type="button"
              className="dangerButton actionButton wide inspectorDangerAction"
              onClick={onRequestUninstall}
              disabled={!uninstallAvailability.enabled}
              aria-label={uninstallAvailability.reason ?? ui.uninstallAbility}
            >
              <Trash2 size={16} />
              <span>{ui.uninstallAbility}</span>
            </button>
          )}
        </div>
      ) : null}
    </div>
  );
  const inspectorInfoSection = (
    <>
      <div className={`inspectorBlock decisionBlock ${installedLifecycleItem ? "installedDecision" : "needsInstallDecision"}`}>
        <div className="decisionHeader">
          <div className="blockTitle decisionTitle">
            <Pin size={16} />
            <span>{ui.releaseLifecycle}</span>
          </div>
          <div className="decisionHeaderStatus">
            <strong className="decisionHeaderValue">{decisionHeaderValue}</strong>
            {decisionStateLabel ? (
              <span className={`statePill ${inspectorSummary?.tone ?? "success"} decisionStatePill`}>
                {decisionStateLabel}
              </span>
            ) : null}
          </div>
        </div>
        <div className="lifecycleBlock">
          <label className="lifecycleField">
            <span>{ui.releaseTarget}</span>
            <select
              className={versionsLoading ? "selectControl loading" : "selectControl"}
              value={selectedVersion}
              onChange={(event) => onVersionChange(event.target.value)}
              disabled={busy || versionsLoading || releaseVersions.length === 0}
            >
              {releaseVersions.length === 0 ? (
                <option value="">{versionsLoading ? ui.loadingVersions : ui.noVersions}</option>
              ) : null}
              {releaseVersions.map((version) => (
                <option key={version.tagName} value={version.tagName}>
                  {version.tagName}
                </option>
              ))}
            </select>
          </label>
          {showLifecyclePreviewAction ? (
            <TooltipButton
              label={ui.previewSelectedVersion}
              onClick={onPreviewSelectedVersion}
              disabled={busy || !selectedVersion}
              className="primaryButton actionButton lifecyclePreviewAction"
            >
              <Download size={16} />
              <span>{ui.previewSelectedVersion}</span>
            </TooltipButton>
          ) : null}
        </div>

        {showInspectorActions ? inspectorActionSection : null}

        <dl className="detailList decisionDetailList">
          {detailItems.map((detail, index) => (
            <div key={`${detail.label}-${index}`} className={detail.fullWidth ? "detailListWide" : undefined}>
              <dt>{detail.label}</dt>
              <dd className={detail.monospace ? "mono wrapText" : "wrapText"}>
                <CopyableValue
                  label={detail.label}
                  value={detail.value}
                  copyLabel={ui.copyValue}
                  monospace={detail.monospace}
                  onCopy={onCopyValue}
                />
              </dd>
            </div>
          ))}
        </dl>
      </div>

      {releaseGuidance ? (
        <div className="inspectorBlock guidanceBlock">
          <div className="blockTitle">
            <CircleAlert size={16} />
            <span>{ui.releaseGuidanceTitle}</span>
          </div>
          <p className="guidanceHeadline">{releaseGuidance.title}</p>
          <p className="mutedText">{releaseGuidance.summary}</p>
          <ul className="guidanceList">
            {releaseGuidance.bullets.map((bullet) => (
              <li key={bullet}>{bullet}</li>
            ))}
          </ul>
        </div>
      ) : null}

      <div className="releaseNoteBlock">
        <div className="releaseNoteHeader">
          <div className="blockTitle">
            <Clipboard size={15} />
            <span>{ui.releaseNote}</span>
          </div>
          <TooltipButton
            label={ui.copyReleaseNote}
            onClick={() => onCopyReleaseNote(item.releaseNote)}
            disabled={!item.releaseNote}
            className="ghostButton copyButton"
          >
            <Clipboard size={15} />
            <span>{ui.copy}</span>
          </TooltipButton>
        </div>
        <ReleaseNoteView note={item.releaseNote?.trim() || ""} emptyText={ui.notes.noReleaseNote} />
      </div>

      {lifecycleHistory.length > 0 ? (
        <div className="historyBlock">
          <div className="blockTitle">
            <Layers3 size={16} />
            <span>{ui.activityHistory}</span>
          </div>
          <ul className="historyList">
            {lifecycleHistory.map((entry, index) => (
              <li key={`${entry.recordedAt}-${index}`} className={entry.failed ? "historyItem failed" : "historyItem"}>
                <div className="historyItemHeader">
                  <strong>{entry.summary}</strong>
                  <span className={entry.failed ? "statePill danger" : "statePill success"}>
                    {entry.failed ? ui.activityFailed : ui.activitySucceeded}
                  </span>
                </div>
                <div className="historyItemMeta">
                  <span className="mono">{entry.recordedAt}</span>
                  {entry.error ? <span className="historyItemError">{entry.error}</span> : null}
                </div>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </>
  );

  return (
    <aside className="inspector" aria-label={ui.managedAppsTitle}>
      <div className="inspectorHead">
        <div className="inspectorHeadCopy">
          <h2>{item.name}</h2>
          {inspectorSummary ? (
            <div className="inspectorSummary">
              <span className={`statePill ${inspectorSummary.tone}`}>{inspectorSummary.label}</span>
              <span className="inspectorSummaryDetail">{inspectorSummaryDetail}</span>
            </div>
          ) : null}
        </div>
      </div>

      {pendingInstall ? (
        <div className="installPreview pendingInstall" role="alertdialog" aria-label={ui.installPreview}>
          <div className="blockTitle">
            <Download size={16} />
            <span>{ui.installPreview}</span>
          </div>
          <div className="previewHero">
            <div className="previewHeroCopy">
              <strong className="previewHeroTitle">
                {pendingInstall.version} · {pendingInstall.asset_name}
              </strong>
              <span className="previewHeroMeta">
                {formatPublishedAt(selectedReleaseVersion?.publishedAt ?? item.publishedAt, language)}
              </span>
            </div>
            <span className="previewBadge">{installTypeLabel(pendingInstall.install_type, language)}</span>
          </div>
          <div className="previewMeta">
            <div className="previewMetaRow">
              <span className="previewMetaLabel">{ui.assetFile}</span>
              <span className="previewMetaValue mono wrapText">
                <CopyableValue
                  label={ui.assetFile}
                  value={pendingInstall.asset_name}
                  copyLabel={ui.copyValue}
                  monospace
                  onCopy={onCopyValue}
                />
              </span>
            </div>
            <div className="previewMetaRow">
              <span className="previewMetaLabel">{ui.installPath}</span>
              <span className="previewMetaValue mono wrapText">
                <CopyableValue
                  label={ui.installPath}
                  value={item.installPath}
                  copyLabel={ui.copyValue}
                  monospace
                  onCopy={onCopyValue}
                />
              </span>
            </div>
            {pendingInstall.system_package_manager ? (
              <div className="previewMetaRow">
                <span className="previewMetaLabel">{ui.systemPackageManager}</span>
                <span className="previewMetaValue">{systemPackageManagerLabel(pendingInstall.system_package_manager)}</span>
              </div>
            ) : null}
          </div>
          <div className="previewSafetyNote">
            <CircleAlert size={15} />
            <div>
              <strong>{installPreviewIntegrityLabel(pendingInstall.integrity, language)}</strong>
              <span>{pendingInstallSafetyText}</span>
            </div>
          </div>
          {installRetrying ? <p className="previewFailureHint">{ui.installRetryHint}</p> : null}
          <div className="previewActions">
            <TooltipButton
              label={ui.cancel}
              onClick={onCancelInstall}
              disabled={busy}
              className="ghostButton actionButton previewCancelAction"
            >
              <RotateCcw size={16} />
              <span>{ui.cancel}</span>
            </TooltipButton>
            <TooltipButton
              label={confirmInstallAvailability.reason ?? (installRetrying ? ui.retryInstall : ui.confirmInstall)}
              onClick={onConfirmInstall}
              disabled={!confirmInstallAvailability.enabled}
              className="primaryButton actionButton previewConfirmAction"
              autoFocus
            >
              <Download size={16} />
              <span>{installRetrying ? ui.retryInstall : ui.confirmInstall}</span>
            </TooltipButton>
          </div>
        </div>
      ) : null}
      {pendingRollback ? (
        <div className="installPreview pendingRollback" role="alertdialog" aria-label={ui.confirmRollback}>
          <div className="blockTitle">
            <RotateCcw size={16} />
            <span>{ui.confirmRollback}</span>
          </div>
          <p className="previewLine">
            {pendingRollback.activeVersion} → {pendingRollback.snapshotVersion}
          </p>
          <div className="previewActions">
            <TooltipButton
              label={ui.cancel}
              onClick={onCancelRollback}
              disabled={busy}
              className="ghostButton actionButton previewCancelAction"
            >
              <RotateCcw size={16} />
              <span>{ui.cancel}</span>
            </TooltipButton>
            <TooltipButton
              label={ui.confirmRollback}
              onClick={onConfirmRollback}
              disabled={busy}
              className="primaryButton actionButton previewConfirmAction"
              autoFocus
            >
              <RotateCcw size={16} />
              <span>{ui.confirmRollback}</span>
            </TooltipButton>
          </div>
        </div>
      ) : null}
      {pendingUninstall ? (
        <div className="installPreview pendingUninstall" role="alertdialog" aria-label={ui.confirmUninstall}>
          <div className="blockTitle">
            <Trash2 size={16} />
            <span>{ui.confirmUninstall}</span>
          </div>
          <p className="previewLine">{pendingUninstall.name}</p>
          <div className="previewMeta">
            <div className="previewMetaRow">
              <span className="previewMetaLabel">{ui.installPath}</span>
              <span className="previewMetaValue mono wrapText">
                <CopyableValue
                  label={ui.installPath}
                  value={pendingUninstall.installPath}
                  copyLabel={ui.copyValue}
                  monospace
                  onCopy={onCopyValue}
                />
              </span>
            </div>
            <div className="previewMetaRow">
              <span className="previewMetaLabel">{ui.installManagement}</span>
              <span className="previewMetaValue">
                {pendingUninstall.managementKind
                  ? installManagementKindLabel(pendingUninstall.managementKind, language)
                  : ui.installManagement}
              </span>
            </div>
            {pendingUninstall.installType === "LinuxPackage" && pendingUninstall.systemPackageManager ? (
              <div className="previewMetaRow">
                <span className="previewMetaLabel">{ui.systemPackageManager}</span>
                <span className="previewMetaValue">{systemPackageManagerLabel(pendingUninstall.systemPackageManager)}</span>
              </div>
            ) : null}
          </div>
          <p className="mutedText">
            {pendingUninstall.installPathKind === "managedPath"
              ? ui.uninstallManagedConfirmation
              : pendingUninstall.installType === "LinuxPackage"
                ? ui.uninstallLinuxPackageConfirmation
                : ui.uninstallExternalInstallerConfirmation}
          </p>
          <div className="previewActions">
            <TooltipButton
              label={ui.cancel}
              onClick={onCancelUninstall}
              disabled={busy}
              className="ghostButton actionButton previewCancelAction"
            >
              <RotateCcw size={16} />
              <span>{ui.cancel}</span>
            </TooltipButton>
            <TooltipButton
              label={ui.confirmUninstall}
              onClick={onConfirmUninstall}
              disabled={busy}
              className="dangerButton actionButton previewConfirmAction"
            >
              <Trash2 size={16} />
              <span>{ui.confirmUninstall}</span>
            </TooltipButton>
          </div>
        </div>
      ) : null}
      {inspectorInfoSection}
    </aside>
  );
}

function StatusDock({
  taskStatus,
  taskProgress,
  busy,
  language
}: {
  taskStatus: string;
  taskProgress: TaskProgressView | null;
  busy: boolean;
  language: Language;
}) {
  const presentation = buildStatusDockPresentation(taskProgress, busy, taskStatus, language);
  const progressPercent = presentation.progressPercent;
  const progressValuePercent = progressPercent == null ? null : progressPercent === 0 ? 6 : progressPercent;
  const progressClassName = presentation.progressMode === "indeterminate" ? "taskProgressTrack busy" : "taskProgressTrack";
  const progressValueClassName = presentation.progressMode === "indeterminate" ? "taskProgressValue busy" : "taskProgressValue";
  const statusDockClassName = [
    "statusDock",
    presentation.showProgress ? "withProgress" : "idle",
    presentation.failed ? "failed" : ""
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <footer
      className={statusDockClassName}
      aria-live="polite"
      aria-atomic="true"
      aria-busy={busy || undefined}
    >
      <div className="statusDockCopy">
        <span className={busy ? "statusDot busy" : "statusDot"} />
        <div>
          <p className="eyebrow">{presentation.eyebrow}</p>
          <strong title={presentation.headline}>{presentation.headline}</strong>
        </div>
      </div>
      <div className="statusDockDetail">
        {presentation.detail ? (
          <span className="taskProgressMessage" title={presentation.detail}>
            {presentation.detail}
          </span>
        ) : null}
      </div>
      {presentation.showPill ? (
        <span className={presentation.failed ? "taskProgressPercent danger statusDockPercent" : "taskProgressPercent statusDockPercent"}>
          {presentation.pillLabel}
        </span>
      ) : null}
      {presentation.showProgress ? (
        <div
          className={progressClassName}
          role="progressbar"
          aria-label={presentation.detail}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={presentation.progressMode === "determinate" && progressPercent != null ? progressPercent : undefined}
          aria-valuetext={presentation.progressMode === "indeterminate" ? presentation.pillLabel : undefined}
        >
          <div
            className={progressValueClassName}
            style={presentation.progressMode === "indeterminate" ? undefined : { width: `${progressValuePercent ?? 0}%` }}
          />
        </div>
      ) : null}
    </footer>
  );
}

function ReleaseNoteView({ note, emptyText }: { note: string; emptyText: string }) {
  if (!note) {
    return <div className="releaseNotePreview empty">{emptyText}</div>;
  }

  const blocks = parseReleaseNote(note);

  return (
    <div className="releaseNotePreview">
      {blocks.map((block, index) => {
        if (block.type === "heading") {
          const Tag = `h${block.level}` as "h1" | "h2" | "h3";
          return (
            <Tag key={`${block.type}-${index}`} className={`noteHeading level${block.level}`}>
              {renderInlineMarkdown(block.text)}
            </Tag>
          );
        }

        if (block.type === "list") {
          return (
            <ul key={`${block.type}-${index}`} className={block.ordered ? "noteList ordered" : "noteList"}>
              {block.items.map((item) => (
                <li key={item}>{renderListItem(item)}</li>
              ))}
            </ul>
          );
        }

        if (block.type === "quote") {
          return (
            <blockquote key={`${block.type}-${index}`} className="noteQuote">
              {renderInlineMarkdown(block.text)}
            </blockquote>
          );
        }

        if (block.type === "table") {
          return (
            <div key={`${block.type}-${index}`} className="noteTableScroller">
              <table className="noteTable">
                <thead>
                  <tr>
                    {block.header.map((cell, cellIndex) => (
                      <th key={`${block.type}-${index}-head-${cellIndex}`} scope="col">
                        {renderInlineMarkdown(cell)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {block.rows.map((row, rowIndex) => (
                    <tr key={`${block.type}-${index}-row-${rowIndex}`}>
                      {row.map((cell, cellIndex) => (
                        <td key={`${block.type}-${index}-row-${rowIndex}-cell-${cellIndex}`}>
                          {renderInlineMarkdown(cell)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          );
        }

        if (block.type === "divider") {
          return <hr key={`${block.type}-${index}`} className="noteDivider" aria-hidden="true" />;
        }

        if (block.type === "code") {
          return (
            <pre key={`${block.type}-${index}`} className="noteCode">
              <code>{block.text}</code>
            </pre>
          );
        }

        return (
          <p key={`${block.type}-${index}`} className="noteParagraph">
            {renderInlineMarkdown(block.text)}
          </p>
        );
      })}
    </div>
  );
}

function renderInlineMarkdown(text: string) {
  return text.split(/(`[^`]+`|!\[[^\]]*\]\([^)]+\)|\[[^\]]+\]\([^)]+\)|<br\s*\/?>|https?:\/\/[^\s<]+)/gi).map((part, index) => {
    if (part.startsWith("`") && part.endsWith("`")) {
      return <code key={`${part}-${index}`}>{part.slice(1, -1)}</code>;
    }
    const imageMatch = /^!\[([^\]]*)\]\(([^)]+)\)$/.exec(part);
    if (imageMatch) {
      return (
        <img
          key={`${part}-${index}`}
          className="noteImage"
          src={imageMatch[2].trim()}
          alt={imageMatch[1]}
          loading="lazy"
        />
      );
    }
    const linkMatch = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(part);
    if (linkMatch) {
      const href = linkMatch[2].trim();
      if (/^https?:\/\//i.test(href)) {
        return (
          <a key={`${part}-${index}`} href={href} target="_blank" rel="noreferrer">
            {linkMatch[1]}
          </a>
        );
      }
      return linkMatch[1];
    }
    if (/^<br\s*\/?>$/i.test(part)) {
      return <br key={`${part}-${index}`} />;
    }
    if (/^https?:\/\//i.test(part)) {
      return (
        <a key={`${part}-${index}`} href={part} target="_blank" rel="noreferrer">
          {part}
        </a>
      );
    }
    return part;
  });
}

function renderListItem(item: string) {
  const checklistMatch = /^\[(x| )\]\s+(.*)$/i.exec(item);
  if (!checklistMatch) {
    return renderInlineMarkdown(item);
  }

  const checked = checklistMatch[1].toLowerCase() === "x";
  return (
    <span className="noteChecklist">
      <input type="checkbox" checked={checked} readOnly tabIndex={-1} />
      <span>{renderInlineMarkdown(checklistMatch[2])}</span>
    </span>
  );
}

function StatusIcon({ status }: { status: InboxItem["status"] }) {
  if (status === "current") {
    return <CheckCircle2 className="statusIcon current" size={18} />;
  }
  if (status === "needsChoice") {
    return <ShieldAlert className="statusIcon needsChoice" size={18} />;
  }
  if (status === "noRelease") {
    return <EyeOff className="statusIcon noRelease" size={18} />;
  }
  if (status === "downgradeAvailable") {
    return <RotateCcw className="statusIcon downgradeAvailable" size={18} />;
  }
  return <RefreshCw className="statusIcon updateAvailable" size={18} />;
}

function statusLabel(status: InboxItem["status"], language: Language) {
  const ui = createUiText(language);
  switch (status) {
    case "updateAvailable":
      return ui.status.updateAvailable;
    case "downgradeAvailable":
      return ui.status.downgradeAvailable;
    case "needsChoice":
      return ui.status.needsChoice;
    case "noRelease":
      return ui.status.noRelease;
    case "failed":
      return ui.status.failed;
    case "current":
      return ui.status.current;
  }
}

function configDraftKey(config: ConfigDraft) {
  return JSON.stringify({
    githubToken: config.githubToken.trim(),
    proxyUrl: config.proxyUrl.trim(),
    installRoot: config.installRoot.trim(),
    language: normalizeLanguage(config.language),
    backgroundCheckEnabled: config.backgroundCheckEnabled,
    checkIntervalMinutes: config.checkIntervalMinutes
  });
}

function desktopConfigFromDraft(config: ConfigDraft): DesktopConfig {
  return {
    githubToken: config.githubToken.trim() || null,
    proxyUrl: config.proxyUrl.trim() || null,
    installRoot: config.installRoot.trim() || null,
    effectiveInstallRoot: config.effectiveInstallRoot.trim() || null,
    language: config.language,
    backgroundCheckEnabled: config.backgroundCheckEnabled,
    checkIntervalMinutes: config.checkIntervalMinutes
  };
}

function filterLabel(status: InboxFilter, language: Language) {
  const ui = createUiText(language);
  switch (status) {
    case "all":
      return ui.all;
    case "updateAvailable":
      return ui.updateAvailable;
    case "actionRequired":
      return ui.needsChoice;
    case "failed":
      return ui.failed;
    default:
      return ui.all;
  }
}

function FilterIcon({ status }: { status: InboxFilter }) {
  switch (status) {
    case "all":
      return <Layers3 size={15} />;
    case "updateAvailable":
      return <Download size={15} />;
    case "actionRequired":
      return <ShieldAlert size={15} />;
    case "failed":
      return <CircleAlert size={15} />;
    default:
      return <Layers3 size={15} />;
  }
}

function TooltipButton({
  label,
  className = "iconButton",
  children,
  onClick,
  disabled = false,
  type = "button",
  active = false,
  autoFocus = false
}: {
  label: string;
  className?: string;
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  type?: "button" | "submit" | "reset";
  active?: boolean;
  autoFocus?: boolean;
}) {
  return (
    <button
      className={active ? `${className} active` : className}
      type={type}
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      title={label}
      autoFocus={autoFocus}
    >
      {children}
    </button>
  );
}

function CopyableValue({
  label,
  value,
  copyLabel,
  monospace = false,
  onCopy
}: {
  label: string;
  value: string;
  copyLabel: string;
  monospace?: boolean;
  onCopy: (label: string, value: string) => void;
}) {
  return (
    <span className={monospace ? "copyableValue mono" : "copyableValue"}>
      <span className="copyableText" title={value}>
        {value}
      </span>
      <TooltipButton
        label={`${copyLabel}: ${label}`}
        onClick={() => onCopy(label, value)}
        disabled={!value}
        className="ghostButton copyValueButton"
      >
        <Clipboard size={14} />
      </TooltipButton>
    </span>
  );
}

function installTypeLabel(value: InstallPlan["install_type"], language: Language) {
  const ui = createUiText(language);
  switch (value) {
    case "WindowsInstaller":
      return ui.installType.WindowsInstaller;
    case "PortableArchive":
      return ui.installType.PortableArchive;
    case "AppImage":
      return ui.installType.AppImage;
    case "LinuxPackage":
      return ui.installType.LinuxPackage;
    case "Executable":
      return ui.installType.Executable;
    case "Archive":
      return ui.installType.Archive;
    case "Unknown":
      return ui.installType.Unknown;
  }
}

function normalizeRepoId(input: string) {
  const trimmed = input.trim();
  if (/^https?:\/\//i.test(trimmed)) {
    try {
      const url = new URL(trimmed);
      const parts = url.pathname.split("/").filter(Boolean);
      if (parts.length >= 2) {
        return `${parts[0]}/${parts[1].replace(/\.git$/i, "")}`;
      }
    } catch {
      return trimmed;
    }
  }

  return trimmed.replace(/\.git$/i, "");
}

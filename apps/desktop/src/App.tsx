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
  loadConfig,
  loadDashboard,
  openUrl,
  openPath,
  previewInstall,
  removeTrackedRepo,
  saveConfig,
  uninstallRepo,
  openSystemUninstallSettings
} from "./backend";
import {
  buildStatusDockPresentation,
  buildUpdateInbox,
  getBulkRemoveAvailability,
  getConfirmInstallAvailability,
  getOpenReleaseAvailability,
  getPrimaryActionAvailability,
  getRemoveTrackedAvailability,
  parseReleaseNote,
  pruneSelection,
  getUninstallAvailability,
  filterManagedApps,
  inboxFilters,
  selectVisibleIds,
  toggleSelection,
  type InboxFilter,
  type InboxItem,
  type ManagedApp
} from "./appModel";
import {
  createUiText,
  formatPublishedAt,
  isWindowsPlatform,
  languageOptions,
  normalizeLanguage,
  type Language
} from "./i18n";
import type { InstallPlan, TaskProgressEvent } from "./backend";

type ConfigDraft = {
  githubToken: string;
  proxyUrl: string;
  installRoot: string;
  effectiveInstallRoot: string;
  language: Language;
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
  const [taskProgress, setTaskProgress] = useState<TaskProgressView | null>(null);
  const [configDraft, setConfigDraft] = useState<ConfigDraft>({
    githubToken: "",
    proxyUrl: "",
    installRoot: "",
    effectiveInstallRoot: "",
    language: "en"
  });
  const currentConfigKey = useRef(configDraftKey(configDraft));
  const lastSavedConfigKey = useRef("");
  const pendingConfigSaves = useRef(0);
  const [showGithubToken, setShowGithubToken] = useState(false);
  const [configLoaded, setConfigLoaded] = useState(false);
  const [configSaving, setConfigSaving] = useState(false);
  const [taskStatus, setTaskStatus] = useState("Loading GitHub Release data");
  const [error, setError] = useState<string | null>(null);
  const activeTaskProgress = useRef<TaskProgressContext>(null);
  const dashboardRefreshId = useRef(0);
  const dashboardOrder = useRef<Map<string, number>>(new Map());

  const language = normalizeLanguage(configDraft.language);
  const ui = createUiText(language);
  const visibleApps = filterManagedApps(apps, filter, searchQuery);
  const inbox = buildUpdateInbox(visibleApps, language);
  const selected = inbox.find((item) => item.id === selectedId) ?? inbox[0] ?? null;
  const hasGithubToken = configDraft.githubToken.trim().length > 0;
  const installRoot = configDraft.installRoot.trim();
  const effectiveInstallRoot = configDraft.effectiveInstallRoot.trim();
  const displayInstallRoot = installRoot || effectiveInstallRoot;
  const usingDefaultInstallRoot = installRoot.length === 0 && effectiveInstallRoot.length > 0;
  const bulkRemoveAvailability = getBulkRemoveAvailability(apps, selectedIds, busy, language);

  function sortAppsByDashboardOrder(nextApps: ManagedApp[]) {
    const order = dashboardOrder.current;
    return [...nextApps].sort((left, right) => {
      const leftIndex = order.get(left.id) ?? Number.MAX_SAFE_INTEGER;
      const rightIndex = order.get(right.id) ?? Number.MAX_SAFE_INTEGER;
      return leftIndex - rightIndex || left.name.localeCompare(right.name);
    });
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
    setPendingInstall(null);
  }, [selectedId]);

  useEffect(() => {
    if (loading) {
      return;
    }

    setSelectedIds((current) => pruneSelection(current, apps));
  }, [apps, loading]);

  useEffect(() => {
    currentConfigKey.current = configDraftKey(configDraft);
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

      setTaskStatus(`Checking latest release (${event.payload.completed}/${event.payload.total})`);
    }).then((dispose) => {
      unlistenProgress = dispose;
    });

    return () => {
      unlistenItem?.();
      unlistenProgress?.();
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

  async function refreshDashboard() {
    clearTaskProgress();
    const refreshId = dashboardRefreshId.current + 1;
    dashboardRefreshId.current = refreshId;
    dashboardOrder.current = new Map(apps.map((app, index) => [app.id, index]));
    setLoading(true);
    setError(null);
    setPendingInstall(null);
    setTaskStatus("Checking latest release");
    try {
      const data = await loadDashboard(refreshId);
      if (dashboardRefreshId.current !== refreshId) {
        return;
      }
      dashboardOrder.current = new Map(data.map((app, index) => [app.id, index]));
      setApps(data);
      setSelectedId((current) => (current && data.some((item) => item.id === current) ? current : data[0]?.id ?? null));
      setTaskStatus(data.length > 0 ? `Loaded ${data.length} apps` : "No managed apps yet");
    } catch (caught) {
      if (dashboardRefreshId.current !== refreshId) {
        return;
      }
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("Failed to refresh updates");
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
        language: normalizeLanguage(data.language)
      };
      lastSavedConfigKey.current = configDraftKey(draft);
      setConfigDraft(draft);
      setConfigLoaded(true);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("Failed to load settings");
    }
  }

  async function refreshWorkspace() {
    await Promise.all([refreshDashboard(), refreshConfig()]);
  }

  async function handleAddRepo() {
    const trimmed = repoInput.trim();
    if (!trimmed) {
      clearTaskProgress();
      setError("Enter owner/repo or a GitHub URL");
      setTaskStatus("Failed to add repository");
      return;
    }

    clearTaskProgress();
    setBusy(true);
    setError(null);
    setPendingInstall(null);
    setTaskStatus(`Adding ${trimmed}`);
    try {
      const data = await addRepo(trimmed);
      setApps(data);
      setSelectedId(normalizeRepoId(trimmed));
      setRepoInput("");
      setTaskStatus(`Added ${trimmed}`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("Failed to add repository");
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
    setTaskStatus(mode === "auto" ? "Auto-saving settings" : "Saving settings");
    try {
      const saved = await saveConfig({
        githubToken: draft.githubToken.trim() || null,
        proxyUrl: draft.proxyUrl.trim() || null,
        installRoot: draft.installRoot.trim() || null,
        effectiveInstallRoot: draft.effectiveInstallRoot.trim() || null,
        language: draft.language
      });
      const savedDraft = {
        githubToken: saved.githubToken ?? "",
        proxyUrl: saved.proxyUrl ?? "",
        installRoot: saved.installRoot ?? "",
        effectiveInstallRoot: saved.effectiveInstallRoot ?? saved.installRoot ?? "",
        language: normalizeLanguage(saved.language)
      };
      lastSavedConfigKey.current = configDraftKey(savedDraft);
      if (currentConfigKey.current === draftKey) {
        setConfigDraft(savedDraft);
        setTaskStatus("Settings saved");
      }
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("Failed to save settings");
    } finally {
      pendingConfigSaves.current = Math.max(0, pendingConfigSaves.current - 1);
      if (pendingConfigSaves.current === 0) {
        setConfigSaving(false);
      }
    }
  }

  async function handlePrimaryAction(item: InboxItem) {
    if (item.status === "current" || item.status === "noRelease") {
      await handleOpenRelease(item);
      return;
    }

    if (item.status === "failed") {
      await refreshDashboard();
      return;
    }

    clearTaskProgress();
    setBusy(true);
    setError(null);
    setTaskStatus(`Generating install preview for ${item.name}`);
    try {
      const plan = await previewInstall(item.id);
      setPendingInstall(plan);
      setTaskStatus(`Generated install preview for ${item.name}`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("Failed to build install preview");
    } finally {
      setBusy(false);
    }
  }

  async function handleConfirmInstall(item: InboxItem) {
    activeTaskProgress.current = { repoId: item.id, action: "install" };
    setBusy(true);
    setError(null);
    setTaskStatus(`Installing ${item.name}`);
    setTaskProgress({
      repoId: item.id,
      action: "install",
      stage: "preparing",
      message: `Preparing to install ${item.name}`,
      percent: 0
    });
    try {
      const data = await installRepo(item.id);
      setApps(data);
      setSelectedId(item.id);
      setPendingInstall(null);
      setTaskProgress({
        repoId: item.id,
        action: "install",
        stage: "finished",
        message: `Finished installing ${item.name}`,
        percent: 100
      });
      setTaskStatus(`Installed or updated ${item.name}`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("Install failed");
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

  async function handleUninstall(item: InboxItem | null) {
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
      message: `Uninstalling ${item.name}`,
      percent: 0
    });
    try {
      const data = await uninstallRepo(item.id);
      setApps(data);
      setSelectedId(data.find((app) => app.id === item.id)?.id ?? data[0]?.id ?? null);
      setPendingInstall(null);
      setTaskProgress({
        repoId: item.id,
        action: "uninstall",
        stage: "finished",
        message: `Finished uninstalling ${item.name}`,
        percent: 100
      });
      setTaskStatus(`Uninstalled ${item.name}`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("Uninstall failed");
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
    if (!item || (item.status !== "needsChoice" && !(item.status === "noRelease" && item.installPathKind === "Unknown"))) {
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
      setTaskStatus(`Stopped tracking ${item.name}`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("Remove tracking failed");
    } finally {
      setBusy(false);
    }
  }

  async function handleBulkRemoveTracked() {
    if (!bulkRemoveAvailability.enabled) {
      clearTaskProgress();
      setError(bulkRemoveAvailability.reason ?? "Select at least one removable item");
      setTaskStatus("Bulk remove failed");
      return;
    }

    const targets = apps.filter(
      (app) => selectedIds.includes(app.id) && (app.status === "needsChoice" || (app.status === "noRelease" && app.installPathKind === "Unknown"))
    );
    if (targets.length === 0) {
      clearTaskProgress();
      setError("Select at least one uninstalled tracked item");
      setTaskStatus("Bulk remove failed");
      return;
    }

    clearTaskProgress();
    setBusy(true);
    setError(null);
    setPendingInstall(null);
    setTaskStatus(`Removing ${targets.length} tracked item(s)`);

    try {
      const result = await bulkRemoveTrackedRepos(targets.map((target) => target.id));
      setApps(result.apps);
      setTaskStatus(
        result.removedCount < targets.length
          ? `Removed ${result.removedCount} tracked item(s), ${targets.length - result.removedCount} expired`
          : `Removed ${result.removedCount} tracked item(s)`
      );
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
      setTaskStatus("Bulk remove failed");
    } finally {
      setBusy(false);
    }
  }

  async function handleOpenRelease(item: InboxItem | null) {
    if (!item?.releaseUrl) {
      clearTaskProgress();
      setTaskStatus("No release link available");
      return;
    }
    clearTaskProgress();
    try {
      await openUrl(item.releaseUrl);
      setTaskStatus(`Opened ${item.name} release page`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("Open failed");
    }
  }

  async function handleOpenInstallPath(item: InboxItem | null) {
    if (!item?.installPath || item.installPath === "unknown" || item.status === "needsChoice" || item.installPathKind !== "ManagedPath") {
      clearTaskProgress();
      setTaskStatus("No install path available");
      return;
    }

    clearTaskProgress();
    try {
      await openPath(item.installPath);
      setTaskStatus(`Opened ${item.name} install location`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("Open folder failed");
    }
  }

  async function handleOpenInstallRoot() {
    if (!displayInstallRoot) {
      clearTaskProgress();
      setTaskStatus("No install root selected");
      return;
    }

    clearTaskProgress();
    try {
      await openPath(displayInstallRoot);
      setTaskStatus("Opened install root");
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("Open folder failed");
    }
  }

  function handleCopyReleaseNote(note?: string) {
    if (!note || !navigator.clipboard) {
      clearTaskProgress();
      return;
    }
    clearTaskProgress();
    void navigator.clipboard.writeText(note);
    setTaskStatus("Release note copied");
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
          <div className="topbarMeta">
            <span className={hasGithubToken ? "statePill success" : "statePill"}>{hasGithubToken ? ui.configReady : ui.configPublic}</span>
          </div>
          {activeView === "dashboard" ? (
            <div className="topbarActions">
              <TooltipButton label={ui.checkUpdates} onClick={() => void refreshDashboard()} disabled={busy || loading} className="ghostButton topbarButton">
                <RefreshCw size={17} />
                <span>{ui.checkUpdates}</span>
              </TooltipButton>
            </div>
          ) : null}
        </header>

        {error ? <div className="errorBanner">{error}</div> : null}

        {activeView === "dashboard" ? (
          <section className="dashboardView">
            <section className="addRepoPanel" aria-label={ui.addRepoEyebrow}>
              <div className="panelHeading">
                <p className="eyebrow">{ui.addRepoEyebrow}</p>
                <h2>{ui.addRepoTitle}</h2>
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
            </section>

            <section className="contentGrid">
              <section className="inboxPanel" aria-label={ui.managedAppsTitle}>
                <div className="sectionHeader">
                  <div className="sectionTitle">
                    <div className="sectionGlyph">
                      <Layers3 size={16} />
                    </div>
                    <div>
                      <p className="eyebrow">{ui.managedAppsTitle}</p>
                      <h2>{ui.managedAppsCount(inbox.length)}</h2>
                    </div>
                  </div>
                  <div className="sectionMeta">
                    <span className="statePill subtle">{ui.managedAppsPending(inbox.filter((item) => item.status !== "current").length)}</span>
                    <span className="statePill subtle">{ui.filterPrefix}{filterLabel(filter, language)}</span>
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
                        label={bulkRemoveAvailability.reason ?? ui.remove}
                        onClick={() => void handleBulkRemoveTracked()}
                        disabled={!bulkRemoveAvailability.enabled}
                        className="dangerButton bulkButton"
                      >
                        <Trash2 size={17} />
                        <span>{ui.remove}</span>
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
                        selected={item.id === selected?.id}
                        checked={selectedIds.includes(item.id)}
                        onSelect={() => setSelectedId(item.id)}
                        onToggleSelection={() => {
                          setSelectedIds((current) => toggleSelection(current, item.id));
                        }}
                      />
                    ))
                  )}
                </div>
              </section>

              <Inspector
                item={selected}
                busy={busy}
                language={language}
                onOpenInstallPath={() => {
                  void handleOpenInstallPath(selected);
                }}
                onCopyReleaseNote={handleCopyReleaseNote}
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
                onUninstall={() => {
                  void handleUninstall(selected);
                }}
                onRemoveTracked={() => {
                  void handleRemoveTracked(selected);
                }}
                pendingInstall={pendingInstall}
                onCancelInstall={() => setPendingInstall(null)}
              />
            </section>
          </section>
        ) : (
          <section className="settingsPanel" aria-label={ui.navSettings}>
            <div className="sectionHeader">
              <div className="sectionTitle">
                <div className="sectionGlyph">
                  <Settings2 size={16} />
                </div>
                <div>
                  <p className="eyebrow">{ui.settingsEyebrow}</p>
                  <h2>{ui.settingsTitleSmall}</h2>
                </div>
              </div>
            </div>

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

              <label className="fieldRow">
                <span>{ui.githubToken}</span>
                <div className="fieldInputRow">
                  <input
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

              <label className="fieldRow">
                <span>{ui.proxyUrl}</span>
                <input
                  value={configDraft.proxyUrl}
                  onChange={(event) => setConfigDraft((current) => ({ ...current, proxyUrl: event.target.value }))}
                  placeholder="proxy"
                  autoComplete="off"
                />
                <small>{ui.proxyUrlHelp}</small>
              </label>
            </div>

            <div className="settingsActions">
              <TooltipButton label={ui.reloadSettings} onClick={() => void refreshConfig()} disabled={configSaving} className="ghostButton">
                <RefreshCw size={17} />
                <span>{ui.reloadSettings}</span>
              </TooltipButton>
            </div>
          </section>
        )}

        <StatusDock taskStatus={taskStatus} taskProgress={taskProgress} busy={busy || loading || configSaving} language={language} />
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
  selected,
  checked,
  onSelect,
  onToggleSelection
}: {
  item: InboxItem;
  language: Language;
  selected: boolean;
  checked: boolean;
  onSelect: () => void;
  onToggleSelection: () => void;
}) {
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
      <span className="rowAction" aria-label={item.actionLabel}>
        {item.actionLabel}
      </span>
    </div>
  );
}

function Inspector({
  item,
  busy,
  language,
  onOpenInstallPath,
  onOpenRelease,
  onCopyReleaseNote,
  onPrimaryAction,
  onConfirmInstall,
  onUninstall,
  onRemoveTracked,
  onCancelInstall,
  pendingInstall
}: {
  item: InboxItem | null;
  busy: boolean;
  language: Language;
  onOpenInstallPath: () => void;
  onOpenRelease: () => void;
  onCopyReleaseNote: (note?: string) => void;
  onPrimaryAction: () => void;
  onConfirmInstall: () => void;
  onUninstall: () => void;
  onRemoveTracked: () => void;
  onCancelInstall: () => void;
  pendingInstall: InstallPlan | null;
}) {
  const openReleaseAvailability = getOpenReleaseAvailability(item, busy, language);
  const primaryActionAvailability = getPrimaryActionAvailability(item, busy, language);
  const confirmInstallAvailability = getConfirmInstallAvailability(item, busy, language);
  const uninstallAvailability = getUninstallAvailability(item, busy, language);
  const removeTrackedAvailability = getRemoveTrackedAvailability(item, busy, language);
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

  return (
    <aside className="inspector" aria-label={ui.managedAppsTitle}>
      <div className="inspectorHead">
        <div>
          <h2>{item.name}</h2>
          <p className="mono">
            {item.currentVersion} → {item.latestVersion}
          </p>
        </div>
      </div>

      <div className="inspectorBlock accent">
        <div className="blockTitle">
          <ExternalLink size={16} />
          <span>{ui.releaseInfo}</span>
        </div>
        <p>{item.releaseTitle ?? item.latestVersion}</p>
        <p className="mutedText">{formatPublishedAt(item.publishedAt, language)}</p>
      </div>

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

      <dl className="detailList">
        <div>
          <dt>{ui.assetFile}</dt>
          <dd className="mono wrapText">{item.assetName ?? ui.noAssetAvailable}</dd>
        </div>
        <div>
          <dt>{item.installPathKind === "SystemInstaller" ? ui.installerFile : ui.installPath}</dt>
          <dd className="mono wrapText">{item.installPath}</dd>
        </div>
      </dl>

      {pendingInstall ? (
        <div className="installPreview">
          <div className="blockTitle">
            <Download size={16} />
            <span>{ui.installPreview}</span>
          </div>
          <p className="previewLine">
            {pendingInstall.asset_name} · {installTypeLabel(pendingInstall.install_type, language)}
          </p>
          {pendingInstall.notes.length > 0 ? (
            <ul className="previewNotes">
              {pendingInstall.notes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          ) : null}
          {pendingInstall.requires_user_confirmation ? (
            <p className="mutedText">{ui.installPreviewConfirmation}</p>
          ) : null}
          <div className="previewActions">
            <TooltipButton label={ui.cancel} onClick={onCancelInstall} disabled={busy} className="ghostButton actionButton">
              <RotateCcw size={16} />
              <span>{ui.cancel}</span>
            </TooltipButton>
            <TooltipButton
              label={confirmInstallAvailability.reason ?? ui.confirmInstall}
              onClick={onConfirmInstall}
              disabled={!confirmInstallAvailability.enabled}
              className="primaryButton actionButton"
            >
              <Download size={16} />
              <span>{ui.confirmInstall}</span>
            </TooltipButton>
          </div>
        </div>
      ) : null}

      <div className="inspectorActions" aria-label={ui.managedAppsTitle}>
        <button
          type="button"
          className="ghostButton actionButton wide"
          onClick={onOpenRelease}
          disabled={!openReleaseAvailability.enabled}
          aria-label={openReleaseAvailability.reason ?? ui.openRelease}
        >
          <ExternalLink size={16} />
          <span>{ui.openRelease}</span>
        </button>
        {item.status !== "needsChoice" && item.installPathKind === "ManagedPath" ? (
          <button
            type="button"
            className="ghostButton actionButton wide"
            onClick={onOpenInstallPath}
            aria-label={ui.openInstallLocation}
          >
            <FolderOpen size={16} />
            <span>{ui.openInstallLocation}</span>
          </button>
        ) : null}
        <button
          type="button"
          className="primaryButton actionButton wide"
          onClick={onPrimaryAction}
          disabled={!primaryActionAvailability.enabled}
          aria-label={primaryActionAvailability.reason ?? item.actionLabel}
        >
          <Download size={16} />
          <span>{item.actionLabel}</span>
        </button>
        {item.status === "needsChoice" || (item.status === "noRelease" && item.installPathKind === "Unknown") ? (
          <button
            type="button"
            className="dangerButton actionButton wide"
            onClick={onRemoveTracked}
            disabled={!removeTrackedAvailability.enabled}
            aria-label={removeTrackedAvailability.reason ?? ui.removeTracked}
          >
            <Trash2 size={16} />
            <span>{ui.removeTracked}</span>
          </button>
          ) : item.uninstallSupported === false ? (
          isWindowsPlatform() ? (
            <TooltipButton
              label={ui.openSystemUninstall}
              onClick={() => void openSystemUninstallSettings()}
              className="ghostButton actionButton wide"
            >
              <Trash2 size={16} />
              <span>{ui.openSystemUninstall}</span>
            </TooltipButton>
          ) : (
            <button
              type="button"
              className="ghostButton actionButton wide"
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
            className="ghostButton actionButton wide"
            onClick={onUninstall}
            disabled={!uninstallAvailability.enabled}
            aria-label={uninstallAvailability.reason ?? ui.uninstallAbility}
          >
            <Trash2 size={16} />
            <span>{ui.uninstallAbility}</span>
          </button>
        )}
      </div>
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
  const taskRunning = taskProgress != null && taskProgress.stage !== "finished" && taskProgress.stage !== "failed";

  return (
    <footer
      className={presentation.failed ? "statusDock failed" : "statusDock"}
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
        <span className={presentation.failed ? "statePill danger" : "statePill subtle"}>
          {presentation.pillLabel}
        </span>
      </div>
      {presentation.showProgress ? (
        <div
          className={
            presentation.progressMode === "indeterminate"
              ? "taskProgressTrack busy"
              : taskRunning
                ? "taskProgressTrack active"
                : "taskProgressTrack"
          }
          role="progressbar"
          aria-label={presentation.detail}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={presentation.progressMode === "determinate" && progressPercent != null ? progressPercent : undefined}
          aria-valuetext={presentation.progressMode === "indeterminate" ? presentation.pillLabel : undefined}
        >
          <div
            className={
              presentation.progressMode === "indeterminate"
                ? "taskProgressValue busy"
                : taskRunning
                  ? "taskProgressValue active"
                  : "taskProgressValue"
            }
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
  return <RefreshCw className="statusIcon updateAvailable" size={18} />;
}

function statusLabel(status: InboxItem["status"], language: Language) {
  const ui = createUiText(language);
  switch (status) {
    case "updateAvailable":
      return ui.status.updateAvailable;
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
    language: normalizeLanguage(config.language)
  });
}

function filterLabel(status: InboxFilter, language: Language) {
  const ui = createUiText(language);
  switch (status) {
    case "all":
      return ui.all;
    case "updateAvailable":
      return ui.updateAvailable;
    case "needsChoice":
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
    case "needsChoice":
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
  active = false
}: {
  label: string;
  className?: string;
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  type?: "button" | "submit" | "reset";
  active?: boolean;
}) {
  return (
    <button className={active ? `${className} active` : className} type={type} onClick={onClick} disabled={disabled} aria-label={label} title={label}>
      {children}
    </button>
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

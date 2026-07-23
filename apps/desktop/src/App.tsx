import { useEffect, useState, type ReactNode } from "react";
import {
  CheckCircle2,
  Clipboard,
  CircleCheckBig,
  CircleAlert,
  ChevronDown,
  ChevronUp,
  Download,
  ExternalLink,
  FolderOpen,
  Layers3,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  Settings2,
  ShieldAlert,
  Trash2
} from "lucide-react";
import {
  addRepo,
  bulkRemoveTrackedRepos,
  installRepo,
  loadConfig,
  loadDashboard,
  openUrl,
  previewInstall,
  removeTrackedRepo,
  saveConfig,
  uninstallRepo,
  DEFAULT_TRACKED_REPO_ID
} from "./backend";
import {
  buildUpdateInbox,
  getBulkRemoveAvailability,
  getConfirmInstallAvailability,
  getOpenReleaseAvailability,
  getPrimaryActionAvailability,
  getRemoveTrackedAvailability,
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
import type { InstallPlan } from "./backend";

type ConfigDraft = {
  githubToken: string;
  proxyUrl: string;
  installRoot: string;
};

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
  const [configDraft, setConfigDraft] = useState<ConfigDraft>({
    githubToken: "",
    proxyUrl: "",
    installRoot: ""
  });
  const [configSaving, setConfigSaving] = useState(false);
  const [taskStatus, setTaskStatus] = useState("正在加载 GitHub Release 数据");
  const [error, setError] = useState<string | null>(null);

  const visibleApps = filterManagedApps(apps, filter, searchQuery);
  const inbox = buildUpdateInbox(visibleApps);
  const selected = inbox.find((item) => item.id === selectedId) ?? inbox[0] ?? null;
  const hasGithubToken = configDraft.githubToken.trim().length > 0;
  const bulkRemoveAvailability = getBulkRemoveAvailability(apps, selectedIds, busy);

  useEffect(() => {
    void refreshWorkspace();
  }, []);

  useEffect(() => {
    setPendingInstall(null);
  }, [selectedId]);

  useEffect(() => {
    setSelectedIds((current) => pruneSelection(current, apps));
  }, [apps]);

  async function refreshDashboard() {
    setLoading(true);
    setBusy(true);
    setError(null);
    setPendingInstall(null);
    setTaskStatus("正在检查最新 Release");
    try {
      const data = await loadDashboard();
      setApps(data);
      setSelectedId((current) => {
        if (current && data.some((item) => item.id === current)) {
          return current;
        }
        return data[0]?.id ?? null;
      });
      setTaskStatus(data.length > 0 ? `已加载 ${data.length} 个软件` : "当前没有管理的软件");
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("检查更新失败");
    } finally {
      setBusy(false);
      setLoading(false);
    }
  }

  async function refreshConfig() {
    try {
      const data = await loadConfig();
      setConfigDraft({
        githubToken: data.githubToken ?? "",
        proxyUrl: data.proxyUrl ?? "",
        installRoot: data.installRoot ?? ""
      });
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("读取配置失败");
    }
  }

  async function refreshWorkspace() {
    await Promise.all([refreshDashboard(), refreshConfig()]);
  }

  async function handleAddRepo() {
    const trimmed = repoInput.trim();
    if (!trimmed) {
      setError("请输入 owner/repo 或 GitHub URL");
      setTaskStatus("添加失败");
      return;
    }

    setBusy(true);
    setError(null);
    setPendingInstall(null);
    setTaskStatus(`正在添加 ${trimmed}`);
    try {
      const data = await addRepo(trimmed);
      setApps(data);
      setSelectedId(normalizeRepoId(trimmed));
      setRepoInput("");
      setTaskStatus(`已添加 ${trimmed}`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("添加失败");
    } finally {
      setBusy(false);
    }
  }

  async function handleSaveConfig() {
    setConfigSaving(true);
    setError(null);
    setTaskStatus("正在保存设置");
    try {
      const saved = await saveConfig({
        githubToken: configDraft.githubToken.trim() || null,
        proxyUrl: configDraft.proxyUrl.trim() || null,
        installRoot: configDraft.installRoot.trim() || null
      });
      setConfigDraft({
        githubToken: saved.githubToken ?? "",
        proxyUrl: saved.proxyUrl ?? "",
        installRoot: saved.installRoot ?? ""
      });
      setTaskStatus("设置已保存");
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("保存设置失败");
    } finally {
      setConfigSaving(false);
    }
  }

  async function handlePrimaryAction(item: InboxItem) {
    if (item.status === "current") {
      await handleOpenRelease(item);
      return;
    }

    if (item.status === "failed") {
      await refreshDashboard();
      return;
    }

    setBusy(true);
    setError(null);
    setTaskStatus(`正在生成 ${item.name} 的安装预览`);
    try {
      const plan = await previewInstall(item.id);
      setPendingInstall(plan);
      setTaskStatus(`已生成 ${item.name} 的安装预览`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("安装预览失败");
    } finally {
      setBusy(false);
    }
  }

  async function handleConfirmInstall(item: InboxItem) {
    setBusy(true);
    setError(null);
    setTaskStatus(`正在安装 ${item.name}`);
    try {
      const data = await installRepo(item.id);
      setApps(data);
      setSelectedId(item.id);
      setPendingInstall(null);
      setTaskStatus(`已安装或更新 ${item.name}`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("安装失败");
    } finally {
      setBusy(false);
    }
  }

  async function handleUninstall(item: InboxItem | null) {
    if (!item || item.status === "needsChoice" || item.uninstallSupported === false) {
      return;
    }

    setBusy(true);
    setError(null);
    try {
      const data = await uninstallRepo(item.id);
      setApps(data);
      setSelectedId(data.find((app) => app.id === item.id)?.id ?? data[0]?.id ?? null);
      setPendingInstall(null);
      setTaskStatus(`已卸载 ${item.name}`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("卸载失败");
    } finally {
      setBusy(false);
    }
  }

  async function handleRemoveTracked(item: InboxItem | null) {
    if (!item || item.status !== "needsChoice") {
      return;
    }

    setBusy(true);
    setError(null);
    try {
      const data = await removeTrackedRepo(item.id);
      setApps(data);
      setSelectedId(data.find((app) => app.id === item.id)?.id ?? data[0]?.id ?? null);
      setPendingInstall(null);
      setTaskStatus(`已移除 ${item.name} 的跟踪`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("移除失败");
    } finally {
      setBusy(false);
    }
  }

  async function handleBulkRemoveTracked() {
    if (!bulkRemoveAvailability.enabled) {
      setError(bulkRemoveAvailability.reason ?? "请选择至少一个可移除项");
      setTaskStatus("批量移除失败");
      return;
    }

    const targets = apps.filter((app) => selectedIds.includes(app.id) && app.status === "needsChoice");
    if (targets.length === 0) {
      setError("请选择至少一个未安装的跟踪项");
      setTaskStatus("批量移除失败");
      return;
    }

    setBusy(true);
    setError(null);
    setPendingInstall(null);
    setTaskStatus(`正在批量移除 ${targets.length} 个跟踪项`);

    try {
      const result = await bulkRemoveTrackedRepos(targets.map((target) => target.id));
      setApps(result.apps);
      setTaskStatus(
        result.removedCount < targets.length
          ? `已移除 ${result.removedCount} 个跟踪项，${targets.length - result.removedCount} 个已失效`
          : `已移除 ${result.removedCount} 个跟踪项`
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
      setTaskStatus("批量移除失败");
    } finally {
      setBusy(false);
    }
  }

  async function handleOpenRelease(item: InboxItem | null) {
    if (!item?.releaseUrl) {
      setTaskStatus("当前没有可打开的 Release 链接");
      return;
    }
    try {
      await openUrl(item.releaseUrl);
      setTaskStatus(`已打开 ${item.name} 的 Release 页面`);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setTaskStatus("打开失败");
    }
  }

  function handleCopyReleaseNote(note?: string) {
    if (!note || !navigator.clipboard) {
      return;
    }
    void navigator.clipboard.writeText(note);
    setTaskStatus("已复制 release note");
  }

  return (
    <div className="shell">
      <aside className="sidebar" aria-label="主导航">
        <Tooltip label="GitHub Release Manager">
          <button className="brand brandButton" type="button" aria-label="GitHub Release Manager">
            <div className="brandMark">GR</div>
          </button>
        </Tooltip>

        <nav className="navList">
          <NavItem
            icon={<Download size={18} />}
            label="更新收件箱"
            active={activeView === "dashboard"}
            onClick={() => setActiveView("dashboard")}
          />
          <NavItem
            icon={<Settings2 size={18} />}
            label="设置"
            active={activeView === "settings"}
            onClick={() => setActiveView("settings")}
          />
        </nav>

        <Tooltip label={DEFAULT_TRACKED_REPO_ID} className="sourceTooltip">
          <button className="sourceTile sourceButton" type="button" aria-label={DEFAULT_TRACKED_REPO_ID}>
          <ExternalLink size={18} />
          </button>
        </Tooltip>
      </aside>

      <main className="workspace">
        <header className="topbar">
          {activeView === "dashboard" ? (
            <>
              <Tooltip label={taskStatus} className="topbarTooltip">
                <div className="topbarCopy">
                  <Download size={16} />
                  <span className="srOnly">{taskStatus}</span>
                </div>
              </Tooltip>
              <Tooltip label={hasGithubToken ? "已配置 token" : "公开仓库可用"} className="topbarTooltip">
                <div className={hasGithubToken ? "tokenState active" : "tokenState"} aria-label={hasGithubToken ? "已配置 token" : "公开仓库可用"}>
                  <ShieldAlert size={14} />
                </div>
              </Tooltip>
              <TooltipButton label="检查更新" onClick={() => void refreshDashboard()} disabled={busy}>
                <RefreshCw size={17} />
              </TooltipButton>
            </>
          ) : (
            <>
              <Tooltip label="统一保存 GitHub token、代理和安装根目录" className="topbarTooltip">
                <div className="settingsTopbarCopy">
                  <Settings2 size={16} />
                  <span className="srOnly">统一保存 GitHub token、代理和安装根目录。</span>
                </div>
              </Tooltip>
              <TooltipButton label="重新加载" onClick={() => void refreshConfig()} disabled={configSaving}>
                <RefreshCw size={17} />
              </TooltipButton>
              <TooltipButton label="保存设置" onClick={() => void handleSaveConfig()} disabled={configSaving} className="iconButton primaryIconButton">
                <Plus size={17} />
              </TooltipButton>
            </>
          )}
        </header>

        {error ? <div className="errorBanner">{error}</div> : null}

        {activeView === "dashboard" ? (
          <section className="dashboardView">
            <section className="addRepoPanel" aria-label="添加 GitHub 仓库">
              <TooltipButton label="添加 GitHub 仓库" className="iconButton addRepoIcon">
                <Plus size={17} />
              </TooltipButton>
              <div className="repoControl">
                <div className="repoBox">
                  <Plus size={17} />
                  <input
                    placeholder="owner/repo"
                    aria-label="添加 GitHub 仓库"
                    value={repoInput}
                    onChange={(event) => setRepoInput(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        void handleAddRepo();
                      }
                    }}
                  />
                </div>
                <TooltipButton label="添加仓库" onClick={() => void handleAddRepo()} disabled={busy} className="iconButton primaryIconButton">
                  <Plus size={17} />
                </TooltipButton>
              </div>
            </section>

            <section className="contentGrid">
              <section className="inboxPanel" aria-label="已管理软件">
                <div className="sectionHeader">
                  <Tooltip label="已管理软件" className="sectionGlyph">
                    <div>
                      <Layers3 size={16} />
                    </div>
                  </Tooltip>
                  <Tooltip label="需要处理的项目数量" className="updateCount">
                    <div>{inbox.filter((item) => item.status !== "current").length}</div>
                  </Tooltip>
                </div>

                <div className="listTools">
                  <div className="listToolsPrimary">
                    <div className="searchBox">
                      <Search size={17} />
                      <input
                        placeholder="搜索"
                        aria-label="筛选已添加的软件"
                        value={searchQuery}
                        onChange={(event) => setSearchQuery(event.target.value)}
                      />
                    </div>
                  </div>
                  <div className="listToolsActions">
                    <div className="filterRow" aria-label="状态筛选">
                      {inboxFilters.map((item) => (
                        <TooltipButton
                          key={item.id}
                          label={filterLabel(item.id)}
                          onClick={() => setFilter(item.id)}
                          active={filter === item.id}
                          className={filter === item.id ? "filterPill active" : "filterPill"}
                        >
                          <FilterIcon status={item.id} />
                        </TooltipButton>
                      ))}
                    </div>
                    <div className="bulkActions">
                      <TooltipButton
                        type="button"
                        label="全选当前列表"
                        onClick={() => setSelectedIds(selectVisibleIds(inbox))}
                        disabled={visibleApps.length === 0 || busy}
                        className="iconButton"
                      >
                        <CheckCircle2 size={17} />
                      </TooltipButton>
                      <TooltipButton
                        type="button"
                        label="清空选择"
                        onClick={() => setSelectedIds([])}
                        disabled={selectedIds.length === 0 || busy}
                        className="iconButton"
                      >
                        <RotateCcw size={17} />
                      </TooltipButton>
                      <TooltipButton
                        type="button"
                        label={bulkRemoveAvailability.reason ?? "批量移除"}
                        onClick={() => void handleBulkRemoveTracked()}
                        disabled={!bulkRemoveAvailability.enabled}
                        className="iconButton primaryIconButton"
                      >
                        <Trash2 size={17} />
                      </TooltipButton>
                    </div>
                  </div>
                </div>

                <div className="appTable" role="table" aria-label="更新列表">
                  {loading ? (
                    <div className="emptyState">正在同步 GitHub Release 数据...</div>
                  ) : apps.length === 0 ? (
                    <div className="emptyState">还没有添加软件。先在上方输入 GitHub 仓库。</div>
                  ) : inbox.length === 0 ? (
                    <div className="emptyState">没有匹配的软件。筛选只会查找已添加的软件，不会搜索 GitHub 全网。</div>
                  ) : (
                    inbox.map((item) => (
                      <InboxRow
                        key={item.id}
                        item={item}
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
          <section className="settingsPanel" aria-label="设置">
            <div className="sectionHeader">
              <Tooltip label="设置" className="sectionGlyph">
                <div>
                  <Settings2 size={16} />
                </div>
              </Tooltip>
              <Tooltip label="设置项数量" className="updateCount">
                <div>{configSaving ? "…" : "3"}</div>
              </Tooltip>
            </div>

            <div className="settingsForm">
              <label className="fieldRow">
                <span className="srOnly">GitHub Token</span>
                <input
                  value={configDraft.githubToken}
                  onChange={(event) => setConfigDraft((current) => ({ ...current, githubToken: event.target.value }))}
                  placeholder="token"
                  autoComplete="off"
                />
              </label>

              <label className="fieldRow">
                <span className="srOnly">代理地址</span>
                <input
                  value={configDraft.proxyUrl}
                  onChange={(event) => setConfigDraft((current) => ({ ...current, proxyUrl: event.target.value }))}
                  placeholder="proxy"
                  autoComplete="off"
                />
              </label>

              <label className="fieldRow wide">
                <span className="srOnly">安装根目录</span>
                <input
                  value={configDraft.installRoot}
                  onChange={(event) => setConfigDraft((current) => ({ ...current, installRoot: event.target.value }))}
                  placeholder="root"
                  autoComplete="off"
                />
              </label>
            </div>

            <div className="settingsActions">
              <TooltipButton label="重新载入" onClick={() => void refreshConfig()} disabled={configSaving}>
                <RefreshCw size={17} />
              </TooltipButton>
              <TooltipButton label="保存设置" onClick={() => void handleSaveConfig()} disabled={configSaving} className="iconButton primaryIconButton">
                <Plus size={17} />
              </TooltipButton>
            </div>
          </section>
        )}

        <footer className="taskBar">
          <span className="taskLabel" aria-label={taskStatus}>{taskStatus}</span>
          <div className={busy ? "progressTrack busy" : "progressTrack"} aria-label="运行状态">
            <div className="progressValue" />
          </div>
          <TooltipButton label="重新检查" onClick={() => void refreshDashboard()} disabled={busy}>
            <RefreshCw size={17} />
          </TooltipButton>
        </footer>
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
    </TooltipButton>
  );
}

function InboxRow({
  item,
  selected,
  checked,
  onSelect,
  onToggleSelection
}: {
  item: InboxItem;
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
          aria-label={`选择 ${item.name}`}
          onClick={(event) => event.stopPropagation()}
          onChange={onToggleSelection}
        />
      </label>
      <span className="appName">
        <StatusIcon status={item.status} />
        <span className="appNameCopy">
          <Tooltip label={item.id}>
            <strong>{item.name}</strong>
          </Tooltip>
        </span>
      </span>
      <span className="mono">{item.currentVersion}</span>
      <span className="mono">{item.latestVersion}</span>
      <Tooltip label={statusLabel(item.status)}>
        <span className={`statusBadge ${item.status}`} aria-label={statusLabel(item.status)}>
          <StatusGlyph status={item.status} />
        </span>
      </Tooltip>
      <Tooltip label={item.actionLabel}>
        <span className="rowAction" aria-label={item.actionLabel}>
          <RowActionGlyph status={item.status} />
        </span>
      </Tooltip>
    </div>
  );
}

function Inspector({
  item,
  busy,
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
  onOpenRelease: () => void;
  onCopyReleaseNote: (note?: string) => void;
  onPrimaryAction: () => void;
  onConfirmInstall: () => void;
  onUninstall: () => void;
  onRemoveTracked: () => void;
  onCancelInstall: () => void;
  pendingInstall: InstallPlan | null;
}) {
  const openReleaseAvailability = getOpenReleaseAvailability(item, busy);
  const primaryActionAvailability = getPrimaryActionAvailability(item, busy);
  const confirmInstallAvailability = getConfirmInstallAvailability(item, busy);
  const uninstallAvailability = getUninstallAvailability(item, busy);
  const removeTrackedAvailability = getRemoveTrackedAvailability(item, busy);
  const [detailsOpen, setDetailsOpen] = useState(false);

  if (!item) {
    return (
      <aside className="inspector" aria-label="详情检查器">
        <div className="emptyInspector">
          <strong>暂无可展示的软件</strong>
          <span>添加一个 GitHub 仓库，或检查本地清单后再查看详情。</span>
        </div>
      </aside>
    );
  }

  return (
    <aside className="inspector" aria-label="详情检查器">
      <div className="inspectorHead">
        <div>
          <h2>{item.name}</h2>
          <p className="mono">
            {item.currentVersion} → {item.latestVersion}
          </p>
        </div>
        <TooltipButton
          type="button"
          label={openReleaseAvailability.reason ?? "打开 Release 页面"}
          onClick={onOpenRelease}
          disabled={!openReleaseAvailability.enabled}
          className="iconButton"
          placement="left"
        >
          <FolderOpen size={18} />
        </TooltipButton>
      </div>

      <div className="inspectorBlock accent">
        <Tooltip label="Release 信息" className="blockTitle">
          <div>
            <ExternalLink size={16} />
          </div>
        </Tooltip>
        <p>{item.releaseTitle ?? item.latestVersion}</p>
        <p className="mutedText">{formatPublishedAt(item.publishedAt)}</p>
      </div>

      <div className="releaseNoteBlock">
        <div className="releaseNoteHeader">
          <Tooltip label="Release note" className="blockTitle">
            <div>
              <Clipboard size={15} />
            </div>
          </Tooltip>
          <TooltipButton
            label="复制 release note"
            onClick={() => onCopyReleaseNote(item.releaseNote)}
            disabled={!item.releaseNote}
            className="iconButton copyButton"
            placement="left"
          >
            <Clipboard size={15} />
          </TooltipButton>
        </div>
        <pre className={detailsOpen ? "releaseNotePreview expanded" : "releaseNotePreview"}>
          {item.releaseNote?.trim() || "这个 release 没有填写 release note。"}
        </pre>
        <div className="noteFooter">
          <span className="mutedText">{detailsOpen ? "完整内容已展开" : "默认显示摘要，查看更多字段后可继续展开"}</span>
          <button
            type="button"
            className="ghostButton detailToggleButton"
            onClick={() => setDetailsOpen((current) => !current)}
          >
            {detailsOpen ? "收起字段" : "更多字段"}
            {detailsOpen ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
          </button>
        </div>
      </div>

      {detailsOpen ? (
        <dl className="detailList">
          <div>
            <dt>
              <Download size={14} />
            </dt>
            <dd className="mono wrapText">{item.assetName ?? "暂无可用资产"}</dd>
          </div>
          <div>
            <dt>
              <FolderOpen size={14} />
            </dt>
            <dd className="mono wrapText">{item.installPath}</dd>
          </div>
          <div>
            <dt>
              <Layers3 size={14} />
            </dt>
            <dd>{installTypeLabel(item.installType ?? "Unknown")}</dd>
          </div>
          <div>
            <dt>
              <Settings2 size={14} />
            </dt>
            <dd>{installPathKindLabel(item.installPathKind ?? "Unknown")}</dd>
          </div>
          <div>
            <dt>
              <Trash2 size={14} />
            </dt>
            <dd>{item.uninstallSupported === false ? "需系统卸载" : "可自动卸载"}</dd>
          </div>
          <div>
            <dt>
              <ExternalLink size={14} />
            </dt>
            <dd>{item.source}</dd>
          </div>
        </dl>
      ) : null}

      {pendingInstall ? (
        <div className="installPreview">
          <Tooltip label="安装预览" className="blockTitle">
            <div>
              <Download size={16} />
            </div>
          </Tooltip>
          <p className="previewLine">
            {pendingInstall.asset_name} · {installTypeLabel(pendingInstall.install_type)}
          </p>
          {pendingInstall.notes.length > 0 ? (
            <ul className="previewNotes">
              {pendingInstall.notes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          ) : null}
          {pendingInstall.requires_user_confirmation ? (
            <p className="mutedText">这个安装包需要在系统权限确认后继续执行。</p>
          ) : null}
          <div className="previewActions">
            <button type="button" className="ghostButton actionButton" onClick={onCancelInstall} disabled={busy}>
              <RotateCcw size={16} />
              <span>取消</span>
            </button>
            <button
              type="button"
              className="primaryButton actionButton"
              onClick={onConfirmInstall}
              disabled={!confirmInstallAvailability.enabled}
              aria-label={confirmInstallAvailability.reason ?? "确认安装"}
            >
              <Download size={16} />
              <span>确认安装</span>
            </button>
          </div>
        </div>
      ) : null}

      <div className="inspectorActions" aria-label="软件操作">
        <button
          type="button"
          className="ghostButton actionButton wide"
          onClick={onOpenRelease}
          disabled={!openReleaseAvailability.enabled}
          aria-label={openReleaseAvailability.reason ?? "打开 Release 页面"}
        >
          <ExternalLink size={16} />
          <span>打开 Release</span>
        </button>
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
        {item.status === "needsChoice" ? (
          <button
            type="button"
            className="ghostButton actionButton wide"
            onClick={onRemoveTracked}
            disabled={!removeTrackedAvailability.enabled}
            aria-label={removeTrackedAvailability.reason ?? "移除跟踪"}
          >
            <Trash2 size={16} />
            <span>移除跟踪</span>
          </button>
        ) : item.uninstallSupported === false ? (
          <button
            type="button"
            className="ghostButton actionButton wide"
            disabled
            aria-label={uninstallAvailability.reason ?? "需系统卸载"}
          >
            <Trash2 size={16} />
            <span>需系统卸载</span>
          </button>
        ) : (
          <button
            type="button"
            className="ghostButton actionButton wide"
            onClick={onUninstall}
            disabled={!uninstallAvailability.enabled}
            aria-label={uninstallAvailability.reason ?? "卸载"}
          >
            <Trash2 size={16} />
            <span>卸载</span>
          </button>
        )}
      </div>
    </aside>
  );
}

function formatPublishedAt(value?: string) {
  if (!value) {
    return "发布时间未知";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return `发布于 ${date.toLocaleString("zh-CN")}`;
}

function StatusIcon({ status }: { status: InboxItem["status"] }) {
  if (status === "current") {
    return <CheckCircle2 className="statusIcon current" size={18} />;
  }
  if (status === "needsChoice") {
    return <ShieldAlert className="statusIcon needsChoice" size={18} />;
  }
  return <RefreshCw className="statusIcon updateAvailable" size={18} />;
}

function statusLabel(status: InboxItem["status"]) {
  switch (status) {
    case "updateAvailable":
      return "建议更新";
    case "needsChoice":
      return "需确认";
    case "failed":
      return "失败";
    case "current":
      return "最新";
  }
}

function filterLabel(status: InboxFilter) {
  switch (status) {
    case "all":
      return "全部";
    case "updateAvailable":
      return "有更新";
    case "needsChoice":
      return "需确认";
    case "failed":
      return "失败";
    case "current":
      return "最新";
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
    case "current":
      return <CircleCheckBig size={15} />;
  }
}

function StatusGlyph({ status }: { status: InboxItem["status"] }) {
  switch (status) {
    case "updateAvailable":
      return <Download size={13} />;
    case "needsChoice":
      return <ShieldAlert size={13} />;
    case "failed":
      return <CircleAlert size={13} />;
    case "current":
      return <CircleCheckBig size={13} />;
  }
}

function RowActionGlyph({ status }: { status: InboxItem["status"] }) {
  switch (status) {
    case "updateAvailable":
      return <Download size={14} />;
    case "needsChoice":
      return <FolderOpen size={14} />;
    case "failed":
      return <RefreshCw size={14} />;
    case "current":
      return <ExternalLink size={14} />;
  }
}

function Tooltip({
  label,
  className,
  placement = "right",
  children
}: {
  label: string;
  className?: string;
  placement?: TooltipPlacement;
  children: ReactNode;
}) {
  return (
    <span className={className ? `tooltipWrap ${className}` : "tooltipWrap"} data-placement={placement} aria-label={label}>
      {children}
      <span className="tooltipBubble" role="tooltip">
        {label}
      </span>
    </span>
  );
}

function TooltipButton({
  label,
  className = "iconButton",
  children,
  onClick,
  disabled = false,
  type = "button",
  active = false,
  placement = "right"
}: {
  label: string;
  className?: string;
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  type?: "button" | "submit" | "reset";
  active?: boolean;
  placement?: TooltipPlacement;
}) {
  return (
    <Tooltip label={label} className="tooltipButton" placement={placement}>
      <button
        className={active ? `${className} active` : className}
        type={type}
        onClick={onClick}
        disabled={disabled}
        aria-label={label}
      >
        {children}
      </button>
    </Tooltip>
  );
}

type TooltipPlacement = "right" | "left" | "bottom";

function installTypeLabel(value: InstallPlan["install_type"]) {
  switch (value) {
    case "WindowsInstaller":
      return "Windows 安装包";
    case "PortableArchive":
      return "便携压缩包";
    case "AppImage":
      return "AppImage";
    case "LinuxPackage":
      return "Linux 安装包";
    case "Archive":
      return "归档包";
    case "Unknown":
      return "未知";
  }
}

function installPathKindLabel(value: ManagedApp["installPathKind"]) {
  switch (value) {
    case "ManagedPath":
      return "本地托管";
    case "SystemInstaller":
      return "系统安装器";
    case "Unknown":
    default:
      return "未知";
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

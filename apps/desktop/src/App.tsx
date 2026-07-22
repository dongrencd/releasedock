import { useEffect, useState, type ReactNode } from "react";
import {
  CheckCircle2,
  Clipboard,
  Download,
  ExternalLink,
  FolderOpen,
  Plus,
  RefreshCw,
  Search,
  Settings,
  ShieldAlert,
  Trash2
} from "lucide-react";
import {
  addRepo,
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
  filterManagedApps,
  inboxFilters,
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

  useEffect(() => {
    void refreshWorkspace();
  }, []);

  useEffect(() => {
    setPendingInstall(null);
  }, [selectedId]);

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

  async function handleOpenRelease(item: InboxItem | null) {
    if (!item?.releaseUrl) {
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
        <div className="brand">
          <div className="brandMark">GR</div>
          <div>
            <div className="brandName">GitHub Release</div>
            <div className="brandSub">Manager</div>
          </div>
        </div>

        <nav className="navList">
          <NavItem
            icon={<Download size={18} />}
            label="更新收件箱"
            active={activeView === "dashboard"}
            onClick={() => setActiveView("dashboard")}
          />
          <NavItem
            icon={<Settings size={18} />}
            label="设置"
            active={activeView === "settings"}
            onClick={() => setActiveView("settings")}
          />
        </nav>

        <div className="sourceTile">
          <ExternalLink size={18} />
          <div>
            <strong>当前项目</strong>
            <span>{DEFAULT_TRACKED_REPO_ID}</span>
          </div>
        </div>
      </aside>

      <main className="workspace">
        <header className="topbar">
          {activeView === "dashboard" ? (
            <>
              <div className="searchBox">
                <Search size={17} />
                <input
                  placeholder="搜索软件或仓库"
                  aria-label="搜索软件"
                  value={searchQuery}
                  onChange={(event) => setSearchQuery(event.target.value)}
                />
              </div>
              <div className="repoBox">
                <Search size={17} />
                <input
                  placeholder="owner/repo 或 GitHub URL"
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
              <button className="ghostButton" type="button" onClick={() => void refreshDashboard()} disabled={busy}>
                <RefreshCw size={17} />
                检查更新
              </button>
              <button className="primaryButton" type="button" onClick={() => void handleAddRepo()} disabled={busy}>
                <Plus size={17} />
                添加
              </button>
            </>
          ) : (
            <>
              <div className="settingsTopbarCopy">
                <strong>设置</strong>
                <span>统一保存 GitHub token、代理和安装根目录。</span>
              </div>
              <button className="ghostButton" type="button" onClick={() => void refreshConfig()} disabled={configSaving}>
                <RefreshCw size={17} />
                重新加载
              </button>
              <button className="primaryButton" type="button" onClick={() => void handleSaveConfig()} disabled={configSaving}>
                <Plus size={17} />
                保存设置
              </button>
            </>
          )}
        </header>

        {error ? <div className="errorBanner">{error}</div> : null}

        {activeView === "dashboard" ? (
          <section className="contentGrid">
            <section className="inboxPanel" aria-labelledby="inbox-title">
              <div className="sectionHeader">
                <div>
                  <h1 id="inbox-title">待处理更新</h1>
                  <p>加载本地清单，并根据 GitHub Release 实时刷新最新版本和 release note。</p>
                </div>
                <div className="updateCount">{inbox.filter((item) => item.status !== "current").length}</div>
              </div>

              <div className="filterRow" aria-label="状态筛选">
                {inboxFilters.map((item) => (
                  <button
                    key={item.id}
                    className={filter === item.id ? "filterPill active" : "filterPill"}
                    type="button"
                    onClick={() => setFilter(item.id)}
                    aria-pressed={filter === item.id}
                  >
                    {item.label}
                  </button>
                ))}
              </div>

              <div className="appTable" role="table" aria-label="更新列表">
                <div className="tableHead" role="row">
                  <span>应用</span>
                  <span>当前</span>
                  <span>最新</span>
                  <span>判断</span>
                  <span>操作</span>
                </div>
                {loading ? (
                  <div className="emptyState">正在同步 GitHub Release 数据...</div>
                ) : inbox.length === 0 ? (
                  <div className="emptyState">没有符合当前筛选条件的软件。</div>
                ) : (
                  inbox.map((item) => (
                    <InboxRow
                      key={item.id}
                      item={item}
                      selected={item.id === selected?.id}
                      onSelect={() => setSelectedId(item.id)}
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
        ) : (
          <section className="settingsPanel" aria-labelledby="settings-title">
              <div className="sectionHeader">
                <div>
                  <h1 id="settings-title">设置</h1>
                  <p>保存当前机器的 GitHub token、代理和默认安装根目录，CLI 和桌面端共用同一份配置。</p>
                </div>
              <div className="updateCount">{configSaving ? "…" : "3"}</div>
              </div>

            <div className="settingsForm">
              <label className="fieldRow">
                <span>GitHub Token</span>
                <input
                  value={configDraft.githubToken}
                  onChange={(event) => setConfigDraft((current) => ({ ...current, githubToken: event.target.value }))}
                  placeholder="ghp_..."
                  autoComplete="off"
                />
                <small>留空时使用环境变量或未认证请求。</small>
              </label>

              <label className="fieldRow">
                <span>代理地址</span>
                <input
                  value={configDraft.proxyUrl}
                  onChange={(event) => setConfigDraft((current) => ({ ...current, proxyUrl: event.target.value }))}
                  placeholder="http://127.0.0.1:7890"
                  autoComplete="off"
                />
                <small>留空时不启用代理。</small>
              </label>

              <label className="fieldRow wide">
                <span>安装根目录</span>
                <input
                  value={configDraft.installRoot}
                  onChange={(event) => setConfigDraft((current) => ({ ...current, installRoot: event.target.value }))}
                  placeholder="/data/ghrm"
                  autoComplete="off"
                />
                <small>留空时使用清单所在目录或默认数据目录。</small>
              </label>
            </div>

            <div className="settingsActions">
              <button className="ghostButton" type="button" onClick={() => void refreshConfig()} disabled={configSaving}>
                重新载入
              </button>
              <button className="primaryButton" type="button" onClick={() => void handleSaveConfig()} disabled={configSaving}>
                保存设置
              </button>
            </div>
          </section>
        )}

        <footer className="taskBar">
          <span className="taskLabel">{taskStatus}</span>
          <div className={busy ? "progressTrack busy" : "progressTrack"} aria-label="运行状态">
            <div className="progressValue" />
          </div>
          <button className="linkButton" type="button" onClick={() => void refreshDashboard()} disabled={busy}>
            重新检查
          </button>
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
    <button className={active ? "navItem active" : "navItem"} type="button" onClick={onClick}>
      {icon}
      <span>{label}</span>
    </button>
  );
}

function InboxRow({
  item,
  selected,
  onSelect
}: {
  item: InboxItem;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button className={selected ? "tableRow selected" : "tableRow"} type="button" onClick={onSelect}>
      <span className="appName">
        <StatusIcon status={item.status} />
        <span>
          <strong>{item.name}</strong>
          <small>{item.id}</small>
        </span>
      </span>
      <span className="mono">{item.currentVersion}</span>
      <span className="mono">{item.latestVersion}</span>
      <span className={`statusBadge ${item.status}`}>{statusLabel(item.status)}</span>
      <span className="rowAction">{item.actionLabel}</span>
    </button>
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
        <button className="iconButton" type="button" aria-label="打开 GitHub" onClick={onOpenRelease} disabled={!item.releaseUrl}>
          <FolderOpen size={18} />
        </button>
      </div>

      <div className="inspectorBlock accent">
        <div className="blockTitle">
          <ExternalLink size={16} />
          Release 信息
        </div>
        <p>{item.releaseTitle ?? item.latestVersion}</p>
        <p className="mutedText">{formatPublishedAt(item.publishedAt)}</p>
      </div>

      <div className="releaseNoteBlock">
        <div className="releaseNoteHeader">
          <div className="blockTitle">Release note</div>
          <button className="copyButton" type="button" onClick={() => onCopyReleaseNote(item.releaseNote)} disabled={!item.releaseNote}>
            <Clipboard size={15} />
            复制
          </button>
        </div>
        <pre>{item.releaseNote?.trim() || "这个 release 没有填写 release note。"}</pre>
      </div>

      <dl className="detailList">
        <div>
          <dt>资产文件</dt>
          <dd className="mono">{item.assetName ?? "暂无可用资产"}</dd>
        </div>
        <div>
          <dt>安装路径</dt>
          <dd className="mono">{item.installPath}</dd>
        </div>
        <div>
          <dt>安装类型</dt>
          <dd>{installTypeLabel(item.installType ?? "Unknown")}</dd>
        </div>
        <div>
          <dt>记录类型</dt>
          <dd>{installPathKindLabel(item.installPathKind ?? "Unknown")}</dd>
        </div>
        <div>
          <dt>卸载能力</dt>
          <dd>{item.uninstallSupported === false ? "需系统卸载" : "可自动卸载"}</dd>
        </div>
        <div>
          <dt>来源</dt>
          <dd>{item.source}</dd>
        </div>
      </dl>

      {pendingInstall ? (
        <div className="installPreview">
          <div className="blockTitle">
            <Download size={16} />
            安装预览
          </div>
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
            <button className="ghostButton" type="button" onClick={onCancelInstall} disabled={busy}>
              取消
            </button>
            <button className="primaryButton" type="button" onClick={onConfirmInstall} disabled={busy}>
              <Download size={17} />
              确认安装
            </button>
          </div>
        </div>
      ) : null}

      <button className="ghostButton wide" type="button" onClick={onOpenRelease} disabled={!item.releaseUrl || busy}>
        <ExternalLink size={17} />
        打开 Release 页面
      </button>
      <button className="primaryButton wide" type="button" onClick={onPrimaryAction} disabled={busy}>
        <Download size={17} />
        {item.actionLabel}
      </button>
      {item.status === "needsChoice" ? (
        <button className="ghostButton wide" type="button" onClick={onRemoveTracked} disabled={busy}>
          <Trash2 size={17} />
          移除跟踪
        </button>
      ) : item.uninstallSupported === false ? (
        <button className="ghostButton wide" type="button" disabled>
          <Trash2 size={17} />
          需系统卸载
        </button>
      ) : (
        <button className="ghostButton wide" type="button" onClick={onUninstall} disabled={busy}>
          <Trash2 size={17} />
          卸载
        </button>
      )}
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

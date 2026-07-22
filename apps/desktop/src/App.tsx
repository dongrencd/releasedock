import {
  Activity,
  CheckCircle2,
  Clipboard,
  Download,
  ExternalLink,
  FolderOpen,
  Library,
  Plus,
  RefreshCw,
  Search,
  Settings,
  ShieldAlert
} from "lucide-react";
import { buildUpdateInbox, demoApps, type InboxItem } from "./appModel";

const inbox = buildUpdateInbox(demoApps);
const selected = inbox[0];

export function App() {
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
          <NavItem icon={<Download size={18} />} label="更新收件箱" active />
          <NavItem icon={<Library size={18} />} label="软件库" />
          <NavItem icon={<Search size={18} />} label="发现" />
          <NavItem icon={<Activity size={18} />} label="活动" />
          <NavItem icon={<Settings size={18} />} label="设置" />
        </nav>

        <div className="sourceTile">
          <ExternalLink size={18} />
          <div>
            <strong>Release note</strong>
            <span>更新前查看 GitHub Release 原文</span>
          </div>
        </div>
      </aside>

      <main className="workspace">
        <header className="topbar">
          <div className="searchBox">
            <Search size={17} />
            <input placeholder="搜索软件或粘贴 GitHub 仓库 URL" aria-label="搜索软件或仓库" />
          </div>
          <button className="ghostButton" type="button">
            <RefreshCw size={17} />
            检查更新
          </button>
          <button className="primaryButton" type="button">
            <Plus size={17} />
            添加
          </button>
        </header>

        <section className="contentGrid">
          <section className="inboxPanel" aria-labelledby="inbox-title">
            <div className="sectionHeader">
              <div>
                <h1 id="inbox-title">待处理更新</h1>
                <p>只显示需要判断的软件，减少逐个打开 Release 页面的时间。</p>
              </div>
              <div className="updateCount">{inbox.filter((item) => item.status !== "current").length}</div>
            </div>

            <div className="filterRow" aria-label="状态筛选">
              <button className="filterPill active" type="button">全部</button>
              <button className="filterPill" type="button">有更新</button>
              <button className="filterPill" type="button">需确认</button>
              <button className="filterPill" type="button">失败</button>
            </div>

            <div className="appTable" role="table" aria-label="更新列表">
              <div className="tableHead" role="row">
                <span>应用</span>
                <span>当前</span>
                <span>最新</span>
                <span>判断</span>
                <span>操作</span>
              </div>
              {inbox.map((item) => (
                <InboxRow key={item.id} item={item} selected={item.id === selected.id} />
              ))}
            </div>
          </section>

          <Inspector item={selected} />
        </section>

        <footer className="taskBar">
          <span className="taskLabel">下载 mifi/lossless-cut v3.65.0</span>
          <div className="progressTrack" aria-label="下载进度">
            <div className="progressValue" />
          </div>
          <button className="linkButton" type="button">取消</button>
        </footer>
      </main>
    </div>
  );
}

function NavItem({ icon, label, active = false }: { icon: React.ReactNode; label: string; active?: boolean }) {
  return (
    <button className={active ? "navItem active" : "navItem"} type="button">
      {icon}
      <span>{label}</span>
    </button>
  );
}

function InboxRow({ item, selected }: { item: InboxItem; selected: boolean }) {
  return (
    <button className={selected ? "tableRow selected" : "tableRow"} type="button">
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

function Inspector({ item }: { item: InboxItem }) {
  return (
    <aside className="inspector" aria-label="详情检查器">
      <div className="inspectorHead">
        <div>
          <h2>{item.name}</h2>
          <p className="mono">{item.currentVersion} → {item.latestVersion}</p>
        </div>
        <button className="iconButton" type="button" aria-label="打开 GitHub">
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
          <button className="copyButton" type="button" onClick={() => copyReleaseNote(item.releaseNote)}>
            <Clipboard size={15} />
            复制
          </button>
        </div>
        <pre>{item.releaseNote?.trim() || "这个 release 没有填写 release note。"}</pre>
      </div>

      <dl className="detailList">
        <div>
          <dt>资产文件</dt>
          <dd className="mono">{item.assetName}</dd>
        </div>
        <div>
          <dt>安装路径</dt>
          <dd className="mono">{item.installPath}</dd>
        </div>
        <div>
          <dt>来源</dt>
          <dd>{item.source}</dd>
        </div>
      </dl>

      <button className="ghostButton wide" type="button">
        <ExternalLink size={17} />
        打开 Release 页面
      </button>
      <button className="primaryButton wide" type="button">
        <Download size={17} />
        {item.actionLabel}
      </button>
    </aside>
  );
}

function copyReleaseNote(releaseNote?: string) {
  if (!releaseNote || !navigator.clipboard) {
    return;
  }
  void navigator.clipboard.writeText(releaseNote);
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

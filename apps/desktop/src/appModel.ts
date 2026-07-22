export type AppStatus = "updateAvailable" | "current" | "needsChoice" | "failed";

export type ManagedApp = {
  id: string;
  name: string;
  currentVersion: string;
  latestVersion: string;
  status: AppStatus;
  source?: string;
  releaseTitle?: string;
  releaseNote?: string;
  releaseUrl?: string;
  publishedAt?: string;
  assetName?: string;
  installPath?: string;
};

export type InboxItem = ManagedApp & {
  actionLabel: "更新" | "查看" | "打开" | "重试";
  priority: number;
};

export const demoApps: ManagedApp[] = [
  {
    id: "mifi/lossless-cut",
    name: "LosslessCut",
    currentVersion: "v3.64.0",
    latestVersion: "v3.65.0",
    status: "updateAvailable",
    source: "GitHub",
    releaseTitle: "Stable release",
    releaseNote:
      "Fix crash and improve startup.\n\n- Windows startup is faster\n- Portable archive layout is unchanged\n- Export progress no longer freezes on long videos",
    releaseUrl: "https://github.com/mifi/lossless-cut/releases/tag/v3.65.0",
    publishedAt: "2026-07-21T10:20:30Z",
    assetName: "LosslessCut-win-x64.7z",
    installPath: "%LOCALAPPDATA%\\Programs\\ghrm\\LosslessCut"
  },
  {
    id: "zyedidia/micro",
    name: "micro",
    currentVersion: "v2.0.14",
    latestVersion: "v2.0.14",
    status: "current",
    source: "GitHub",
    releaseTitle: "micro v2.0.14",
    releaseNote: "This release does not include a release note.",
    releaseUrl: "https://github.com/zyedidia/micro/releases/tag/v2.0.14",
    publishedAt: "2026-07-16T08:00:00Z",
    assetName: "micro-2.0.14-win64.zip",
    installPath: "%LOCALAPPDATA%\\Programs\\ghrm\\micro"
  },
  {
    id: "rustdesk/rustdesk",
    name: "RustDesk",
    currentVersion: "v1.4.1",
    latestVersion: "v1.4.2",
    status: "needsChoice",
    source: "GitHub",
    releaseTitle: "RustDesk 1.4.2",
    releaseNote:
      "Release contains multiple Windows assets.\n\nChoose the installer when you want Start Menu integration. Choose portable when you want files managed only by GitHub Release Manager.",
    releaseUrl: "https://github.com/rustdesk/rustdesk/releases/tag/1.4.2",
    publishedAt: "2026-07-20T12:30:00Z",
    assetName: "多个 Windows 资产候选",
    installPath: "%LOCALAPPDATA%\\Programs\\ghrm\\RustDesk"
  }
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

export type Language = "en" | "zh-CN";

export type InboxFilter = "all" | "updateAvailable" | "needsChoice" | "failed";

type Copy = {
  appName: string;
  appSubtitle: string;
  navUpdates: string;
  navSettings: string;
  updatesEyebrow: string;
  settingsEyebrow: string;
  updatesTitle: string;
  settingsTitle: string;
  managedAppsTitle: string;
  managedAppsCount: (count: number) => string;
  managedAppsPending: (count: number) => string;
  configReady: string;
  configPublic: string;
  checkUpdates: string;
  addRepoEyebrow: string;
  addRepoTitle: string;
  addRepoPlaceholder: string;
  addRepoButton: string;
  searchPlaceholder: string;
  filterPrefix: string;
  all: string;
  updateAvailable: string;
  needsChoice: string;
  noRelease: string;
  failed: string;
  current: string;
  selectAll: string;
  clearSelection: string;
  remove: string;
  loadingDashboard: string;
  noApps: string;
  noMatch: string;
  statusBar: string;
  processing: string;
  releaseInfo: string;
  releaseNote: string;
  copyReleaseNote: string;
  copy: string;
  assetFile: string;
  noAssetAvailable: string;
  installerFile: string;
  installPath: string;
  uninstallAbility: string;
  installPreview: string;
  installPreviewConfirmation: string;
  cancel: string;
  confirmInstall: string;
  openRelease: string;
  openInstallLocation: string;
  openInstallerFile: string;
  openSystemUninstall: string;
  removeTracked: string;
  noSelection: string;
  settingsTitleSmall: string;
  installRoot: string;
  installRootHelp: string;
  usingDefaultInstallRoot: string;
  restoreDefault: string;
  openInstallRoot: string;
  githubToken: string;
  githubTokenHelp: string;
  proxyUrl: string;
  proxyUrlHelp: string;
  language: string;
  languageEnglish: string;
  languageChinese: string;
  showToken: string;
  hideToken: string;
  saveSettings: string;
  reloadSettings: string;
  currentStatusLoading: string;
  currentStatusLoaded: (count: number) => string;
  currentStatusEmpty: string;
  addRepoSuccess: (repo: string) => string;
  addRepoFailed: string;
  saveSettingsSuccess: string;
  saveSettingsFailed: string;
  task: {
    install: string;
    uninstall: string;
  };
  stage: {
    preparing: string;
    downloading: string;
    copyingAsset: string;
    extractingArchive: string;
    runningSystemInstaller: string;
    updatingManifest: string;
    locatingRecord: string;
    removingFiles: string;
    finished: string;
    failed: string;
  };
  action: {
    update: string;
    install: string;
    retry: string;
    open: string;
  };
  status: {
    updateAvailable: string;
    current: string;
    needsChoice: string;
    noRelease: string;
    failed: string;
  };
  installType: {
    WindowsInstaller: string;
    PortableArchive: string;
    AppImage: string;
    LinuxPackage: string;
    Archive: string;
    Unknown: string;
  };
  installPathKind: {
    ManagedPath: string;
    SystemInstaller: string;
    Unknown: string;
  };
  model: {
    busy: string;
    noRelease: string;
    selectApp: string;
    selectAssetBeforeUninstall: string;
    useSystemUninstall: string;
    onlyUntracked: string;
    selectAtLeastOne: string;
    skippedCount: (count: number) => string;
    searchHint: string;
  };
  notes: {
    noReleaseNote: string;
    windowsInstaller: string;
    linuxPackage: string;
  };
};

const copy: Record<Language, Copy> = {
  en: {
    appName: "ReleaseDock",
    appSubtitle: "Release manager",
    navUpdates: "Updates",
    navSettings: "Settings",
    updatesEyebrow: "Updates",
    settingsEyebrow: "Settings",
    updatesTitle: "Managed apps",
    settingsTitle: "Local settings",
    managedAppsTitle: "Managed apps",
    managedAppsCount: (count) => `${count} apps`,
    managedAppsPending: (count) => `${count} need attention`,
    configReady: "Token configured",
    configPublic: "Public repos ready",
    checkUpdates: "Check updates",
    addRepoEyebrow: "Add repository",
    addRepoTitle: "Enter owner/repo or GitHub URL",
    addRepoPlaceholder: "owner/repo",
    addRepoButton: "Add repository",
    searchPlaceholder: "Search apps, repos, versions",
    filterPrefix: "Filter:",
    all: "All",
    updateAvailable: "Updates",
    needsChoice: "Action needed",
    noRelease: "No release",
    failed: "Failed",
    current: "Latest",
    selectAll: "Select all",
    clearSelection: "Clear selection",
    remove: "Remove",
    loadingDashboard: "Loading GitHub Release data",
    noApps: "No managed apps yet. Add a GitHub repository above.",
    noMatch: "No matching apps. This search only filters the local list.",
    statusBar: "Status",
    processing: "Processing",
    releaseInfo: "Release info",
    releaseNote: "Release note",
    copyReleaseNote: "Copy release note",
    copy: "Copy",
    assetFile: "Asset file",
    noAssetAvailable: "No asset available",
    installerFile: "Installer file",
    installPath: "Install path",
    uninstallAbility: "Uninstall",
    installPreview: "Install preview",
    installPreviewConfirmation: "This installer needs confirmation before it runs.",
    cancel: "Cancel",
    confirmInstall: "Confirm install",
    openRelease: "Open release",
    openInstallLocation: "Open install location",
    openInstallerFile: "Open installer file",
    openSystemUninstall: "Open system uninstall",
    removeTracked: "Remove tracking",
    noSelection: "No app selected",
    settingsTitleSmall: "4 local settings",
    installRoot: "Install root",
    installRootHelp: "Downloaded installers and managed apps live under this root.",
    usingDefaultInstallRoot: "Using default install root",
    restoreDefault: "Restore default",
    openInstallRoot: "Open folder",
    githubToken: "GitHub token",
    githubTokenHelp: "Public repos work without a token. Private repos and frequent checks should use one.",
    proxyUrl: "Proxy URL",
    proxyUrlHelp: "Affects GitHub requests only.",
    language: "Language",
    languageEnglish: "English",
    languageChinese: "简体中文",
    showToken: "Show token",
    hideToken: "Hide token",
    saveSettings: "Save settings",
    reloadSettings: "Reload settings",
    currentStatusLoading: "Loading GitHub Release data",
    currentStatusLoaded: (count) => `Loaded ${count} apps`,
    currentStatusEmpty: "No managed apps yet",
    addRepoSuccess: (repo) => `Added ${repo}`,
    addRepoFailed: "Add repository failed",
    saveSettingsSuccess: "Settings saved",
    saveSettingsFailed: "Save settings failed",
    task: {
      install: "Install",
      uninstall: "Uninstall"
    },
    stage: {
      preparing: "Preparing",
      downloading: "Downloading",
      copyingAsset: "Copying asset",
      extractingArchive: "Extracting archive",
      runningSystemInstaller: "Running system installer",
      updatingManifest: "Writing record",
      locatingRecord: "Locating record",
      removingFiles: "Removing files",
      finished: "Finished",
      failed: "Failed"
    },
    action: {
      update: "Update",
      install: "Install",
      retry: "Retry",
      open: "Open"
    },
    status: {
      updateAvailable: "Update available",
      current: "Up to date",
      needsChoice: "Action needed",
      noRelease: "No release",
      failed: "Failed"
    },
    installType: {
      WindowsInstaller: "Windows installer",
      PortableArchive: "Portable archive",
      AppImage: "AppImage",
      LinuxPackage: "Linux package",
      Archive: "Archive",
      Unknown: "Unknown"
    },
    installPathKind: {
      ManagedPath: "Managed path",
      SystemInstaller: "System installer",
      Unknown: "Unknown"
    },
    model: {
      busy: "A task is already running",
      noRelease: "No release link available",
      selectApp: "Select an app first",
      selectAssetBeforeUninstall: "Pick an asset before uninstalling",
      useSystemUninstall: "Use system uninstall",
      onlyUntracked: "Only uninstalled tracked items can be removed",
      selectAtLeastOne: "Select at least one uninstalled tracked item",
      skippedCount: (count) => `Skipping ${count} non-removable item(s)`,
      searchHint: "Local filtering only; this does not search GitHub"
    },
    notes: {
      noReleaseNote: "This release does not include a release note.",
      windowsInstaller:
        "Windows .exe/.msi installers are downloaded first and must be confirmed before execution.",
      linuxPackage:
        "Linux .deb/.rpm packages are downloaded first and must be confirmed before system installation."
    }
  },
  "zh-CN": {
    appName: "ReleaseDock",
    appSubtitle: "Release 管理台",
    navUpdates: "更新管理",
    navSettings: "设置",
    updatesEyebrow: "更新管理",
    settingsEyebrow: "设置",
    updatesTitle: "已管理软件",
    settingsTitle: "本地配置",
    managedAppsTitle: "已管理软件",
    managedAppsCount: (count) => `${count} 项`,
    managedAppsPending: (count) => `${count} 个需处理`,
    configReady: "已配置 token",
    configPublic: "公开仓库可用",
    checkUpdates: "检查更新",
    addRepoEyebrow: "添加仓库",
    addRepoTitle: "输入 owner/repo 或 GitHub URL",
    addRepoPlaceholder: "owner/repo",
    addRepoButton: "添加仓库",
    searchPlaceholder: "搜索软件、仓库、版本",
    filterPrefix: "筛选：",
    all: "全部",
    updateAvailable: "有更新",
    needsChoice: "待处理",
    noRelease: "无 release",
    failed: "失败",
    current: "最新",
    selectAll: "全选",
    clearSelection: "清空",
    remove: "移除",
    loadingDashboard: "正在加载 GitHub Release 数据",
    noApps: "还没有添加软件。先在上方输入 GitHub 仓库。",
    noMatch: "没有匹配的软件。筛选只会查找本地列表，不会搜索 GitHub 全网。",
    statusBar: "状态",
    processing: "处理中",
    releaseInfo: "Release 信息",
    releaseNote: "Release note",
    copyReleaseNote: "复制 release note",
    copy: "复制",
    assetFile: "资产文件",
    noAssetAvailable: "暂无资产文件",
    installerFile: "安装包保存位置",
    installPath: "安装路径",
    uninstallAbility: "卸载能力",
    installPreview: "安装预览",
    installPreviewConfirmation: "这个安装包需要在系统权限确认后继续执行。",
    cancel: "取消",
    confirmInstall: "确认安装",
    openRelease: "打开 Release",
    openInstallLocation: "打开安装目录",
    openInstallerFile: "打开安装包位置",
    openSystemUninstall: "打开系统卸载",
    removeTracked: "移除跟踪",
    noSelection: "暂无可展示的软件",
    settingsTitleSmall: "4 个本地配置项",
    installRoot: "软件安装位置",
    installRootHelp: "下载缓存和自动管理的软件会放在这个位置下的 `apps` 目录中。",
    usingDefaultInstallRoot: "使用默认安装目录",
    restoreDefault: "恢复默认",
    openInstallRoot: "打开目录",
    githubToken: "GitHub Token",
    githubTokenHelp: "公开仓库可以不填；私有仓库或频繁检查更新时建议填写。",
    proxyUrl: "代理地址",
    proxyUrlHelp: "只影响 GitHub 请求。",
    language: "界面语言",
    languageEnglish: "English",
    languageChinese: "简体中文",
    showToken: "显示 token",
    hideToken: "隐藏 token",
    saveSettings: "保存设置",
    reloadSettings: "重新载入",
    currentStatusLoading: "正在加载 GitHub Release 数据",
    currentStatusLoaded: (count) => `已加载 ${count} 个软件`,
    currentStatusEmpty: "当前没有管理的软件",
    addRepoSuccess: (repo) => `已添加 ${repo}`,
    addRepoFailed: "添加失败",
    saveSettingsSuccess: "设置已保存",
    saveSettingsFailed: "保存设置失败",
    task: {
      install: "安装任务",
      uninstall: "卸载任务"
    },
    stage: {
      preparing: "准备中",
      downloading: "下载中",
      copyingAsset: "复制文件",
      extractingArchive: "解压文件",
      runningSystemInstaller: "执行安装器",
      updatingManifest: "写入记录",
      locatingRecord: "查找记录",
      removingFiles: "移除文件",
      finished: "已完成",
      failed: "已失败"
    },
    action: {
      update: "更新",
      install: "安装",
      retry: "重试",
      open: "打开"
    },
    status: {
      updateAvailable: "建议更新",
      current: "最新",
      needsChoice: "待处理",
      noRelease: "无 release",
      failed: "失败"
    },
    installType: {
      WindowsInstaller: "Windows 安装包",
      PortableArchive: "便携压缩包",
      AppImage: "AppImage",
      LinuxPackage: "Linux 安装包",
      Archive: "归档包",
      Unknown: "未知"
    },
    installPathKind: {
      ManagedPath: "本地托管",
      SystemInstaller: "系统安装器",
      Unknown: "未知"
    },
    model: {
      busy: "当前有任务在执行",
      noRelease: "当前没有可打开的 Release 链接",
      selectApp: "请先选择一个软件",
      selectAssetBeforeUninstall: "先选择资产后才能卸载",
      useSystemUninstall: "需使用系统卸载",
      onlyUntracked: "只有未安装的跟踪项可以移除",
      selectAtLeastOne: "选择至少一个未安装的跟踪项",
      skippedCount: (count) => `将跳过 ${count} 个不可移除项`,
      searchHint: "这是本地筛选，不是 GitHub 全网搜索"
    },
    notes: {
      noReleaseNote: "这个 release 没有填写 release note。",
      windowsInstaller: "Windows .exe/.msi 安装包会先下载，确认后才会执行。",
      linuxPackage: "Linux .deb/.rpm 安装包会先下载，确认后才会执行系统安装。"
    }
  }
};

export function normalizeLanguage(value?: string | null): Language {
  return value === "zh-CN" ? "zh-CN" : "en";
}

export function createUiText(language: Language) {
  return copy[language];
}

export function languageOptions(language: Language) {
  const ui = createUiText(language);
  return [
    { value: "en" as const, label: ui.languageEnglish },
    { value: "zh-CN" as const, label: ui.languageChinese }
  ];
}

export function isWindowsPlatform() {
  if (typeof navigator === "undefined") {
    return false;
  }

  return /win/i.test(navigator.platform);
}

export function formatPublishedAt(value: string | undefined, language: Language) {
  if (!value) {
    return language === "zh-CN" ? "发布时间未知" : "Published time unknown";
  }

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return language === "zh-CN"
    ? `发布于 ${date.toLocaleString("zh-CN")}`
    : `Published ${date.toLocaleString("en-US")}`;
}

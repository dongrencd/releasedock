export type Language = "en" | "zh-CN";

export type InboxFilter = "all" | "updateAvailable" | "actionRequired" | "failed";

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
  systemPackage: string;
  systemPackageManager: string;
  installPath: string;
  defaultInstallPath: string;
  installManagement: string;
  recentActivity: string;
  activityHistory: string;
  activitySucceeded: string;
  activityFailed: string;
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
  backgroundCheck: string;
  backgroundCheckEnabled: string;
  backgroundCheckDisabled: string;
  backgroundCheckHelp: string;
  checkInterval: string;
  checkIntervalUnit: string;
  checkIntervalHelp: string;
  trayBadge: (count: number) => string;
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
    openApp: string;
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
    Executable: string;
    Archive: string;
    Unknown: string;
  };
  installPathKind: {
    ManagedPath: string;
    SystemInstaller: string;
    Unknown: string;
  };
  managementKind: {
    managedLocal: string;
    systemPackage: string;
    externalInstaller: string;
  };
  model: {
    busy: string;
    noRelease: string;
    selectApp: string;
    noInstallableAsset: string;
    selectAssetBeforeUninstall: string;
    useSystemUninstall: string;
    noLaunchTarget: string;
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
    systemPackage: "System package",
    systemPackageManager: "Package manager",
    installPath: "Install path",
    defaultInstallPath: "Default install path",
    installManagement: "Management",
    recentActivity: "Recent activity",
    activityHistory: "Activity history",
    activitySucceeded: "Succeeded",
    activityFailed: "Failed",
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
    backgroundCheck: "Background check",
    backgroundCheckEnabled: "Enabled",
    backgroundCheckDisabled: "Disabled",
    backgroundCheckHelp: "Periodically check GitHub for new releases while the app runs in the tray.",
    checkInterval: "Check interval",
    checkIntervalUnit: "minutes",
    checkIntervalHelp: "Time between background update checks. Default is 30 minutes.",
    trayBadge: (count) => `${count} updates available`,
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
      openApp: "Open app",
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
      Executable: "Executable",
      Archive: "Archive",
      Unknown: "Unknown"
    },
    installPathKind: {
      ManagedPath: "Managed path",
      SystemInstaller: "System installer",
      Unknown: "Unknown"
    },
    managementKind: {
      managedLocal: "Managed locally",
      systemPackage: "Managed by system package manager",
      externalInstaller: "External installer"
    },
    model: {
      busy: "A task is already running",
      noRelease: "No release link available",
      selectApp: "Select an app first",
      noInstallableAsset: "No installable asset for this platform",
      selectAssetBeforeUninstall: "Pick an asset before uninstalling",
      useSystemUninstall: "Use system uninstall",
      noLaunchTarget: "No launch target found",
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
    systemPackage: "系统包",
    systemPackageManager: "包管理器",
    installPath: "安装路径",
    defaultInstallPath: "默认安装路径",
    installManagement: "管理方式",
    recentActivity: "最近操作",
    activityHistory: "操作历史",
    activitySucceeded: "已完成",
    activityFailed: "失败",
    uninstallAbility: "卸载",
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
    backgroundCheck: "后台检查",
    backgroundCheckEnabled: "已启用",
    backgroundCheckDisabled: "已关闭",
    backgroundCheckHelp: "应用驻留托盘时定时检查 GitHub 是否有新 release。",
    checkInterval: "检查间隔",
    checkIntervalUnit: "分钟",
    checkIntervalHelp: "后台检查更新的时间间隔，默认 30 分钟。",
    trayBadge: (count) => `${count} 个有更新`,
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
      openApp: "打开软件",
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
      Executable: "可执行文件",
      Archive: "归档包",
      Unknown: "未知"
    },
    installPathKind: {
      ManagedPath: "本地托管",
      SystemInstaller: "系统安装器",
      Unknown: "未知"
    },
    managementKind: {
      managedLocal: "本地托管",
      systemPackage: "由系统包管理器托管",
      externalInstaller: "外部安装器"
    },
    model: {
      busy: "当前有任务在执行",
      noRelease: "当前没有可打开的 Release 链接",
      selectApp: "请先选择一个软件",
      noInstallableAsset: "当前平台没有可安装资产",
      selectAssetBeforeUninstall: "先选择资产后才能卸载",
      useSystemUninstall: "需使用系统卸载",
      noLaunchTarget: "未找到可启动目标",
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

function localizedText(language: Language, en: string, zh: string) {
  return language === "zh-CN" ? zh : en;
}

function localizedTemplate(language: Language, en: string, zh: string) {
  return localizedText(language, en, zh);
}

// 任务状态文案集中到这里，避免页面里到处散落硬编码英文。
export function createTaskStatusText(language: Language) {
  const ui = createUiText(language);
  return {
    loadingDashboard: ui.loadingDashboard,
    checkingLatestRelease: localizedText(language, "Checking latest release", "正在检查最新 release"),
    checkingLatestReleaseProgress: (completed: number, total: number) =>
      localizedTemplate(
        language,
        `Checking latest release (${completed}/${total})`,
        `正在检查最新 release（${completed}/${total}）`
      ),
    loadedApps: ui.currentStatusLoaded,
    noApps: ui.currentStatusEmpty,
    refreshFailed: localizedText(language, "Failed to refresh updates", "刷新更新失败"),
    failedToLoadSettings: localizedText(language, "Failed to load settings", "加载设置失败"),
    enterRepo: localizedText(language, "Enter owner/repo or a GitHub URL", "请输入 owner/repo 或 GitHub URL"),
    addRepoFailed: ui.addRepoFailed,
    addingRepo: (repo: string) => localizedTemplate(language, `Adding ${repo}`, `正在添加 ${repo}`),
    addedRepo: ui.addRepoSuccess,
    autoSavingSettings: localizedText(language, "Auto-saving settings", "自动保存设置"),
    savingSettings: localizedText(language, "Saving settings", "正在保存设置"),
    settingsSaved: ui.saveSettingsSuccess,
    failedToSaveSettings: ui.saveSettingsFailed,
    generatingInstallPreview: (name: string) =>
      localizedTemplate(language, `Generating install preview for ${name}`, `正在为 ${name} 生成安装预览`),
    generatedInstallPreview: (name: string) =>
      localizedTemplate(language, `Generated install preview for ${name}`, `已生成 ${name} 的安装预览`),
    failedToBuildInstallPreview: localizedText(language, "Failed to build install preview", "生成安装预览失败"),
    installing: (name: string) => localizedTemplate(language, `Installing ${name}`, `正在安装 ${name}`),
    preparingInstall: (name: string) =>
      localizedTemplate(language, `Preparing to install ${name}`, `正在准备安装 ${name}`),
    finishedInstalling: (name: string) =>
      localizedTemplate(language, `Finished installing ${name}`, `已完成安装 ${name}`),
    installedOrUpdated: (name: string) =>
      localizedTemplate(language, `Installed or updated ${name}`, `已安装或更新 ${name}`),
    installFailed: localizedText(language, "Install failed", "安装失败"),
    uninstalling: (name: string) => localizedTemplate(language, `Uninstalling ${name}`, `正在卸载 ${name}`),
    finishedUninstalling: (name: string) =>
      localizedTemplate(language, `Finished uninstalling ${name}`, `已完成卸载 ${name}`),
    uninstalled: (name: string) => localizedTemplate(language, `Uninstalled ${name}`, `已卸载 ${name}`),
    uninstallFailed: localizedText(language, "Uninstall failed", "卸载失败"),
    stoppedTracking: (name: string) => localizedTemplate(language, `Stopped tracking ${name}`, `已停止跟踪 ${name}`),
    removeTrackingFailed: localizedText(language, "Remove tracking failed", "移除跟踪失败"),
    selectAtLeastOneRemovableItem: localizedText(language, "Select at least one removable item", "请选择至少一个可移除项"),
    selectAtLeastOneUninstalledTrackedItem: localizedText(
      language,
      "Select at least one uninstalled tracked item",
      "请选择至少一个未安装的跟踪项"
    ),
    removingTracked: (count: number) =>
      localizedTemplate(language, `Removing ${count} tracked item(s)`, `正在移除 ${count} 个跟踪项`),
    removedTracked: (removedCount: number, totalCount: number) =>
      removedCount < totalCount
        ? localizedTemplate(
            language,
            `Removed ${removedCount} tracked item(s), ${totalCount - removedCount} expired`,
            `已移除 ${removedCount} 个跟踪项，${totalCount - removedCount} 个已失效`
          )
        : localizedTemplate(language, `Removed ${removedCount} tracked item(s)`, `已移除 ${removedCount} 个跟踪项`),
    bulkRemoveFailed: localizedText(language, "Bulk remove failed", "批量移除失败"),
    noReleaseLinkAvailable: ui.model.noRelease,
    openedReleasePage: (name: string) => localizedTemplate(language, `Opened ${name} release page`, `已打开 ${name} 的 release 页面`),
    openedApp: (name: string) => localizedTemplate(language, `Opened ${name}`, `已打开 ${name}`),
    openFailed: localizedText(language, "Open failed", "打开失败"),
    noInstallPathAvailable: localizedText(language, "No install path available", "没有可用的安装路径"),
    openedInstallerFile: (name: string) => localizedTemplate(language, `Opened ${name} installer file`, `已打开 ${name} 的安装包文件`),
    openedInstallLocation: (name: string) => localizedTemplate(language, `Opened ${name} install location`, `已打开 ${name} 的安装目录`),
    openFolderFailed: localizedText(language, "Open folder failed", "打开目录失败"),
    noInstallRootSelected: localizedText(language, "No install root selected", "没有选择安装根目录"),
    openedInstallRoot: localizedText(language, "Opened install root", "已打开安装根目录"),
    releaseNoteCopied: localizedText(language, "Release note copied", "已复制 release note")
  };
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

export function formatRecordedAt(value: string | undefined, language: Language) {
  if (!value) {
    return language === "zh-CN" ? "记录时间未知" : "Recorded time unknown";
  }

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return language === "zh-CN"
    ? `记录于 ${date.toLocaleString("zh-CN")}`
    : `Recorded ${date.toLocaleString("en-US")}`;
}

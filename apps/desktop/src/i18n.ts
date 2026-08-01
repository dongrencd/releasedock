export type Language = "en" | "zh-CN";
export type ThemeMode = "system" | "light" | "dark";

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
  searchGitHub: string;
  discoveryTitle: string;
  discoveryEmpty: string;
  discoveryInstallable: string;
  discoveryNoInstallableAsset: string;
  discoveryNoRelease: string;
  discoveryStars: (count: number) => string;
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
  selectionNone: string;
  selectionCount: (count: number) => string;
  mixedSelection: string;
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
  copyValue: string;
  releaseGuidanceTitle: string;
  releaseGuidanceDockerTitle: string;
  releaseGuidanceDockerSummary: string;
  releaseGuidanceSourceTitle: string;
  releaseGuidanceSourceSummary: string;
  releaseGuidanceManualTitle: string;
  releaseGuidanceManualSummary: string;
  releaseGuidanceScopeNote: string;
  releaseGuidanceOpenRelease: string;
  releaseGuidanceManualFallback: string;
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
  releaseLifecycle: string;
  installedState: string;
  installableState: string;
  releaseTarget: string;
  releaseChannel: string;
  releaseChannelStable: string;
  releaseChannelPrerelease: string;
  loadingVersions: string;
  noVersions: string;
  versionListUnavailable: string;
  versionListUnavailableHelp: string;
  previewSelectedVersion: string;
  pinSelectedVersion: string;
  ignoreSelectedVersion: string;
  unignoreSelectedVersion: string;
  rollbackTo: (version: string) => string;
  confirmRollback: string;
  rollbackManagedOnly: string;
  noRollbackSnapshot: string;
  integritySource: string;
  noIntegritySource: string;
  pendingSha256Verification: string;
  integrityStatusLabel: string;
  releaseDirectionLabel: string;
  downgradeAvailable: string;
  installPreviewConfirmation: string;
  installPreviewNoChecksumHint: string;
  installPreviewSystemConfirmationHint: string;
  installRetryHint: string;
  cancel: string;
  confirmInstall: string;
  retryInstall: string;
  confirmUninstall: string;
  uninstallManagedConfirmation: string;
  uninstallLinuxPackageConfirmation: string;
  uninstallExternalInstallerConfirmation: string;
  openRelease: string;
  openInstallLocation: string;
  openInstallerFile: string;
  openInstallerFolder: string;
  refreshSystemInstallDetection: string;
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
  proxyUrlPlaceholder: string;
  proxyUrlHelp: string;
  proxyConfigured: string;
  proxyNotConfigured: string;
  githubTokenConfigured: string;
  githubTokenNotConfigured: string;
  githubTokenOptional: string;
  configProxyWarning: string;
  configProxyWarningHelp: string;
  configTokenWarning: string;
  configTokenWarningHelp: string;
  configUnknownConnectivityWarning: string;
  configUnknownConnectivityWarningHelp: string;
  openNetworkSettings: string;
  networkConfigHealth: string;
  networkConfigHealthHelp: string;
  networkProxyFormat: string;
  testGithubConnectivity: string;
  connectivityTestIdle: string;
  connectivityTestTesting: string;
  connectivityTestSuccess: string;
  connectivityTestFailed: string;
  connectivityTestStale: string;
  connectivityTestHelp: string;
  connectivityTestSuccessHelp: string;
  connectivityTestFailedHelp: string;
  connectivityTestStaleHelp: string;
  connectivityNetworkFailureHelp: string;
  connectivityProxyFailureHelp: string;
  connectivityRateLimitHelp: string;
  connectivityAuthFailureHelp: string;
  connectivityUnknownFailureHelp: string;
  language: string;
  languageEnglish: string;
  languageChinese: string;
  theme: string;
  themeSystem: string;
  themeLight: string;
  themeDark: string;
  showToken: string;
  hideToken: string;
  saveSettings: string;
  autostart: string;
  autostartEnabled: string;
  autostartDisabled: string;
  autostartHelp: string;
  notificationPermission: string;
  notificationPermissionGranted: string;
  notificationPermissionDenied: string;
  notificationPermissionPrompt: string;
  requestNotificationPermission: string;
  openNotificationSettings: string;
  notificationPermissionHelp: string;
  autostartSaveFailed: string;
  backgroundCheck: string;
  backgroundCheckEnabled: string;
  backgroundCheckDisabled: string;
  backgroundCheckHelp: string;
  backgroundCheckPartial: string;
  backgroundCheckPartialDetail: (count: number) => string;
  backgroundCheckFailed: string;
  backgroundCheckFailedDetail: (count: number) => string;
  checkInterval: string;
  checkIntervalUnit: string;
  checkIntervalHelp: string;
  downloadAcceleration: string;
  downloadAccelerationEnabled: string;
  downloadAccelerationDisabled: string;
  downloadAccelerationHelp: string;
  downloadMaxConnections: string;
  downloadConnectionsUnit: string;
  downloadMaxConnectionsHelp: string;
  trayBadge: (count: number) => string;
  currentStatusLoading: string;
  currentStatusLoaded: (count: number) => string;
  currentStatusLocal: (count: number) => string;
  currentStatusEmpty: string;
  addRepoSuccess: (repo: string) => string;
  addRepoFailed: string;
  saveSettingsSuccess: string;
  saveSettingsFailed: string;
  task: {
    install: string;
    rollback: string;
    uninstall: string;
  };
  stage: {
    preparing: string;
    downloading: string;
    copyingAsset: string;
    verifyingArtifact: string;
    extractingArchive: string;
    creatingRollback: string;
    restoringRollback: string;
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
    downgradeAvailable: string;
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
  releaseDirection: {
    upgrade: string;
    downgrade: string;
    reinstall: string;
    unknown: string;
  };
  integrityStatus: {
    verifiedChecksum: string;
    recordedOnly: string;
  };
  model: {
    busy: string;
    noRelease: string;
    selectApp: string;
    noInstallableAsset: string;
    selectAssetBeforeUninstall: string;
    noLaunchTarget: string;
    onlyUntracked: string;
    selectAtLeastOne: string;
    selectInstalledSeparately: string;
    installManagementKindChange: string;
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
    searchGitHub: "Search GitHub",
    discoveryTitle: "GitHub candidates",
    discoveryEmpty: "No matching GitHub repositories found.",
    discoveryInstallable: "Installable asset",
    discoveryNoInstallableAsset: "No current-platform asset",
    discoveryNoRelease: "No published release",
    discoveryStars: (count) => `${count} stars`,
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
    selectionNone: "No selection",
    selectionCount: (count) => `${count} selected`,
    mixedSelection: "Mixed selection",
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
    copyValue: "Copy value",
    releaseGuidanceTitle: "Non-managed install guidance",
    releaseGuidanceDockerTitle: "Docker release",
    releaseGuidanceDockerSummary: "ReleaseDock cannot install this release on the current platform. The release notes mention Docker or Compose, so open the release page and follow those instructions there.",
    releaseGuidanceSourceTitle: "Source build release",
    releaseGuidanceSourceSummary: "ReleaseDock cannot install this release on the current platform. The release notes mention source build steps, so open the release page and follow those instructions there.",
    releaseGuidanceManualTitle: "Manual install release",
    releaseGuidanceManualSummary: "ReleaseDock cannot install this release on the current platform. Open the release page to continue with the published instructions.",
    releaseGuidanceScopeNote: "This guidance is based only on the release title and notes.",
    releaseGuidanceOpenRelease: "Open the release page to read the full instructions.",
    releaseGuidanceManualFallback: "Check for a manual download, a source build path, or a container image.",
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
    releaseLifecycle: "Version strategy",
    installedState: "Installed",
    installableState: "Installable",
    releaseTarget: "Target version",
    releaseChannel: "Channel",
    releaseChannelStable: "Stable",
    releaseChannelPrerelease: "Prerelease",
    loadingVersions: "Loading versions",
    noVersions: "No published versions",
    versionListUnavailable: "Version list unavailable",
    versionListUnavailableHelp: "Check GitHub network or token settings, then retry.",
    previewSelectedVersion: "Preview install",
    pinSelectedVersion: "Pin selected version",
    ignoreSelectedVersion: "Ignore selected version",
    unignoreSelectedVersion: "Stop ignoring selected version",
    rollbackTo: (version) => `Rollback to ${version}`,
    confirmRollback: "Confirm rollback",
    rollbackManagedOnly: "Rollback is available only for managed installs.",
    noRollbackSnapshot: "No rollback snapshot is available.",
    integritySource: "Integrity source",
    noIntegritySource: "No upstream checksum",
    pendingSha256Verification: "Pending SHA-256 verification",
    integrityStatusLabel: "Integrity status",
    releaseDirectionLabel: "Direction",
    downgradeAvailable: "Downgrade available",
    installPreviewConfirmation: "This installer needs confirmation before it runs.",
    installPreviewNoChecksumHint: "No upstream checksum. Confirm the file source before installing.",
    installPreviewSystemConfirmationHint: "Requires system permission confirmation.",
    installRetryHint: "The last install failed. Review the error above and retry from the same preview.",
    cancel: "Cancel",
    confirmInstall: "Confirm install",
    retryInstall: "Retry install",
    confirmUninstall: "Confirm uninstall",
    uninstallManagedConfirmation: "This will remove ReleaseDock's managed install directory and rollback snapshot. It will not scan external user data directories.",
    uninstallLinuxPackageConfirmation: "This will remove the package through the system package manager and then remove the ReleaseDock record.",
    uninstallExternalInstallerConfirmation: "This will open the system uninstall path for this app record.",
    openRelease: "Open release",
    openInstallLocation: "Open install location",
    openInstallerFile: "Run installer",
    openInstallerFolder: "Open installer folder",
    refreshSystemInstallDetection: "Re-detect install",
    removeTracked: "Remove tracking",
    noSelection: "No app selected",
    settingsTitleSmall: "8 local settings",
    installRoot: "Install root",
    installRootHelp: "Downloaded installers and managed apps live under this root.",
    usingDefaultInstallRoot: "Using default install root",
    restoreDefault: "Restore default",
    openInstallRoot: "Open folder",
    githubToken: "GitHub token",
    githubTokenHelp: "Public repos work without a token. Private repos and frequent checks should use one.",
    proxyUrl: "GitHub proxy URL",
    proxyUrlPlaceholder: "http://proxy.example.com:port",
    proxyUrlHelp: "Format: http://host:port or https://host:port. Affects GitHub queries and Release asset downloads only.",
    proxyConfigured: "Proxy configured",
    proxyNotConfigured: "Proxy not configured",
    githubTokenConfigured: "Token configured",
    githubTokenNotConfigured: "Token not configured",
    githubTokenOptional: "Token optional",
    configProxyWarning: "GitHub connection issue",
    configProxyWarningHelp: "GitHub is not reachable with the current network path. Configure or check the GitHub proxy first.",
    configTokenWarning: "GitHub API limited",
    configTokenWarningHelp: "GitHub is reachable, but API access is limited by rate limit or authentication. Configure a token for private repos or frequent checks.",
    configUnknownConnectivityWarning: "Check GitHub connection",
    configUnknownConnectivityWarningHelp: "GitHub returned an unexpected response. Review the connection test details and retry.",
    openNetworkSettings: "Open network settings",
    networkConfigHealth: "Network configuration",
    networkConfigHealthHelp: "Proxy affects GitHub reachability and Release downloads. Token is optional for public repos and useful for private repos or rate limits.",
    networkProxyFormat: "Proxy format: http://proxy.example.com:port. Use your own proxy host and port.",
    testGithubConnectivity: "Test GitHub connection",
    connectivityTestIdle: "Not tested",
    connectivityTestTesting: "Testing",
    connectivityTestSuccess: "Connection OK",
    connectivityTestFailed: "Connection failed",
    connectivityTestStale: "Configuration changed",
    connectivityTestHelp: "Use the current proxy and optional token settings to test whether the GitHub API is reachable.",
    connectivityTestSuccessHelp: "GitHub API is reachable. Public repositories do not need a token.",
    connectivityTestFailedHelp: "GitHub API is not reachable with the current settings.",
    connectivityTestStaleHelp: "Token or proxy changed. Test the GitHub connection again.",
    connectivityNetworkFailureHelp: "Check direct GitHub access first. If GitHub is blocked, configure a GitHub proxy.",
    connectivityProxyFailureHelp: "Check the GitHub proxy format and whether the proxy service is reachable.",
    connectivityRateLimitHelp: "GitHub is reachable, but the API is rate-limited. Configure a token for frequent checks.",
    connectivityAuthFailureHelp: "GitHub is reachable, but authentication failed. Check the token for private repos or API access.",
    connectivityUnknownFailureHelp: "Review the GitHub error details and retry the connection test.",
    language: "Language",
    languageEnglish: "English",
    languageChinese: "简体中文",
    theme: "Theme",
    themeSystem: "Follow system",
    themeLight: "Light",
    themeDark: "Dark",
    showToken: "Show token",
    hideToken: "Hide token",
    saveSettings: "Save settings",
    autostart: "Start with Windows",
    autostartEnabled: "Enabled",
    autostartDisabled: "Disabled",
    autostartHelp: "Start ReleaseDock in the background after sign-in. It only runs update checks; it never installs updates automatically.",
    notificationPermission: "Notifications",
    notificationPermissionGranted: "Allowed",
    notificationPermissionDenied: "Blocked",
    notificationPermissionPrompt: "Not requested",
    requestNotificationPermission: "Allow notifications",
    openNotificationSettings: "Open notification settings",
    notificationPermissionHelp: "Background update notifications need OS notification permission. The top-bar badge still works when notifications are blocked.",
    autostartSaveFailed: "Start-with-Windows setting failed",
    backgroundCheck: "Background check",
    backgroundCheckEnabled: "Enabled",
    backgroundCheckDisabled: "Disabled",
    backgroundCheckHelp: "Periodically check GitHub for new releases while the app runs in the tray.",
    backgroundCheckPartial: "Background check partially failed",
    backgroundCheckPartialDetail: (count) => `${count} repository checks failed. The last successful result is still shown.`,
    backgroundCheckFailed: "Background check failed",
    backgroundCheckFailedDetail: (count) =>
      count > 0
        ? `All ${count} repository checks failed. Review GitHub network or token settings.`
        : "The background check could not start. Review GitHub network or token settings.",
    checkInterval: "Check interval",
    checkIntervalUnit: "minutes",
    checkIntervalHelp: "Time between background update checks. Default is 30 minutes.",
    downloadAcceleration: "Download acceleration",
    downloadAccelerationEnabled: "Enabled",
    downloadAccelerationDisabled: "Disabled",
    downloadAccelerationHelp: "Use multiple HTTP Range connections for large Release assets when supported; otherwise use the single-connection resume path.",
    downloadMaxConnections: "Max connections",
    downloadConnectionsUnit: "connections",
    downloadMaxConnectionsHelp: "Default is 4. Values are limited to 1-8.",
    trayBadge: (count) => `${count} updates available`,
    currentStatusLoading: "Loading GitHub Release data",
    currentStatusLoaded: (count) => `Loaded ${count} apps`,
    currentStatusLocal: (count) => `Showing ${count} local records`,
    currentStatusEmpty: "No managed apps yet",
    addRepoSuccess: (repo) => `Added ${repo}`,
    addRepoFailed: "Add repository failed",
    saveSettingsSuccess: "Settings saved",
    saveSettingsFailed: "Save settings failed",
    task: {
      install: "Install",
      rollback: "Rollback",
      uninstall: "Uninstall"
    },
    stage: {
      preparing: "Preparing",
      downloading: "Downloading",
      copyingAsset: "Copying asset",
      verifyingArtifact: "Verifying artifact",
      extractingArchive: "Extracting archive",
      creatingRollback: "Creating rollback snapshot",
      restoringRollback: "Restoring rollback",
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
      downgradeAvailable: "Downgrade available",
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
    releaseDirection: {
      upgrade: "Upgrade",
      downgrade: "Downgrade",
      reinstall: "Reinstall",
      unknown: "Unknown"
    },
    integrityStatus: {
      verifiedChecksum: "Verified checksum",
      recordedOnly: "Unverified; digest recorded only"
    },
    model: {
      busy: "A task is already running",
      noRelease: "No release link available",
      selectApp: "Select an app first",
      noInstallableAsset: "No installable asset for this platform",
      selectAssetBeforeUninstall: "Pick an asset before uninstalling",
      noLaunchTarget: "No launch target found",
      onlyUntracked: "Only uninstalled tracked items can be removed",
      selectAtLeastOne: "Select at least one uninstalled tracked item",
      selectInstalledSeparately: "Select installed apps separately before uninstalling",
      installManagementKindChange: "This record still looks like an external installer. Remove the old record, then install the new executable again.",
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
    searchGitHub: "搜索 GitHub",
    discoveryTitle: "GitHub 候选仓库",
    discoveryEmpty: "没有找到匹配的 GitHub 仓库。",
    discoveryInstallable: "有可安装资产",
    discoveryNoInstallableAsset: "无当前平台资产",
    discoveryNoRelease: "没有已发布 release",
    discoveryStars: (count) => `${count} stars`,
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
    selectionNone: "未选择",
    selectionCount: (count) => `已选 ${count} 项`,
    mixedSelection: "混合选择",
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
    copyValue: "复制值",
    releaseGuidanceTitle: "非托管安装提示",
    releaseGuidanceDockerTitle: "Docker 发布",
    releaseGuidanceDockerSummary: "ReleaseDock 无法在当前平台直接安装这个 release。说明里提到了 Docker 或 Compose，请打开 release 页面并按其中的说明继续。",
    releaseGuidanceSourceTitle: "源码编译发布",
    releaseGuidanceSourceSummary: "ReleaseDock 无法在当前平台直接安装这个 release。说明里提到了源码编译步骤，请打开 release 页面并按其中的说明继续。",
    releaseGuidanceManualTitle: "手动安装发布",
    releaseGuidanceManualSummary: "ReleaseDock 无法在当前平台直接安装这个 release。请打开 release 页面继续查看发布者给出的说明。",
    releaseGuidanceScopeNote: "这里只根据 release 标题和说明判断，不会额外读取仓库文件。",
    releaseGuidanceOpenRelease: "打开 release 页面查看完整说明。",
    releaseGuidanceManualFallback: "再查看是否提供了手动下载、源码编译或容器镜像。",
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
    releaseLifecycle: "版本策略",
    installedState: "已安装",
    installableState: "可安装",
    releaseTarget: "目标版本",
    releaseChannel: "版本通道",
    releaseChannelStable: "稳定版",
    releaseChannelPrerelease: "预发布版",
    loadingVersions: "正在加载版本",
    noVersions: "没有已发布版本",
    versionListUnavailable: "版本列表暂时不可用",
    versionListUnavailableHelp: "请检查 GitHub 网络或 Token 配置后重试。",
    previewSelectedVersion: "预览安装",
    pinSelectedVersion: "固定所选版本",
    ignoreSelectedVersion: "忽略所选版本",
    unignoreSelectedVersion: "取消忽略所选版本",
    rollbackTo: (version) => `回滚到 ${version}`,
    confirmRollback: "确认回滚",
    rollbackManagedOnly: "只有本地托管安装支持回滚。",
    noRollbackSnapshot: "当前没有可用的回滚快照。",
    integritySource: "完整性来源",
    noIntegritySource: "无上游校验值",
    pendingSha256Verification: "待 SHA-256 验证",
    integrityStatusLabel: "完整性状态",
    releaseDirectionLabel: "版本方向",
    downgradeAvailable: "可降级",
    installPreviewConfirmation: "这个安装包需要在系统权限确认后继续执行。",
    installPreviewNoChecksumHint: "无上游校验值，安装前请确认文件来源。",
    installPreviewSystemConfirmationHint: "需要系统权限确认。",
    installRetryHint: "上一次安装失败。查看上面的错误后，可以直接在这里重试。",
    cancel: "取消",
    confirmInstall: "确认安装",
    retryInstall: "重试安装",
    confirmUninstall: "确认卸载",
    uninstallManagedConfirmation: "这会删除 ReleaseDock 托管的安装目录和回滚快照，不会主动扫描外部用户数据目录。",
    uninstallLinuxPackageConfirmation: "这会通过系统包管理器移除软件包，然后删除 ReleaseDock 记录。",
    uninstallExternalInstallerConfirmation: "这会打开这个软件记录对应的系统卸载入口。",
    openRelease: "打开 Release",
    openInstallLocation: "打开安装目录",
    openInstallerFile: "执行安装包",
    openInstallerFolder: "打开安装包目录",
    refreshSystemInstallDetection: "重新检测安装状态",
    removeTracked: "移除跟踪",
    noSelection: "暂无可展示的软件",
    settingsTitleSmall: "8 个本地配置项",
    installRoot: "软件安装位置",
    installRootHelp: "下载缓存和自动管理的软件会放在这个位置下的 `apps` 目录中。",
    usingDefaultInstallRoot: "使用默认安装目录",
    restoreDefault: "恢复默认",
    openInstallRoot: "打开目录",
    githubToken: "GitHub Token",
    githubTokenHelp: "公开仓库可以不填；私有仓库或频繁检查更新时建议填写。",
    proxyUrl: "GitHub 代理地址",
    proxyUrlPlaceholder: "http://proxy.example.com:port",
    proxyUrlHelp: "格式：http://主机:端口 或 https://主机:端口。只影响 GitHub 查询和 Release 资产下载。",
    proxyConfigured: "已配置代理",
    proxyNotConfigured: "代理未配置",
    githubTokenConfigured: "Token 已配置",
    githubTokenNotConfigured: "Token 未配置",
    githubTokenOptional: "Token 可选",
    configProxyWarning: "GitHub 连接异常，检查代理",
    configProxyWarningHelp: "当前网络路径无法访问 GitHub。请先配置或检查 GitHub 代理。",
    configTokenWarning: "GitHub API 受限，配置 Token",
    configTokenWarningHelp: "GitHub 可访问，但 API 受到限流或认证限制。私有仓库或频繁检查更新时请配置 Token。",
    configUnknownConnectivityWarning: "检查 GitHub 连接",
    configUnknownConnectivityWarningHelp: "GitHub 返回了未预期的响应，请查看连接测试详情后重试。",
    openNetworkSettings: "前往网络配置",
    networkConfigHealth: "网络配置健康",
    networkConfigHealthHelp: "代理影响 GitHub 连通性和 Release 下载；公开仓库无需 Token，私有仓库或限流时再配置。",
    networkProxyFormat: "代理格式：http://proxy.example.com:port。请替换为自己的代理主机和端口。",
    testGithubConnectivity: "测试 GitHub 连接",
    connectivityTestIdle: "尚未测试",
    connectivityTestTesting: "正在测试",
    connectivityTestSuccess: "连接正常",
    connectivityTestFailed: "连接失败",
    connectivityTestStale: "配置已更改",
    connectivityTestHelp: "使用当前代理和可选 Token 设置测试 GitHub API 是否可访问。",
    connectivityTestSuccessHelp: "当前设置可以访问 GitHub API；公开仓库无需 Token。",
    connectivityTestFailedHelp: "当前设置无法访问 GitHub API。",
    connectivityTestStaleHelp: "Token 或代理已变化，请重新测试 GitHub 连接。",
    connectivityNetworkFailureHelp: "先检查是否能直连 GitHub；如果 GitHub 被阻断，请配置 GitHub 代理。",
    connectivityProxyFailureHelp: "检查 GitHub 代理格式以及代理服务是否可达。",
    connectivityRateLimitHelp: "GitHub 可访问，但 API 已限流。频繁检查更新时请配置 Token。",
    connectivityAuthFailureHelp: "GitHub 可访问，但认证失败。私有仓库或 API 访问请检查 Token。",
    connectivityUnknownFailureHelp: "查看 GitHub 错误详情并重试连接测试。",
    language: "界面语言",
    languageEnglish: "English",
    languageChinese: "简体中文",
    theme: "主题",
    themeSystem: "跟随系统",
    themeLight: "浅色",
    themeDark: "深色",
    showToken: "显示 token",
    hideToken: "隐藏 token",
    saveSettings: "保存设置",
    autostart: "开机后后台启动",
    autostartEnabled: "已启用",
    autostartDisabled: "已关闭",
    autostartHelp: "登录系统后在后台启动 ReleaseDock，只做更新检查，不会自动安装更新。",
    notificationPermission: "系统通知",
    notificationPermissionGranted: "已允许",
    notificationPermissionDenied: "已阻止",
    notificationPermissionPrompt: "未请求",
    requestNotificationPermission: "允许通知",
    openNotificationSettings: "打开系统通知设置",
    notificationPermissionHelp: "后台更新提醒需要系统通知权限。通知被阻止时，顶部更新数量仍会显示。",
    autostartSaveFailed: "开机启动设置失败",
    backgroundCheck: "后台检查",
    backgroundCheckEnabled: "已启用",
    backgroundCheckDisabled: "已关闭",
    backgroundCheckHelp: "应用驻留托盘时定时检查 GitHub 是否有新 release。",
    backgroundCheckPartial: "后台检查部分失败",
    backgroundCheckPartialDetail: (count) => `${count} 个仓库检查失败，已保留上次成功结果。`,
    backgroundCheckFailed: "后台检查失败",
    backgroundCheckFailedDetail: (count) =>
      count > 0 ? `${count} 个仓库检查全部失败，请检查 GitHub 网络或 Token 设置。` : "后台检查无法启动，请检查 GitHub 网络或 Token 设置。",
    checkInterval: "检查间隔",
    checkIntervalUnit: "分钟",
    checkIntervalHelp: "后台检查更新的时间间隔，默认 30 分钟。",
    downloadAcceleration: "下载加速",
    downloadAccelerationEnabled: "已启用",
    downloadAccelerationDisabled: "已关闭",
    downloadAccelerationHelp: "服务器支持时，大型 Release 资产会使用多个 HTTP Range 连接下载；否则使用单连接断点续传。",
    downloadMaxConnections: "最大连接数",
    downloadConnectionsUnit: "连接",
    downloadMaxConnectionsHelp: "默认 4，取值限制为 1-8。",
    trayBadge: (count) => `${count} 个有更新`,
    currentStatusLoading: "正在加载 GitHub Release 数据",
    currentStatusLoaded: (count) => `已加载 ${count} 个软件`,
    currentStatusLocal: (count) => `正在显示 ${count} 条本地记录`,
    currentStatusEmpty: "当前没有管理的软件",
    addRepoSuccess: (repo) => `已添加 ${repo}`,
    addRepoFailed: "添加失败",
    saveSettingsSuccess: "设置已保存",
    saveSettingsFailed: "保存设置失败",
    task: {
      install: "安装任务",
      rollback: "回滚任务",
      uninstall: "卸载任务"
    },
    stage: {
      preparing: "准备中",
      downloading: "下载中",
      copyingAsset: "复制文件",
      verifyingArtifact: "校验安装文件",
      extractingArchive: "解压文件",
      creatingRollback: "创建回滚快照",
      restoringRollback: "恢复回滚快照",
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
      downgradeAvailable: "可降级",
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
    releaseDirection: {
      upgrade: "升级",
      downgrade: "降级",
      reinstall: "重新安装",
      unknown: "未知"
    },
    integrityStatus: {
      verifiedChecksum: "已验证校验值",
      recordedOnly: "未验证；仅记录摘要"
    },
    model: {
      busy: "当前有任务在执行",
      noRelease: "当前没有可打开的 Release 链接",
      selectApp: "请先选择一个软件",
      noInstallableAsset: "当前平台没有可安装资产",
      selectAssetBeforeUninstall: "先选择资产后才能卸载",
      noLaunchTarget: "未找到可启动目标",
      onlyUntracked: "只有未安装的跟踪项可以移除",
      selectAtLeastOne: "选择至少一个未安装的跟踪项",
      selectInstalledSeparately: "请单独选择已安装软件后再卸载",
      installManagementKindChange: "这条记录仍像外部安装器。请先移除旧记录，再用新的可执行文件重新安装。",
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

export function normalizeThemeMode(value?: string | null): ThemeMode {
  return value === "light" || value === "dark" ? value : "system";
}

export function resolveEffectiveThemeMode(themeMode: ThemeMode, prefersDark: boolean): "light" | "dark" {
  if (themeMode === "light") {
    return "light";
  }

  if (themeMode === "dark") {
    return "dark";
  }

  return prefersDark ? "dark" : "light";
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
    currentStatusLocal: ui.currentStatusLocal,
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
    autostartSaveFailed: ui.autostartSaveFailed,
    requestingNotificationPermission: localizedText(language, "Requesting notification permission", "正在请求通知权限"),
    notificationPermissionUpdated: localizedText(language, "Notification permission updated", "通知权限已更新"),
    notificationPermissionFailed: localizedText(language, "Notification permission request failed", "通知权限请求失败"),
    openedNotificationSettings: localizedText(language, "Opened notification settings", "已打开系统通知设置"),
    openNotificationSettingsFailed: localizedText(language, "Failed to open notification settings", "打开系统通知设置失败"),
    testingGithubConnectivity: localizedText(language, "Testing GitHub connection", "正在测试 GitHub 连接"),
    githubConnectivitySucceeded: localizedText(language, "GitHub connection test passed", "GitHub 连接测试通过"),
    githubConnectivityFailed: localizedText(language, "GitHub connection test failed", "GitHub 连接测试失败"),
    generatingInstallPreview: (name: string) =>
      localizedTemplate(language, `Generating install preview for ${name}`, `正在为 ${name} 生成安装预览`),
    generatedInstallPreview: (name: string) =>
      localizedTemplate(language, `Generated install preview for ${name}`, `已生成 ${name} 的安装预览`),
    failedToBuildInstallPreview: localizedText(language, "Failed to build install preview", "生成安装预览失败"),
    loadingReleaseVersions: ui.loadingVersions,
    releasePolicyUpdated: localizedText(language, "Release policy updated", "Release 策略已更新"),
    releasePolicyFailed: localizedText(language, "Failed to update release policy", "更新 Release 策略失败"),
    preparingRollback: (name: string) =>
      localizedTemplate(language, `Preparing rollback for ${name}`, `正在为 ${name} 准备回滚`),
    rollingBack: (name: string) => localizedTemplate(language, `Rolling back ${name}`, `正在回滚 ${name}`),
    rolledBack: (name: string) => localizedTemplate(language, `Rolled back ${name}`, `已回滚 ${name}`),
    rollbackFailed: localizedText(language, "Rollback failed", "回滚失败"),
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
    openedInstallerFile: (name: string) => localizedTemplate(language, `Opened ${name} installer`, `已打开 ${name} 的安装包`),
    openedInstallerFolder: (name: string) => localizedTemplate(language, `Opened ${name} installer folder`, `已打开 ${name} 的安装包目录`),
    openedInstallLocation: (name: string) => localizedTemplate(language, `Opened ${name} install location`, `已打开 ${name} 的安装目录`),
    detectingSystemInstall: (name: string) =>
      localizedTemplate(language, `Re-detecting ${name} system install`, `正在重新检测 ${name} 的系统安装状态`),
    detectedSystemInstall: (name: string) =>
      localizedTemplate(language, `Refreshed ${name} install detection`, `已刷新 ${name} 的安装检测结果`),
    systemInstallDetectionFailed: localizedText(language, "Install detection failed", "安装检测失败"),
    openedSystemUninstall: (name: string) =>
      localizedTemplate(
        language,
        `Opened system uninstall for ${name}. Finish uninstalling there, then refresh ReleaseDock.`,
        `已打开 ${name} 的系统卸载入口。请在系统工具中完成卸载，然后刷新 ReleaseDock。`
      ),
    openFolderFailed: localizedText(language, "Open folder failed", "打开目录失败"),
    noInstallRootSelected: localizedText(language, "No install root selected", "没有选择安装根目录"),
    openedInstallRoot: localizedText(language, "Opened install root", "已打开安装根目录"),
    releaseNoteCopied: localizedText(language, "Release note copied", "已复制 release note"),
    copiedValue: (label: string) => localizedTemplate(language, `Copied ${label}`, `已复制${label}`)
  };
}

export function languageOptions(language: Language) {
  const ui = createUiText(language);
  return [
    { value: "en" as const, label: ui.languageEnglish },
    { value: "zh-CN" as const, label: ui.languageChinese }
  ];
}

export function themeModeOptions(language: Language) {
  const ui = createUiText(language);
  return [
    { value: "system" as const, label: ui.themeSystem },
    { value: "light" as const, label: ui.themeLight },
    { value: "dark" as const, label: ui.themeDark }
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

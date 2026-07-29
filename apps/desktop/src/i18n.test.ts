import { describe, expect, it } from "vitest";
import { createTaskStatusText, createUiText, normalizeThemeMode, resolveEffectiveThemeMode, themeModeOptions } from "./i18n";

describe("createTaskStatusText", () => {
  it("localizes dashboard and action status strings", () => {
    const zh = createTaskStatusText("zh-CN");

    expect(zh.loadingDashboard).toBe("正在加载 GitHub Release 数据");
    expect(zh.checkingLatestRelease).toBe("正在检查最新 release");
    expect(zh.checkingLatestReleaseProgress(1, 3)).toBe("正在检查最新 release（1/3）");
    expect(zh.addingRepo("owner/repo")).toBe("正在添加 owner/repo");
    expect(zh.bulkRemoveFailed).toBe("批量移除失败");
    expect(zh.openedInstallRoot).toBe("已打开安装根目录");
  });

  it("keeps English wording when requested", () => {
    const en = createTaskStatusText("en");

    expect(en.loadingDashboard).toBe("Loading GitHub Release data");
    expect(en.checkingLatestReleaseProgress(2, 5)).toBe("Checking latest release (2/5)");
    expect(en.installedOrUpdated("micro")).toBe("Installed or updated micro");
    expect(en.selectAtLeastOneUninstalledTrackedItem).toBe("Select at least one uninstalled tracked item");
    expect(en.copiedValue("Install path")).toBe("Copied Install path");
    expect(createUiText("en").model.selectInstalledSeparately).toBe("Select installed apps separately before uninstalling");
  });

  it("distinguishes opening installer files from opening installer folders", () => {
    const zh = createUiText("zh-CN");
    const en = createUiText("en");
    const zhTask = createTaskStatusText("zh-CN");

    expect(zh.openInstallerFile).toBe("执行安装包");
    expect(zh.openInstallerFolder).toBe("打开安装包目录");
    expect(zhTask.openedInstallerFile("ReleaseDock")).toContain("打开 ReleaseDock 的安装包");
    expect(zhTask.openedInstallerFolder("ReleaseDock")).toContain("打开 ReleaseDock 的安装包目录");
    expect(en.openInstallerFile).toBe("Run installer");
    expect(en.openInstallerFolder).toBe("Open installer folder");
  });

  it("labels background check state with real enablement wording", () => {
    const zh = createUiText("zh-CN");
    const en = createUiText("en");

    expect(zh.backgroundCheckEnabled).toBe("已启用");
    expect(zh.backgroundCheckDisabled).toBe("已关闭");
    expect(en.backgroundCheckEnabled).toBe("Enabled");
    expect(en.backgroundCheckDisabled).toBe("Disabled");
  });

  it("describes the GitHub proxy format without hard-coding a local address", () => {
    const zh = createUiText("zh-CN");
    const en = createUiText("en");

    expect(zh.proxyUrl).toBe("GitHub 代理地址");
    expect(en.proxyUrl).toBe("GitHub proxy URL");
    expect(zh.proxyUrlPlaceholder).toBe("http://proxy.example.com:port");
    expect(en.proxyUrlPlaceholder).toBe("http://proxy.example.com:port");
    expect(zh.proxyUrlHelp).toContain("GitHub 查询和 Release 资产下载");
    expect(en.proxyUrlHelp).toContain("GitHub queries and Release asset downloads");
    expect(zh.proxyUrlHelp).not.toMatch(/\b\d{1,3}(?:\.\d{1,3}){3}:\d+\b/);
    expect(en.proxyUrlHelp).not.toMatch(/\b\d{1,3}(?:\.\d{1,3}){3}:\d+\b/);
  });

  it("provides problem-specific network configuration warning copy", () => {
    const zh = createUiText("zh-CN");
    const en = createUiText("en");

    expect(zh.configProxyWarning).toBe("GitHub 连接异常，检查代理");
    expect(zh.configProxyWarningHelp).toContain("代理");
    expect(zh.configTokenWarning).toBe("GitHub API 受限，配置 Token");
    expect(zh.configTokenWarningHelp).toContain("限流");
    expect(en.configProxyWarning).toBe("GitHub connection issue");
    expect(en.configTokenWarning).toBe("GitHub API limited");
  });

  it("provides network guidance card and GitHub connectivity test copy", () => {
    const zh = createUiText("zh-CN");
    const en = createUiText("en");
    const zhTask = createTaskStatusText("zh-CN");
    const enTask = createTaskStatusText("en");

    expect(zh.networkConfigHealth).toBe("网络配置健康");
    expect(zh.networkConfigHealthHelp).toContain("公开仓库无需 Token");
    expect(zh.networkProxyFormat).toContain("http://proxy.example.com:port");
    expect(zh.networkProxyFormat).not.toMatch(/\b\d{1,3}(?:\.\d{1,3}){3}:\d+\b/);
    expect(zh.testGithubConnectivity).toBe("测试 GitHub 连接");
    expect(zh.connectivityTestIdle).toBe("尚未测试");
    expect(zh.connectivityTestTesting).toBe("正在测试");
    expect(zh.connectivityTestSuccess).toBe("连接正常");
    expect(zh.connectivityTestFailed).toBe("连接失败");
    expect(zh.connectivityTestStale).toBe("配置已更改");
    expect(zh.connectivityTestStaleHelp).toContain("重新测试");
    expect(zh.connectivityTestSuccessHelp).toContain("公开仓库无需 Token");
    expect(zh.connectivityNetworkFailureHelp).toContain("代理");
    expect(zh.connectivityRateLimitHelp).toContain("Token");
    expect(zh.openNetworkSettings).toBe("前往网络配置");
    expect(zhTask.testingGithubConnectivity).toBe("正在测试 GitHub 连接");
    expect(zhTask.githubConnectivitySucceeded).toBe("GitHub 连接测试通过");
    expect(zhTask.githubConnectivityFailed).toBe("GitHub 连接测试失败");

    expect(en.networkConfigHealth).toBe("Network configuration");
    expect(en.networkProxyFormat).toContain("http://proxy.example.com:port");
    expect(en.networkProxyFormat).not.toMatch(/\b\d{1,3}(?:\.\d{1,3}){3}:\d+\b/);
    expect(enTask.testingGithubConnectivity).toBe("Testing GitHub connection");
  });

  it("provides theme labels and normalizes unknown values to follow system", () => {
    const zh = createUiText("zh-CN");
    const en = createUiText("en");

    expect(zh.theme).toBe("主题");
    expect(zh.themeSystem).toBe("跟随系统");
    expect(zh.themeLight).toBe("浅色");
    expect(zh.themeDark).toBe("深色");
    expect(en.theme).toBe("Theme");
    expect(themeModeOptions("en").map((item) => item.value)).toEqual(["system", "light", "dark"]);
    expect(normalizeThemeMode(null)).toBe("system");
    expect(normalizeThemeMode("dark")).toBe("dark");
    expect(resolveEffectiveThemeMode("system", true)).toBe("dark");
    expect(resolveEffectiveThemeMode("light", true)).toBe("light");
  });

  it("counts the local settings summary after adding theme controls", () => {
    const zh = createUiText("zh-CN");
    const en = createUiText("en");

    expect(zh.settingsTitleSmall).toBe("5 个本地配置项");
    expect(en.settingsTitleSmall).toBe("5 local settings");
  });

  it("provides task labels for verification and rollback progress", () => {
    const zh = createUiText("zh-CN");
    const en = createUiText("en");

    expect(en.task.rollback).toBe("Rollback");
    expect(en.stage.verifyingArtifact).toBe("Verifying artifact");
    expect(en.stage.creatingRollback).toBe("Creating rollback snapshot");
    expect(en.stage.restoringRollback).toBe("Restoring rollback");
    expect(zh.task.rollback).toBe("回滚任务");
    expect(zh.stage.verifyingArtifact).toBe("校验安装文件");
    expect(zh.stage.creatingRollback).toBe("创建回滚快照");
    expect(zh.stage.restoringRollback).toBe("恢复回滚快照");
  });

  it("provides release lifecycle controls and preview copy", () => {
    const zh = createUiText("zh-CN");
    const en = createUiText("en");

    expect(en.releaseLifecycle).toBe("Version strategy");
    expect(en.installedState).toBe("Installed");
    expect(en.installableState).toBe("Installable");
    expect(en.selectionNone).toBe("No selection");
    expect(en.selectionCount(2)).toBe("2 selected");
    expect(en.mixedSelection).toBe("Mixed selection");
    expect(en.copyValue).toBe("Copy value");
    expect(en.releaseChannelStable).toBe("Stable");
    expect(en.releaseChannelPrerelease).toBe("Prerelease");
    expect(en.previewSelectedVersion).toBe("Preview install");
    expect(en.pinSelectedVersion).toBe("Pin selected version");
    expect(en.ignoreSelectedVersion).toBe("Ignore selected version");
    expect(en.pendingSha256Verification).toBe("Pending SHA-256 verification");
    expect(en.installPreviewNoChecksumHint).toBe("No upstream checksum. Confirm the file source before installing.");
    expect(en.installPreviewSystemConfirmationHint).toBe("Requires system permission confirmation.");
    expect(en.integrityStatus.recordedOnly).toBe("Unverified; digest recorded only");
    expect(en.rollbackTo("v1.0.0")).toBe("Rollback to v1.0.0");
    expect(zh.releaseLifecycle).toBe("版本策略");
    expect(zh.installedState).toBe("已安装");
    expect(zh.installableState).toBe("可安装");
    expect(zh.selectionNone).toBe("未选择");
    expect(zh.selectionCount(2)).toBe("已选 2 项");
    expect(zh.mixedSelection).toBe("混合选择");
    expect(zh.copyValue).toBe("复制值");
    expect(zh.releaseTarget).toBe("目标版本");
    expect(zh.previewSelectedVersion).toBe("预览安装");
    expect(zh.integritySource).toBe("完整性来源");
    expect(zh.pendingSha256Verification).toBe("待 SHA-256 验证");
    expect(zh.installPreviewNoChecksumHint).toBe("无上游校验值，安装前请确认文件来源。");
    expect(zh.installPreviewSystemConfirmationHint).toBe("需要系统权限确认。");
    expect(zh.integrityStatus.recordedOnly).toBe("未验证；仅记录摘要");
    expect(zh.confirmRollback).toBe("确认回滚");
  });

  it("provides release guidance copy for no-asset repos", () => {
    const zh = createUiText("zh-CN");
    const en = createUiText("en");

    expect(en.releaseGuidanceTitle).toBe("Non-managed install guidance");
    expect(en.releaseGuidanceDockerTitle).toBe("Docker release");
    expect(en.releaseGuidanceSourceTitle).toBe("Source build release");
    expect(en.releaseGuidanceManualTitle).toBe("Manual install release");
    expect(en.releaseGuidanceScopeNote).toContain("release title and notes");
    expect(zh.releaseGuidanceTitle).toBe("非托管安装提示");
    expect(zh.releaseGuidanceDockerSummary).toContain("ReleaseDock 无法在当前平台直接安装");
    expect(zh.releaseGuidanceSourceSummary).toContain("ReleaseDock 无法在当前平台直接安装");
    expect(zh.releaseGuidanceManualFallback).toContain("容器镜像");
  });

  it("describes uninstall confirmation without implying external user data deletion", () => {
    const zh = createUiText("zh-CN");
    const en = createUiText("en");

    expect(zh.confirmUninstall).toBe("确认卸载");
    expect(en.confirmUninstall).toBe("Confirm uninstall");
    expect(zh.uninstallManagedConfirmation).toContain("不会主动扫描外部用户数据目录");
    expect(en.uninstallManagedConfirmation).toContain("It will not scan external user data directories.");
    expect(zh.uninstallLinuxPackageConfirmation).toContain("系统包管理器");
    expect(en.uninstallLinuxPackageConfirmation).toContain("system package manager");
    expect(zh.uninstallExternalInstallerConfirmation).toContain("系统卸载入口");
    expect(en.uninstallExternalInstallerConfirmation).toContain("system uninstall path");
  });
});

import { describe, expect, it } from "vitest";
import { createUiText, formatRecordedAt, type Language } from "./i18n";
import {
  buildConfigConnectivityWarning,
  buildConnectivityTestStatus,
  buildConnectivityTestViewState,
  buildNetworkConfigHealth,
  getNetworkConfigKey,
  shouldRunAutoConnectivityCheck,
  buildStatusDockPresentation,
  buildUpdateInbox,
  getBulkRemoveAvailability,
  getSelectionActionAvailability,
  getSelectionSummary,
  filterManagedApps,
  getConfirmInstallAvailability,
  getOpenAppAvailability,
  getOpenReleaseAvailability,
  getPrimaryActionAvailability,
  getRemoveTrackedAvailability,
  getRollbackAvailability,
  hasInstallableAsset,
  getInspectorDetailItems,
  getLifecycleHistoryEntries,
  hasSecondaryInspectorActions,
  isFailedInstallProgress,
  installManagementKindLabel,
  integrityStatusLabel,
  installPreviewIntegrityLabel,
  buildReleaseActionGuidance,
  buildInspectorStatusSummary,
  isPreviewRequestCurrent,
  isPreviewResponseCurrent,
  releaseChannelForVersion,
  releaseDirectionLabel,
  resolveLifecycleSelection,
  getDetailPathLabel,
  isRemovableTrackedItem,
  isRemovableNoRelease,
  shouldShowLifecyclePreviewAction,
  shouldShowOpenAppSecondary,
  shouldShowOpenReleaseSecondary,
  shouldShowInstallerFolderSecondary,
  shouldShowInstallLocationAction,
  shouldShowInstallLocationSecondary,
  systemPackageManagerLabel,
  taskActionLabel,
  taskStageLabel,
  getUninstallAvailability,
  parseReleaseNote,
  pruneSelection,
  inboxFilters,
  isActionRequired,
  selectVisibleIds,
  toggleSelection
} from "./appModel";

const language: Language = "en";

describe("release lifecycle inspector state", () => {
  const managed = {
    id: "owner/project",
    name: "Project",
    currentVersion: "v2.0.0",
    latestVersion: "v1.0.0",
    status: "downgradeAvailable" as const,
    source: "GitHub",
    installPath: "/managed/project.AppImage",
    installPathKind: "managedPath" as const,
    rollback: {
      version: "v1.0.0",
      assetName: "project.AppImage"
    }
  };

  it("offers rollback only for managed installs with a snapshot", () => {
    expect(getRollbackAvailability(managed, false, "en").enabled).toBe(true);
    expect(
      getRollbackAvailability(
        { ...managed, installPathKind: "systemInstaller" },
        false,
        "en"
      ).enabled
    ).toBe(false);
    expect(getRollbackAvailability({ ...managed, rollback: null }, false, "en").enabled).toBe(false);
  });

  it("labels direction and integrity without presenting a downgrade as an update", () => {
    expect(releaseDirectionLabel("downgrade", "en")).toBe("Downgrade");
    expect(releaseDirectionLabel("upgrade", "zh-CN")).toBe("升级");
    expect(integrityStatusLabel("verifiedChecksum", "en")).toBe("Verified checksum");
    expect(integrityStatusLabel("recordedOnly", "zh-CN")).toBe("未验证；仅记录摘要");
  });

  it("hides the lifecycle preview action for failed installs", () => {
    expect(shouldShowLifecyclePreviewAction(managed)).toBe(true);
    expect(shouldShowLifecyclePreviewAction({ ...managed, status: "failed" })).toBe(false);
  });

  it("builds a compact inspector summary for the visible state", () => {
    expect(buildInspectorStatusSummary(
      { ...managed, status: "updateAvailable" },
      "v1.0.0",
      false,
      "en"
    )).toEqual({
      label: "Update available",
      detail: "v2.0.0 → v1.0.0",
      tone: "warning"
    });

    expect(buildInspectorStatusSummary(
      { ...managed, status: "needsChoice", assetName: undefined },
      "v1.0.0",
      false,
      "en"
    )).toEqual({
      label: "No installable asset for this platform",
      detail: "v1.0.0",
      tone: "neutral"
    });

    expect(buildInspectorStatusSummary(
      { ...managed, status: "failed" },
      "v1.0.0",
      true,
      "en"
    )).toEqual({
      label: "Failed",
      detail: "The last install failed. Review the error above and retry from the same preview.",
      tone: "danger"
    });
  });

  it("reselects the policy target after channel, pin, or ignore mutations", () => {
    const versions = ["v3.0.0", "v2.0.0", "v1.0.0"];
    expect(resolveLifecycleSelection({
      ...managed,
      latestVersion: "v2.0.0",
      releasePolicy: { channel: "stable", pinnedVersion: null, ignoredVersions: ["v3.0.0"] }
    }, versions)).toEqual({ selectedVersion: "v2.0.0", channel: "stable" });
    expect(resolveLifecycleSelection({
      ...managed,
      latestVersion: "v3.0.0",
      releasePolicy: { channel: "prerelease", pinnedVersion: "v1.0.0", ignoredVersions: [] }
    }, versions)).toEqual({ selectedVersion: "v1.0.0", channel: "prerelease" });
  });

  it("derives the preview channel from the selected release metadata", () => {
    expect(releaseChannelForVersion({ prerelease: false })).toBe("stable");
    expect(releaseChannelForVersion({ prerelease: true })).toBe("prerelease");
    expect(releaseChannelForVersion(null)).toBe("stable");
  });

  it("rejects late or cross-repository preview responses", () => {
    expect(isPreviewRequestCurrent(2, 2, "owner/b", "owner/b")).toBe(true);
    expect(isPreviewRequestCurrent(1, 2, "owner/a", "owner/b")).toBe(false);
    expect(isPreviewRequestCurrent(2, 2, "owner/a", "owner/b")).toBe(false);
    expect(isPreviewResponseCurrent(2, 2, "owner/b", "owner/b", "owner/b")).toBe(true);
    expect(isPreviewResponseCurrent(1, 2, "owner/a", "owner/b", "owner/a")).toBe(false);
    expect(isPreviewResponseCurrent(2, 2, "owner/b", "owner/b", "owner/a")).toBe(false);
  });

  it("labels discovered checksums as pending verification until install", () => {
    expect(installPreviewIntegrityLabel({
      expectedSha256: "a".repeat(64),
      checksumAssetName: "SHA256SUMS",
      status: "recordedOnly"
    }, "en")).toBe("Pending SHA-256 verification");
    expect(installPreviewIntegrityLabel({ status: "recordedOnly" }, "en"))
      .toBe("Unverified; digest recorded only");
  });
});

describe("buildConfigConnectivityWarning", () => {
  it("does not warn about missing token before a GitHub connectivity problem is known", () => {
    expect(buildConfigConnectivityWarning({ githubToken: "", proxyUrl: "" }, "zh-CN")).toBeNull();
    expect(buildConfigConnectivityWarning({ githubToken: "ghp_token", proxyUrl: "" }, "zh-CN")).toBeNull();
    expect(buildConfigConnectivityWarning({ githubToken: "", proxyUrl: "http://proxy.example.com:port" }, "zh-CN")).toBeNull();
  });

  it("warns for proxy only on network failures and token only on rate limit or auth failures", () => {
    expect(buildConfigConnectivityWarning({ githubToken: "", proxyUrl: "" }, "zh-CN", {
      status: "failed",
      problem: "network"
    })?.label).toBe("GitHub 连接异常，检查代理");
    expect(buildConfigConnectivityWarning({ githubToken: "", proxyUrl: "" }, "zh-CN", {
      status: "failed",
      problem: "rateLimit"
    })?.label).toBe("GitHub API 受限，配置 Token");
    expect(buildConfigConnectivityWarning({ githubToken: "", proxyUrl: "" }, "zh-CN", {
      status: "failed",
      problem: "auth"
    })?.label).toBe("GitHub API 受限，配置 Token");
  });
});

describe("buildNetworkConfigHealth", () => {
  it("summarizes token as optional until GitHub reports an auth or rate-limit problem", () => {
    expect(buildNetworkConfigHealth({ githubToken: " ghp_token ", proxyUrl: "" }, "zh-CN")).toEqual({
      tokenConfigured: true,
      proxyConfigured: false,
      tokenLabel: "Token 已配置",
      proxyLabel: "代理未配置",
      formatExample: "http://proxy.example.com:port",
      warning: null
    });

    const missing = buildNetworkConfigHealth({ githubToken: "", proxyUrl: "" }, "en");
    expect(missing.tokenConfigured).toBe(false);
    expect(missing.proxyConfigured).toBe(false);
    expect(missing.tokenLabel).toBe("Token optional");
    expect(missing.warning).toBeNull();
  });
});

describe("buildConnectivityTestStatus", () => {
  it("keeps connection test state presentation compact and localizable", () => {
    expect(buildConnectivityTestStatus({ status: "idle" }, "zh-CN")).toEqual({
      label: "尚未测试",
      detail: "使用当前代理和可选 Token 设置测试 GitHub API 是否可访问。",
      tone: "neutral"
    });
    expect(buildConnectivityTestStatus({ status: "testing" }, "zh-CN").tone).toBe("busy");
    expect(buildConnectivityTestStatus({ status: "success", message: "GitHub API is reachable" }, "en")).toEqual({
      label: "Connection OK",
      detail: "GitHub API is reachable",
      tone: "success"
    });
    expect(buildConnectivityTestStatus({ status: "failed", message: "proxy timeout" }, "en")).toEqual({
      label: "Connection failed",
      detail: "proxy timeout Check the GitHub proxy format and whether the proxy service is reachable.",
      tone: "danger"
    });
  });

  it("marks a previous connectivity result stale after token or proxy changes", () => {
    const config = { githubToken: "", proxyUrl: "" };
    const testedKey = getNetworkConfigKey(config);
    const changedConfig = { githubToken: "", proxyUrl: "http://proxy.example.com:port" };

    expect(buildConnectivityTestStatus({
      status: "success",
      message: "GitHub API is reachable",
      configKey: testedKey
    }, "zh-CN", changedConfig)).toEqual({
      label: "配置已更改",
      detail: "Token 或代理已变化，请重新测试 GitHub 连接。",
      tone: "warning"
    });
  });
});

describe("automatic GitHub connectivity checks", () => {
  it("runs once per token and proxy configuration", () => {
    const config = { githubToken: "", proxyUrl: "" };
    const key = getNetworkConfigKey(config);

    expect(shouldRunAutoConnectivityCheck(config, null)).toBe(true);
    expect(shouldRunAutoConnectivityCheck(config, key)).toBe(false);
    expect(shouldRunAutoConnectivityCheck({ githubToken: "", proxyUrl: "http://proxy.example.com:port" }, key)).toBe(true);
  });

  it("normalizes backend connectivity results into the shared view state", () => {
    expect(buildConnectivityTestViewState({
      ok: true,
      message: "GitHub API is reachable",
      problem: "none",
      usedToken: false,
      usedProxy: false
    }, { githubToken: "", proxyUrl: "" })).toEqual({
      status: "success",
      message: "GitHub API is reachable",
      problem: "none",
      configKey: getNetworkConfigKey({ githubToken: "", proxyUrl: "" })
    });

    expect(buildConnectivityTestViewState({
      ok: false,
      message: "API rate limit exceeded",
      problem: "rateLimit",
      usedToken: false,
      usedProxy: false
    }, { githubToken: "", proxyUrl: "" }).status).toBe("failed");
  });
});

describe("buildUpdateInbox", () => {
  it("labels install management details for previews", () => {
    expect(installManagementKindLabel("managedLocal", "zh-CN")).toBe("本地托管");
    expect(installManagementKindLabel("systemPackage", "zh-CN")).toBe("由系统包管理器托管");
    expect(installManagementKindLabel("externalInstaller", "en")).toBe("External installer");
    expect(systemPackageManagerLabel("Pacman")).toBe("Pacman");
    expect(systemPackageManagerLabel("Rpm")).toBe("RPM");
  });

  it("shows recent lifecycle history in inspector details", () => {
    const detailItems = getInspectorDetailItems(
      {
        id: "owner/current",
        name: "Current",
        currentVersion: "v1.0.0",
        latestVersion: "v1.0.0",
        status: "current",
        source: "GitHub",
        installPath: "/tmp/current",
        installPathKind: "managedPath",
        launchPath: "/tmp/current/current",
        recentActivities: [
          {
            repoId: "owner/current",
            repoName: "Current",
            action: "install",
            outcome: "succeeded",
            recordedAt: "2026-07-21T10:20:30Z",
            version: "v1.0.0",
            assetName: "current-linux-x86_64.tar.gz",
            installPath: "/tmp/current",
            installPathKind: "managedPath",
            summary: "Installed Current v1.0.0"
          },
          {
            repoId: "owner/current",
            repoName: "Current",
            action: "update",
            outcome: "failed",
            recordedAt: "2026-07-22T10:20:30Z",
            version: "v1.1.0",
            assetName: "current-linux-x86_64.tar.gz",
            installPath: "/tmp/current",
            installPathKind: "managedPath",
            summary: "Updated Current v1.1.0",
            error: "download failed"
          }
        ]
      },
      language
    );

    expect(detailItems.some((item) => item.label === createUiText(language).recentActivity)).toBe(false);
    expect(detailItems.some((item) => item.label === createUiText(language).installManagement)).toBe(false);

    const history = getLifecycleHistoryEntries(
      {
        id: "owner/current",
        name: "Current",
        currentVersion: "v1.0.0",
        latestVersion: "v1.0.0",
        status: "current",
        source: "GitHub",
        installPath: "/tmp/current",
        installPathKind: "managedPath",
        launchPath: "/tmp/current/current",
        recentActivities: [
          {
            repoId: "owner/current",
            repoName: "Current",
            action: "install",
            outcome: "succeeded",
            recordedAt: "2026-07-21T10:20:30Z",
            version: "v1.0.0",
            assetName: "current-linux-x86_64.tar.gz",
            installPath: "/tmp/current",
            installPathKind: "managedPath",
            summary: "Installed Current v1.0.0"
          }
        ]
      },
      language
    );

    expect(history).toEqual([
      {
        summary: "Installed Current v1.0.0",
        recordedAt: formatRecordedAt("2026-07-21T10:20:30Z", language),
        failed: false,
        error: undefined
      }
    ]);
  });

  it("keeps actionable updates before current apps", () => {
    const inbox = buildUpdateInbox([
      {
        id: "owner/current",
        name: "Current",
        currentVersion: "v1.0.0",
        latestVersion: "v1.0.0",
        status: "current",
        source: "GitHub",
        installPath: "/tmp/current",
        installPathKind: "managedPath",
        launchPath: "/tmp/current/current"
      },
      {
        id: "owner/update",
        name: "Update",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "updateAvailable",
        source: "GitHub",
        installPath: "/tmp/update"
      }
    ], language);

    expect(inbox[0].id).toBe("owner/update");
    expect(inbox[0].actionLabel).toBe("Update");
    expect(inbox[1].actionLabel).toBe("Open app");
  });

  it("opens the release when a needs-choice app has no installable asset", () => {
    const inbox = buildUpdateInbox([
      {
        id: "owner/choice",
        name: "Choice",
        currentVersion: "Not installed",
        latestVersion: "v1.1.0",
        status: "needsChoice",
        source: "GitHub",
        releaseUrl: "https://github.com/owner/choice/releases/tag/v1.1.0",
        installPath: "/tmp/choice"
      }
    ], language);

    expect(inbox[0].actionLabel).toBe("Open release");
  });

  it("hides the secondary release action when release is already the primary action", () => {
    const releaseApp = buildUpdateInbox([
      {
        id: "owner/choice",
        name: "Choice",
        currentVersion: "Not installed",
        latestVersion: "v1.1.0",
        status: "needsChoice",
        source: "GitHub",
        releaseUrl: "https://github.com/owner/choice/releases/tag/v1.1.0",
        installPath: "/tmp/choice"
      }
    ], language)[0];

    const updateApp = buildUpdateInbox([
      {
        id: "owner/update",
        name: "Update",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "updateAvailable",
        source: "GitHub",
        installPath: "/tmp/update",
        installPathKind: "managedPath",
        launchPath: "/tmp/update/update"
      }
    ], language)[0];

    expect(shouldShowOpenReleaseSecondary(releaseApp, language)).toBe(false);
    expect(shouldShowOpenReleaseSecondary(updateApp, language)).toBe(true);
  });

  it("hides the secondary action group when no secondary action is available", () => {
    const releaseApp = buildUpdateInbox([
      {
        id: "owner/choice",
        name: "Choice",
        currentVersion: "Not installed",
        latestVersion: "v1.1.0",
        status: "needsChoice",
        source: "GitHub",
        releaseUrl: "https://github.com/owner/choice/releases/tag/v1.1.0",
        installPath: "/tmp/choice"
      }
    ], language)[0];

    const updateApp = buildUpdateInbox([
      {
        id: "owner/update",
        name: "Update",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "updateAvailable",
        source: "GitHub",
        installPath: "/tmp/update",
        installPathKind: "managedPath",
        launchPath: "/tmp/update/update"
      }
    ], language)[0];

    expect(hasSecondaryInspectorActions(releaseApp, language)).toBe(false);
    expect(hasSecondaryInspectorActions(updateApp, language)).toBe(true);
  });

  it("keeps install as the action when a needs-choice app has a matched asset", () => {
    const inbox = buildUpdateInbox([
      {
        id: "owner/choice",
        name: "Choice",
        currentVersion: "Not installed",
        latestVersion: "v1.1.0",
        status: "needsChoice",
        source: "GitHub",
        assetName: "choice-windows-x64.zip",
        installPath: "/tmp/choice"
      }
    ], language);

    expect(inbox[0].actionLabel).toBe("Install");
    expect(getPrimaryActionAvailability(inbox[0], false, language)).toEqual({ enabled: true });
  });

  it("labels the detail path as the default install path when no asset can be installed", () => {
    const item = buildUpdateInbox([
      {
        id: "owner/choice",
        name: "Choice",
        currentVersion: "Not installed",
        latestVersion: "v1.1.0",
        status: "needsChoice",
        source: "GitHub",
        releaseUrl: "https://github.com/owner/choice/releases/tag/v1.1.0",
        installPath: "/tmp/choice"
      }
    ], language)[0];

    expect(getDetailPathLabel(item, language)).toBe("Default install path");
  });

  it("marks asset and install path as full-width detail rows", () => {
    const item = buildUpdateInbox([
      {
        id: "owner/choice",
        name: "Choice",
        currentVersion: "Not installed",
        latestVersion: "v1.1.0",
        status: "needsChoice",
        source: "GitHub",
        assetName: "choice-windows-x64.zip",
        installPath: "/tmp/choice"
      }
    ], language)[0];

    const details = getInspectorDetailItems(item, language);

    expect(details[0]).toMatchObject({
      label: "Asset file",
      value: "choice-windows-x64.zip",
      fullWidth: true
    });
    expect(details[1]).toMatchObject({
      label: "Install path",
      value: "/tmp/choice",
      fullWidth: true
    });
  });

  it("detects installable assets only for needs-choice apps", () => {
    expect(hasInstallableAsset({
      id: "owner/choice",
      name: "Choice",
      currentVersion: "Not installed",
      latestVersion: "v1.1.0",
      status: "needsChoice",
      source: "GitHub",
      assetName: "choice-windows-x64.zip",
      installPath: "/tmp/choice"
    })).toBe(true);

    expect(hasInstallableAsset({
      id: "owner/choice",
      name: "Choice",
      currentVersion: "Not installed",
      latestVersion: "v1.1.0",
      status: "needsChoice",
      source: "GitHub",
      assetName: "   ",
      installPath: "/tmp/choice"
    })).toBe(false);

    expect(hasInstallableAsset({
      id: "owner/update",
      name: "Update",
      currentVersion: "v1.0.0",
      latestVersion: "v1.1.0",
      status: "updateAvailable",
      source: "GitHub",
      assetName: "update.zip",
      installPath: "/tmp/update"
    })).toBe(false);
  });

  it("builds release guidance from release notes only when no installable asset exists", () => {
    expect(buildReleaseActionGuidance({
      id: "owner/docker",
      name: "Docker",
      currentVersion: "Not installed",
      latestVersion: "v1.0.0",
      status: "needsChoice",
      source: "GitHub",
      releaseTitle: "Docker release",
      releaseNote: "Run with docker compose after pulling the image.",
      installPath: "/tmp/docker"
    }, language)).toEqual({
      kind: "docker",
      title: "Docker release",
      summary: "ReleaseDock cannot install this release on the current platform. The release notes mention Docker or Compose, so open the release page and follow those instructions there.",
      bullets: [
        "This guidance is based only on the release title and notes.",
        "Open the release page to read the full instructions."
      ]
    });

    expect(buildReleaseActionGuidance({
      id: "owner/source",
      name: "Source",
      currentVersion: "Not installed",
      latestVersion: "v1.0.0",
      status: "needsChoice",
      source: "GitHub",
      releaseTitle: "Source build release",
      releaseNote: "Build from source with cargo build or cmake.",
      installPath: "/tmp/source"
    }, language)?.kind).toBe("source");

    expect(buildReleaseActionGuidance({
      id: "owner/manual",
      name: "Manual",
      currentVersion: "Not installed",
      latestVersion: "v1.0.0",
      status: "needsChoice",
      source: "GitHub",
      releaseTitle: "Manual release",
      releaseNote: "No installable asset for this platform.",
      installPath: "/tmp/manual"
    }, language)).toEqual({
      kind: "manual",
      title: "Manual install release",
      summary: "ReleaseDock cannot install this release on the current platform. Open the release page to continue with the published instructions.",
      bullets: [
        "This guidance is based only on the release title and notes.",
        "Check for a manual download, a source build path, or a container image."
      ]
    });

    expect(buildReleaseActionGuidance({
      id: "owner/asset",
      name: "Asset",
      currentVersion: "Not installed",
      latestVersion: "v1.0.0",
      status: "needsChoice",
      source: "GitHub",
      assetName: "asset.zip",
      releaseTitle: "Docker release",
      releaseNote: "Run with docker compose.",
      installPath: "/tmp/asset"
    }, language)).toBeNull();
  });

  it("preserves original release note for detail view", () => {
    const inbox = buildUpdateInbox([
      {
        id: "owner/update",
        name: "Update",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "updateAvailable",
        source: "GitHub",
        installPath: "/tmp/update",
        installPathKind: "managedPath",
        launchPath: "/tmp/update/update",
        releaseNote: "Fix crash\n\n- Keep original markdown-like text"
      }
    ], language);

    expect(inbox[0].releaseNote).toContain("Keep original markdown-like text");
    expect("summary" in inbox[0]).toBe(false);
    expect("risk" in inbox[0]).toBe(false);
  });

  it("filters by status and text query", () => {
    const apps = [
      {
        id: "owner/update",
        name: "Update",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "updateAvailable",
        source: "GitHub",
        installPath: "/tmp/update",
        assetName: "update.zip"
      },
      {
        id: "owner/current",
        name: "Current",
        currentVersion: "v1.0.0",
        latestVersion: "v1.0.0",
        status: "current",
        source: "GitHub",
        installPath: "/tmp/current"
      }
    ] as const;

    expect(filterManagedApps([...apps], "updateAvailable", "")).toHaveLength(1);
    expect(filterManagedApps([...apps], "all", "update.zip")).toHaveLength(1);
  });

  it("keeps the inbox filters compact", () => {
    expect(inboxFilters(language).map((item) => item.id)).toEqual([
      "all",
      "updateAvailable",
      "actionRequired",
      "failed"
    ]);
  });

  it("treats needsChoice, removable noRelease, and failed unknown installs as action required", () => {
    expect(isActionRequired({
      id: "owner/choice",
      name: "Choice",
      currentVersion: "Not installed",
      latestVersion: "v1.0.0",
      status: "needsChoice",
      source: "GitHub",
      installPath: "/tmp/choice"
    })).toBe(true);

    expect(isActionRequired({
      id: "owner/none",
      name: "No Release",
      currentVersion: "Not installed",
      latestVersion: "No release",
      status: "noRelease",
      source: "GitHub",
      installPath: "/tmp/none",
      installPathKind: "unknown"
    })).toBe(true);

    // 已安装但仓库无 release 的项不可移除，不算待处理
    expect(isActionRequired({
      id: "owner/installed-none",
      name: "Installed No Release",
      currentVersion: "v1.0.0",
      latestVersion: "No release",
      status: "noRelease",
      source: "GitHub",
      installPath: "/tmp/installed",
      installPathKind: "managedPath"
    })).toBe(false);

    expect(isActionRequired({
      id: "owner/current",
      name: "Current",
      currentVersion: "v1.0.0",
      latestVersion: "v1.0.0",
      status: "current",
      source: "GitHub",
      installPath: "/tmp/current"
    })).toBe(false);

    expect(isActionRequired({
      id: "owner/failed",
      name: "Failed",
      currentVersion: "Unknown",
      latestVersion: "Unknown",
      status: "failed",
      source: "GitHub",
      installPath: "unknown"
    })).toBe(true);
  });

  it("filters actionRequired as a composite of needsChoice and removable noRelease", () => {
    const apps = [
      {
        id: "owner/choice",
        name: "Choice",
        currentVersion: "Not installed",
        latestVersion: "v1.0.0",
        status: "needsChoice",
        source: "GitHub",
        installPath: "/tmp/choice"
      },
      {
        id: "owner/none",
        name: "No Release",
        currentVersion: "Not installed",
        latestVersion: "No release",
        status: "noRelease",
        source: "GitHub",
        installPath: "/tmp/none",
        installPathKind: "unknown"
      },
      {
        id: "owner/installed-none",
        name: "Installed No Release",
        currentVersion: "v1.0.0",
        latestVersion: "No release",
        status: "noRelease",
        source: "GitHub",
        installPath: "/tmp/installed",
        installPathKind: "managedPath"
      },
      {
        id: "owner/current",
        name: "Current",
        currentVersion: "v1.0.0",
        latestVersion: "v1.0.0",
        status: "current",
        source: "GitHub",
        installPath: "/tmp/current"
      },
      {
        id: "owner/update",
        name: "Update",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "updateAvailable",
        source: "GitHub",
        installPath: "/tmp/update"
      }
    ] as const;

    const filtered = filterManagedApps([...apps], "actionRequired", "");
    expect(filtered.map((app) => app.id)).toEqual(["owner/choice", "owner/none"]);
  });

  it("filters only the managed app list and ignores release note body text", () => {
    const apps = [
      {
        id: "owner/update",
        name: "Update",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "updateAvailable",
        source: "GitHub",
        installPath: "/tmp/update",
        releaseNote: "This note mentions appimage but should not act like global repository search.",
        releaseUrl: "https://github.com/hidden-owner/hidden-project/releases/tag/v1.1.0"
      }
    ] as const;

    expect(filterManagedApps([...apps], "all", "appimage")).toHaveLength(0);
    expect(filterManagedApps([...apps], "all", "hidden-project")).toHaveLength(0);
  });

  it("describes action availability with reasons", () => {
    const app = buildUpdateInbox([
      {
        id: "owner/update",
        name: "Update",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "updateAvailable",
        source: "GitHub",
        installPath: "/tmp/update",
        installPathKind: "managedPath",
        launchPath: "/tmp/update/update",
        releaseUrl: "https://github.com/owner/update/releases/tag/v1.1.0"
      }
    ], language)[0];

    expect(getOpenReleaseAvailability(app, false, language)).toEqual({ enabled: true });
    expect(getOpenAppAvailability(app, false, language)).toEqual({ enabled: true });
    expect(getOpenReleaseAvailability(app, true, language)).toEqual({
      enabled: false,
      reason: "A task is already running"
    });
    expect(getPrimaryActionAvailability(app, false, language)).toEqual({ enabled: true });
    expect(getConfirmInstallAvailability(app, false, language)).toEqual({ enabled: true });
    expect(getUninstallAvailability(app, false, language)).toEqual({ enabled: true });
    expect(getRemoveTrackedAvailability(app, false, language)).toEqual({
      enabled: false,
      reason: "Only uninstalled tracked items can be removed"
    });
  });

  it("treats undefined install path kind as unknown for inspector actions", () => {
    const app = buildUpdateInbox([
      {
        id: "owner/legacy",
        name: "Legacy",
        currentVersion: "Not installed",
        latestVersion: "No release",
        status: "noRelease",
        source: "GitHub",
        installPath: "/tmp/legacy"
      }
    ], language)[0];

    expect(isRemovableNoRelease(app)).toBe(true);
    expect(isActionRequired(app)).toBe(true);
    expect(shouldShowInstallLocationAction(app)).toBe(false);
    expect(hasSecondaryInspectorActions(app, language)).toBe(false);
  });

  it("treats failed tracked repos with unknown install paths as removable", () => {
    const app = buildUpdateInbox([
      {
        id: "owner/broken",
        name: "Broken",
        currentVersion: "Unknown",
        latestVersion: "Unknown",
        status: "failed",
        source: "GitHub",
        installPath: "/tmp/broken",
        installPathKind: "unknown"
      }
    ], language)[0];

    expect(isRemovableTrackedItem(app)).toBe(true);
    expect(isRemovableNoRelease(app)).toBe(false);
    expect(getRemoveTrackedAvailability(app, false, language)).toEqual({
      enabled: true
    });
    expect(getBulkRemoveAvailability([app], [app.id], false, language)).toEqual({
      enabled: true,
      candidateCount: 1,
      skippedCount: 0
    });
    expect(isActionRequired(app)).toBe(true);
  });

  it("keeps failed managed installs on the uninstall path instead of remove tracking", () => {
    const app = buildUpdateInbox([
      {
        id: "owner/broken-managed",
        name: "Broken Managed",
        currentVersion: "Unknown",
        latestVersion: "Unknown",
        status: "failed",
        source: "GitHub",
        installPath: "/tmp/broken-managed",
        installPathKind: "managedPath"
      }
    ], language)[0];

    expect(isRemovableTrackedItem(app)).toBe(false);
    expect(getRemoveTrackedAvailability(app, false, language).enabled).toBe(false);
    expect(isActionRequired(app)).toBe(false);
  });

  it("shows the secondary open-app button only for updateable managed installs", () => {
    const updateApp = buildUpdateInbox([
      {
        id: "owner/update",
        name: "Update",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "updateAvailable",
        source: "GitHub",
        installPath: "/tmp/update",
        installPathKind: "managedPath",
        launchPath: "/tmp/update/update"
      }
    ], language)[0];

    const currentApp = buildUpdateInbox([
      {
        id: "owner/current",
        name: "Current",
        currentVersion: "v1.0.0",
        latestVersion: "v1.0.0",
        status: "current",
        source: "GitHub",
        installPath: "/tmp/current",
        installPathKind: "managedPath",
        launchPath: "/tmp/current/current"
      }
    ], language)[0];

    expect(shouldShowOpenAppSecondary(updateApp)).toBe(true);
    expect(shouldShowOpenAppSecondary(currentApp)).toBe(false);
  });

  it("does not offer open-app for executable installs without a launch target", () => {
    const ui = createUiText(language);
    const currentApp = buildUpdateInbox([
      {
        id: "owner/cli",
        name: "CLI",
        currentVersion: "v1.0.0",
        latestVersion: "v1.0.0",
        status: "current",
        source: "GitHub",
        installPath: "/tmp/cli",
        installPathKind: "managedPath"
      }
    ], language)[0];

    expect(currentApp.actionLabel).toBe("Open install location");
    expect(getOpenAppAvailability(currentApp, false, language)).toEqual({
      enabled: false,
      reason: ui.model.noLaunchTarget
    });
    expect(shouldShowOpenAppSecondary(currentApp)).toBe(false);
    expect(shouldShowInstallLocationSecondary(currentApp)).toBe(false);
  });

  it("uses open-install-location as the primary action when current apps have no launch target", () => {
    const currentApp = buildUpdateInbox([
      {
        id: "owner/cli",
        name: "CLI",
        currentVersion: "v1.0.0",
        latestVersion: "v1.0.0",
        status: "current",
        source: "GitHub",
        installPath: "/tmp/cli",
        installPathKind: "managedPath"
      }
    ], language)[0];

    expect(currentApp.actionLabel).toBe("Open install location");
  });

  it("uses distinct installer file and folder actions for system installers", () => {
    const systemInstaller = buildUpdateInbox([
      {
        id: "owner/setup",
        name: "Setup",
        currentVersion: "v1.0.0",
        latestVersion: "v1.0.0",
        status: "current",
        source: "GitHub",
        installPath: "/tmp/setup.msi",
        installPathKind: "systemInstaller"
      }
    ], "zh-CN")[0];

    expect(systemInstaller.actionLabel).toBe("打开安装包");
    expect(shouldShowInstallerFolderSecondary(systemInstaller)).toBe(true);
    expect(shouldShowInstallLocationSecondary(systemInstaller)).toBe(false);
  });

  it("uses a plain uninstall label in Chinese", () => {
    expect(createUiText("zh-CN").uninstallAbility).toBe("卸载");
  });

  it("treats no-release tracked repos as removable and openable", () => {
    const app = buildUpdateInbox([
      {
        id: "owner/none",
        name: "No Release",
        currentVersion: "Not installed",
        latestVersion: "No release",
        status: "noRelease",
        source: "GitHub",
        installPath: "/tmp/none",
        installPathKind: "unknown",
        launchPath: "/tmp/none/app",
        releaseUrl: "https://github.com/owner/none"
      }
    ], language)[0];

    expect(app.actionLabel).toBe("Open release");
    expect(getPrimaryActionAvailability(app, false, language)).toEqual({ enabled: true });
    expect(getRemoveTrackedAvailability(app, false, language)).toEqual({ enabled: true });
    expect(getBulkRemoveAvailability([app], [app.id], false, language)).toEqual({
      enabled: true,
      candidateCount: 1,
      skippedCount: 0
    });
  });

  it("updates bulk selection helpers deterministically", () => {
    const apps = buildUpdateInbox([
      {
        id: "owner/update",
        name: "Update",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "updateAvailable",
        source: "GitHub",
        installPath: "/tmp/update"
      },
      {
        id: "owner/choice",
        name: "Choice",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "needsChoice",
        source: "GitHub",
        installPath: "/tmp/choice"
      }
    ], language);

    expect(toggleSelection([], "owner/update")).toEqual(["owner/update"]);
    expect(toggleSelection(["owner/update"], "owner/update")).toEqual([]);
    expect(selectVisibleIds([
      {
        id: "owner/update",
        name: "Update",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "updateAvailable",
        source: "GitHub",
        installPath: "/tmp/update"
      },
      {
        id: "owner/choice",
        name: "Choice",
        currentVersion: "v1.0.0",
        latestVersion: "v1.1.0",
        status: "needsChoice",
        source: "GitHub",
        installPath: "/tmp/choice"
      }
    ])).toEqual(["owner/update", "owner/choice"]);
    expect(pruneSelection(["owner/update", "missing"], apps)).toEqual(["owner/update"]);
    expect(getBulkRemoveAvailability(apps, ["owner/update", "owner/choice"], false, language)).toEqual({
      enabled: true,
      candidateCount: 1,
      skippedCount: 1,
      reason: "Skipping 1 non-removable item(s)"
    });
    expect(getBulkRemoveAvailability(apps, ["owner/update"], false, language)).toEqual({
      enabled: false,
      candidateCount: 0,
      skippedCount: 1,
      reason: "Select at least one uninstalled tracked item"
    });
  });

  it("uses uninstall instead of remove for a single installed selection", () => {
    const apps = buildUpdateInbox([
      {
        id: "owner/current",
        name: "Current",
        currentVersion: "v1.0.0",
        latestVersion: "v1.0.0",
        status: "current",
        source: "GitHub",
        installPath: "/tmp/current",
        installPathKind: "managedPath"
      },
      {
        id: "owner/choice",
        name: "Choice",
        currentVersion: "Not installed",
        latestVersion: "v1.0.0",
        status: "needsChoice",
        source: "GitHub",
        installPath: "/tmp/choice",
        installPathKind: "unknown"
      }
    ], language);

    expect(getSelectionActionAvailability(apps, ["owner/current"], false, language)).toEqual({
      enabled: true,
      kind: "uninstall",
      label: "Uninstall",
      uninstallTargetId: "owner/current",
      candidateCount: 0,
      skippedCount: 0
    });
    expect(getSelectionActionAvailability(apps, ["owner/choice"], false, language)).toEqual({
      enabled: true,
      kind: "remove",
      label: "Remove",
      candidateCount: 1,
      skippedCount: 0
    });
    expect(getSelectionActionAvailability(apps, ["owner/choice"], true, language)).toEqual({
      enabled: false,
      kind: "remove",
      label: "Remove",
      candidateCount: 0,
      skippedCount: 0,
      reason: "A task is already running"
    });
    expect(getSelectionActionAvailability(apps, ["owner/choice", "owner/choice"], false, language)).toEqual({
      enabled: true,
      kind: "remove",
      label: "Remove",
      candidateCount: 1,
      skippedCount: 0
    });
    expect(getSelectionActionAvailability(apps, ["owner/current", "owner/choice"], false, language)).toEqual({
      enabled: false,
      kind: "mixed",
      label: "Remove",
      reason: "Select installed apps separately before uninstalling",
      candidateCount: 1,
      skippedCount: 1
    });
    expect(getSelectionSummary(apps, [], getSelectionActionAvailability(apps, [], false, language), language)).toBe("No selection");
    expect(getSelectionSummary(apps, ["owner/current"], getSelectionActionAvailability(apps, ["owner/current"], false, language), language)).toBe("1 selected");
    expect(getSelectionSummary(apps, ["owner/current", "owner/choice"], getSelectionActionAvailability(apps, ["owner/current", "owner/choice"], false, language), language)).toBe("Mixed selection");
  });
});

describe("parseReleaseNote", () => {
  it("skips generated html comments and keeps readable markdown blocks", () => {
    const blocks = parseReleaseNote(`<!-- cliproxyapi-linux-release-assets:start -->
## Linux release assets

See [release docs](https://example.com/docs)

> Built for Linux users.

---

1. Default Linux build
2. Portable Linux build

- \`CLIProxyAPI_<version>_linux_<arch>.tar.gz\`
* \`CLIProxyAPI_<version>_linux_<arch>_no-plugin.tar.gz\`

\`\`\`bash
cliproxyapi --version
\`\`\`

Plain paragraph
<!-- cliproxyapi-linux-release-assets:end -->`);

    expect(blocks).toEqual([
      { type: "heading", level: 2, text: "Linux release assets" },
      { type: "paragraph", text: "See [release docs](https://example.com/docs)" },
      { type: "quote", text: "Built for Linux users." },
      { type: "divider" },
      {
        type: "list",
        ordered: true,
        items: ["Default Linux build", "Portable Linux build"]
      },
      {
        type: "list",
        ordered: false,
        items: [
          "`CLIProxyAPI_<version>_linux_<arch>.tar.gz`",
          "`CLIProxyAPI_<version>_linux_<arch>_no-plugin.tar.gz`"
        ]
      },
      { type: "code", text: "cliproxyapi --version" },
      { type: "paragraph", text: "Plain paragraph" }
    ]);
  });

  it("parses markdown tables into structured blocks", () => {
    const blocks = parseReleaseNote(`<!-- table:start -->
| Architecture | Windows | Ubuntu |
| --- | --- | --- |
| x86-64 (64-bit) | EXE | Download |
| AArch64 (ARM64) | EXE | Download |
| x86-32 (32-bit) | EXE (Legacy) |

After table
<!-- table:end -->`);

    expect(blocks).toEqual([
      {
        type: "table",
        header: ["Architecture", "Windows", "Ubuntu"],
        rows: [
          ["x86-64 (64-bit)", "EXE", "Download"],
          ["AArch64 (ARM64)", "EXE", "Download"],
          ["x86-32 (32-bit)", "EXE (Legacy)", ""]
        ]
      },
      { type: "paragraph", text: "After table" }
    ]);
  });

  it("keeps checklist markers inside list items for rendering", () => {
    const blocks = parseReleaseNote(`- [x] Done
- [ ] Pending`);

    expect(blocks).toEqual([
      {
        type: "list",
        ordered: false,
        items: ["[x] Done", "[ ] Pending"]
      }
    ]);
  });
});

describe("buildStatusDockPresentation", () => {
  it("labels integrity and rollback task progress", () => {
    expect(taskActionLabel("rollback", "en")).toBe("Rollback");
    expect(taskActionLabel("rollback", "zh-CN")).toBe("回滚任务");
    expect(taskStageLabel("verifyingArtifact", "en")).toBe("Verifying artifact");
    expect(taskStageLabel("creatingRollback", "zh-CN")).toBe("创建回滚快照");
    expect(taskStageLabel("restoringRollback", "en")).toBe("Restoring rollback");
  });

  it("keeps idle status compact without duplicating the status pill", () => {
    const presentation = buildStatusDockPresentation(null, false, "设置已保存", "zh-CN");

    expect(presentation).toEqual({
      eyebrow: "状态",
      headline: "设置已保存",
      detail: "",
      pillLabel: "设置已保存",
      failed: false,
      showPill: false,
      showProgress: false,
      progressMode: "indeterminate",
      progressPercent: null
    });
  });

  it("shows an indeterminate bar for busy work without task progress", () => {
    const presentation = buildStatusDockPresentation(null, true, "Checking latest release", language);

    expect(presentation).toEqual({
      eyebrow: "Status",
      headline: "Checking latest release",
      detail: "Checking latest release",
      pillLabel: "Processing",
      failed: false,
      showPill: true,
      showProgress: true,
      progressMode: "indeterminate",
      progressPercent: null
    });
  });

  it("keeps zero percent visible and marks finished work as complete", () => {
    const zero = buildStatusDockPresentation(
      {
        repoId: "owner/repo",
        action: "install",
        stage: "preparing",
        message: "Preparing to install",
        percent: 0
      },
      false,
      "Installing",
      language
    );

    expect(zero.progressMode).toBe("determinate");
    expect(zero.progressPercent).toBe(0);
    expect(zero.pillLabel).toBe("0%");
    expect(zero.showPill).toBe(true);

    const finished = buildStatusDockPresentation(
      {
        repoId: "owner/repo",
        action: "uninstall",
        stage: "finished",
        message: "Done",
        percent: null
      },
      false,
      "Uninstalling",
      language
    );

    expect(finished.progressMode).toBe("determinate");
    expect(finished.progressPercent).toBe(100);
    expect(finished.pillLabel).toBe("100%");
  });

  it("reports middle download percentages directly", () => {
    const presentation = buildStatusDockPresentation(
      {
        repoId: "owner/repo",
        action: "install",
        stage: "downloading",
        message: "Downloading asset",
        percent: 77
      },
      false,
      "Installing",
      language
    );

    expect(presentation.progressMode).toBe("determinate");
    expect(presentation.progressPercent).toBe(77);
    expect(presentation.pillLabel).toBe("77%");
  });

  it("clamps out-of-range percentages and keeps failures visible", () => {
    const clamped = buildStatusDockPresentation(
      {
        repoId: "owner/repo",
        action: "install",
        stage: "downloading",
        message: "Downloading asset",
        percent: 142
      },
      false,
      "Installing",
      language
    );

    expect(clamped.progressPercent).toBe(100);
    expect(clamped.pillLabel).toBe("100%");

    const failed = buildStatusDockPresentation(
      {
        repoId: "owner/repo",
        action: "uninstall",
        stage: "failed",
        message: "Remove failed",
        percent: 67
      },
      false,
      "Uninstalling",
      language
    );

    expect(failed.failed).toBe(true);
    expect(failed.pillLabel).toBe("Failed");
    expect(failed.progressMode).toBe("determinate");
  });
});

describe("isFailedInstallProgress", () => {
  it("detects a failed install for the same repo", () => {
    expect(
      isFailedInstallProgress(
        {
          repoId: "owner/repo",
          action: "install",
          stage: "failed",
          message: "Install failed",
          percent: 42,
        },
        "owner/repo"
      )
    ).toBe(true);
  });

  it("ignores other task types and other repos", () => {
    expect(
      isFailedInstallProgress(
        {
          repoId: "owner/repo",
          action: "uninstall",
          stage: "failed",
          message: "Uninstall failed",
          percent: 42,
        },
        "owner/repo"
      )
    ).toBe(false);

    expect(
      isFailedInstallProgress(
        {
          repoId: "owner/repo",
          action: "install",
          stage: "failed",
          message: "Install failed",
          percent: 42,
        },
        "other/repo"
      )
    ).toBe(false);
  });
});

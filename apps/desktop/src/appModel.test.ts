import { describe, expect, it } from "vitest";
import {
  buildUpdateInbox,
  getBulkRemoveAvailability,
  filterManagedApps,
  getConfirmInstallAvailability,
  getOpenReleaseAvailability,
  getPrimaryActionAvailability,
  getRemoveTrackedAvailability,
  getUninstallAvailability,
  pruneSelection,
  selectVisibleIds,
  toggleSelection
} from "./appModel";

describe("buildUpdateInbox", () => {
  it("keeps actionable updates before current apps", () => {
    const inbox = buildUpdateInbox([
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
    ]);

    expect(inbox[0].id).toBe("owner/update");
    expect(inbox[0].actionLabel).toBe("更新");
    expect(inbox[1].actionLabel).toBe("打开");
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
        releaseNote: "Fix crash\n\n- Keep original markdown-like text"
      }
    ]);

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
        releaseUrl: "https://github.com/owner/update/releases/tag/v1.1.0"
      }
    ])[0];

    expect(getOpenReleaseAvailability(app, false)).toEqual({ enabled: true });
    expect(getOpenReleaseAvailability(app, true)).toEqual({
      enabled: false,
      reason: "当前有任务在执行"
    });
    expect(getPrimaryActionAvailability(app, false)).toEqual({ enabled: true });
    expect(getConfirmInstallAvailability(app, false)).toEqual({ enabled: true });
    expect(getUninstallAvailability(app, false)).toEqual({ enabled: true });
    expect(getRemoveTrackedAvailability(app, false)).toEqual({
      enabled: false,
      reason: "只有未安装的跟踪项可以移除"
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
    ]);

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
    expect(getBulkRemoveAvailability(apps, ["owner/update", "owner/choice"], false)).toEqual({
      enabled: true,
      candidateCount: 1,
      skippedCount: 1,
      reason: "将跳过 1 个不可移除项"
    });
    expect(getBulkRemoveAvailability(apps, ["owner/update"], false)).toEqual({
      enabled: false,
      candidateCount: 0,
      skippedCount: 1,
      reason: "选择至少一个未安装的跟踪项"
    });
  });
});

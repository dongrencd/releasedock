import { describe, expect, it } from "vitest";
import { createTaskStatusText } from "./i18n";

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
  });
});

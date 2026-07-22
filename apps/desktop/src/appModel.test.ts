import { describe, expect, it } from "vitest";
import { buildUpdateInbox, filterManagedApps } from "./appModel";

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
});

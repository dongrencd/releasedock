import { describe, expect, it } from "vitest";
import { buildUpdateInbox } from "./appModel";

describe("buildUpdateInbox", () => {
  it("keeps actionable updates before current apps", () => {
    const inbox = buildUpdateInbox([
      { id: "owner/current", name: "Current", currentVersion: "v1.0.0", latestVersion: "v1.0.0", status: "current" },
      { id: "owner/update", name: "Update", currentVersion: "v1.0.0", latestVersion: "v1.1.0", status: "updateAvailable" }
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
        releaseNote: "Fix crash\n\n- Keep original markdown-like text"
      }
    ]);

    expect(inbox[0].releaseNote).toContain("Keep original markdown-like text");
    expect("summary" in inbox[0]).toBe(false);
    expect("risk" in inbox[0]).toBe(false);
  });
});

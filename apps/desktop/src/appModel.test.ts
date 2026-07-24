import { describe, expect, it } from "vitest";
import { type Language } from "./i18n";
import {
  buildStatusDockPresentation,
  buildUpdateInbox,
  getBulkRemoveAvailability,
  filterManagedApps,
  getConfirmInstallAvailability,
  getOpenReleaseAvailability,
  getPrimaryActionAvailability,
  getRemoveTrackedAvailability,
  getUninstallAvailability,
  parseReleaseNote,
  pruneSelection,
  inboxFilters,
  selectVisibleIds,
  toggleSelection
} from "./appModel";

const language: Language = "en";

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
    ], language);

    expect(inbox[0].id).toBe("owner/update");
    expect(inbox[0].actionLabel).toBe("Update");
    expect(inbox[1].actionLabel).toBe("Open");
  });

  it("labels needs-choice apps as install actions", () => {
    const inbox = buildUpdateInbox([
      {
        id: "owner/choice",
        name: "Choice",
        currentVersion: "Not installed",
        latestVersion: "v1.1.0",
        status: "needsChoice",
        source: "GitHub",
        installPath: "/tmp/choice"
      }
    ], language);

    expect(inbox[0].actionLabel).toBe("Install");
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
      "needsChoice",
      "failed"
    ]);
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
    ], language)[0];

    expect(getOpenReleaseAvailability(app, false, language)).toEqual({ enabled: true });
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
        installPathKind: "Unknown",
        releaseUrl: "https://github.com/owner/none"
      }
    ], language)[0];

    expect(app.actionLabel).toBe("Open");
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
  it("shows an indeterminate bar for busy work without task progress", () => {
    const presentation = buildStatusDockPresentation(null, true, "Checking latest release", language);

    expect(presentation).toEqual({
      eyebrow: "Status",
      headline: "Checking latest release",
      detail: "Checking latest release",
      pillLabel: "Processing",
      failed: false,
      showProgress: true,
      progressMode: "indeterminate",
      progressPercent: null
    });
  });

  it("keeps zero percent visible and marks finished work as complete", () => {
    const zero = buildStatusDockPresentation(
      {
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

    const finished = buildStatusDockPresentation(
      {
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

  it("clamps out-of-range percentages and keeps failures visible", () => {
    const clamped = buildStatusDockPresentation(
      {
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

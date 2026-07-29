import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { describe, expect, it } from "vitest";

const stylesPath = join(dirname(fileURLToPath(import.meta.url)), "styles.css");
const styles = readFileSync(stylesPath, "utf8");
const appSource = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "App.tsx"), "utf8");

function ruleBody(selector) {
  const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = styles.match(new RegExp(`${escapedSelector}\\s*\\{(?<body>[\\s\\S]*?)\\}`));
  return match?.groups?.body ?? "";
}

function mediaRuleBody(mediaQuery, selector) {
  const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const start = styles.indexOf(mediaQuery);
  if (start === -1) {
    return "";
  }
  const end = styles.indexOf("@media", start + mediaQuery.length);
  const block = styles.slice(start, end === -1 ? styles.length : end);
  const match = block.match(new RegExp(`${escapedSelector}\\s*\\{(?<body>[\\s\\S]*?)\\}`));
  return match?.groups?.body ?? "";
}

describe("workspace layout CSS", () => {
  it("exposes lifecycle state and confirmations to assistive technology", () => {
    expect(appSource).toContain("shouldShowLifecyclePreviewAction(item)");
    expect(appSource).toContain("buildInspectorStatusSummary(item, selectedVersion, installRetrying, language)");
    expect(appSource).toContain('className="installPreview pendingInstall" role="alertdialog"');
    expect(appSource).toContain('className="installPreview pendingRollback" role="alertdialog"');
    expect(appSource).toMatch(/label=\{confirmInstallAvailability[\s\S]*?className="primaryButton actionButton previewConfirmAction"\s+autoFocus/);
    expect(appSource).toMatch(/label=\{ui\.confirmRollback\}[\s\S]*?className="primaryButton actionButton previewConfirmAction"\s+autoFocus/);
  });

  it("pins optional workspace regions to explicit grid rows", () => {
    // The error banner is conditional. Explicit rows keep the status dock compact
    // when the banner is absent instead of letting it occupy the content row.
    expect(ruleBody(".topbar")).toContain("grid-row: 1");
    expect(ruleBody(".errorBanner")).toContain("grid-row: 2");
    expect(ruleBody(".dashboardView")).toContain("grid-row: 3");
    expect(ruleBody(".settingsPanel")).toContain("grid-row: 3");
    expect(ruleBody(".statusDock")).toContain("grid-row: 4");
  });

  it("keeps idle and progress status docks visually distinct", () => {
    expect(ruleBody(".statusDock.idle")).toContain("grid-template-columns: minmax(0, 1fr)");
    expect(ruleBody(".statusDock.idle")).toContain("padding: 7px 24px");
    expect(ruleBody(".statusDock.withProgress")).toContain("grid-template-columns: minmax(220px, 1.1fr) minmax(0, 1.9fr) auto");
    expect(ruleBody(".statusDock.withProgress .taskProgressTrack")).toContain("grid-column: 1 / -1");
  });

  it("styles non-blocking configuration warnings as subtle attention pills", () => {
    expect(ruleBody(".statePill.warning")).toContain("background: #fffbeb");
    expect(ruleBody(".statePill.warning")).toContain("color: #92400e");
  });

  it("styles the network configuration guidance card and highlighted fields", () => {
    expect(ruleBody(".statePillButton")).toContain("cursor: pointer");
    expect(ruleBody(".fieldRow.attention")).toContain("border-color: #f59e0b");
    expect(ruleBody(".networkConfigCard")).toContain("gap: 12px");
    expect(ruleBody(".networkConfigStatusRow")).toContain("display: flex");
    expect(ruleBody(".connectivityResult.warning")).toContain("color: #92400e");
    expect(ruleBody(".connectivityResult.danger")).toContain("color: #b91c1c");
    expect(appSource).not.toContain("settingsFormActions");
    expect(appSource).not.toContain("reloadSettings");
    expect(appSource).not.toContain("settingsSummaryList");
    expect(appSource).not.toContain("settingsSidebarActions");
    expect(appSource).not.toContain("removeTrackedAvailability");
    expect(appSource).not.toContain("ui.releaseInfo");
    expect(appSource).not.toContain("ui.releaseChannel");
    expect(appSource).not.toContain("ui.releaseChannelStable");
    expect(appSource).not.toContain("ui.releaseChannelPrerelease");
    expect(appSource).not.toContain("onChannelChange");
    expect(appSource).not.toContain("infoDetailItems");
    expect(appSource).toContain("decisionBlock");
    expect(appSource).toContain("selectedReleasePublishedAt");
    expect(appSource).toContain("inspectorSummaryDetail");
    expect(appSource).toContain("decisionStateLabel");
    expect(appSource).toContain("decisionStatePill");
    expect(appSource).toContain("detailItems.map(");
    expect(appSource).toContain('item.status !== "needsChoice" && installedLifecycleItem');
    expect(appSource).toContain("getSelectionActionAvailability(apps, selectedIds, busy, language)");
    expect(appSource).toContain("getSelectionSummary(apps, selectedIds, selectionActionAvailability, language)");
    expect(appSource).toContain('className={`selectionSummary ${selectionActionAvailability.kind === "mixed" ? "warning" : ""}`}');
    expect(appSource).toContain("showDangerInspectorActions ? (");
    expect(appSource).toContain("releaseChannelForVersion(selectedRelease)");
    expect((appSource.match(/showInspectorActions \? inspectorActionSection : null/g) ?? []).length).toBe(1);
    expect(appSource).not.toContain("lifecycleActions");
    expect(appSource).toContain("lifecyclePreviewAction");
    expect(appSource).toMatch(/pendingInstall[\s\S]*?previewHero[\s\S]*?ui\.assetFile[\s\S]*?ui\.installPath/);
    expect(appSource).toContain('className={versionsLoading ? "selectControl loading" : "selectControl"}');
  });

  it("renders settings language and theme rows without label click traps", () => {
    expect(appSource).toContain('<div className="fieldRow">\n                    <span>{ui.language}</span>');
    expect(appSource).toContain('themeModeOptions(language).map');
    expect(appSource).toContain('<div className="fieldRow">\n                    <span>{ui.theme}</span>');
    expect(appSource).not.toContain('<label className="fieldRow">\n                    <span>{ui.language}</span>');
    expect(appSource).toContain('className="segmentedControl"');
    expect(appSource).toContain('className={configDraft.language === option.value ? "segmentedPill active" : "segmentedPill"}');
    expect(appSource).toContain('className={themeMode === option.value ? "segmentedPill active" : "segmentedPill"}');
    expect(appSource).not.toContain("languageSwitch");
    expect(appSource).not.toContain("languagePill");
  });

  it("keeps the install preview focused on install-critical details", () => {
    const pendingInstallStart = appSource.indexOf("{pendingInstall ? (");
    const pendingInstallEnd = appSource.indexOf("className=\"installPreview pendingRollback\"", pendingInstallStart);
    const pendingInstallBlock = appSource.slice(pendingInstallStart, pendingInstallEnd);

    expect(pendingInstallBlock).toContain("previewHero");
    expect(pendingInstallBlock).toContain("previewSafetyNote");
    expect(pendingInstallBlock).toContain("pendingInstallSafetyText");
    expect(appSource).toContain("ui.installPreviewNoChecksumHint");
    expect(appSource).toContain("ui.installPreviewSystemConfirmationHint");
    expect(pendingInstallBlock).toContain("ui.assetFile");
    expect(pendingInstallBlock).toContain("ui.installPath");
    expect(pendingInstallBlock).toContain("CopyableValue");
    expect(pendingInstallBlock).not.toContain("ui.installManagement");
    expect(pendingInstallBlock).not.toContain("ui.releaseDirectionLabel");
    expect(pendingInstallBlock).not.toContain("ui.integritySource");
    expect(pendingInstallBlock).not.toContain("previewNotes");
    expect(pendingInstallBlock).not.toContain("pendingInstall.notes");
  });

  it("uses neutral install preview surfaces with color reserved for type and risk", () => {
    expect(ruleBody(".installPreview.pendingInstall")).toContain("background: #fffdf7");
    expect(ruleBody(".installPreview.pendingInstall .blockTitle")).toContain("color: #0f766e");
    expect(ruleBody(".installPreview.pendingInstall .previewMetaLabel")).toContain("color: #64748b");
    expect(ruleBody(".installPreview.pendingInstall .previewMetaValue")).toContain("color: #243044");
    expect(ruleBody(".previewHeroTitle")).toContain("color: #18212f");
    expect(ruleBody(".previewHeroMeta")).toContain("color: #64748b");
    expect(ruleBody(".previewBadge")).toContain("border: 1px solid #99d4ce");
    expect(ruleBody(".previewBadge")).toContain("color: #0f766e");
    expect(ruleBody(".previewBadge")).toContain("background: #ecfdf5");
    expect(ruleBody(".previewSafetyNote")).toContain("background: #fffbeb");
  });

  it("defines a dark theme override for the main app surfaces", () => {
    expect(styles).toContain('html[data-theme="dark"]');
    expect(styles).toContain('html[data-theme="dark"] .sidebar');
    expect(styles).toContain('html[data-theme="dark"] .settingsForm');
    expect(styles).toContain('html[data-theme="dark"] .previewBadge');
    expect(styles).toContain('html[data-theme="dark"] .segmentedPill');
  });

  it("keeps install preview controls dark in dark theme", () => {
    expect(styles).toContain('html[data-theme="dark"] .installPreview.pendingInstall');
    expect(styles).toContain('html[data-theme="dark"] .installPreview.pendingInstall .previewMetaLabel');
    expect(styles).toContain('html[data-theme="dark"] .installPreview.pendingInstall .previewMetaValue');
    expect(styles).toContain('html[data-theme="dark"] .installPreview.pendingInstall .copyableValue');
    expect(styles).toContain('html[data-theme="dark"] .installPreview.pendingInstall .copyValueButton');
  });

  it("keeps inspector decision and history surfaces dark in dark theme", () => {
    expect(styles).toContain('html[data-theme="dark"] .decisionBlock.installedDecision');
    expect(styles).toContain('html[data-theme="dark"] .decisionBlock.installedDecision .lifecycleBlock');
    expect(styles).toContain('html[data-theme="dark"] .decisionBlock.installedDecision .dangerActionGroup');
    expect(styles).toContain('html[data-theme="dark"] .inspectorSecondaryAction');
    expect(styles).toContain('html[data-theme="dark"] .inspectorDangerAction');
    expect(styles).toContain('html[data-theme="dark"] .historyItem');
    expect(styles).toContain('html[data-theme="dark"] .historyItem.failed');
  });

  it("keeps release note nested content dark in dark theme", () => {
    expect(styles).toContain('html[data-theme="dark"] .releaseNotePreview .noteImage');
    expect(styles).toContain('html[data-theme="dark"] .releaseNotePreview .noteTableScroller');
    expect(styles).toContain('html[data-theme="dark"] .releaseNotePreview .noteCode');
  });

  it("renders the inspector summary as a compact two-line state block", () => {
    expect(ruleBody(".inspectorHead")).toContain("display: grid");
    expect(ruleBody(".inspectorHeadCopy")).toContain("gap: 6px");
    expect(ruleBody(".inspectorSummary")).toContain("flex-wrap: wrap");
    expect(ruleBody(".inspectorSummaryDetail")).toContain("word-break: break-word");
  });

  it("keeps a compressed two-column update manager until narrow widths", () => {
    expect(ruleBody(".contentGrid")).toContain("grid-template-columns: minmax(0, 1fr) minmax(320px, 360px)");
    expect(ruleBody(".inboxPanel")).toContain("overflow: hidden");
    expect(ruleBody(".appTable")).toContain("overflow-x: auto");
    expect(ruleBody(".tableRow")).toContain("min-width: 620px");
    expect(mediaRuleBody("@media (max-width: 1360px)", ".contentGrid")).toContain("grid-template-columns: minmax(0, 1fr) minmax(300px, 340px)");
    expect(mediaRuleBody("@media (max-width: 1360px)", ".tableRow")).toContain("min-width: 580px");
    expect(mediaRuleBody("@media (max-width: 1080px)", ".contentGrid")).toContain("grid-template-columns: minmax(0, 1fr)");
    expect(mediaRuleBody("@media (max-width: 1080px)", ".inspector")).toContain("position: static");
  });

  it("styles the lifecycle version select like the rest of the form controls", () => {
    expect(ruleBody(".decisionBlock")).toContain("background: #f8fafc");
    expect(appSource).toContain('installedLifecycleItem ? "installedDecision" : "needsInstallDecision"');
    expect(ruleBody(".decisionBlock.needsInstallDecision .decisionDetailList")).toContain("margin-top: 16px");
    expect(ruleBody(".decisionBlock.installedDecision")).toContain("background: #ffffff");
    expect(ruleBody(".decisionBlock.installedDecision .decisionHeaderValue")).toContain("font-size: 16px");
    expect(ruleBody(".decisionHeaderStatus")).toContain("display: flex");
    expect(ruleBody(".decisionStatePill")).toContain("flex: 0 0 auto");
    expect(ruleBody(".selectionSummary")).toContain("white-space: nowrap");
    expect(ruleBody(".selectionSummary.warning")).toContain("color: #92400e");
    expect(ruleBody(".decisionHeader")).toContain("justify-content: space-between");
    expect(ruleBody(".decisionHeader")).toContain("flex-wrap: wrap");
    expect(ruleBody(".decisionDetailList")).toContain("gap: 0");
    expect(ruleBody(".decisionDetailList")).toContain("border-top: 1px solid #e2e8f0");
    expect(ruleBody(".decisionDetailList div")).toContain("grid-template-columns: 72px minmax(0, 1fr)");
    expect(ruleBody(".decisionDetailList div")).toContain("padding: 7px 0");
    expect(ruleBody(".decisionDetailList div")).toContain("background: transparent");
    expect(ruleBody(".lifecycleBlock")).toContain("grid-template-columns: minmax(0, 1fr) auto");
    expect(ruleBody(".decisionBlock .lifecycleBlock")).toContain("border-top: 0");
    expect(ruleBody(".decisionBlock.installedDecision .lifecycleBlock")).toContain("background: #f8fafc");
    expect(ruleBody(".decisionBlock.installedDecision .lifecycleBlock")).toContain("border: 1px solid #e2e8f0");
    expect(ruleBody(".decisionBlock .lifecyclePreviewAction")).toContain("justify-content: center");
    expect(ruleBody(".decisionBlock .lifecyclePreviewAction")).toContain("min-width: 128px");
    expect(mediaRuleBody("@media (max-width: 560px)", ".lifecycleBlock")).toContain("grid-template-columns: minmax(0, 1fr)");
    expect(mediaRuleBody("@media (max-width: 560px)", ".decisionBlock .lifecyclePreviewAction")).toContain("width: 100%");
    expect(ruleBody(".decisionBlock .inspectorDangerAction")).toContain("color: #dc6b75");
    expect(ruleBody(".decisionBlock.installedDecision .dangerActionGroup")).toContain("background: #fff7f7");
    expect(ruleBody(".decisionBlock.installedDecision .dangerActionGroup")).toContain("border: 1px solid #fecdd3");
    expect(ruleBody(".decisionBlock.installedDecision .inspectorDangerAction")).toContain("min-height: 42px");
    expect(ruleBody(".selectControl")).toContain("min-height: 38px");
    expect(ruleBody(".selectControl")).toContain("border-radius: 8px");
    expect(ruleBody(".selectControl")).toContain("background: #ffffff");
    expect(ruleBody(".selectControl.loading")).toContain("border-color: #99d4ce");
    expect(ruleBody(".copyableValue")).toContain("grid-template-columns: minmax(0, 1fr) auto");
    expect(ruleBody(".copyableText")).toContain("text-overflow: ellipsis");
    expect(ruleBody(".copyValueButton")).toContain("width: 28px");
    expect(ruleBody(".actionButton:active:not(:disabled)")).toContain("transform: translateY(1px)");
  });
});

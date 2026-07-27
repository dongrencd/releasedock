# Implementation Notes

## Goal

Build a cross-platform GitHub Release manager. The user provides an `owner/repo` string or a GitHub URL, the tool reads the latest release, selects the right asset for the current platform and CPU architecture, builds an install plan, executes supported installs, and stores the result in a local manifest.

## Architecture

The repository uses a Rust workspace for the shared core and CLI. The Tauri desktop crate is built separately so a clean clone does not depend on frontend `dist` artifacts during workspace-level checks.

- `crates/core`: shared business rules used by both the CLI and the GUI.
- `crates/cli`: command-line entry point for argument parsing and output.
- `apps/desktop`: Tauri 2 + React desktop manager.

Core modules:

- `repo`: parse `owner/repo` strings and GitHub URLs.
- `release`: release data models and latest-release client logic.
- `asset_matcher`: deterministic asset scoring with managed-local formats preferred over system installers.
- `install_plan`: turn a release and matching asset into a confirmation-ready install plan, including whether the asset is managed locally, handled by a system package manager, or executed as an external installer.
- `config`: runtime configuration for the GitHub token, proxy, UI language, and install root.
- `manifest`: JSON manifest read and write support.
- `installer`: download assets, unpack archives and AppImage files, write manifests, and uninstall local installs. Managed-local updates stage the new file or directory before replacing the previous contents, so a failed replacement can restore the old install. System installers are handled conservatively and kept traceable; Linux `.deb` / `.rpm` / `.pkg.tar.*` installs also persist package identity so updates and uninstalls can flow through the system package manager.

## Current Scope

- Release parsing, release note handling, asset matching, manifest persistence, and install plan generation are implemented.
- Manifest format v2 distinguishes between managed local installs and system installer records, and tracks whether automatic uninstall is available. Linux system-package records also keep the package name and manager so the app can remove them without guessing from the cached installer file.
- CLI `install` supports live GitHub requests as well as `--release-fixture` and `--artifact-fixture` for offline testing. `--json` prints only the install plan, and `--yes` skips confirmation prompts. Interactive install and update confirmations show the selected management mode; bulk updates also summarize how many items are local managed installs, system packages, or external installers.
- CLI `config` reads, updates, and clears the GitHub token, proxy, and install root. The desktop settings page also exposes language switching, defaults to English, and shows the effective install root when no custom path is set.
- CLI `info` shows the latest release note and asset list.
- CLI `doctor` prints config file location, token/proxy state, and the install root without leaking token values.
- CLI `list`, `check`, `update`, and `uninstall` support the default manifest path and a `--manifest` override. `check` compares each installed app with the latest release and reports status.
- The GUI reads the real manifest and refreshes live GitHub release data. It uses a three-pane workbench layout, shows the important decisions directly, renders release notes with headings, lists, tables, quotes, links, and inline code, streams dashboard refresh progress item by item, keeps dashboard refresh state separate from install/uninstall progress, keeps task progress in the bottom status bar with visible motion for indeterminate work, and localizes those task/status strings with the selected UI language. The inspector also exposes each installed app's management mode and package manager label when available, and it shows a compact recent lifecycle history when the manifest has history for that repo. Lifecycle history is retained per repo with a small bounded tail so the manifest stays readable while still supporting troubleshooting. It also supports a compact four-state local filter set and bulk removal of uninstalled tracked items, and runs install flows through an install-preview-and-confirm step. GitHub API requests keep a short timeout, while asset downloads use a separate idle timeout so large files do not fail just because the total transfer takes longer than the preview budget.
- The GUI now also exposes a launch target for managed installs when one can be inferred, so the detail inspector can open the software itself in addition to the install location. AppImage installs use the installed file directly; archive installs infer a launchable executable from the extracted tree when possible.
- Asset selection prefers managed-local formats first, including Linux executables without an extension when they clearly match the current platform and architecture, then system installers as fallback assets. That keeps future update and uninstall flows inside ReleaseDock whenever a portable or directly runnable release artifact exists.
- Linux system packages record the package name and manager in the manifest so the app can refresh, uninstall, and display system-managed records without guessing from the cached installer file. The package inspect, install, and remove commands are generated from a single manager-specific command spec for Debian, RPM, and Pacman packages.
- Install and uninstall success paths keep the task context alive until the finished or failed state is visible, so the UI does not depend on the final event delivery alone and the bottom bar can show the terminal frame reliably.
- Successful installs clear the downloaded cache copy after the managed install finishes, while failed installs leave the download file in place for retry or diagnosis.
- Desktop settings are auto-saved after editing GitHub token, proxy, language, or install root values; reload remains available for discarding local draft changes. The install root field now shows the resolved default directory until a custom path is entered.
- Repositories without a published release render as a neutral `No release` state instead of a failure, and the seeded `releasedock` tracking entry can still be removed from the list.
- The install root is the local workspace base directory. Download caches go under `downloads/`, and managed installs go under `apps/<owner>-<repo>`. The settings page label "install root" refers to this base directory.
- GUI filtering only applies to the local managed list. It does not search GitHub globally. Public repositories do not require a token, while private repositories and frequent refreshes should use one.
- The installer supports `.tar.xz` archives. Windows `.exe` / `.msi` and Linux `.deb` / `.rpm` assets are marked as requiring user confirmation before installation.
- **System tray + background check**: closing the main window hides it to the system tray instead of quitting the app. The tray icon context menu offers "Check updates", "Open window", and "Quit" entries. A background timer re-checks all tracked and installed repositories for new releases at a configurable interval (default 30 minutes). When new updates are found, a native system notification fires and the tray tooltip shows the pending update count. The settings page exposes a "background check" toggle and an interval field. The feature is enabled by default.

## Next Steps

- Tighten system-level uninstall and permission confirmation flow.
- Add install history and more detailed failure feedback if needed.

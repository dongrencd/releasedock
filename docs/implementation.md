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
- `asset_matcher`: deterministic asset scoring.
- `install_plan`: turn a release and matching asset into a confirmation-ready install plan.
- `config`: runtime configuration for the GitHub token, proxy, UI language, and install root.
- `manifest`: JSON manifest read and write support.
- `installer`: download assets, unpack archives and AppImage files, write manifests, and uninstall local installs. System installers are handled conservatively and kept traceable.

## Current Scope

- Release parsing, release note handling, asset matching, manifest persistence, and install plan generation are implemented.
- Manifest format v2 distinguishes between managed local installs and system installer records, and tracks whether automatic uninstall is available.
- CLI `install` supports live GitHub requests as well as `--release-fixture` and `--artifact-fixture` for offline testing. `--json` prints only the install plan, and `--yes` skips confirmation prompts.
- CLI `config` reads, updates, and clears the GitHub token, proxy, and install root. The desktop settings page also exposes language switching and defaults to English.
- CLI `info` shows the latest release note and asset list.
- CLI `doctor` prints config file location, token/proxy state, and the install root without leaking token values.
- CLI `list`, `check`, `update`, and `uninstall` support the default manifest path and a `--manifest` override. `check` compares each installed app with the latest release and reports status.
- The GUI reads the real manifest and refreshes live GitHub release data. It uses a three-pane workbench layout, shows the important decisions directly, displays release note summaries and install paths in the detail pane, supports local list filtering and bulk removal of uninstalled tracked items, and runs install flows through a preview-and-confirm step.
- The install root is the local workspace base directory. Download caches go under `downloads/`, and managed installs go under `apps/<owner>-<repo>`. The settings page label "install root" refers to this base directory.
- GUI filtering only applies to the local managed list. It does not search GitHub globally. Public repositories do not require a token, while private repositories and frequent refreshes should use one.
- The installer supports `.tar.xz` archives. Windows `.exe` / `.msi` and Linux `.deb` / `.rpm` assets are marked as requiring user confirmation before installation.

## Next Steps

- Tighten system-level uninstall and permission confirmation flow.
- Add install history and more detailed failure feedback if needed.

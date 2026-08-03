# Changelog

## [0.2.14] - 2026-08-03

### Fixed
- Windows managed self-relaunch now uses the process access constant exposed by the current Windows API crate.

## [0.2.13] - 2026-07-31

### Changed
- Windows start-with-Windows launches in a lightweight hidden mode and defers dashboard and GitHub loading until the window is restored
- Background update checks expose complete, partial, and failed results instead of treating network or Token errors as no updates

### Fixed
- Windows duplicate launches and tray restores now reliably initialize the workspace, including when the restore event races frontend startup
- Failed background checks preserve the previous successful update badge and show actionable network or Token diagnostics

## [0.2.12] - 2026-07-29

### Changed
- Tagged Windows release artifacts use Authenticode signing when signing secrets are configured and otherwise continue publishing unsigned artifacts with an explicit CI warning

### Fixed
- Bare Windows `.exe` records already stored in ReleaseDock's managed app layout now stay on ReleaseDock-managed uninstall instead of opening Windows system uninstall settings

## [0.2.11] - 2026-07-29

### Added
- Large Release asset downloads can use multi-connection HTTP Range acceleration with settings for enablement and maximum connections
- Managed AppImage installs now create, update, roll back, and remove a basic Linux desktop entry

### Changed
- Windows system-installer records without a launch target are now adopted automatically during dashboard refresh when uninstall-registry metadata is available
- The desktop inspector no longer shows the secondary Open Release or manual re-detect install-result actions
- The desktop inspector and single-selection list action now use one Uninstall entry for managed files, Linux packages, and Windows system-installer records
- Installed app primary actions no longer fall back to Open Release when no launch or install path can be used

### Fixed
- Legacy Windows bare `.exe` records that were previously classified as external installers can update into managed-local executable installs

## [0.2.10] - 2026-07-29

### Changed
- GitHub Actions release workflow now uses current official artifact, checkout, and Node setup action versions to avoid Node.js 20 deprecation annotations
- Release artifact and packaging version markers updated to `0.2.10`

## [0.2.9] - 2026-07-29

### Fixed
- Windows release packaging now renames the built desktop executable relative to the desktop job working directory
- Windows release builds suppress Linux package command helper dead-code warnings on non-Linux targets

## [0.2.8] - 2026-07-29

### Fixed
- Windows Tauri release builds now compile the registry adoption matcher with explicit score typing
- Windows release builds no longer emit the related platform-only unused import and argument warnings

## [0.2.7] - 2026-07-29

### Changed
- README and Chinese README now document comparison boundaries and current limitations for GitHub Release asset management
- Release artifact and packaging version markers updated to `0.2.7`

### Fixed
- Windows CI frontend tests now normalize source line endings before checking the settings language/theme row structure

## [0.2.6] - 2026-07-29

### Added
- Bilingual GitHub README landing pages with top-of-file language switching
- A new dock-and-download desktop icon asset with matching PNG and ICO outputs

### Changed
- README content rewritten to describe the current GitHub Release manager behavior instead of only the initial launch scope
- Download handling now resumes interrupted assets with `.part` files and retries temporary network failures
- Windows system-installer adoption now reports the business error before platform gating for non-system records

### Fixed
- Release artifact and packaging version markers updated to `0.2.6`

## [0.2.0] - 2026-07-24

### Added
- System tray: close window hides to tray, right-click menu (Check Updates / Open Window / Quit), left-click restores window
- Background update check: tokio interval timer re-checks all managed repos for new releases, configurable interval (default 30 min)
- Native system notification when new updates are found in background
- Tray tooltip shows pending update count ("ReleaseDock · 3 updates available")
- Top-bar badge shows "N updates available" when background check finds updates
- Filter semantic: composite `actionRequired` filter aggregates apps needing user action (needsChoice + removable noRelease)
- Inspector: grouped action buttons with visual separator (primary / secondary / danger)
- Cache cleanup: successful installs remove download cache; failed installs preserve cache for retry
- i18n: notification body, tray menu, and tooltip support English and Simplified Chinese
- Config: `backgroundCheckEnabled`, `checkIntervalMinutes`, `trayHintShown` fields
- LICENSE file (MIT)
- Changelog file

### Changed
- InboxFilter internal type: `needsChoice` → `actionRequired` (visible labels unchanged)
- Install preview now hides regular action buttons while preview is active
- Settings page: added background check toggle and interval input
- Tray menu items use i18n strings (English / Simplified Chinese)
- `build-desktop.sh` cleanup: removed stale `ghrm`/`ghrm-desktop` binary references

### Fixed
- Background check: eliminated N× manifest load (load once, pass HashMap)
- Background check: hot-reload task when settings are saved (no restart needed)
- CI workflow: fixed package name references from old `ghrm-cli`/`ghrm-core` to `releasedock-cli`/`releasedock-core`
- `index.html` lang attribute: `zh-CN` → `en` (matches default English UI)
- Workspace `Cargo.toml`: added missing `thiserror` workspace dependency declaration
- `.gitignore`: removed dead `.env.example` reference

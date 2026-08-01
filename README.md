# ReleaseDock

**English | [简体中文](README_zh-CN.md)**

ReleaseDock is a desktop and CLI manager for software distributed through GitHub Releases. It helps you track, inspect, update, launch, roll back, and uninstall apps that usually start as manually downloaded `.exe`, `.zip`, `.AppImage`, `.tar.gz`, `.deb`, `.rpm`, or `.pkg.tar.*` assets.

ReleaseDock is not a package manager replacement. Keep using `winget`, `scoop`, `apt`, `flatpak`, Homebrew, or your OS package manager when a project already publishes through those channels. Use ReleaseDock for release-asset workflows that otherwise become scattered across your Downloads folder.

## What It Does

- Tracks GitHub repositories by `owner/repo` or GitHub URL.
- Reads release metadata, release notes, assets, publish time, and version history from GitHub.
- Selects the best asset for the current OS and CPU architecture.
- Shows an install preview before running installers or copying managed files.
- Downloads release assets with progress, retry, `.part` resume support, and multi-connection Range acceleration for large assets when supported.
- Verifies SHA-256 checksums when upstream checksum assets are available, or records the artifact digest when they are not.
- Manages AppImage, archive, portable executable, and Linux package installs with a local manifest.
- Creates and cleans up a basic Linux desktop entry for managed AppImage installs.
- Keeps Windows `.exe` / `.msi` and Linux `.deb` / `.rpm` installers behind explicit confirmation.
- Opens managed apps, install locations, installer package folders, release pages, and routes every installed app through one Uninstall entry.
- Supports guarded update, downgrade, rollback, uninstall, and remove-tracking flows.
- Runs background update checks from the system tray.
- Provides English and Simplified Chinese UI, plus follow-system, light, and dark themes.

## When To Use It

Use ReleaseDock when:

- You install tools directly from GitHub Releases.
- You want to see which tracked projects have updates without opening every repository.
- You want a local record of installed version, asset name, install path, package manager metadata, checksum state, and recent lifecycle activity.
- You need safer install/update confirmation for executable installers and system packages.

Do not use ReleaseDock as a blind installer for unknown binaries. It helps surface release information and guard local state, but a GitHub Release asset can still execute arbitrary code.

## Desktop App

The desktop app is a compact update workbench built with Tauri 2 and React.

- Left side: tracked repositories, local filters, selection, bulk remove/uninstall affordances.
- Right side: selected release, version policy, install preview, lifecycle history, release notes, and contextual actions.
- Bottom status strip: refresh, download, install, uninstall, rollback, and failure progress.
- Settings: GitHub token, GitHub proxy, install root, language, theme, background checks, start-with-Windows, notification permission actions, check interval, and download acceleration.
- System tray: close-to-tray behavior, consistent unminimize/focus restore, single-instance restore, manual check, restore window, quit, and update-count tooltip. Start-with-Windows launches in a lightweight hidden mode and defers dashboard/network loading until the window is restored. Background network or Token failures are surfaced as partial/failed diagnostics instead of being reported as zero updates.

Public repositories work without a token. Private repositories and frequent refreshes should use a GitHub token. The proxy setting applies to GitHub API queries and Release asset downloads.

On first startup, ReleaseDock shows local manifest and tracked-repository records before contacting GitHub. It performs one connection check and only loads release data when that check succeeds. Network, proxy, rate-limit, or token failures keep the local records visible and can be recovered through the existing network settings and Check updates actions instead of repeatedly refreshing the release list.

## CLI

The CLI uses the same Rust core as the desktop app.

```bash
cargo run -p releasedock-cli -- --help
cargo run -p releasedock-cli -- releases zyedidia/micro
cargo run -p releasedock-cli -- check
cargo run -p releasedock-cli -- install zyedidia/micro --json
cargo run -p releasedock-cli -- update zyedidia/micro --yes
cargo run -p releasedock-cli -- rollback zyedidia/micro
cargo run -p releasedock-cli -- uninstall zyedidia/micro
cargo run -p releasedock-cli -- config get
```

Use `--json` when you need machine-readable plans or reports. Use `--yes` only after reviewing what will run.

## Install Model

ReleaseDock distinguishes how an asset is managed:

- **Managed local**: AppImage, archives, portable executables, and directly runnable files copied under the ReleaseDock install root.
- **System package**: Linux `.deb`, `.rpm`, and `.pkg.tar.*` packages installed and removed through the system package manager.
- **External installer**: Windows `.exe` / `.msi` installers that may install software outside ReleaseDock's managed root.

Managed-local updates use staging and rollback snapshots so a failed replacement can keep or restore the previous install. System installers stay traceable: ReleaseDock records the installer package path, and on Windows automatically re-detects a real installed app location from the system uninstall registry during install follow-up, dashboard refresh, or the inspector's manual re-detect action when metadata is available.

Older Windows records may have classified a bare runnable `.exe` as an external installer. When the next selected asset is still a bare executable, ReleaseDock migrates that record into a managed-local install during the successful update. Real installers such as `.msi` and `setup.exe`, or records already adopted to a real installed app location, stay external installer records.

## Download Reliability

Release asset downloads write to a sibling `.part` file first. If a transfer is interrupted, the next attempt resumes with HTTP Range when the server supports it. Large assets use up to four Range connections by default when the server reports byte-range support, and fall back to the single-connection `.part` resume path when acceleration is disabled, unsupported, or a range probe/segment request fails. Temporary network failures, read timeouts, connection resets, and 5xx responses are retried before the cache file is finalized.

Checksum verification still happens after the full artifact is present. A partial file is never treated as an installed artifact.

## How It Compares

ReleaseDock focuses on GitHub Releases assets for desktop and CLI software on Windows and Linux.

- Use your OS package manager first when a project already ships through `winget`, `scoop`, `apt`, Flatpak, Homebrew, or similar channels.
- Use AppImage-focused tools such as Gear Lever, Zap, AM/AppMan, or AppImage Installer when you mainly need AppImage catalogs, icon extraction, file associations, or delta updates.
- Use Obtainium when you want Android app updates from APK sources.
- Use ReleaseDock when you manually install tools from GitHub Releases and want tracking, install previews, release notes, checksum records, resumable downloads, rollback for managed local installs, and a shared desktop/CLI workflow.

## Current Limitations

- macOS builds are not published yet.
- ReleaseDock does not provide an app catalog or app discovery store.
- AppImage integration creates a basic desktop entry only; it does not provide catalogs, icon extraction, file associations, or AppStream metadata.
- Delta updates such as zsync are not implemented; interrupted downloads can resume with HTTP Range when supported, and large full downloads can use multiple Range connections.
- Windows `.exe` / `.msi` installers still decide their own install behavior. ReleaseDock records the installer path and automatically re-detects install locations when registry metadata is available.
- ReleaseDock does not verify publisher identity beyond available checksum assets and locally recorded SHA-256 digests.

## Build From Source

### Workspace

```bash
cargo test
cargo run -p releasedock-cli -- --help
```

### Desktop

```bash
cd apps/desktop
npm install
npm test
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

### Linux Local Builds

```bash
bash scripts/linux/build-cli.sh
bash scripts/linux/build-desktop.sh
```

`build-desktop.sh` removes stale desktop outputs before building and then writes the desktop executable to `apps/desktop/src-tauri/target/release/releasedock` by default. Pass `--bundles` only when you want to try Debian/RPM packaging.

## Release Artifacts

Pushes to `main` and manual `CI Release Artifacts` workflow runs publish GitHub Actions artifacts:

- `releasedock-linux-x64`: Linux CLI.
- `releasedock-linux-x64-desktop`: Linux desktop build, including the executable plus Debian and RPM bundles when available.
- `releasedock-windows-x64-desktop`: Windows desktop build, including the executable plus NSIS and MSI installers when available.

Tags that match `v*.*.*` create a GitHub Release with the same version and upload available Linux and Windows artifacts.
Tagged Windows releases publish Authenticode-signed executable, NSIS, and MSI assets when the repository's code-signing secrets are configured. Builds without a trusted certificate may still show Windows SmartScreen warnings.
Tagged releases also include `SHA256SUMS` for the uploaded assets.

```bash
git tag v0.2.13
git push origin v0.2.13
```

## Documentation

- [Implementation notes](docs/implementation.md)
- [Release catalog and policy](docs/release-policy.md)
- [Release note behavior](docs/release-notes.md)
- [Windows UI notes](docs/windows-ui.md)
- [Asset matching rules](docs/asset-matching.md)
- [Build artifacts](docs/release-builds.md)
- [Security notes](docs/security.md)
- [Linux build scripts](scripts/linux/README.md)

## Security Notes

- Windows `.exe` / `.msi` and Linux `.deb` / `.rpm` installers require explicit confirmation.
- Official tagged Windows release assets are code-signed only when the repository's Windows signing secrets are configured. Unsigned release, local, or fork builds can appear as an unknown publisher in Windows SmartScreen.
- `GITHUB_TOKEN` is only used for GitHub API requests and must not be written to logs.
- ReleaseDock manages files it installed under its own install root. It only auto-adopts Windows system-installer records that ReleaseDock created and still lack a launch target.
- Windows system install detection reads uninstall-registry metadata only; it does not run uninstall commands during adoption.
- Windows system-installer uninstall opens the OS uninstall tool from the normal Uninstall confirmation instead of deleting system files directly.

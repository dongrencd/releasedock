# ReleaseDock

ReleaseDock helps you manage software that you installed from GitHub Releases and later need to update, uninstall, or inspect again. It targets the common workflow where you download `.exe`, `.zip`, `.AppImage`, `.tar.gz`, `.deb`, `.rpm`, or `.pkg.tar.*` files by hand and then lose track of them. It is not a replacement for `winget`, `scoop`, `apt`, `flatpak`, or Homebrew.

## Overview

The project is already usable in its first release:

- Rust core: repository parsing, release data models, release notes, asset matching, install plans, manifests, and runtime configuration.
- CLI: `install` can use a live GitHub latest release or fixtures, `--json` prints the install plan only, `--yes` skips interactive confirmation, interactive confirmations show the selected management mode, `config` manages the GitHub token, proxy, and install root, `check` compares installed apps with the latest release, `list` reads the manifest, and `update` / `uninstall` use the real execution path.
- Desktop GUI: Tauri 2 + React dashboard with real manifest loading, GitHub release refresh, release note viewing, install preview and confirmation, install execution, update actions for managed apps, uninstall and tracking removal, visible install progress, open-app and open-location shortcuts, and a settings page for token, proxy, language, and install root. The details panel now also shows the management mode, system package manager, and recent lifecycle history for installed apps. The default UI language is English, and the task/status strip follows the selected UI language.
- The sidebar footer shows the product name and subtitle; it does not act as a repository shortcut.
- Public repositories do not require a token. Private repositories and frequent API calls should use one.
- ReleaseDock prefers portable or directly runnable release assets first, including Linux executables without an extension when they are clearly marked for the current platform and architecture, then falls back to system installers when no managed format is available.
- The install root stores downloaded installers in `downloads/` and managed software in `apps/`. AppImage and archive installs stay under ReleaseDock control and update through staging replacement so a failed update keeps the previous managed contents. Linux `.deb` / `.rpm` / `.pkg.tar.*` installs are tracked with their package name so updates and uninstall use the system package manager. Windows `.exe` / `.msi` installers are still tracked as system installers, so the file is kept for reference while the actual install location is owned by the installer itself.

## Technology

- Core: Rust
- CLI: Rust + `clap`
- Desktop: Tauri 2
- Frontend: React + TypeScript + Vite
- Storage: JSON manifest v3

## Usage

```bash
cargo test
cargo run -p releasedock-cli -- --help
cargo run -p releasedock-cli -- list
cargo run -p releasedock-cli -- check
cargo run -p releasedock-cli -- install zyedidia/micro --json
cargo run -p releasedock-cli -- config get
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

### Linux local builds

```bash
bash scripts/linux/build-cli.sh
bash scripts/linux/build-desktop.sh
```

`build-desktop.sh` removes stale desktop outputs before building and then writes the desktop executable to `apps/desktop/src-tauri/target/release/releasedock` by default. Pass `--bundles` only when you want to try packaging.

### GitHub Actions artifacts

Pushes to `main` and manual `CI Release Artifacts` runs publish download artifacts in GitHub Actions:

- `releasedock-linux-x64`: Linux CLI.
- `releasedock-linux-x64-desktop`: Linux desktop build, including the executable plus Debian and RPM bundles when available.
- `releasedock-windows-x64-desktop`: Windows desktop build, including the executable plus NSIS and MSI installers when available.

After downloading the CLI artifact, verify it with:

```bash
./releasedock-linux-x64 --help
```

### GitHub Releases

Tags that match `v*.*.*` create a GitHub Release with the same version and upload the Linux CLI, Linux desktop executable, Linux Debian package, Linux RPM package, Windows desktop executable, NSIS installer, and MSI.

```bash
git tag v0.2.0
git push origin v0.2.0
```

## Documentation

- [Implementation notes](docs/implementation.md)
- [Release note behavior](docs/release-notes.md)
- [Windows UI notes](docs/windows-ui.md)
- [Asset matching rules](docs/asset-matching.md)
- [Build artifacts](docs/release-builds.md)
- [Security notes](docs/security.md)
- [Linux build scripts](scripts/linux/README.md)

## Security

- Windows `.exe` / `.msi` and Linux `.deb` / `.rpm` installers require explicit confirmation.
- `GITHUB_TOKEN` is only used for GitHub API requests and should never be written to logs.
- This first release only manages software installed by the tool itself or explicitly tracked by the user. It does not take over system-installed software.

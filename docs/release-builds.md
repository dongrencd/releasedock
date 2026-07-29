# Release Builds

## Goal

GitHub Actions builds downloadable artifacts on pushes to `main`, pull requests, and manual runs so the Linux CLI, Linux desktop app, and Windows desktop app can be verified quickly. Tag pushes create a GitHub Release with the official release assets.

## Workflow

Workflow file:

```text
.github/workflows/ci-release.yml
```

Triggers:

- push to `main`
- push a tag such as `v0.2.8`
- pull request
- manual `workflow_dispatch` from the GitHub UI

## Artifacts

- `releasedock-linux-x64`: Linux CLI executable, runnable with `./releasedock-linux-x64 --help`
- `releasedock-linux-x64-desktop`: Linux desktop build, including the executable plus Debian and RPM bundles when available
- `releasedock-windows-x64-desktop`: Windows Tauri desktop build, including the executable plus NSIS and MSI installers when available

CLI jobs only test and build `releasedock-core` and `releasedock-cli` so command-line artifacts do not need desktop dependencies. The desktop builds run in separate Linux and Windows jobs, and the Linux build uses the shared `scripts/linux/build-desktop.sh` helper so local and CI packaging stay aligned.
Artifact upload paths use repository-relative `apps/desktop/src-tauri/target/...` paths because `actions/upload-artifact` does not inherit the command step working directory.
The Windows desktop release uses the GUI subsystem, so it should open without a console window when launched normally. Windows open actions are routed through the system shell instead of `cmd`.

## GitHub Releases

Create a release by tagging the repository:

```bash
git tag v0.2.8
git push origin v0.2.8
```

When the tag workflow finishes, the matching release appears on the repository Releases page. Release assets include the Linux CLI executable, Linux desktop executable, Linux Debian package, Linux RPM package, Windows desktop executable, NSIS installer, and MSI.

If a release with the same tag already exists, the publish job fails. Remove the previous tag and release or use a new version number.

## Local Verification

Run these checks before submitting changes:

```bash
cargo test --workspace
bash scripts/linux/build-cli.sh
bash scripts/linux/build-desktop.sh
cd apps/desktop
npm test
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

The local desktop script removes stale desktop outputs first and then builds only `apps/desktop/src-tauri/target/release/releasedock` by default. It does not enter the AppImage bundling path.
If you want to exercise local packaging, pass `--bundles deb,rpm` explicitly. That matches the Linux release job and exercises the packaging toolchain that is verified in CI.

## Notes

- `main`, pull request, and manual runs upload Actions artifacts only; tag pushes create GitHub Releases.
- Do not commit `target/`, `node_modules/`, or `dist/`.
- GitHub tokens are for authentication only and must not be written into the repository, logs, or remote URLs.

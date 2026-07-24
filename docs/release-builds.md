# Release Builds

## Goal

GitHub Actions builds downloadable artifacts on pushes to `main`, pull requests, and manual runs so the Linux CLI and Windows desktop app can be verified quickly. Tag pushes create a GitHub Release with the official release assets.

## Workflow

Workflow file:

```text
.github/workflows/ci-release.yml
```

Triggers:

- push to `main`
- push a tag such as `v0.2.0`
- pull request
- manual `workflow_dispatch` from the GitHub UI

## Artifacts

- `releasedock-linux-x64`: Linux CLI, runnable with `./releasedock-linux-x64 --help`
- `releasedock-windows-x64-desktop`: Windows Tauri desktop build, including the app bundle and installers when available

CLI jobs only test and build `releasedock-core` and `releasedock-cli` so command-line artifacts do not need desktop dependencies. The desktop build runs in a separate Windows job with `npm run tauri build`.
Artifact upload paths use repository-relative `apps/desktop/src-tauri/target/...` paths because `actions/upload-artifact` does not inherit the command step working directory.

## GitHub Releases

Create a release by tagging the repository:

```bash
git tag v0.2.0
git push origin v0.2.0
```

When the tag workflow finishes, the matching release appears on the repository Releases page. Release assets include the Linux CLI, Windows desktop executable, NSIS installer, and MSI.

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
If you want to exercise local packaging, pass `--bundles deb,rpm` explicitly. AppImage is still better handled in Actions or in an environment with the full packaging toolchain.

## Notes

- `main`, pull request, and manual runs upload Actions artifacts only; tag pushes create GitHub Releases.
- Do not commit `target/`, `node_modules/`, or `dist/`.
- GitHub tokens are for authentication only and must not be written into the repository, logs, or remote URLs.

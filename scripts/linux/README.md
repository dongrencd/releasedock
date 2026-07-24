# Linux Build Scripts

## CLI

```bash
bash scripts/linux/build-cli.sh
```

## Desktop

```bash
bash scripts/linux/build-desktop.sh
```

The desktop script removes stale desktop outputs first and then builds only the desktop executable at `apps/desktop/src-tauri/target/release/releasedock` by default so local development does not get stuck in the AppImage packaging toolchain.
If you want to try desktop packaging, pass `--bundles`, for example:

```bash
bash scripts/linux/build-desktop.sh --bundles deb,rpm
```

The CLI artifact lands in `target/release/releasedock`, and the desktop artifact lands in `apps/desktop/src-tauri/target/release/releasedock`.

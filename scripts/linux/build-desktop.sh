#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DESKTOP_DIR="$ROOT_DIR/apps/desktop"
BUILD_BUNDLES=""

while [ $# -gt 0 ]; do
  case "$1" in
    --bundles)
      if [ $# -lt 2 ]; then
        echo "[desktop] --bundles needs a comma-separated value"
        exit 1
      fi
      BUILD_BUNDLES="$2"
      shift 2
      ;;
    *)
      echo "[desktop] unknown argument: $1"
      exit 1
      ;;
  esac
done

cd "$DESKTOP_DIR"

RELEASE_DIR="$DESKTOP_DIR/src-tauri/target/release"
STALE_TARGETS=(
  "$DESKTOP_DIR/dist"
  "$RELEASE_DIR/releasedock"
  "$RELEASE_DIR/releasedock.d"
  "$RELEASE_DIR/bundle"
)

for target in "${STALE_TARGETS[@]}"; do
  if [ -e "$target" ] || [ -L "$target" ]; then
    echo "[desktop] rm -rf $target"
    rm -rf "$target"
  fi
done

if [ ! -d node_modules ]; then
  echo "[desktop] npm ci --registry=https://registry.npmmirror.com"
  npm ci --registry=https://registry.npmmirror.com
fi

echo "[desktop] npm test"
npm test

echo "[desktop] npm run build"
npm run build

echo "[desktop] cargo check --manifest-path src-tauri/Cargo.toml"
cargo check --manifest-path src-tauri/Cargo.toml

echo "[desktop] cargo build --manifest-path src-tauri/Cargo.toml --release"
cargo build --manifest-path src-tauri/Cargo.toml --release

echo "[desktop] built desktop binary: $DESKTOP_DIR/src-tauri/target/release/releasedock"

if [ -n "$BUILD_BUNDLES" ]; then
  echo "[desktop] npm run tauri build -- --bundles $BUILD_BUNDLES"
  npm run tauri build -- --bundles "$BUILD_BUNDLES"
fi

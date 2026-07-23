#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "$ROOT_DIR"

echo "[cli] cargo build -p releasedock-cli --release"
cargo build -p releasedock-cli --release

echo "[cli] cargo test -p releasedock-cli"
cargo test -p releasedock-cli

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "$ROOT_DIR"

echo "[cli] cargo build -p ghrm-cli --release"
cargo build -p ghrm-cli --release

echo "[cli] cargo test -p ghrm-cli"
cargo test -p ghrm-cli

# Repository Guidelines

## Project Structure

ReleaseDock is a Rust workspace. `crates/core` contains release parsing, asset matching, install plans, manifests, and config logic. `crates/cli` provides the command-line interface and its integration tests. The desktop app lives in `apps/desktop`, with React and TypeScript sources in `src` and the Tauri backend in `src-tauri/src`. Project docs are in `docs/`, and Linux build scripts live in `scripts/linux/`.

## Build, Test, and Development Commands

- `cargo test` runs the Rust workspace test suite.
- `cargo run -p releasedock-cli -- --help` prints the CLI surface.
- `cargo run -p releasedock-cli -- list` reads the local manifest.
- `cargo run -p releasedock-cli -- install zyedidia/micro --json` prints an install plan without executing it.
- `cd apps/desktop && npm test` runs the frontend tests.
- `cd apps/desktop && npm run build` type-checks and builds the Vite app.
- `cd apps/desktop && cargo check --manifest-path src-tauri/Cargo.toml` checks the Tauri backend.
- `bash scripts/linux/build-cli.sh` builds the Linux CLI artifact.

## Coding Style & Naming

Use Rust 2024 edition conventions and keep formatting consistent with `cargo fmt`. Prefer `snake_case` for files, modules, functions, and variables, and `PascalCase` for types and React components. Keep business logic in `crates/core`; keep CLI code focused on argument parsing and output; keep the desktop UI talking to the backend through `backend.ts`.

## Testing Guidelines

Add core behavior tests under `crates/core/tests/` or module unit tests near the code they cover. Add CLI behavior tests under `crates/cli/tests/`, with fixtures in `crates/cli/tests/fixtures/`. Put frontend model tests in `apps/desktop/src/*.test.ts`. Run the smallest test set that covers your change, and note the commands in your PR.

## Commit & Pull Request Guidelines

Recent commits use short prefixes such as `feat:`, `ci:`, and `chore:`. Follow the same pattern when it fits, with a concise imperative subject. PRs should explain the user-facing change, list the tests you ran, and link the related issue when one exists. Include screenshots or screen recordings for UI work, and call out any risk or rollback concerns for installer, security, or release-path changes.

## Security & Configuration

Do not log `GITHUB_TOKEN`, proxy credentials, or local install roots. Prefer fixtures, `--json`, or temporary directories when testing install, update, and uninstall flows so you do not touch real system locations by accident.

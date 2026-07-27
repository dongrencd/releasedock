# Asset Matching

## Inputs

- GitHub Release asset name
- current operating system
- current CPU architecture
- file extension

## Platform Keywords

- Windows: `windows`, `win32`, `win64`, `win`
- Linux: `linux`, `appimage`
- macOS: `macos`, `darwin`, `apple`

## Architecture Keywords

- x64: `x86_64`, `amd64`, `x64`
- arm64: `aarch64`, `arm64`

## Format Priority

Windows:

1. `.zip` and other portable archives
2. `.msi`, `.exe`

Linux:

1. `.AppImage`
2. `linux` + arch keywords with no file extension, for example `releasedock-linux-x64`
3. `.tar.gz`, `.tgz`, `.tar.xz`, `.zip`
4. `.deb`, `.rpm`, `.pkg.tar.zst`, `.pkg.tar.xz`, `.pkg.tar.gz`

General rule:

1. Direct-run or managed-local formats are preferred first.
2. Linux executables without an extension are treated as managed-local assets when the file name clearly identifies the platform and architecture.
3. System installers are treated as fallback assets when no managed format is available.
4. Auxiliary files such as checksums, release notes, manifests, readmes, licenses, and source archives are not installable assets, even if their names contain platform keywords.

## Conflict Handling

The first release chooses the highest-scoring installable asset. If multiple assets tie later, the CLI should require `--asset` or an interactive choice, and the GUI should show the candidate list.

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

1. `.msi`
2. `.exe`
3. `.zip`

Linux:

1. `.AppImage`
2. `.tar.gz`, `.tgz`, `.tar.xz`
3. `.zip`
4. `.deb`, `.rpm`

## Conflict Handling

The first release chooses the highest-scoring asset. If multiple assets tie later, the CLI should require `--asset` or an interactive choice, and the GUI should show the candidate list.

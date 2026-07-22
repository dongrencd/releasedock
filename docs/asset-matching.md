# Asset Matching

## 输入

- GitHub Release asset name
- 当前 OS
- 当前 CPU 架构
- 文件扩展名

## 平台关键词

- Windows: `windows`, `win32`, `win64`, `win`
- Linux: `linux`, `appimage`
- macOS: `macos`, `darwin`, `apple`

## 架构关键词

- x64: `x86_64`, `amd64`, `x64`
- arm64: `aarch64`, `arm64`

## 格式优先级

Windows：

1. `.msi`
2. `.exe`
3. `.zip`

Linux：

1. `.AppImage`
2. `.tar.gz`, `.tgz`, `.tar.xz`
3. `.zip`
4. `.deb`, `.rpm`

## 冲突处理

第一版选择最高分 asset。后续如果多个 asset 分数相同，CLI 应要求用户传 `--asset` 或进入交互选择，GUI 应展示候选列表。

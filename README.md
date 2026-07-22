# GitHub Release Manager

GitHub Release Manager 管理从 GitHub Releases 安装的软件。它面向手工下载 `.exe`、`.zip`、`.AppImage`、`.tar.gz` 后难以更新的场景，不替代 `winget`、`scoop`、`apt`、`flatpak` 或 Homebrew。

## 当前状态

这是第一版可用实现，已经包含：

- Rust core：仓库解析、release 数据模型、release note、asset 匹配、安装计划、manifest、运行时配置。
- CLI：`install` 可基于真实 GitHub latest release 或 fixture 执行安装，`--json` 可只输出安装计划，`--yes` 可跳过交互确认，`config` 可统一管理 GitHub token、代理和安装根目录，`check` 会对已安装软件逐个比对 latest release，`list` 可读取 manifest，`update`/`uninstall` 已接真实执行路径，系统安装器记录会明确标记为需系统卸载。
- Desktop GUI：Tauri 2 + React 管理台，已接真实 manifest 读取、GitHub release 刷新、release note 查看、安装预览和确认、安装执行以及卸载/移除跟踪，并提供设置页管理 GitHub token、代理和安装根目录。
- Desktop GUI 首次启动会默认跟踪当前项目 `dongrencd/gh-release-manager`，方便直接查看本仓库的 release。
- 文档：实现方案、桌面 UI、release note、安全边界、asset 匹配规则。

## 技术栈

- Core: Rust
- CLI: Rust + `clap`
- Desktop: Tauri 2
- Frontend: React + TypeScript + Vite
- Storage: JSON manifest v2

## 常用命令

```bash
cargo test
cargo run -p ghrm-cli -- --help
cargo run -p ghrm-cli -- list
cargo run -p ghrm-cli -- check
cargo run -p ghrm-cli -- install zyedidia/micro --json
cargo run -p ghrm-cli -- config get
```

## Actions 产物下载

每次推送到 `main` 或手动触发 `CI Release Artifacts` workflow 后，可以在 GitHub Actions 页面下载构建产物：

- `ghrm-linux-x64`：Linux CLI。
- `ghrm-windows-x64-desktop`：Windows Tauri 桌面程序，可包含免安装 exe 和安装包。

CLI 下载后可先验证：

```bash
./ghrm-linux-x64 --help
```

桌面端：

```bash
cd apps/desktop
npm install
npm test
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

## Releases 下载

推送 `v*.*.*` tag 后，GitHub Actions 会创建同名 GitHub Release，并上传 Linux CLI、Windows 桌面 exe、NSIS 安装包和 MSI。

```bash
git tag v0.1.0
git push origin v0.1.0
```

## 安全边界

- 当前安装器支持 archive、AppImage，以及在对应平台上的 Windows `.exe/.msi` 和 Linux `.deb/.rpm` 执行；系统级卸载仍保守处理。
- `GITHUB_TOKEN` 只用于 GitHub API，不应写入日志。
- 本工具第一版只管理自己安装的软件，不接管系统已有软件。

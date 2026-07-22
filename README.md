# GitHub Release Manager

GitHub Release Manager 管理从 GitHub Releases 安装的软件。它面向手工下载 `.exe`、`.zip`、`.AppImage`、`.tar.gz` 后难以更新的场景，不替代 `winget`、`scoop`、`apt`、`flatpak` 或 Homebrew。

## 当前状态

这是第一版工程骨架，已经包含：

- Rust core：仓库解析、release 数据模型、release note、asset 匹配、安装计划、manifest。
- CLI：`install` 可基于真实 GitHub latest release 或 fixture 生成安装计划，`list` 可读取 manifest。
- Windows GUI 原型：Tauri 2 + React 管理台布局，包含更新收件箱、详情检查器和 release note 查看。
- 文档：实现方案、Windows UI、release note、安全边界、asset 匹配规则。

## 技术栈

- Core: Rust
- CLI: Rust + `clap`
- Desktop: Tauri 2
- Frontend: React + TypeScript + Vite
- Storage: JSON manifest v1

## 常用命令

```bash
cargo test
cargo run -p ghrm -- --help
cargo run -p ghrm -- list
cargo run -p ghrm -- install zyedidia/micro
```

## Actions 产物下载

每次推送到 `main` 或手动触发 `CI Release Artifacts` workflow 后，可以在 GitHub Actions 页面下载构建产物：

- `ghrm-linux-x64`：Linux CLI。
- `ghrm-windows-x64`：Windows CLI。
- `ghrm-desktop-windows-x64`：Windows Tauri 桌面程序，可包含免安装 exe 和安装包。

CLI 下载后可先验证：

```bash
./ghrm-linux-x64 --help
```

Windows 上验证：

```powershell
.\ghrm-windows-x64.exe --help
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

推送 `v*.*.*` tag 后，GitHub Actions 会创建同名 GitHub Release，并上传 Linux CLI、Windows CLI、Windows 桌面 exe、NSIS 安装包和 MSI。

```bash
git tag v0.1.0
git push origin v0.1.0
```

## 安全边界

- Windows `.exe/.msi` 第一版只生成安装计划，执行前必须用户确认。
- `GITHUB_TOKEN` 只用于 GitHub API，不应写入日志。
- 本工具第一版只管理自己安装的软件，不接管系统已有软件。

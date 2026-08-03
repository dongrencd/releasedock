# ReleaseDock

**[English](README.md) | 简体中文**

ReleaseDock 是一个面向 GitHub Releases 软件分发方式的桌面和命令行管理工具。它适合管理那些通常需要手动下载 `.exe`、`.zip`、`.AppImage`、`.tar.gz`、`.deb`、`.rpm` 或 `.pkg.tar.*` 资产的软件，并帮助你后续检查、更新、打开、回滚和卸载。

ReleaseDock 不是系统包管理器的替代品。如果项目已经通过 `winget`、`scoop`、`apt`、`flatpak`、Homebrew 或系统自带包管理器发布，优先继续使用这些渠道。ReleaseDock 解决的是 GitHub Release 资产手动下载后容易散落在 Downloads 目录、后续难以追踪的问题。

## 能做什么

- 通过 `owner/repo` 或 GitHub URL 跟踪仓库。
- 读取 GitHub Release 元数据、发布说明、资产列表、发布时间和版本历史。
- 可以在添加仓库工作区搜索 GitHub 仓库，并预检最新 release 是否包含当前平台可安装资产。
- 根据当前操作系统和 CPU 架构选择最合适的资产。
- 在安装或执行安装器前展示安装预览。
- 下载 Release 资产时显示进度，并支持失败重试、`.part` 断点续传，以及服务器支持时的大文件多连接 Range 加速。
- 上游提供 SHA-256 校验文件时执行校验；没有校验文件时记录本地计算出的摘要。
- 通过本地 manifest 管理 AppImage、压缩包、便携可执行文件和 Linux 系统包安装。
- 为托管的 AppImage 创建和清理基础 Linux 桌面启动项。
- Windows `.exe` / `.msi` 和 Linux `.deb` / `.rpm` 等系统安装器必须显式确认后才会执行。
- 根据软件状态打开应用、安装目录、安装包目录、Release 页面，并让所有已安装软件共用同一个“卸载”入口。
- 支持受保护的更新、降级、回滚、卸载和移除跟踪流程。
- 通过系统托盘执行后台更新检查。
- 支持英文和简体中文界面，并提供跟随系统、浅色、深色主题。

## 适用场景

适合使用 ReleaseDock 的情况：

- 你经常直接从 GitHub Releases 安装工具或桌面软件。
- 你想集中查看跟踪的软件是否有新版本，而不是逐个打开仓库。
- 你需要保存本机安装版本、资产名称、安装路径、系统包元数据、校验状态和最近操作记录。
- 你希望对可执行安装器和系统包安装多一层确认与记录。

不要把 ReleaseDock 当成未知二进制文件的安全背书。它会展示 Release 信息并保护本地状态，但 GitHub Release 资产本身仍然可能执行任意代码。

## 桌面应用

桌面版是一个紧凑的更新工作台，基于 Tauri 2 和 React 构建。

- 左侧：跟踪仓库、本地筛选、选择状态、批量移除和卸载入口。
- 右侧：当前 Release、版本策略、安装预览、生命周期历史、发布说明和上下文动作。
- 底部状态栏：刷新、下载、安装、卸载、回滚和失败进度。
- 设置：GitHub Token、GitHub 代理、安装根目录、界面语言、主题、关闭窗口行为、后台检查、开机后后台启动、通知权限操作、检查间隔和下载加速；同时提供打开本地数据目录和退出应用操作。
- 系统托盘：可配置关闭后驻留托盘或退出程序、一致的取消最小化/聚焦恢复、单实例恢复窗口、手动检查、恢复窗口、本地化退出菜单和更新数量提示。

公开仓库不需要 Token。私有仓库或频繁刷新建议配置 GitHub Token。代理设置会作用于 GitHub API 查询和 Release 资产下载。

版本策略按仓库在当前进程内缓存。切换已加载的软件时会立即恢复该软件的版本列表和上次目标版本，不会重复请求 GitHub；手动重试、Dashboard 刷新或修改 Token/代理后才会重新加载。

首次打开时，ReleaseDock 会先显示本地 manifest 和已跟踪仓库记录，再联系 GitHub。只有连接检查成功后才会加载 Release 数据；网络、代理、限流或 Token 问题会保留本地记录，并通过现有网络设置和“检查更新”入口恢复，不会反复刷新 Release 列表。

## 命令行

CLI 和桌面版共用同一套 Rust core 逻辑。

```bash
cargo run -p releasedock-cli -- --help
cargo run -p releasedock-cli -- releases zyedidia/micro
cargo run -p releasedock-cli -- check
cargo run -p releasedock-cli -- install zyedidia/micro --json
cargo run -p releasedock-cli -- update zyedidia/micro --yes
cargo run -p releasedock-cli -- rollback zyedidia/micro
cargo run -p releasedock-cli -- uninstall zyedidia/micro
cargo run -p releasedock-cli -- config get
```

需要机器可读输出时使用 `--json`。只有在确认即将执行的操作后，才建议使用 `--yes` 跳过交互确认。

## 安装模型

ReleaseDock 会区分资产的管理方式：

- **本地托管**：AppImage、压缩包、便携可执行文件和直接可运行文件会复制到 ReleaseDock 安装根目录下。
- **系统包**：Linux `.deb`、`.rpm` 和 `.pkg.tar.*` 通过系统包管理器安装和移除。
- **外部安装器**：Windows `.exe` / `.msi` 可能把软件安装到 ReleaseDock 管理目录之外。

本地托管更新使用 staging 和 rollback snapshot，替换失败时可以保留或恢复旧版本。系统安装器会保留可追踪信息：ReleaseDock 记录安装包路径，并且在 Windows 上会在安装后、dashboard 刷新或 Inspector 手动重新检测时，自动从系统卸载注册表重新探测真实安装目录。如果安装器已经执行但暂时没有可用的注册表元数据，桌面端会继续显示该软件，把主动作切换为“重新检测安装状态”，并保留“执行安装包”和“打开安装包目录”两个明确的次级动作。

当 Windows `.exe` 或 `.msi` 安装器执行后仍未发现安装位置时，ReleaseDock 会每四秒检查一次卸载注册表，最多持续两分钟。匹配成功只更新该软件的 dashboard 行；超时后仍保留“重新检测安装状态”和安装包恢复动作。

当 Windows 桌面版第一次从手动下载的 `ReleaseDock-windows-x64.exe` 启动时，ReleaseDock 会把当前运行的 exe 复制到自己的管理目录，并提示重启一次。重启会通过 ReleaseDock 内部 helper 和 Windows shell 交接，不使用 `cmd` 脚本，也不会临时弹出控制台。重启后，后续更新和打开动作都会使用托管副本；原下载文件不会被删除。

较早版本可能把裸可运行的 Windows `.exe` 记录成外部安装器。当下一次选中的资产仍是裸可执行文件时，ReleaseDock 会在成功更新时把这条记录迁移为本地托管安装。`.msi`、`setup.exe` 这类真实安装器，或已经接管到真实安装目录的记录，仍保持外部安装器语义。

## 下载可靠性

Release 资产下载会先写入同目录的 `.part` 文件。传输中断后，下次会在服务器支持时通过 HTTP Range 继续下载。服务器声明支持字节范围时，大文件默认最多使用 4 个 Range 连接下载；关闭加速、服务器不支持、Range 探测失败或分片请求失败时会回退到单连接 `.part` 续传路径。临时网络错误、读超时、连接重置和 5xx 响应会自动重试，成功后才会把缓存文件最终落盘。

完整文件下载完成后才会执行 checksum 校验。部分下载文件不会被当成已安装资产使用。

## 和同类工具的区别

ReleaseDock 聚焦 Windows 和 Linux 上通过 GitHub Releases 分发的桌面软件和 CLI 工具。

- 如果项目已经通过 `winget`、`scoop`、`apt`、Flatpak、Homebrew 或类似渠道发布，优先使用系统包管理器。
- 如果主要管理 AppImage，并且需要应用目录、图标抽取、文件关联或 delta 更新，Gear Lever、Zap、AM/AppMan、AppImage Installer 这类工具更适合。
- 如果要管理 Android APK 来源更新，Obtainium 更适合。
- 如果你现在主要是手动从 GitHub Releases 下载软件，并希望统一跟踪版本、安装预览、发布说明、checksum 记录、断点续传、本地托管回滚，以及桌面版和 CLI 共用流程，ReleaseDock 更适合。

## 当前限制

- 目前还没有发布 macOS 构建。
- ReleaseDock 不提供应用目录或应用发现商店。
- AppImage 集成只创建基础桌面启动项，不提供应用目录、图标抽取、文件关联或 AppStream 元数据。
- 尚未实现 zsync 这类 delta 更新；中断下载会在服务器支持时通过 HTTP Range 断点续传，大文件完整下载可使用多个 Range 连接。
- Windows `.exe` / `.msi` 安装器仍由安装包自身决定安装行为。ReleaseDock 会记录安装包路径，并在注册表元数据可用时自动重新探测安装目录。
- ReleaseDock 不验证发布者身份，只使用上游 checksum 资产和本地记录的 SHA-256 摘要。

## 从源码构建

### Workspace

```bash
cargo test
cargo run -p releasedock-cli -- --help
```

### 桌面版

```bash
cd apps/desktop
npm install
npm test
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

### Linux 本地构建

```bash
bash scripts/linux/build-cli.sh
bash scripts/linux/build-desktop.sh
```

`build-desktop.sh` 会先清理旧的桌面构建产物，再默认生成 `apps/desktop/src-tauri/target/release/releasedock`。只有在需要尝试 Debian/RPM 打包时才传入 `--bundles`。

## Release 产物

推送到 `main` 或手动运行 `CI Release Artifacts` workflow 会发布 GitHub Actions artifacts：

- `releasedock-linux-x64`：Linux CLI。
- `releasedock-linux-x64-desktop`：Linux 桌面版，包含可执行文件，以及可用时的 Debian/RPM 包。
- `releasedock-windows-x64-desktop`：Windows 桌面版，包含可执行文件，以及可用时的 NSIS/MSI 安装器。

匹配 `v*.*.*` 的 tag 会创建同版本 GitHub Release，并上传可用的 Linux 和 Windows 产物。
带 tag 的 Windows Release 在仓库配置代码签名 secrets 时，会发布 Authenticode 签名后的可执行文件、NSIS 和 MSI 资产。没有可信证书的构建仍可能触发 Windows SmartScreen 提示。
带 tag 的 Release 还会包含 `SHA256SUMS`，用于核对已上传产物的 SHA-256。

```bash
git tag v0.2.14
git push origin v0.2.14
```

## 文档

- [实现说明](docs/implementation.md)
- [Release 目录和版本策略](docs/release-policy.md)
- [发布说明解析行为](docs/release-notes.md)
- [Windows UI 说明](docs/windows-ui.md)
- [资产匹配规则](docs/asset-matching.md)
- [构建产物](docs/release-builds.md)
- [安全说明](docs/security.md)
- [Linux 构建脚本](scripts/linux/README.md)

## 安全说明

- Windows `.exe` / `.msi` 和 Linux `.deb` / `.rpm` 安装器必须显式确认。
- 官方 tag 发布的 Windows 产物只有在仓库配置 Windows 签名 secrets 后才会执行代码签名；未签名的 Release、本地构建或 fork 构建仍可能在 Windows SmartScreen 中显示发布者未知。
- `GITHUB_TOKEN` 只用于 GitHub API 请求，不能写入日志。
- ReleaseDock 只管理由它安装到自身安装根目录下的文件；自动接管只针对 ReleaseDock 已创建且还没有启动路径的 Windows 系统安装器记录。
- Windows 系统安装探测只读取卸载注册表元数据；接管探测不会执行卸载命令。
- Windows 系统安装器记录确认“卸载”后会打开系统卸载工具，不会直接删除系统安装目录。

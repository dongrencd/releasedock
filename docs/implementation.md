# Implementation

## 目标

第一版实现一个跨平台 GitHub Release 软件管理器。用户输入 `owner/repo` 或 GitHub URL，工具读取 latest release，按当前平台和 CPU 架构选择合适 asset，生成安装计划并执行可支持的安装，最后记录到本地 manifest。

## 架构

项目采用 Rust workspace 管理 core 和 CLI；Tauri 桌面 crate 独立构建，避免 `cargo test --workspace` 在干净 clone 上依赖前端 `dist`。

- `crates/core`：所有业务规则。CLI 和 GUI 必须复用这里的接口。
- `crates/cli`：命令行入口，负责参数解析和结果输出。
- `apps/desktop`：Tauri 2 + React 桌面管理台。

core 中的主要模块：

- `repo`：解析 `owner/repo` 和 GitHub URL。
- `release`：GitHub release 数据结构和 latest release client。
- `asset_matcher`：确定性 asset 打分。
- `install_plan`：把 release + asset 转成可确认的安装计划。
- `config`：本地运行时配置，统一保存 GitHub token、代理和安装根目录。
- `manifest`：JSON manifest 读写。
- `installer`：下载 asset、解包 archive/AppImage、写入 manifest、卸载本地安装；系统安装器会保守执行并保留可追踪状态。

## 当前实现范围

- 已实现 release 解析、release note 字段、asset 匹配、manifest 读写和安装计划生成。
- manifest 已升级到 v2，安装记录会区分本地托管路径和系统安装器记录，并标记是否支持自动卸载。
- CLI `install` 支持真实 GitHub 请求，也支持 `--release-fixture` 和 `--artifact-fixture` 做离线测试；`--json` 仅输出安装计划，`--yes` 可跳过交互确认。
- CLI `config` 可以读取、设置和清除 GitHub token、代理和安装根目录。
- CLI `info` 支持查看 latest release 的 release note 和 asset 列表。
- CLI `list`、`check`、`update`、`uninstall` 支持默认 manifest 和 `--manifest` 覆盖路径；`check` 会逐个对已安装软件比对 latest release，并输出更新状态。
- GUI 已接入真实 manifest 读取和 GitHub release 刷新，不再依赖静态 demo 数据；首次启动会默认跟踪当前项目 `dongrencd/gh-release-manager`，安装流程先生成预览，再由用户确认后执行，设置页可编辑 GitHub token、代理和安装根目录，外部 GitHub 链接通过 Tauri 后端命令打开，并限制为 https://github.com 域名。

## 下一阶段

- 增加更细的安装进度和失败回显。
- 让更新/卸载在桌面端有更明确的动作反馈和历史记录。
- 继续收紧系统级卸载与权限确认策略。

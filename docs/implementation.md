# Implementation

## 目标

第一版实现一个跨平台 GitHub Release 软件管理器。用户输入 `owner/repo` 或 GitHub URL，工具读取 latest release，按当前平台和 CPU 架构选择合适 asset，生成安装计划，并记录到本地 manifest。

## 架构

项目采用 Rust workspace：

- `crates/core`：所有业务规则。CLI 和 GUI 必须复用这里的接口。
- `crates/cli`：命令行入口，负责参数解析和结果输出。
- `apps/desktop`：Tauri 2 + React Windows 管理台。

core 中的主要模块：

- `repo`：解析 `owner/repo` 和 GitHub URL。
- `release`：GitHub release 数据结构和 latest release client。
- `asset_matcher`：确定性 asset 打分。
- `install_plan`：把 release + asset 转成可确认的安装计划。
- `manifest`：JSON manifest 读写。

## 当前实现范围

- 已实现 release 解析、release note 字段、asset 匹配、manifest 读写和安装计划生成。
- CLI `install` 支持真实 GitHub 请求，也支持 `--release-fixture` 做离线测试。
- CLI `info` 支持查看 latest release 的 release note 和 asset 列表。
- CLI `list` 支持默认 manifest 和 `--manifest` 覆盖路径。
- GUI 当前是可构建的 Windows 管理台原型，展示静态 demo 数据。

## 下一阶段

- 实现下载器和 archive 解压。
- 实现 manifest upsert 和 uninstall 删除逻辑。
- GUI 从静态 demo 数据切换为 Tauri command 调 core。
- 更新收件箱接入真实 release note，并支持打开 GitHub release 页面。

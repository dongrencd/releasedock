# Security

## 威胁模型

GitHub Release asset 不等于可信软件。项目可能被攻击，release asset 可能被替换，安装器可能执行任意代码。

## 第一版策略

- Windows `.exe/.msi` 不静默执行。
- 执行安装器前必须二次确认；CLI 提供 `--yes`，桌面端保留确认按钮。
- 桌面端会先展示安装预览，再由用户确认后执行安装。
- Linux `.deb/.rpm` 走系统安装器时只保留可追踪状态，不把本工具的缓存目录当作真实安装结果。
- private token 只用于 GitHub API，配置会保存在本机数据目录，不回传仓库。
- token 不写日志。

## 后续增强

- 支持 SHA256 校验。
- 支持签名信息展示。
- 支持下载前显示 release 作者和 tag 信息。
- 支持更新前展示 release note 原文。
- 支持更新失败回滚。

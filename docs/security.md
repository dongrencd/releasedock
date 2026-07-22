# Security

## 威胁模型

GitHub Release asset 不等于可信软件。项目可能被攻击，release asset 可能被替换，安装器可能执行任意代码。

## 第一版策略

- Windows `.exe/.msi` 不静默执行。
- 执行安装器前必须二次确认。
- private token 只用于 GitHub API。
- token 不写日志。

## 后续增强

- 支持 SHA256 校验。
- 支持签名信息展示。
- 支持下载前显示 release 作者和 tag 信息。
- 支持更新前展示 release note 原文。
- 支持更新失败回滚。

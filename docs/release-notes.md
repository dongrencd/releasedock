# Release Notes

## 目标

用户在更新 GitHub Release 软件前，需要直接看到这个版本改了什么。第一版展示 GitHub Release `body` 原文，不做 AI 摘要、不改写、不翻译。

## 数据来源

GitHub Releases API 返回：

- `name`：release 标题。
- `tag_name`：版本 tag。
- `body`：release note 原文。
- `html_url`：GitHub release 页面。
- `published_at`：发布时间。
- `assets`：发布资产列表。

## CLI 行为

`info owner/repo` 展示：

- 仓库 URL。
- release title 和 tag。
- 发布时间。
- release 页面 URL。
- release note 原文。
- asset 列表。

如果 release note 为空，显示：

```text
This release does not include a release note.
```

## GUI 行为

更新收件箱中点击软件后，右侧详情检查器展示：

- release title。
- 当前版本到最新版本。
- 发布时间。
- release note 原文。
- asset 文件。
- 安装路径。
- 打开 Release 页面。
- 复制 Release Note。

长 release note 必须在详情检查器内部滚动，不能撑破布局。

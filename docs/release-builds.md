# Release Builds

## 目标

GitHub Actions 在每次推送到 `main`、创建 pull request 或手动触发时构建可下载产物，方便验证 Linux CLI 和 Windows 桌面程序。推送版本 tag 时，工作流还会创建 GitHub Release 并上传正式发布产物。

## Workflow

工作流文件：

```text
.github/workflows/ci-release.yml
```

触发方式：

- push 到 `main`
- push tag，例如 `v0.1.0`
- pull request
- GitHub UI 手动 `workflow_dispatch`

## Artifacts

- `ghrm-linux-x64`：Linux CLI，可直接执行 `./ghrm-linux-x64 --help`。
- `ghrm-windows-x64-desktop`：Windows Tauri 桌面程序，可包含免安装 exe 和安装包。

CLI jobs 只测试和构建 `ghrm-core`、`ghrm-cli`，避免为了命令行产物编译桌面端依赖。桌面端由 Windows Desktop job 单独执行 `npm run tauri build`。
桌面端 artifact 上传路径使用仓库根相对路径 `apps/desktop/src-tauri/target/...`，因为 `actions/upload-artifact` 不继承命令步骤的 `working-directory`。

## GitHub Releases

正式发布通过 tag 触发：

```bash
git tag v0.1.0
git push origin v0.1.0
```

tag workflow 成功后，项目 Releases 页面会出现同名 Release。Release assets 包含 Linux CLI、Windows 桌面 exe、NSIS 安装包和 MSI。

如果同名 Release 已存在，发布 job 会失败。此时应删除旧 Release 和 tag，或者发布新的版本号。

## 本地验证

提交前运行：

```bash
cargo test --workspace
cd apps/desktop
npm test
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

`npm run tauri build` 在部分本地环境里会继续到 AppImage 打包阶段，并依赖 `linuxdeploy`；如果系统里缺少该工具，前面的 Rust 编译、前端构建和 deb/rpm 打包仍可作为有效验证结果。

## 注意

- main、pull request、手动触发只上传 Actions artifacts；tag 触发会创建 GitHub Release。
- 不提交 `target/`、`node_modules/`、`dist/` 等生成目录。
- GitHub token 只用于认证，不能写入仓库、日志或 remote URL。

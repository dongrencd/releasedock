# Release Builds

## 目标

GitHub Actions 在每次推送到 `main`、创建 pull request 或手动触发时构建可下载产物，方便验证 CLI 和 Windows 桌面程序。

## Workflow

工作流文件：

```text
.github/workflows/ci-release.yml
```

触发方式：

- push 到 `main`
- pull request
- GitHub UI 手动 `workflow_dispatch`

## Artifacts

- `ghrm-linux-x64`：Linux CLI，可直接执行 `./ghrm-linux-x64 --help`。
- `ghrm-windows-x64`：Windows CLI，可执行 `.\ghrm-windows-x64.exe --help`。
- `ghrm-desktop-windows-x64`：Windows Tauri 桌面程序，可包含免安装 exe 和安装包。

CLI jobs 只测试和构建 `ghrm-core`、`ghrm`，避免为了命令行产物编译桌面端依赖。桌面端由 Windows Desktop job 单独执行 `npm run tauri build`。
桌面端 artifact 上传路径使用仓库根相对路径 `apps/desktop/src-tauri/target/...`，因为 `actions/upload-artifact` 不继承命令步骤的 `working-directory`。

## 本地验证

提交前运行：

```bash
rtk cargo test --workspace
rtk cd apps/desktop
rtk npm test
rtk npm run build
rtk cargo check --manifest-path src-tauri/Cargo.toml
```

## 注意

- Actions 只上传 artifacts，不自动创建 GitHub Release。
- 不提交 `target/`、`node_modules/`、`dist/` 等生成目录。
- GitHub token 只用于认证，不能写入仓库、日志或 remote URL。

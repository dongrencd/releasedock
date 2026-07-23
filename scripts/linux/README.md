# Linux Build Scripts

## CLI

```bash
bash scripts/linux/build-cli.sh
```

## Desktop

```bash
bash scripts/linux/build-desktop.sh
```

Desktop 脚本默认只构建桌面可执行文件 `apps/desktop/src-tauri/target/release/ghrm`，这样本地开发不会被 AppImage 打包工具卡住。
如果需要额外尝试桌面打包，可以显式传 `--bundles`，例如：

```bash
bash scripts/linux/build-desktop.sh --bundles deb,rpm
```

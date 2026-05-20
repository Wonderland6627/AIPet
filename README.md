# AIPet -- AI 桌面宠物

AIPet 是一个基于 **Tauri 2 + React + Rust** 的 Windows 桌面宠物应用。宠物会根据用户当前聚焦的软件自动切换动画状态（如工作、思考、待机等），所有动画均通过 AI 生成的 Sprite Sheet 序列帧驱动。

## 目录结构

```
AIPet/
├── app/                  # Tauri 工程（前端 + Rust 后端）
│   ├── src/              #   React 前端源码
│   ├── src-tauri/        #   Rust 后端源码 & Tauri 配置
│   │   ├── assets/       #   编译时嵌入的资源（layout guides 等）
│   │   └── bundled-pets/ #   随安装包分发的内置默认宠物
│   └── package.json
├── docs/                 # 版本变更文档
├── .github/workflows/    # CI/CD 自动构建发布
└── README.md
```

## 前置依赖

| 工具 | 最低版本 | 安装方式 |
|------|---------|---------|
| Node.js | 18+ | [nodejs.org](https://nodejs.org) |
| pnpm | 8+ | `npm install -g pnpm` |
| Rust | stable | [rustup.rs](https://rustup.rs) |
| VS Build Tools | 2022 | `winget install Microsoft.VisualStudio.2022.BuildTools` (需勾选 C++ 工作负载) |

## 开发运行

```bash
cd app
pnpm install
pnpm tauri dev
```

## 生产构建

```bash
cd app
pnpm tauri build
```

构建产物位于 `app/src-tauri/target/release/`，同时会在 `app/src-tauri/target/release/bundle/` 下生成 `.msi` 安装包和 NSIS `.exe` 安装程序。

## 安装方式

- **NSIS 安装程序**（推荐）：运行 `AIPet_x.y.z_x64-setup.exe`，按向导完成安装
- **MSI 安装包**：运行 `.msi` 文件，按向导完成安装

## 发布新版本

项目通过 GitHub Actions 自动构建发布：

1. 更新版本号（`tauri.conf.json`、`Cargo.toml`、`package.json` 三处保持一致）
2. 提交并推送与当前应用版本一致的 tag（例如当前版本是 `0.4.0` 时使用 `v0.4.0`）：
   ```bash
   git tag v<version>
   git push origin v<version>
   ```
3. CI 会自动构建并创建 GitHub Release，附带安装包和更新清单

## 自动更新

应用内置 Tauri Updater 插件，启动时自动检查 GitHub Releases 上的 `latest.json`。
有新版本时会提示用户下载并自动安装重启，无需手动重新下载。

首次配置需要生成签名密钥（参见下方"签名密钥配置"章节）。

## 签名密钥配置

1. 生成密钥对：
   ```bash
   cd app
   pnpm tauri signer generate -w ~/.tauri/aipet.key
   ```
2. 将输出的公钥填入 `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`
3. 在 GitHub 仓库 Settings → Secrets and variables → Actions 中添加：
   - `TAURI_SIGNING_PRIVATE_KEY`：私钥内容
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：私钥密码（如设置了的话）

## 本地数据（Windows）

应用数据目录（与 Tauri 标识一致）：

`%APPDATA%\com.wonderland6627.aipet\`

| 文件 / 目录 | 说明 |
|-------------|------|
| `config.json` | 全局设置：置顶、自启、动画速度/尺寸、当前宠物文件夹名 |
| `state-config.json` | 前台进程 → 动画状态 映射 |
| `pets\<宠物文件夹>\` | 每个宠物一个子文件夹，包含 **pet.json** + **spritesheet.webp**；首次扫描会自动生成 **pet-atlas.json** |

首次安装时，内置默认宠物会自动复制到 `pets\` 目录下。

## 许可证

私有项目，暂不公开。

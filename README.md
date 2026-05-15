# AIPet -- AI 桌面宠物

AIPet 是一个基于 **Tauri 2 + React + Rust** 的 Windows 桌面宠物应用。宠物会根据用户当前聚焦的软件自动切换动画状态（如工作、思考、待机等），所有动画均通过 AI 生成的 Sprite Sheet 序列帧驱动。

## 目录结构

```
AIPet/
├── app/                  # Tauri 工程（前端 + Rust 后端）
│   ├── src/              #   React 前端源码
│   ├── src-tauri/        #   Rust 后端源码 & Tauri 配置
│   └── package.json
├── templates/            # 宠物模板 & AI 生成配置
│   ├── dva-pet/          #   D.Va 灵感宠物的 Prompt & Job 配置
│   └── petdex/           #   Petdex 规范模板
├── docs/                 # 版本变更文档
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

构建产物位于 `app/src-tauri/target/release/`，同时会在 `app/src-tauri/target/release/bundle/` 下生成 `.msi` 安装包和 `.exe` 便携版。

## 安装方式

- **安装包**：运行 `.msi` 文件，按向导完成安装
- **便携版**：直接运行 `AIPet.exe`

## 许可证

私有项目，暂不公开。

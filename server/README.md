# AIPet 远程捏宠服务端（内网首版）

在你的 Windows 机器上常驻运行，替其他客户端调用 AI API 并生成宠物包。

## 快速启动

1. 复制配置：

```bat
copy config.example.json ai-config.json
```

2. 编辑 `ai-config.json`，填入可用的 Provider / API Key / Base URL / Model。

3. 启动：

```bat
start-server.bat
```

或：

```bash
cargo run -p aipet-server -- --bind 0.0.0.0:8787 --data-dir ./data --ai-config ./ai-config.json
```

默认监听 `http://0.0.0.0:8787`。内网其他电脑在 AIPet 中选择「我的生成服务」，填写 `http://<你的内网IP>:8787`。

启动后，**控制台会实时打印任务日志**；同时写入本地文件。

## 本地归档目录（`data/`）

| 路径 | 说明 |
|------|------|
| `logs/server.log` | 全服务实时日志（含 IP、阶段、错误） |
| `history.jsonl` | 每次生成一条摘要（成功/失败/取消、IP、时间、petId） |
| `runs/<taskId>/meta.json` | 单次任务完整元数据 |
| `runs/<taskId>/task.log` | 单次任务完整过程日志 |
| `runs/<taskId>/request.json` | 请求摘要（不含大图 base64） |
| `runs/<taskId>/reference.png` | 参考图（如有） |
| `runs/<taskId>/base-preview.png` | 基础形象预览 |
| `runs/<taskId>/work/` | 中间条带、progress 等 |
| `runs/<taskId>/pets/<petId>/` | 本次产出的宠物包 |
| `runs/<taskId>/artifact.zip` | 本次下载用 ZIP |
| `pets/<petId>/` | 成功任务的镜像副本，方便浏览 |
| `artifacts/<taskId>.zip` | ZIP 稳定下载路径镜像 |

## API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 存活检查 |
| GET | `/status` | `idle` / `busy` / `awaiting_confirmation`（含 clientIp） |
| GET | `/history?limit=50` | 最近生成记录 |
| POST | `/tasks` | 创建任务（忙碌返回 409） |
| GET | `/tasks/{id}` | 任务进度与日志 |
| POST | `/tasks/{id}/confirm-base` | `{"confirmed": true/false}` |
| POST | `/tasks/{id}/cancel` | 取消 |
| GET | `/tasks/{id}/base-image` | 基础形象预览 |
| GET | `/tasks/{id}/artifact` | 下载 ZIP，响应头 `x-aipet-sha256` |

全局同一时刻只允许一个任务（含等待基础形象确认）。

## 安全说明

- 首版面向内网/VPN，不做鉴权。
- 不要把端口映射到公网。
- AI Key 只保存在服务端 `ai-config.json`。
- 客户端 IP 会记录到 `meta.json` / `history.jsonl` / `server.log`。

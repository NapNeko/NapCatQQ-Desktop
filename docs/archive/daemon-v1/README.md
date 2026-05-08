# Daemon v1 归档

此目录包含 v1 规划中构建的 Agent/Daemon 双模式架构代码。

## 归档原因

v1 方案在推进过程中偏离了产品初衷（透明远程管理），演变成了复杂的双模式架构：
- Go Daemon（WebSocket + JSON-RPC 2.0 服务器）
- Desktop Agent 客户端
- 5 层安全架构

v2 方案回归 VS Code Remote SSH 模型，仅使用 SSH/SFTP + NapCat 自带 WebUI API，不需要独立常驻服务。

详见 `docs/general/remote_ssh_plan.md`（v2）。

## 归档内容

- `src/daemon/` — 完整 Go Daemon 项目（~1239 行 Go + 安全库 ~1210 行）
- `src/desktop/core/remote/agent_*.py` — Agent 客户端和后端
- `src/desktop/core/remote/jsonrpc_protocol.py` — JSON-RPC 2.0 协议
- `src/desktop/core/remote/daemon_*.py` — Daemon 配置和部署器
- `src/desktop/ui/page/remote_page/agent_*.py` — Agent UI 组件

## 归档日期

2025-04-29

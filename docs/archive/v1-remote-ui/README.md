# archive/v1-remote-ui

本目录保存 v1 远程页面残留的 5 个 UI 组件，**仅作历史参考**，不被运行时引用。

## 归档原因

v1 设计是 **SSH + Go Daemon 双模式**，因此 UI 上有 `mode: "ssh" | "agent"` 的语义。
v2 (参考 [`docs/general/remote_ssh_plan.md`](../../docs/general/remote_ssh_plan.md))
回归 **VS Code Remote 透明模型** —— 用户只感知"添加服务器"，不再选择模式。
v1 UI 的双模式语义、单服务器配置、Daemon 状态展示等概念，全部不再适用。

## 文件清单

| 文件 | 原职责 | v2 替代 |
| --- | --- | --- |
| `connection_base.py` | `BaseConnectionHandler` ABC，含 `mode: "ssh"\|"agent"` 字段 | 已被 [`OperationBackend`](../../src/desktop/core/operation/backend.py) 取代 |
| `ssh_handler.py` | SSH 模式连接处理器 (`QRunnable` 调度) | 已被 [`ConnectionTester`](../../src/desktop/ui/page/remote_page/connection_tester.py) 取代 |
| `ssh_panel.py` | 单服务器全局 SSH 配置面板 | 已被多服务器版的 [`ServerEditDialog`](../../src/desktop/ui/page/remote_page/server_edit_dialog.py) 取代 |
| `status_panel.py` | Daemon 状态展示面板 (含 `daemon_version` 字段) | 已被 v2 [`ServerDetailPanel`](../../src/desktop/ui/page/remote_page/__init__.py) 取代 |
| `widgets.py` | v1 公共小组件 | 已被项目通用 [`ui/components`](../../src/desktop/ui/components) 取代 |

## 复用 vs 重写

新版 [`ServerEditDialog`](../../src/desktop/ui/page/remote_page/server_edit_dialog.py) 在视觉风格上沿用了 `ssh_panel.py`
的紧凑 Fluent Design 卡片布局（主机/端口同行、私钥/密码切换、可折叠高级选项），
但语义层完全重写：从 "全局 cfg" 改为"按服务器档案存取"，
并按 §6.2 规定不再持久化密码。

如需删除本归档，可在 v3 稳定后整体移除该目录。

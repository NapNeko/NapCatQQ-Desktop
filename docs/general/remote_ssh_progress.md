# 远程方案推进进度（v2）

## 文档说明

本文件记录 NapCatQQ Desktop 远程方案的推进状态。
规划文档见 [docs/general/remote_ssh_plan.md](docs/general/remote_ssh_plan.md)（v2 — VS Code Remote 模型）。

> **v2 说明**：本文档已基于 v2 规划全面重写。
> 原 v1 进度中记录的 Agent/Daemon 双模式开发成果已归档至 rchive/daemon-v1/。

---

## 当前阶段

当前处于：**P4 进行中 — Wave 1 done, regression green; W2/W3/W4 待启动**

各阶段计划与验收文档：
- P0 → [`remote_ssh_p0_acceptance.md`](./remote_ssh_p0_acceptance.md)
- P1 → [`remote_ssh_p1_plan.md`](./remote_ssh_p1_plan.md) / [`remote_ssh_p1_acceptance.md`](./remote_ssh_p1_acceptance.md)
- P2 → [`remote_ssh_p2_acceptance.md`](./remote_ssh_p2_acceptance.md)
- P3 → [`remote_ssh_p3_plan.md`](./remote_ssh_p3_plan.md) / [`remote_ssh_p3_acceptance.md`](./remote_ssh_p3_acceptance.md)
- P3 perf → [需求](../requirements/2026-05-06-remote-ssh-p3-perf.md) / [`remote_ssh_p3_perf_acceptance.md`](./remote_ssh_p3_perf_acceptance.md)
- P4 → [需求](../requirements/2026-05-06-remote-ssh-p4.md) / [实施计划](../plans/2026-05-06-remote-ssh-p4-execution-plan.md)

### 总体判断

- v1 的 SSH 基础设施（SSH 客户端、SFTP、错误模型、部署骨架）**已复用**
- v1 的 Agent/Daemon 全套代码已归档至 `archive/daemon-v1/`
- v2 P0（操作抽象层 + 服务器管理）**已完成 49/49**
- v2 P1（远端部署 MVP）**已完成 81/81**
- v2 P1.5（独立部署控制台）**已完成 87/87**
- v2 P2（远端 Bot 运行闭环）**已完成 244/244**
- v2 P3（体验优化与稳态收尾）**已完成 317/317**, 含 P3 新增 73 个 (W1·14 + W2·16 + W3·B 14 + W3·E 7 + 邻接 22)
- v2 P3 perf（启动流程异步化与状态可见性, 进度反馈走组件库 ProgressInfoBar）**已完成**, 新增 41 测试 (background_task_center·13 + bot_card_starting·5 + runnable_wiring·5 + run_napcat·4 适配 + operate_config_async·5 + progress_info_bar_bridge·9)
- v2 P4 W1（体验补丁: 指纹确认 / keyring / 私钥拖拽 / 错误文案统一 + 批量 Bot 管理）**Wave 1 done, regression green** — 新增 106 用例 (host_key_policy·13 + host_key_confirm_dialog·12 + credential_keyring·15 + server_edit_dialog_drag_drop·13 + server_edit_dialog_remember·14 + friendly_errors·23 + batch_bot_dispatcher·10 + bot_page_batch_mode·6), 同步修复 P3 perf 遗留 collection 阶段 sys.modules 污染.

---

## 方向切换清理

### 已完成

- [x] v2 规划文档重写（docs/general/remote_ssh_plan.md）
- [x] v2 进度文档重写（本文档）

### 进行中

- [ ] 归档 Agent/Daemon 相关代码至 rchive/daemon-v1/

归档清单：

| 文件                                     | 说明                   |
| ---------------------------------------- | ---------------------- |
| src/daemon/ (整个目录)                   | Go Daemon 项目         |
| src/core/remote/agent_backend.py         | Agent 执行后端         |
| src/core/remote/agent_client.py          | WebSocket Agent 客户端 |
| src/core/remote/jsonrpc_protocol.py      | JSON-RPC 2.0 协议定义  |
| src/core/remote/daemon_config.py         | Daemon 配置管理        |
| src/core/remote/daemon_deployer.py       | Daemon 部署器          |
| src/ui/page/remote_page/agent_handler.py | Agent UI 处理器        |
| src/ui/page/remote_page/agent_panel.py   | Agent 配置面板         |

- [ ] 清理 src/core/remote/__init__.py（移除 Agent/Daemon 导出）
- [ ] 清理 src/ui/page/remote_page/__init__.py（移除 Agent 依赖）

---

## v2 进度

### P0：操作抽象层 + 服务器管理

状态：**已完成 ✅**（验收文档: [`remote_ssh_p0_acceptance.md`](./remote_ssh_p0_acceptance.md)）

- [x] 定义 OperationBackend 抽象接口
- [x] 实现 LocalBackend（封装现有本地逻辑）
- [x] 实现 RemoteBackend 骨架（SSH/SFTP）
- [x] 服务器配置模型和持久化
- [ ] SSHClient 升级为持久连接管理器（keepalive、重连、多 channel）— 排期至 P3
- [x] 服务器管理 UI（添加/测试/删除）

### P1：远端部署（MVP）

状态：**已完成 ✅**（实施计划: [`remote_ssh_p1_plan.md`](./remote_ssh_p1_plan.md), 验收报告: [`remote_ssh_p1_acceptance.md`](./remote_ssh_p1_acceptance.md)）

设计冻结决策：
- 部署粒度: **分两步 install_qq / install_napcat**（用户决策）
- UI 入口: **P1 仅暴露“部署”一个一键入口**, 单步重跑/强制更新推到 P3（用户决策）
- 脚本拆分: 原 `remote_deploy_napcat.sh` 拆为 `remote_install_linuxqq.sh` / `remote_install_napcat.sh` / `remote_napcat_launcher.sh`

任务清单:
- [x] (A1) 增强 LinuxCoreDeployment.probe_environment（OS / arch / 已有安装）
- [x] (A2) 拆分远端脚本: install_linuxqq.sh / install_napcat.sh / napcat_launcher.sh
- [x] (A3) 拓展 templates.py 推三个脚本构建函数
- [x] (A4) LinuxCoreDeployment 拆出 install_linuxqq / install_napcat 独立方法
- [x] (A5) 进度协议 [PROGRESS] N message 解析
- [x] (B1/B2) RemoteBackend.install_qq / install_napcat 实现
- [x] (B3/B4) ServerManager.deploy_server + Qt 信号
- [x] (C1) DeploymentRunner (QRunnable)
- [x] (C2) RemotePage 部署按钮 + 进度区
- [x] (D1-D3) 自动化测试: probe / runner / server_manager_deploy（32/32 全绿）
- [x] (D4) 编写 P1 验收文档

### P2：远端 Bot 运行闭环

状态：**已完成 ✅** (验收报告: [`remote_ssh_p2_acceptance.md`](./remote_ssh_p2_acceptance.md))

- [x] BotConfig 增加 runtime_target 字段
- [x] 远端启动/停止 NapCat 进程
- [x] 远端配置读写（SFTP）
- [x] 远端日志读取 (RemoteNapCatQQLog)
- [x] SSH 端口转发 + WebUI API 透传
- [x] Bot 页面增加运行位置选择

### P3：体验优化

状态：**已完成 ✅ (核心范围 A/B/C/E/F)** (验收报告: [`remote_ssh_p3_acceptance.md`](./remote_ssh_p3_acceptance.md))

- [x] **A** 远端版本检测与强制更新 (RemotePage 维护菜单)
- [x] **B** Bot 运行位置迁移服务 + 向导对话框 (BotMigrationService / MigrationDialog)
- [x] **C** SSH 持久连接升级 (keepalive + 自动重连一次)
- [x] **E** 远端 BotPage 日志面板体验优化 (退避 + 标题后缀)
- [x] **F** 部署失败 / 手动回滚 UI 入口 (RollbackConfirmBox)
- [ ] **D** 多服务器/Bot 状态聚合面板 (推迟到 P3.5/P4)
- [ ] **G** 体验细节: 指纹确认对话框 / keyring / 私钥拖拽 / 错误文案统一 (推迟到 P3.5/P4)

### P4：高级能力 + P3 体验补丁收尾

状态：**Wave 1 done, regression green; W2/W3/W4 待启动**
（需求: [`docs/requirements/2026-05-06-remote-ssh-p4.md`](../requirements/2026-05-06-remote-ssh-p4.md), 实施计划: [`docs/plans/2026-05-06-remote-ssh-p4-execution-plan.md`](../plans/2026-05-06-remote-ssh-p4-execution-plan.md)）

#### Wave 1 — 体验补丁 + 批量 Bot 管理 ✅

- [x] **F5.1** SSH 指纹首次连接确认 (`InteractiveHostKeyPolicy` + `HostKeyConfirmDialog` 跨线程桥, 复用 v1 `KnownHostsStore`)
- [x] **F5.2** Windows Credential Manager 凭据库 (`CredentialStore`, ServerManager `remember_password` 参数 + 启动期 `_preload_passwords_from_keyring`)
- [x] **F5.3** ServerEditDialog 私钥路径拖拽 (`_PrivateKeyDropLineEdit`)
- [x] **F5.4** 错误文案统一 (`friendly_errors.to_friendly`, `ServerManager.test_connection` 接入)
- [x] **F2** 批量 Bot 管理 (`BatchDispatcher` + BotCard 复选框 + BotListPage 工具条; 支持批量启停/删除)
- [x] 修复 `test_bot_card_starting_state.py` 在 collection 阶段污染 `sys.modules["src.ui.page"]` 的遗留问题
- 用例新增: 106 个; W1 + 基线全量回归 473/473 + 1 expected skip

#### Wave 2 — 远端资源监控 + 状态聚合面板 (待启动)

- [ ] **F3** `ResourceMonitorService` 10s 后台轮询 (CPU / Mem / Disk / Net)
- [ ] **F4** `RemoteSummaryCard` (HomePage 入口) + `StatusOverviewDialog` (多服务器聚合)
- [ ] **F1** 自动切换机制 (本地不可用 → 远端) — 排期复议中

#### Wave 3 — 持久数据搬运 (待启动)

- [ ] **B** NapCat 持久数据迁移 (账号缓存/数据库; 复用 P3 `BotMigrationService` 骨架)

#### Wave 4 — `_iter_stream_with_resume` (待启动)

- [ ] **F7** `SSHClient._iter_stream_with_resume` (按 PROGRESS 步长断点重连; 重试退避 1s/3s/8s)

---

## 可复用的 v1 资产

以下模块在 v1 中已完成，可直接用于 v2：

| 模块                                        | 用途                    | v2 定位                              |
| ------------------------------------------- | ----------------------- | ------------------------------------ |
| src/core/remote/ssh_client.py               | SSH 连接封装 (paramiko) | RemoteBackend 底层，需升级为持久连接 |
| src/core/remote/models.py                   | SSH 凭据 + 远端路径模型 | 扩展为服务器配置模型                 |
| src/core/remote/errors.py                   | SSH 错误类型            | 保留                                 |
| src/core/remote/execution_backend.py        | 执行抽象层              | 重构为 OperationBackend              |
| src/core/remote/deployment.py               | 部署逻辑                | 重构为 RemoteBackend 安装方法        |
| src/core/remote/status.py                   | 状态查询                | 重构为 RemoteBackend 状态方法        |
| src/core/remote/templates.py                | 脚本模板                | 保留                                 |
| src/core/remote/remote_manager.py           | 连接管理                | 重构为服务器管理器                   |
| src/resource/script/remote_deploy_napcat.sh | 部署脚本                | 保留并完善                           |

---

## v1 历史记录（摘要）

以下为 v1 阶段的主要成果记录，详细内容已随代码归档至 rchive/daemon-v1/。

### v1 已完成的可复用工作

- SSH 客户端封装（paramiko）
- SSH 凭据模型与错误类型
- 远端部署脚本（支持 Ubuntu 24.04+ libasound2t64 兼容）
- 远端状态查询与日志读取骨架
- 远程管理独立页面（从设置子页提升为一级页面）
- 适配 NapCat-Installer 标准安装路径 $HOME/Napcat/

### v1 已完成但不再使用的工作（已归档）

- Go Daemon 项目（WebSocket + JSON-RPC 2.0 服务器，~1239 行 Go）
- Desktop Agent 客户端（jsonrpc_protocol.py + agent_client.py + agent_backend.py，~1234 行 Python）
- 5 层安全架构（挑战-响应、JWT、TLS、速率限制、审计日志，~1210 行 Go）
- Daemon 自动部署系统（install.sh + DaemonDeployer + DaemonConfigManager）
- Agent/Daemon UI 面板（AgentHandler + AgentConfigPanel）

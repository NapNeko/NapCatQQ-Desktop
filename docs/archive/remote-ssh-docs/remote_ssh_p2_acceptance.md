# P2 验收清单 — 远端 Bot 运行闭环

> 本文档总结 [`docs/general/remote_ssh_plan.md`](./remote_ssh_plan.md) §7 P2 阶段
> "远端 Bot 运行闭环" 的实现进度与对应的测试覆盖。

## 1. 完成项

| 验收点 | 实现位置 | 测试覆盖 |
| --- | --- | --- |
| **P2.1** `BotConfig.runtime_target` 字段 + legacy 迁移 | [`src/core/config/config_model.py`](../../src/core/config/config_model.py) | [`test_config_model.py::TestRuntimeTargetField`](../../script/test/test_config_model.py) |
| **P2.2** Backend 解析层 `resolve_backend_for_bot()` | [`src/core/operation/resolver.py`](../../src/core/operation/resolver.py) | [`test_backend_resolver.py`](../../script/test/test_backend_resolver.py) |
| **P2.3** 远端启停 `RemoteBackend.start_napcat` / `stop_napcat` | [`src/core/operation/remote_backend.py`](../../src/core/operation/remote_backend.py)、[`src/resource/script/remote_napcat_launcher.sh`](../../src/resource/script/remote_napcat_launcher.sh) | [`test_remote_backend_process.py::TestStartNapcat`](../../script/test/test_remote_backend_process.py)、[`TestStopNapcat`](../../script/test/test_remote_backend_process.py)、[`TestQQIdGuard`](../../script/test/test_remote_backend_process.py) |
| **P2.4** 远端配置同步 (`write_bot_runtime_config` + `update_config` 钩子) | [`src/core/operation/remote_backend.py`](../../src/core/operation/remote_backend.py)、[`src/core/config/operate_config.py`](../../src/core/config/operate_config.py) | [`test_remote_backend_process.py::TestRuntimeConfigSync`](../../script/test/test_remote_backend_process.py)、[`test_operate_config.py`](../../script/test/test_operate_config.py) `P2.4: 远端配置同步钩子` 区块 |
| **P2.5** SSH 本地端口转发 + WebUI endpoint 探测 | [`src/core/remote/tunnel.py`](../../src/core/remote/tunnel.py)、[`src/core/remote/ssh_client.py`](../../src/core/remote/ssh_client.py) (`open_local_tunnel`)、[`src/core/operation/remote_backend.py`](../../src/core/operation/remote_backend.py) (`get_webui_endpoint`) | [`test_local_port_forwarder.py`](../../script/test/test_local_port_forwarder.py)、[`test_remote_backend_process.py::TestWebUIEndpoint`](../../script/test/test_remote_backend_process.py) |
| **P2.6** `ManagerNapCatQQProcess` 远端 Bot 路由 + 状态轮询 + WebUI 登录状态发布 | [`src/core/runtime/napcat.py`](../../src/core/runtime/napcat.py) `RemoteProcessRecord` / `RemoteBotOperationRunnable` / `_create_remote_process` 等 | [`test_remote_process_manager.py`](../../script/test/test_remote_process_manager.py) |
| **P2.7** `BotConfigWidget` 增加运行位置选择器 | [`src/ui/page/bot_page/widget/config.py`](../../src/ui/page/bot_page/widget/config.py) `RuntimeTargetConfigCard` | [`test_bot_config_widget.py`](../../script/test/test_bot_config_widget.py) `P2.7: runtime_target 选择器` 区块 |
| **P2.8** 集成验证 | 见上述各 seam 的单元测试 | 244 个针对性测试一并通过, 见下文命令 |

## 2. 验收命令

```bash
# 全量 P2 + 邻接领域回归 (244 用例)
python -m pytest \
  script/test/test_config_model.py \
  script/test/test_operate_config.py \
  script/test/test_legacy_import.py \
  script/test/test_config_load.py \
  script/test/test_local_backend.py \
  script/test/test_backend_resolver.py \
  script/test/test_remote_backend_process.py \
  script/test/test_local_port_forwarder.py \
  script/test/test_remote_deploy_runner.py \
  script/test/test_remote_deploy_probe.py \
  script/test/test_server_manager_deploy.py \
  script/test/test_server_registry.py \
  script/test/test_run_napcat.py \
  script/test/test_remote_process_manager.py \
  script/test/test_ssh_line_splitter.py \
  script/test/test_path_func.py \
  script/test/test_bot_config_widget.py \
  script/test/test_bot_log_page.py \
  script/test/test_qr_code_dialog.py \
  script/test/test_connect_dialogs.py \
  -q
```

## 3. 用户体验闭环

按照 §1.1 用户故事走一遍, 现状如下:

1. **添加服务器** ✓ (P0/P1 已完成)
2. **远端部署 NapCat** ✓ (P1 已完成)
3. **添加 Bot 时选择运行位置** ✓ — `BotConfigWidget` 增加 "运行位置" 下拉
4. **远端 Bot 启停 / 配置编辑** ✓ — 通过 `BotCard` 的"启动 / 停止 / 设置"按钮路由到 [`ManagerNapCatQQProcess`](../../src/core/runtime/napcat.py) 远端分支, 配置保存自动同步到远端
5. **远端 Bot WebUI / 二维码** ✓ — `RemoteBackend.get_webui_endpoint` 通过 SSH 隧道暴露远端 WebUI 到本地 loopback, 现有 `NapCatQQLoginState` HTTP 轮询代码无需改动
6. **远端 Bot 日志查看** ▶︎ 部分 — `RemoteBackend.tail_log` 已实现, UI 端 `BotPage.log_page` 仍是本地 QProcess stdout 流式视图; 远端日志面板留待 P3

## 4. 已知遗留 / P3 范围

按 [`docs/general/remote_ssh_plan.md`](./remote_ssh_plan.md) §7 P3 计划内项:
- 远端 NapCat 版本检测与更新（已有探测能力, 缺 UI 触发）
- Bot 运行位置迁移（本地 ↔ 远程）
- SSH 连接断线重连（P2 实现使用单次 SSH 会话, 中断后由用户手动重启 Bot）
- 多服务器/多 Bot 状态聚合面板
- 远端 BotPage 日志页改造（流式 SSH tail 替代 QProcess stdout）
- 部署失败回滚（`ServerManager.rollback_server` 已就绪, UI 入口可补足）

## 5. 实现层注意事项

- **多 Bot 远端启动**: launcher 脚本现按 `qq_id` 拆分 PID/log/status 文件, 命令格式
  `bash $launcher start <qq_id>`. 使用 `xvfb-run -a $qq --no-sandbox -q <qq_id>`
  与 [`RemoteRuntimeService.get_status`](../../src/core/remote/status.py) 的 pgrep 规则
  完全对齐.
- **`runtime_target` 兼容**: 缺省值为 `"local"`, 老版本 `bot.json` 自动迁移. 配置模型
  对非法输入（None / 空白 / 非字符串）静默回退, 不阻塞 Desktop 启动.
- **隧道生命周期**: WebUI 隧道按 `qq_id` 缓存, Bot 停止 / 进程消失 / 端口漂移
  时自动关闭. `RemoteBackend.close()` 统一释放所有隧道再关 SSH.
- **跨线程**: 远端 SSH 调用（启停 / 轮询 / WebUI 探测）全部走
  [`QThreadPool`](https://doc.qt.io/qt-6/qthreadpool.html) 后台线程,
  Qt 信号回到主线程更新状态.
- **配置同步失败**: 不阻断本地保存, 仅记录 warning. 用户能感知本地保存成功,
  远端同步可由"启动前再写"兜底 (`start_napcat` 内部会再次调用
  `write_bot_runtime_config`).

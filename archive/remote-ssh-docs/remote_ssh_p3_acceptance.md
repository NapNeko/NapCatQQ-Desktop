# P3 验收清单 — 体验优化与稳态收尾

> 本文档总结 [`docs/general/remote_ssh_plan.md`](./remote_ssh_plan.md) §7 P3 阶段
> "体验优化" 的实现进度与对应测试覆盖。
> 范围与执行计划见 [`remote_ssh_p3_plan.md`](./remote_ssh_p3_plan.md)。

## 1. 完成项

| 验收点 | 实现位置 | 测试覆盖 |
| --- | --- | --- |
| **W1·C** SSHClient 持久连接 + 自愈 | [`src/core/remote/ssh_client.py`](../../src/core/remote/ssh_client.py) (`DEFAULT_KEEPALIVE_INTERVAL`, `ensure_alive`, `_call_with_retry`, `_run_once`, `_exec_stream_once`, `_open_local_tunnel_once`, `_upload_file_once`, `_download_file_once`, `_read_text_once`, `_write_text_once`, `_remote_exists_once`, `_remote_listdir_once`) | [`test_ssh_client_persistent.py`](../../script/test/test_ssh_client_persistent.py) (14 用例) |
| **W2·F** 部署失败 / 手动回滚 UI 入口 | [`src/ui/page/remote_page/__init__.py`](../../src/ui/page/remote_page/__init__.py) (`_on_rollback`, `rollback_btn`, `_update_button_state`)、新建 [`maintenance_dialog.py`](../../src/ui/page/remote_page/maintenance_dialog.py) (`RollbackConfirmBox`) | [`test_remote_page_actions.py::TestRollbackAction`](../../script/test/test_remote_page_actions.py)、`TestRollbackConfirmBox`、`TestButtonState` |
| **W2·A** 远端版本检测 + 强制更新 UI | [`src/ui/page/remote_page/__init__.py`](../../src/ui/page/remote_page/__init__.py) (`_build_maintenance_menu`, `_on_redetect_versions_selected`, `_on_force_update_napcat`, `_on_force_reinstall_linuxqq`, `_start_force_deploy`)、复用 [`DeploymentRunner`](../../src/ui/page/remote_page/deployment_runner.py) `force_*` 参数与 [`RedetectRunner`](../../src/ui/page/remote_page/deployment_runner.py) | [`test_remote_page_actions.py::TestMaintenanceActions`](../../script/test/test_remote_page_actions.py) |
| **W3·B** Bot 运行位置迁移服务 + 向导对话框 | 新建 [`src/core/operation/migration.py`](../../src/core/operation/migration.py) (`BotMigrationService`, `BotMigrationRunnable`, `MigrationPlan`)、新建 [`src/ui/page/bot_page/widget/migration_dialog.py`](../../src/ui/page/bot_page/widget/migration_dialog.py)、改造 [`bot_config.py`](../../src/ui/page/bot_page/sub_page/bot_config.py) (`_handle_target_migration`, `_persist_config`, `_format_target_label`, `_stop_bot_if_running_locally`) | [`test_bot_migration.py`](../../script/test/test_bot_migration.py) (14 用例) |
| **W3·E** 远端 BotPage 日志面板体验 | [`src/core/runtime/napcat.py`](../../src/core/runtime/napcat.py) `RemoteNapCatQQLog._consecutive_errors` / `_MAX_CONSECUTIVE_ERRORS` 退避;  [`src/ui/page/bot_page/sub_page/bot_log.py`](../../src/ui/page/bot_page/sub_page/bot_log.py) `_compose_title_suffix` | [`test_remote_log_buffer.py::TestErrorBackoff`](../../script/test/test_remote_log_buffer.py) (4 用例)、[`test_bot_log_page.py`](../../script/test/test_bot_log_page.py) `test_title_suffix_*` (3 用例) |

## 2. 验收命令

```bash
# 完整回归 (P2 244 + P3 73 + 邻接 = 317 用例)
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
  script/test/test_remote_log_buffer.py \
  script/test/test_ssh_line_splitter.py \
  script/test/test_path_func.py \
  script/test/test_bot_config_widget.py \
  script/test/test_bot_log_page.py \
  script/test/test_qr_code_dialog.py \
  script/test/test_connect_dialogs.py \
  script/test/test_ssh_client_persistent.py \
  script/test/test_remote_page_actions.py \
  script/test/test_bot_migration.py \
  -p no:cacheprovider -q --basetemp="${TEMP}/napcat-pytest-p3"
```

> Windows: `test-artifacts/` 目录在本机存在文件锁问题, 通过 `--basetemp` 指向 ``%TEMP%/napcat-pytest-p3`` 绕过即可。

## 3. 用户体验闭环

按 [`remote_ssh_plan.md`](./remote_ssh_plan.md) §1.1 用户故事走一遍,
现状（含 P3 增量）：

1. **添加服务器** ✓ (P0)
2. **远端部署 NapCat** ✓ (P1)
3. **添加 Bot 时选择运行位置** ✓ (P2)
4. **远端 Bot 启停 / 配置编辑** ✓ (P2)
5. **远端 Bot WebUI / 二维码** ✓ (P2)
6. **远端 Bot 日志查看** ✓ (P3·E) — `BotLogPage` 现在显示 `Bot 日志(QQID) · 远端 [服务器名]`，SSH tail 失败 3 次后自动停轮询并注入错误行
7. **服务器版本检测 / 强制更新 / 回滚** ✓ (P3·A·F) — `RemotePage` 新增「维护 ▾」下拉菜单 + 「回滚部署」红色按钮
8. **运行位置迁移** ✓ (P3·B) — 切换 `runtime_target` 保存时弹 `MigrationDialog` 二次确认；自动停旧 Bot + 后台搬运 NapCat 配置
9. **网络抖动自愈** ✓ (P3·C) — `SSHClient` 启用 30s keepalive；任何 SSH/SFTP 调用在 transport 死亡时自动重连一次并重试

## 4. 已知遗留 / 推迟项

按 [`remote_ssh_plan.md`](./remote_ssh_plan.md) §7 P3/P4 计划：

- **D 多服务器/Bot 状态聚合面板 + 首页远程状态卡片** — 推迟到 P3.5 / P4
- **G 体验细节** (首次指纹确认对话框、密码 keyring、私钥拖拽、人话错误提示统一) — 单独立项
- **B 持久数据迁移** — `MigrationDialog` 已保留 "搬运 NapCat 持久数据" 选项作为 future flag, 当前版本仅迁移 NapCat 配置 (onebot11/napcat JSON), 持久数据 (账号缓存/数据库) 留待 P4 评估目录约定后再实现
- **C `exec_stream` 中途断开自动重连** — paramiko 限制下流式命令重启会重复副作用; 当前仅入口前探活, 中途断开会原样上抛
- **远端资源监控 (CPU/磁盘)** — P4
- **本地不可用自动切换远端** — P4

## 5. 实现层注意事项

- **SSHClient 自动重连**:
  - 用 `threading.RLock` 序列化 `connect/close/ensure_alive`, 多个 `QThreadPool` worker 并发触发断线时只产生一次实际握手
  - `_call_with_retry` 仅在 ``is_connected==False`` 时重试; 命令超时 (transport 仍活) 不重试, 避免 stuck 命令被反复唤起
  - 流式命令 (`exec_stream`) 只做"前置探活", 不在中途自动重启 (避免双倍下载 / 二次写盘)

- **维护菜单按钮**:
  - 仅在选中服务器为 `DEPLOYED` 状态时启用; 部署进行中禁用
  - 「强制重装 LinuxQQ」加强警告文案 (会备份 NapCat → 删除旧 LinuxQQ → 重装), 与「强制更新 NapCat」分开收纳避免误点

- **回滚按钮**:
  - 仅在 `DEPLOYED` / `FAILED` 状态启用; 用红色样式提示破坏性操作
  - `RollbackConfirmBox` 默认勾选 `include_qq=True`, 与 `ServerManager.rollback_server` 默认行为一致
  - 复用 `deployment_progress` / `deployment_finished` / `deployment_log` 信号到部署控制台

- **Bot 运行位置迁移**:
  - 操作顺序: 用户取消 → 无任何副作用; 用户接受 → 主线程停旧 Bot → 后台 runnable 复制+清理 → finished 信号回主线程后才 `update_config` 写本地 + 同步最新到新 backend
  - 这个顺序避免了"runnable 拷贝到新 backend 的陈旧配置覆盖用户最新输入"的竞态
  - 迁移失败时 `update_config` 不被推进, `bot.json` 中的 `runtime_target` 保持原值, UI 自动 `fill_config` 回滚显示
  - `MigrationDialog` 中"持久数据搬运"选项保留为 future flag, P3 阶段仅迁移 onebot11/napcat JSON (覆盖 99% 实际场景, 因为 NapCat 主要状态都在 config 子目录下)

- **远端日志退避**:
  - 连续失败阈值 = 3, 每次轮询间隔 5s → 最长 ~15s 才放弃
  - 任意一次成功拉取 (含空字符串) 即重置计数, 让网络抖动后能恢复
  - 退避停掉后注入一行 `[ERROR] 远端日志拉取连续失败 N 次, 已停止轮询...`, 避免用户面对无声的"日志卡死"
  - 用户重启 Bot 会触发 `ManagerNapCatQQLog.create_remote_log` 重新建一个 `RemoteNapCatQQLog`, 自然恢复

## 6. 跨 Wave 收益

- **C (持久连接)** 是其他几项的稳态底座: B 的迁移搬运、A 的版本探测、F 的回滚、E 的日志拉取全部走 `SSHClient`, 任意一次操作过程中 SSH 断线现在都能自愈一次
- **F (回滚 UI)** 复用了 P1 已有的 `RollbackRunner` 后端 + `deployment_progress` / `deployment_log` 信号, UI 改动 ≈ 60 行
- **A (版本/强制更新)** 复用 P1 的 `DeploymentRunner` `force_*` 参数与 `RedetectRunner`, UI 改动 ≈ 80 行
- **B (迁移)** 全新模块 `migration.py` ~340 行 + UI 整合 ~100 行 + 对话框 ~70 行
- **E (日志体验)** 在已有 `RemoteNapCatQQLog` 上加退避 + 标题后缀, 改动 < 50 行

总代码增量约 1500 行 (含测试 ~700 行), 与 [`remote_ssh_plan.md`](./remote_ssh_plan.md) §12 "v2 ~1500 行" 预估对齐。

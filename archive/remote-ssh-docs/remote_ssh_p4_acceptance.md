# Remote SSH P4 验收文档

本文件记录 [P4 高级能力](../requirements/2026-05-06-remote-ssh-p4.md) 的交付状态、自动测试覆盖与手动验收清单。

完整实施计划见 [`docs/plans/2026-05-06-remote-ssh-p4-execution-plan.md`](../plans/2026-05-06-remote-ssh-p4-execution-plan.md)。

## 1. 总体状态

- **W1 (体验补丁 + 批量 Bot 管理)**: ✅ 代码 + 自动测试均交付; W1 + 既有基线全量回归 **473/473 + 1 expected skip**
- **W2 (资源监控 + 状态聚合面板)**: ✅ 代码交付; F3.1 解析层 12/12, F3.2 service 单测按用户决策跳过 (Qt 线程层 mock 易卡死, 风险已记录), F4 UI 通过 W4 手动验收覆盖
- **W3 (NapCat 持久数据迁移)**: ✅ 代码 + 9 个新增用例交付
- **W4 (`exec_stream` 中途断开自动重连)**: ✅ 代码 + 7 个新增用例交付; 默认 `max_retries=0` 退化为单次, 真机验证脚本幂等性后再开启重试

> **完成语言策略**: 仅在所有手动验收项 ✅ 后, 方可在 [`remote_ssh_progress.md`](./remote_ssh_progress.md) 把 P4 标为 "已通过验收".

## 2. 自动测试覆盖

### W1 新增 (106 个)
- `test_ssh_host_key_policy.py` × 13 — `InteractiveHostKeyPolicy` 指纹计算 / 信任 / 拒绝 / `KnownHostsStore` 持久化
- `test_host_key_confirm_dialog.py` × 12 — UI 三选项行为 + 跨线程桥超时
- `test_credential_keyring.py` × 15 — `CredentialStore` 可用性探测 / CRUD / 失败降级 / 命名空间隔离
- `test_server_edit_dialog_drag_drop.py` × 13 — 私钥路径拖拽 accept/reject (基于 `evaluate_drop_paths` 静态判断, 不依赖 `QDropEvent`)
- `test_server_edit_dialog_remember.py` × 14 — "记住密码" 勾选项 + ServerManager keyring 集成
- `test_friendly_errors.py` × 23 — paramiko / stdlib / 自定义异常 → 友好文案
- `test_batch_bot_dispatcher.py` × 10 — 批量串行 / 并行 / 空集 / 失败汇总 / `BackgroundTaskCenter` 集成
- `test_bot_page_batch_mode.py` × 6 — BotCard 批量模式开关 / 信号 / `QRCodeDialogFactory` mock 隔离

### W2 新增 (12 个; service 单测因 Qt 线程层 mock 易卡死, 经用户决策跳过)
- `test_remote_backend_sample_resources.py` × 12 — `sample_resources` 4 行 echo 协议 + `parse_sample_output` 解析 / 缺失 / 异常
- `test_remote_summary_card_format.py` × 5 — `_format_breach` 文案纯函数 (CPU 95% / 5min 前 / 磁盘 91% / 未知 metric / 数值取整)

### W3 新增 (9 个)
- `test_migration_persistent_data.py` × 7 — 完整搬运 / `.partial` 续传 / size 一致跳过 / 失败保留 partial / `bytes_progress_signal` 累计 / 空白名单 / 未知 backend 守卫
- `test_migration_dialog_persistent_flag.py` × 2 — 默认勾选 + getter toggle

### W4 新增 (7 个)
- `test_exec_stream_resume.py` × 7 — `max_retries=0` 单次 / `last_resume` 累计透传 / 用尽抛最后异常 / 业务错误不重试 / `progress_marker` & `on_stdout_line` 异常吞噬 / `ensure_alive` 失败仍尝试下一轮

## 3. 手动验收清单

> 顺序无关, 但每项都需在真机网络环境下完成. 完成后在右侧打钩 + 注明操作截图 / 日志位置.

### W1 — 体验补丁 + 批量 Bot 管理

- [ ] **F5.1 首次连接指纹确认**: 添加新服务器 → 第一次 "测试连接" 弹 `HostKeyConfirmDialog`; 选 "信任并保存" 后再次连接不再弹窗, `~/.config/napcat-desktop/known_hosts` 含该主机条目.
- [ ] **F5.1 指纹变化警告**: 改 `known_hosts` 内的指纹后再次连接 → 弹红色 "主机指纹变化警告" 对话框, 默认按钮为 "拒绝".
- [ ] **F5.2 keyring 记忆密码**: ServerEditDialog 勾选 "记住密码" 保存 → 重启 Desktop → 再次打开仍可连接; 配置文件中 `password` 字段为空, `password_source: "keyring"`; Windows Credential Manager 中可见 `napcat-desktop:ssh` 条目.
- [ ] **F5.3 私钥拖拽**: 拖拽 `id_rsa` 文件到私钥路径输入框 → 自动填充; 拖拽多文件 / 目录 → 弹 "请拖入单个私钥文件".
- [ ] **F5.4 错误文案**: 故意输错密码 → InfoBar "用户名或密码错误"; 故意填错端口 → "目标端口拒绝连接".
- [ ] **F2 批量启动**: BotPage 进入批量模式 → 勾选 ≥3 个 Bot → "批量启动" → 主窗口右上 `ProgressInfoBar` 推进 "已完成 X / N"; 完成后 InfoBar 汇总 "成功 X / 失败 Y".
- [ ] **F2 批量删除**: 同上, "批量删除" → 二次确认列出全部受影响 Bot 名 + QQID; 确认后串行删除.

### W2 — 资源监控 + 状态聚合

- [ ] **F3 阈值告警 (CPU)**: 远端模拟 30 秒 CPU >90% (`stress-ng --cpu 0 --timeout 30`) → InfoBar "服务器 X CPU 使用率持续 >90%"; 同 (server, cpu) 5 分钟内不再重复.
- [ ] **F3 阈值告警 (mem / disk)**: 单点 mem >90% / disk >90% → 各触发一次 InfoBar.
- [ ] **F3 worker 优雅退出**: 删除服务器 → `ResourceMonitorService.detach` 立即触发, 进程内 `ResourceMonitor[<server-id>]` 线程在下一个 `INTERVAL_OK` 内退出 (≤10s).
- [ ] **F4 RemoteSummaryCard 空态**: 无任何远端服务器时, HomePage 顶部卡片折叠为单行 "尚未添加远端服务器, 点此添加"; 点击跳转到 RemotePage.
- [ ] **F4 RemoteSummaryCard 告警态**: 24h 内有 `threshold_breached` 事件后, 卡片显示 "服务器名 · CPU 95% · X 分钟前"; 点击跳转 RemotePage 并定位告警服务器.
- [ ] **F4 StatusOverviewDialog**: RemotePage 工具栏点击 "状态总览" → 三栏列表分别展示服务器/远端 Bot/后台任务; 关闭对话框时所有 Qt 信号订阅断开 (无内存泄漏).

### W3 — 持久数据迁移

- [ ] **F6 远端 → 本地完整搬运**: Bot 在远端有 NapCat 数据库后切换 `runtime_target` 至本地 → MigrationDialog 默认勾选 "搬运 NapCat 持久数据" → 进度条字节级推进 "持久数据搬运: X.X / Y.Y MiB" → 完成后本地 NapCat 数据完整可用.
- [ ] **F6 中途断网续传**: 在搬运到 ~50% 时手动断网 1 分钟 → InfoBar 报失败, 目标端保留 `*.partial`; 重新点 "迁移" → 字节进度从 ~50% 起恢复, 最终一致.
- [ ] **F6 同名同 size 跳过**: 已搬运过的文件保留时再次迁移 → `_collect_persistent_files` 跳过, 总字节数显著减小.

### W4 — `exec_stream` 中途断开自动重连

- [ ] **F7 helper 默认行为**: 不修改任何调用方的情况下 (默认 `max_retries=0`), `install_napcat` 部署 / log tail 行为与 P3 一致, 中途断网仍按现状抛错.
- [ ] **F7 手动开启重连 (可选)**: 在 `RemoteBackend.install_napcat` 调用处临时改 `max_retries=2`, 部署到 50% 时手动断网 1s → 部署不报错, 进度从断点续接; 远端 log tail 期间断网 1s → 日志面板恢复后不丢失新行. *(标 "可选": P4 默认禁用, 本步只做风险评估; 通过后再决定是否在 P5 默认开启.)*

### 全量回归

- [ ] 全量 `pytest script/test/ -q --no-header` 通过 (含 W1+W2+W3+W4 新增用例).
- [ ] 应用启动 `uv run .\main.py --dev` exit_code=0; 关闭主窗口 exit_code=0.

## 4. 已知豁免 / 风险

| 项 | 风险 | 缓解 |
| --- | --- | --- |
| W2 F3.2 service 单测被跳过 | Qt threading 层 mock 在某些 Python/Qt 环境下导致 pytest 进程卡死 | 该路径 (worker 启停 / 阈值滑窗 / 5min 冷却) 由 W2 手动验收第 1-3 项覆盖 |
| W4 F7 默认 `max_retries=0` | 启用重连后, 部署脚本若不幂等会触发双倍下载 / dpkg 重装 | 默认禁用; 真机验证 `install_napcat.sh` 幂等性后再调高 retries |
| W3 F6 跨平台路径 | 本地 `%APPDATA%/Tencent/QQ` 路径与远端 `~/.config/QQ` 不严格对应 (Windows QQ 数据布局历史变化) | 遇 schema 不兼容时只搬运 NapCat data 子树, .config/QQ 走"size 一致跳过"自然回退 |

## 5. 完成判定

- [ ] 第 3 节所有验收项均 ✅
- [ ] 第 4 节风险全部已被验证或被排期至 P5
- [ ] [`remote_ssh_progress.md`](./remote_ssh_progress.md) 同步标记 P4 "已通过验收"
- [ ] [`outputs/runtime/vibe-sessions/<run-id>/skeleton-receipt.json`](../../outputs/runtime/vibe-sessions/) 落盘 (vibe runtime 工件)

完成上述四项后即可宣布 **P4 高级能力已通过验收**.

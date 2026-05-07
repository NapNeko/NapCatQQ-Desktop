# 远程 SSH P4 — 高级能力 (requirement)

> 关联文档:
> - [`docs/general/remote_ssh_plan.md`](../general/remote_ssh_plan.md) §7 P4
> - [`docs/general/remote_ssh_progress.md`](../general/remote_ssh_progress.md)
> - [`docs/general/remote_ssh_p3_acceptance.md`](../general/remote_ssh_p3_acceptance.md) §4 推迟项
> - [`docs/requirements/2026-05-06-remote-ssh-p3-perf.md`](./2026-05-06-remote-ssh-p3-perf.md)
>
> 运行模式: `vibe interactive_governed`
> 终止阶段: `phase_cleanup`
> 冻结日期: 2026-05-06

## 1. 背景

P3 主线 (A/B/C/E/F) 与 P3 perf (启动异步化 + ProgressInfoBar 进度反馈) 全部交付,
v2 累计 317/317 测试全绿. 进入"高级能力阶段"前, 现有体系尚遗留四类用户可见缺口:

1. **批量操作缺位** — BotPage 单次只能对一个 Bot 启停 / 迁移 / 删除;
   多 Bot (>= 5) 时主线交互重复, 远端 Bot 启停尤其耗时.
2. **远端服务器健康度不可见** — 服务器仅有"在线 / 离线"两态,
   CPU / 内存 / 磁盘水位用户全凭体感; NapCat 远端 OOM / 磁盘写满时只能从日志倒查.
3. **多服务器 / 多 Bot 全局视角缺失** — `BackgroundTaskCenter` (P3 perf) 已经聚合
   后台任务数, 但服务器在线状态、远端 Bot 在线分布仍散落各页面;
   首页 (`HomePage`) 对远端无任何可视提示.
4. **首次连接 / 凭据 / 错误文案不打磨** —
   - 首次连接没有 host key 指纹确认对话框, paramiko 自动 `AutoAddPolicy` 信任新主机, 不符合 §6.2 安全基线.
   - 密码以明文写到本地配置文件, 未走 Windows Credential Manager.
   - 私钥仅支持文件选择, 无拖拽.
   - 远端错误抛 `AuthenticationException` / `SSHException` 直出, 文案不亲民.

此外, P3 还显式推迟两项:

5. **NapCat 持久数据迁移** — `MigrationDialog` 当前仅迁配置, 账号缓存 / 数据库
   留待 P4 评估目录约定后实现.
6. **`exec_stream` 中途断开自动重连** — paramiko 限制下流式命令重启会重复副作用,
   P3·C 仅入口前探活, 中途断开原样上抛.

## 2. In-Scope (本次必须交付)

| 子项 | 标签 | 简述 |
| --- | --- | --- |
| **F2** | 批量 Bot 管理 | BotPage 多选 + 批量启动 / 停止 / 迁移 / 删除; 复用 P3 perf 异步派发 + ProgressInfoBar; 批量结果走单个聚合 InfoBar (成功 N / 失败 N · 详情). |
| **F3** | 远端资源监控 (10s 后台轮询) | `RemoteBackend.sample_resources()` 周期性 SSH 采样 (CPU / mem / disk); 服务器连接成功后由 `ResourceMonitorService` 启动 10s 轮询 worker; 阈值越界 (CPU > 90% 持续 30s, mem > 90%, disk > 90%) 触发一次性 InfoBar 提醒, 同窗口冷却 5min. |
| **F4** | 状态聚合面板 + 首页卡片 | 新增 `StatusOverviewDialog` (从 RemotePage 入口打开), 聚合: 服务器在线状态 + 远端 Bot 状态 + 进行中后台任务 + 资源水位; HomePage 顶部新增 `RemoteSummaryCard`: 服务器数 / 在线 Bot 数 / 最近一条告警, 点击跳转 RemotePage. |
| **F5** | 体验细节 (含安全) | 4 项: ① 首次连接 host key 指纹确认对话框 + `known_hosts` 持久化, 不再无声 `AutoAddPolicy`; ② Windows Credential Manager keyring 存储 SSH 密码 (`keyring` 库, 仅 Windows 启用); ③ ServerEditDialog 私钥字段支持文件拖拽; ④ 远端错误统一映射到 `RemoteFriendlyError` (AuthenticationException → "用户名或密码错误", `NoValidConnectionsError` → "无法连接到主机, 请检查 IP 与端口", etc.). |
| **F6** | NapCat 持久数据迁移 | `MigrationDialog` 的 future flag 兑现: 把 NapCat 持久数据 (账号缓存 + 数据库, 路径白名单详见 §4.F6) 在 Bot 运行位置切换时一并搬运; 含进度反馈 (字节级) + 单文件断点续传 + 失败回滚; UI 在 `MigrationDialog` 解锁"搬运 NapCat 持久数据"勾选项. |
| **F7** | `exec_stream` 中途断开自动重连 (幂等场景) | 仅对 `install_napcat` 部署进度流 + `RemoteNapCatQQLog.tail` 这两条幂等路径启用: 流意外断开时, 检测 transport 死亡 → 重连 → 从日志文件 offset / `[PROGRESS]` 续读; 不对启动 / 停止 / 配置写入这类非幂等命令开启. |

## 3. Out-of-Scope (本期明确不做)

- **F1 自动切换 (本地不可用 → 远端)** — 用户决策放弃, 不再计入 P4/P5 路线.
- 远端**进程级**资源监控 (per-Bot CPU/mem) — F3 仅做主机级, 进程级留待后续.
- F3 告警通知中心 — 不做; 仅 InfoBar 一次性提醒, 历史告警仅在 `StatusOverviewDialog` 内显示最近 N 条.
- F5 macOS / Linux 客户端 keyring — 当前 Desktop 仅 Windows 发行, keyring 仅启用 `WinVaultKeyring`.
- F6 跨 NapCat 大版本数据格式迁移 — 仅原样搬运, 版本不兼容时给出告警, 不做转换.
- F7 启动 / 停止 / 配置写入命令的中途重连 — paramiko 流式重启重复副作用风险高, 维持 P3·C 现状.
- 任务取消按钮 (停止运行中的 SSH worker) — 同 P3 perf 决策, 不在范围.
- 重做 `BackgroundTaskCenter` 数据模型 — 复用 P3 perf 既有结构, F4 仅消费, 不重写.

## 4. 验收标准 (acceptance criteria)

每个子项独立可验收, 任一失败本次优化不通过.

### F2. 批量 Bot 管理

- BotPage 进入"批量模式"后, 每张 BotCard 出现复选框; 顶部工具条显示
  `已选 N` + 「批量启动 / 批量停止 / 批量迁移 / 批量删除」按钮.
- 批量启动 / 停止: 派发 N 个 `RemoteBotOperationRunnable` (远端) 或本地 QProcess 启动,
  全程不阻塞主线程; 主窗口右上 `ProgressInfoBar` 显示 `批量启动: 已完成 X / N`.
- 批量迁移: 弹一次 `BatchMigrationDialog`, 选择目标 runtime_target,
  顺序串行派发 (避免对同一目标服务器并发写配置), 失败 Bot 列出可跳过.
- 批量删除: 二次确认对话框列出全部受影响 Bot 名 + QQID;
  确认后串行删除本地 + 远端配置 (复用 `delete_config` 异步路径).
- 全部子操作完成后弹一次聚合 InfoBar: `成功 X / 失败 Y · 查看详情` (点击展开错误列表).
- 测试: `script/test/test_bot_page_batch_mode.py` (UI 选择态) +
  `script/test/test_batch_bot_dispatcher.py` (派发与聚合).

### F3. 远端资源监控

- 新增 `src/core/remote/resource_monitor.py`:
  ```python
  class ResourceSample(BaseModel):
      timestamp: float
      cpu_percent: float       # 0-100
      mem_percent: float       # 0-100, used / total
      disk_percent: float      # 0-100, $HOME 所在分区
      load_avg_1: float | None
      raw: dict                # 原始 ssh 输出, 调试用
  
  class ResourceMonitorService(QObject):
      sample_arrived = Signal(str, ResourceSample)         # server_id, sample
      threshold_breached = Signal(str, str, float)         # server_id, metric, value
      def attach(self, server_id: str) -> None: ...        # 启动 10s 轮询
      def detach(self, server_id: str) -> None: ...
      def latest(self, server_id: str) -> ResourceSample | None: ...
  ```
- `RemoteBackend.sample_resources()` 单次 SSH 调用, 推荐:
  ```sh
  echo "CPU $(top -bn1 | awk 'NR==3{print $2+$4}')";
  echo "MEM $(free | awk '/Mem/{printf "%.1f", $3/$2*100}')";
  echo "DISK $(df -P "$HOME" | awk 'NR==2{print $5}' | tr -d '%')";
  echo "LOAD $(awk '{print $1}' /proc/loadavg)"
  ```
  解析失败时返回 `None`, 服务保持上次值, 不抛异常到 UI.
- 服务器 `ServerStatus.connected` 时自动 `attach`, 断线 / 删除时 `detach`.
- 阈值: CPU > 90% **连续 3 个采样点 (30s)** / mem > 90% / disk > 90% 各自触发一次
  `threshold_breached`, 同 (server_id, metric) 冷却 5 分钟.
- 测试: `script/test/test_resource_monitor_service.py` (使用 backend mock 注入采样序列, 验证阈值与冷却) +
  `script/test/test_remote_backend_sample_resources.py` (解析).

### F4. 状态聚合面板 + 首页卡片

- 新增 `src/ui/components/status_overview_dialog.py` (`StatusOverviewDialog`):
  - 三栏: ① 服务器列表 (在线 / 离线 / 部署中, 资源水位条)
    ② 远端 Bot 列表 (名称 / runtime_target / 进程状态)
    ③ 后台任务 (`BackgroundTaskCenter.active_tasks()` 直接渲染).
  - RemotePage 工具栏新增"状态总览"按钮入口.
- 新增 `src/ui/components/remote_summary_card.py` (`RemoteSummaryCard`),
  挂在 `HomePage` 顶部:
  - 显示: 服务器总数 / 在线服务器数 / 在线远端 Bot 数 / 最近一条阈值告警 (24h 内).
  - 点击跳转 `RemotePage` 并选中告警相关服务器.
  - 无远端服务器时卡片折叠为单行"尚未添加远端服务器, 点此添加".
- 数据源全部消费 `ServerManager` + `ResourceMonitorService` + `BackgroundTaskCenter` 既有信号,
  不新增持久存储.
- 测试: `script/test/test_status_overview_dialog.py` (mock 三个服务) +
  `script/test/test_remote_summary_card.py` (空态 / 告警态 / 跳转).

### F5. 体验细节 (含安全)

按四个独立子项验收:

#### F5.1 host key 指纹确认对话框

- `SSHClient` 取消 `AutoAddPolicy`, 改用自定义 `InteractiveHostKeyPolicy`.
- 未知主机首次连接时, **主线程**弹 `HostKeyConfirmDialog`:
  显示主机指纹 (SHA256, base64) + 密钥类型 + "信任并保存" / "本次连接" / "拒绝" 三选项.
  选择 "信任并保存" 后写入用户级 `known_hosts` (路径见 §5).
- 已知主机指纹变化时弹**警告对话框** (红色样式), 默认拒绝.
- 测试: `script/test/test_ssh_host_key_policy.py` (mock paramiko transport, 无 UI 依赖) +
  `script/test/test_host_key_confirm_dialog.py` (UI 三态).

#### F5.2 Windows Credential Manager keyring

- 新增依赖 `keyring >= 24` (仅 Windows 启用 `WinVaultKeyring`).
- ServerEditDialog 密码字段保存时, 若用户勾选 "记住密码", 走
  `keyring.set_password("napcat-desktop:ssh", server_id, password)`,
  配置文件中 `password` 字段置空 + `password_source: "keyring"` 标记.
- 加载凭据时优先 keyring, fallback 到配置文件 (兼容旧数据).
- 非 Windows 平台 (开发环境) 自动降级为不勾选 + 文件存储, 给出 InfoBar 提示.
- 测试: `script/test/test_credential_keyring.py` (用 `keyring.backends.fail.Keyring` mock) +
  `script/test/test_server_edit_dialog_remember.py`.

#### F5.3 私钥字段拖拽

- ServerEditDialog 的私钥路径输入框 (`LineEdit`) `setAcceptDrops(True)`,
  `dropEvent` 接受单文件 (`.pem` / `id_rsa` / 无后缀但首行匹配 `-----BEGIN`),
  自动填充路径 + 触发字段校验.
- 多文件 / 目录 / 不匹配文件: 拖入提示 "请拖入单个私钥文件".
- 测试: `script/test/test_server_edit_dialog_drag_drop.py`.

#### F5.4 错误文案统一

- 新增 `src/core/remote/friendly_errors.py`:
  ```python
  FRIENDLY_MESSAGES: dict[type[Exception], Callable[[Exception], str]] = {
      paramiko.AuthenticationException: lambda e: "用户名或密码错误",
      paramiko.BadHostKeyException: lambda e: f"主机指纹与已知记录不匹配: {e.hostname}",
      socket.gaierror: lambda e: f"无法解析主机名 ({e})",
      ConnectionRefusedError: lambda e: "目标端口拒绝连接, 请检查 SSH 服务是否启动",
      TimeoutError: lambda e: "连接超时, 请检查网络与防火墙",
      # ...
  }
  
  def to_friendly(exc: Exception) -> str: ...
  ```
- 所有 RemotePage / ServerEditDialog / BotPage 用户可见的 SSH 错误路径
  改为 `to_friendly(exc)` 输出; 调试日志保留原始 traceback.
- 测试: `script/test/test_friendly_errors.py` 覆盖 8 种典型异常.

### F6. NapCat 持久数据迁移

- 路径白名单 (Linux 远端):
  - `$HOME/Napcat/opt/QQ/resources/app/app_launcher/napcat/data/` (NapCat 数据目录)
  - `$HOME/.config/QQ/` (QQ 账号缓存, 路径需运行时探测确认)
  - **不**搬运: `$HOME/Napcat/log/`, `$HOME/Napcat/run/`, `tmp/`.
- `MigrationService.migrate(...)` 增加参数 `transfer_persistent_data: bool`:
  - True 时除 NapCat 配置外, 顺序搬运白名单目录 (源 backend → 目标 backend).
  - 单文件传输走 `OperationBackend.read_bytes` / `write_bytes` 流式分片 (1 MiB chunk),
    每分片完成回调 `migration_progress_signal(qq_id, transferred, total)`.
  - 单文件断点续传: 目标已存在且 size 一致 + mtime >= 源 mtime 时跳过;
    部分写入文件保留 `.partial` 后缀, 下次续写时检测.
- 失败回滚: 任一文件搬运失败 → 已写入的目标文件标记 `.partial` 不删,
  弹失败 InfoBar + "保留已传输部分以便重试" 提示; 不回退已搬运的配置 JSON
  (与 P3·B 既有语义一致).
- `MigrationDialog` 解锁原 `transfer_persistent_data` future flag, 默认勾选,
  显示估算大小 + 进度条 (字节级).
- 测试: `script/test/test_migration_persistent_data.py` (mock backend 注入文件树, 验证完整 + 续传 + 失败保留 partial) +
  `script/test/test_migration_dialog_persistent_flag.py`.

### F7. `exec_stream` 中途断开自动重连 (幂等场景)

- `RemoteBackend.install_napcat` (流式 `[PROGRESS]` 解析路径) +
  `RemoteNapCatQQLog._tail_loop` (日志 tail) 两条路径接入新增辅助
  `_iter_stream_with_resume(...)`:
  - 检测 `transport` 死亡 → 调用 `SSHClient.ensure_alive()` 重连 (P3·C 既有能力).
  - 部署进度流: 重连后从已读 `[PROGRESS] N` 的 `N` 续判 (脚本本身幂等, 跳过已完成步骤).
  - 日志 tail: 重连后从远端文件已读 byte offset 续读 (`tail -c +{offset}`).
- 重连最多 3 次, 间隔 1s / 3s / 8s; 全部失败后原样上抛 `RemoteConnectionLost`,
  UI 走既有错误处理路径.
- **不**接入: `start_napcat` / `stop_napcat` / 配置写入路径.
- 测试: `script/test/test_exec_stream_resume.py` 用 mock `SSHClient` 模拟两次断流,
  验证进度续判 + log offset 续读 + 三次失败上抛.

## 5. 文件与依赖影响清单

新增模块:

| 路径 | 用途 |
| --- | --- |
| `src/core/remote/resource_monitor.py` | F3 `ResourceMonitorService` + `ResourceSample` 模型 |
| `src/core/remote/friendly_errors.py` | F5.4 异常 → 用户文案映射 |
| `src/core/remote/host_key_policy.py` | F5.1 自定义 paramiko host key policy |
| `src/core/remote/credential_store.py` | F5.2 keyring 抽象 + 配置降级 |
| `src/core/operation/batch_dispatcher.py` | F2 批量任务派发与结果聚合 |
| `src/ui/components/status_overview_dialog.py` | F4 聚合面板 |
| `src/ui/components/remote_summary_card.py` | F4 首页卡片 |
| `src/ui/components/host_key_confirm_dialog.py` | F5.1 |

修改模块:

| 路径 | 改动 |
| --- | --- |
| `src/core/remote/ssh_client.py` | 接入 `host_key_policy` + `_iter_stream_with_resume` (F7) |
| `src/core/remote/server_manager.py` | 启动 / 关闭时 `attach/detach` `ResourceMonitorService` |
| `src/core/remote/deployment.py` | `install_napcat` 走流式重连 (F7) |
| `src/core/runtime/napcat.py` | `RemoteNapCatQQLog` tail 走流式重连 (F7) |
| `src/ui/page/bot_page/__init__.py` | 批量模式入口 + 工具条 (F2) |
| `src/ui/page/bot_page/widget/card.py` | 复选框态 (F2) |
| `src/ui/page/remote_page/server_edit_dialog.py` | 拖拽 (F5.3) + 记住密码 (F5.2) |
| `src/ui/page/remote_page/__init__.py` | 状态总览按钮入口 (F4) |
| `src/ui/page/home_page/` | 嵌入 `RemoteSummaryCard` (F4) |
| `src/core/config/operate_config.py` | `MigrationService` 持久数据搬运分支 (F6) |
| `src/ui/components/migration_dialog.py` | 解锁 future flag (F6) |

新增依赖 (`pyproject.toml` / `requirements.txt`):

- `keyring >= 24` (F5.2, 仅 Windows 启用)

新增配置文件:

- 用户级 `known_hosts`: `%LOCALAPPDATA%/NapCatQQ-Desktop/ssh/known_hosts` (F5.1)

## 6. 完成语言策略 (delivery truth contract)

- 每个子项独立分支 + PR, 通过新增单测 + 全量 P0/P1/P2/P3 既有用例零衰退后, 方可声称该子项 "完成".
- F4 / F5.1 / F6 涉及真实远端的路径标注 "网络相关需手动验证", 在
  `docs/general/remote_ssh_p4_acceptance.md` 记录手动验证 checklist.
- P4 整体 "完成" 仅当 6 个子项 (F2/F3/F4/F5/F6/F7) 全部交付 + 验收文档定稿 + 进度文档同步.

## 7. 非目标 (再次明确)

- 不引入 `asyncio` / `qasync`; 异步仍走 `QThreadPool`.
- 不替换 `BackgroundTaskCenter` 数据模型, F4 仅消费.
- 不改动 `OperationBackend` 公共抽象方法签名 (仅新增 `sample_resources` / 流式重连内部辅助).
- 不动 P3 perf `ProgressInfoBarBridge`, F2 / F6 进度反馈复用既有桥接.
- 不做跨平台 keyring (仅 Windows).
- 不做远端进程级资源监控 (per-Bot).

## 8. 风险与回滚

| 风险 | 应对 |
| --- | --- |
| F3 10s 轮询在多服务器场景累积 SSH 负荷 | 单服务器单 worker, 失败采样指数退避 (10s → 30s → 1min); 用户可在 ServerEditDialog 关闭监控 |
| F5.1 host key 改动导致老用户首次启动全部弹窗 | 首次启动检测既有 `~/.ssh/known_hosts` 自动导入; 提供"信任全部已保存服务器"批量按钮 |
| F5.2 keyring 写入失败 (Windows 环境异常) | 自动降级为配置文件存储 + InfoBar 一次性提示, 不阻断保存流程 |
| F6 大文件搬运中途网络断开 | `.partial` 续传机制, 重试时跳过已完成分片; 用户可手动取消保留已传输部分 |
| F7 流式重连导致部署脚本重复执行副作用 | 仅对 `install_napcat` (脚本本身幂等) + log tail (只读) 启用, 显式排除非幂等命令 |
| BackgroundTaskCenter 数据被 F2 批量模式打爆 | 批量任务作为**单一**逻辑任务上报 Center (label: `批量启动 (N)`), 内部 N 个 worker 不重复登记 |

## 9. 交付节奏建议 (供 xl_plan 阶段参考)

按风险与依赖, 推荐 4 wave 串行 + 内部并行:

- **Wave 1**: F2 批量 Bot 管理 + F5 体验细节 (4 子项独立, UI/UX 增量, 互不冲突)
- **Wave 2**: F3 资源监控 + F4 状态面板 / 首页卡片 (F4 消费 F3 数据, 但接口可先 mock 并行)
- **Wave 3**: F6 持久数据迁移 (体量最大, 独立)
- **Wave 4**: F7 流式重连 (收尾, 改动深入 SSH 层, 单独验证)

最终 wave 划分以 `docs/plans/2026-05-06-remote-ssh-p4-execution-plan.md` 冻结结果为准.

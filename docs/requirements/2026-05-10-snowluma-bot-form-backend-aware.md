# SnowLuma Bot 表单 后端感知 — 需求冻结

- **创建日期**: 2026-05-10
- **冻结时间戳**: governed by `vibe` runtime (`2026-05-10T2238-snowluma-bot-form-backend-aware`)
- **会话主题**: 在 P1 SnowLuma 后端适配基础上, 把 Bot Setup 表单升级为 **后端感知** (backend-aware) + 把 Desktop 变成 SnowLuma WebUI 的**编程客户端**, 让用户像用 NapCat 一样 "一键启动 SnowLuma Bot". 两件事合并交付, 因为只改 UI 显隐不解决"SnowLuma Bot 启动后 bot 根本不在线"的根本问题. P1 手工验收阶段发现双重缺陷: (1) 表单里 NapCat-only / SnowLuma-only 字段互相串通, renderer 不消费用户输入; (2) Desktop 只启动 node.exe, 但 SnowLuma 的 QQ 注入必须通过 WebUI API 触发, Desktop 完全未接入.

---

## 0. 现状对照 (Background)

P1 `docs/requirements/2026-05-10-snowluma-backend-adapter.md` 让 Desktop 支持 SnowLuma 后端并列启动, 但 §2.6 UI 适配只动了 BotCard 徽标 + WebUI 按钮分流 + 二维码按钮隐藏, **没动 BotConfigWidget / ConnectConfigWidget / AdvancedConfigWidget 的字段显隐**.

带来的实际问题 (P1 手工验收阶段发现):

- `render_onebot_json` 现有 signature 只接受 `http_port`/`ws_port`/`access_token`/`message_format`/`report_self_message`/`music_sign_url`/`host` 这 7 个标量, 主路径 `_create_snowluma_process` 全部走默认值, 用户在 ConnectConfigWidget 里改的 httpServers/websocketServers 完全没传给 renderer
- 用户在 AdvancedConfigWidget 看到 `parseMultMsg` / `packetServer` / `packetBackend` / `o3HookMode` / `bypass.*` 等一堆 NapCat 注入式专有字段, 改了以为对 SnowLuma Bot 有效, 其实 SnowLuma 完全不读这些
- 用户在 ConnectConfigWidget 看到 `debug` / `enableCors` / `enableWebsocket` / `enableForcePushEvent` / `heartInterval` 等 NapCat-only 字段, SnowLuma 也完全不消费
- SnowLuma 有些独有字段 (`httpServers[].path` / `wsServers[].path/role` / `wsClients[].role/reconnectIntervalMs` / `httpClients[].timeoutMs`) 表单里完全无法配置, 用户若想改必须跳出 Desktop 去 SnowLuma WebUI, 破坏了 "SnowLuma 在 Desktop 里与 NapCat 平权" 的设计目标
- **(深度审查后新增) SnowLuma 注入流程完全未接入**: Desktop 只启 `node.exe index.mjs`, 然后轮询 OneBot WS `get_status`. 但 SnowLuma 的 QQ 注入要用户去浏览器打开 WebUI (`http://127.0.0.1:5099/`) → 登录密码 → `GET /api/processes` 列 QQ.exe → `POST /api/processes/<pid>/load` 触发注入. Desktop 没实现这一流程, 因此 P1 手工验收时"启动 SnowLuma Bot"后 bot 永远处于"已启动 node 但未注入"状态, OneBot `get_status` 自然永远 timeout. 证据链: `example/SnowLuma-main/packages/core/src/index.ts:23-33` (WebUI 是注入入口) + `packages/core/src/webui/server.ts:350-399` (processes/load/unload API) + `packages/core/src/hook/hook-manager.ts:77-110` (HookManager 本身不会自动扫描+注入, 必须被 API 调用)

字段级对照见 §10.1. WebUI API 全集与注入流程见 §10.3.

## 1. 目标 (Goal)

本需求把 **后端感知 Bot 表单** 与 **SnowLuma WebUI API 客户端化** 合并交付, 目标是:

### 1.1 表单字段双向平权

让 `BotConfigWidget` / `ConnectConfigWidget` / `AdvancedConfigWidget` / 各 NetworkBase 子配置卡片 / 各网络配置 Dialog / `ChooseConfigTypeDialog` 按 `BotConfig.backend_type` **双向平权**地显隐字段:

- SnowLuma 模式看不到 NapCat 注入式字段 (hook/bypass/packet/o3/SSE/plugins/日志开关 等), 但看得到 SnowLuma 独有字段 (path/role/timeoutMs/reconnectIntervalMs)
- NapCat 模式看不到 SnowLuma 独有字段, 看得到所有 NapCat 既有字段 (零行为变化)
- 双向切换 `backend_type` 时, 持久化字段值零丢失 (UI 只做显隐, 不清值)

### 1.2 Renderer 真消费 ConnectConfig

让 `render_onebot_json` 真正消费 `ConnectConfig`, 把用户在 ConnectConfigWidget 里改的 httpServers / httpClients / websocketServers / websocketClients 全量映射到 SnowLuma `networks.*`.

### 1.3 Desktop 作为 SnowLuma WebUI 的编程客户端 (新增)

让 Desktop 像 NapCat 那样 **一键启动 SnowLuma Bot**, 把用户从 "手动打开浏览器 → 登录 WebUI → 选 PID → 点注入" 的繁琐流程解放出来:

- Desktop 启动 Bot 时: spawn QQ.exe + 起 SnowLuma node.exe + 登录 SnowLuma WebUI + 调 `POST /api/processes/<pid>/load` 注入 + 等 QQ 登录成功
- Desktop 停止 Bot 时: 反向清理 (`unload` API + `logout` API + terminate 两个 QProcess)
- BotCard 状态轮询改用 SnowLuma WebUI `GET /api/processes` (而非 OneBot WS `get_status`), 能看到 `available` / `loading` / `connecting` / `loaded` / `online` / `error` / `disconnected` 7 档真实状态
- Desktop 主导 SnowLuma WebUI 密码: 安装 SnowLuma 后 Desktop 自动生成随机强密码写入 `webui.json`, 存 Desktop `config/snowluma-session.json` (chmod 600), 之后用户不需要感知/管理密码 (但用户在 WebUI 改过密码后 Desktop 会在下次启动时覆盖回来)

## 2. 交付物 (Deliverable)

### 2.1 配置模型扩展 (`src/core/config/config_model.py`)

1. `HttpServersConfig` 新增 `path: str = "/"` (SnowLuma 独有, NapCat 不读)
2. `HttpClientsConfig` 新增 `timeoutMs: int | None = None` (SnowLuma 独有, NapCat 不读; None 表示不传给上游 → 走 SnowLuma 默认)
3. `WebsocketServersConfig` 新增 `path: str = "/"` 与 `role: Literal["Api","Event","Universal"] = "Universal"` (SnowLuma 独有)
4. `WebsocketClientsConfig` 新增 `role: Literal["Api","Event","Universal"] = "Universal"` (SnowLuma 独有); **沿用**现有 `reconnectInterval: int = 30000` (与 SnowLuma `reconnectIntervalMs` 同语义同单位, 渲染时名映射)
5. **不**在 `HttpSseServersConfig` 加 SnowLuma 字段 (SSE 本身就是 NapCat-only)
6. 迁移层 (`_migrate_legacy_*`) 不需要动, pydantic 默认值自动补齐老配置文件的新字段
7. 单个 bot.json 的 schema 版本 `BOT_CONFIG_COMPAT_VERSION` **保持 v2.0**, 因为只是加可选字段, 不破坏兼容

### 2.2 Renderer 重构 (`src/core/runtime/snowluma_config_renderer.py`)

8. `render_onebot_json` signature 改为:
   ```python
   def render_onebot_json(
       snowluma_path: Path,
       qqid: int,
       *,
       connect: ConnectConfig,
       music_sign_url: str = "",
   ) -> None:
   ```
   移除 `http_port`/`ws_port`/`access_token`/`message_format`/`report_self_message`/`host` 6 个标量参数
9. 从 `connect.httpServers` / `connect.httpClients` / `connect.websocketServers` / `connect.websocketClients` 各自全量映射到 SnowLuma `networks.httpServers` / `httpClients` / `wsServers` / `wsClients` (数组映射数组, 支持多项; `httpSseServers` / `plugins` 静默丢弃)
10. 字段名映射规则 (NapCat → SnowLuma):
    - `enable` → `enabled`
    - `name` → `name` (同名)
    - `messagePostFormat` → `messageFormat`
    - `token` → `accessToken` (空字符串也写入, 与 P1 现状一致)
    - `reportSelfMessage` → `reportSelfMessage` (同名)
    - `host` → `host` (同名)
    - `port` → `port` (同名)
    - `url` → `url` (同名)
    - `path` → `path` (同名, 默认 `/`; SnowLuma 侧 **HTTP server 的 path 是前缀挂载点** (`path=/api` + 请求 `/api/send_msg` → 走 `send_msg` action; 不匹配则 404), **WS server 的 path 是 exact match** (客户端连接路径必须与 path 完全一致, 由 `ws` 库的 `path` 参数决定); NapCat 两类 server 都固定挂在 `/` 无此配置, 详见 §10.2)
    - `role` → `role` (同名, 默认 `Universal`; SnowLuma 独有, 未显式配置时按 URL 尾部 `/api` / `/event` 自动分类, 详见 §10.2)
    - `reconnectInterval` (ms) → `reconnectIntervalMs` (毫秒同单位; **SnowLuma 侧强制 `Math.max(1000, value)` 下限**, 详见 §10.2 与交付项 #12a)
    - `timeoutMs` → `timeoutMs` (仅当非 None 时写入)
11. NapCat-only 字段 (`debug` / `enableCors` / `enableWebsocket` / `enableForcePushEvent` / `heartInterval`) 映射时**静默丢弃**, 不写入 SnowLuma JSON
12. 若 `connect.httpServers` 与 `connect.websocketServers` 都为空, 自动兜底一份与 SnowLuma `makeDefaultOneBotConfig()` 等价的默认值 (http-default port=3000 + ws-default port=3001 + 随机 accessToken), 避免 SnowLuma 因 `onebot_<uin>.json.networks` 全空而启动失败. `accessToken` 用 `secrets.token_urlsafe(32)` 随机生成
12a. **`reconnectInterval` clamp 行为**: renderer 在渲染 `wsClients[].reconnectIntervalMs` 时, 若用户填的值 <1000ms, renderer 层也执行 `max(1000, value)` clamp 并 `logger.warning` 提示 ("用户配置 X ms 被 clamp 到 1000ms; SnowLuma 上游下限限制"). 这样 Desktop 持久化保留原值 (切回 NapCat 仍是原值), 只在渲染到 SnowLuma 时 clamp, 双向平权语义不破
13. `read_existing_onebot_json` 不动 (升级路径用)

### 2.3 启动器对接 (`src/core/runtime/napcat.py`) — 渲染 onebot_<uin>.json 部分

14. `_create_snowluma_process` 内部对 `render_onebot_json` 的调用改为传 `ConnectConfig`:
    ```python
    render_onebot_json(
        snowluma_path,
        int(config.bot.QQID),
        connect=config.connect,
        music_sign_url=config.bot.musicSignUrl,
    )
    ```
15. **本节仅描述 onebot_<uin>.json 渲染部分的改动**, `_create_snowluma_process` 整体改造涉及 spawn QQ.exe + WebUI login + inject 的完整序列, 见 §2.13; `render_runtime_json` / `render_webui_json` 调用契约见 §2.12 密码管理

### 2.4 UI 显隐 — BotConfigWidget (`src/ui/page/bot_page/widget/config.py`)

16. `BotConfigWidget` 内部新增 `backend_type_changed = Signal(BackendType)` (或直接复用现有 `backend_type_card.view.currentIndexChanged`), 切换 backend_type 时 emit, 让父页面 (`BotWidget`) 转发给 `ConnectConfigWidget` 与 `AdvancedConfigWidget`
17. `BotConfigWidget` 自身字段全部保留, 无显隐变化 (7 个字段都是双后端通用或已决定保留)

### 2.5 UI 显隐 — ConnectConfigWidget + 各 ConfigCard (`config.py` + `card.py`)

18. `ConnectConfigWidget` 新增 `apply_backend_type(backend: BackendType)` 方法, 触发:
    - 遍历 `self.cards`, 调各 card 的 `apply_backend_type(backend)`
    - 如果当前 backend = SNOWLUMA 且存在 `HttpSSEConfigCard` 实例, 在界面上隐藏该类卡片 (持久化保留 —— 切回 NAPCAT 自动重现)
    - 不重建 widget, 仅 `setVisible`
19. `HttpServerConfigCard`: `apply_backend_type(backend)` 按 backend 显隐卡片内的 "摘要标签" (当前 card.py 只是展示 summary, 展开后走 Dialog); 这里**主要**是让卡片**仍可编辑** —— 真显隐发生在 Dialog 层 (§2.6)
20. `HttpSSEConfigCard`: SnowLuma 模式下整卡隐藏 (`setVisible(False)`), 避免出现在 flow layout 里
21. `WebsocketServersConfigCard` / `WebsocketClientConfigCard` / `HttpClientConfigCard`: 同 HttpServerConfigCard 处理 (卡片自身无需隐藏, 仅 Dialog 层显隐)
22. `plugins` 字段当前无卡片类, 无需改

### 2.6 UI 显隐 — 网络配置对话框 (`src/ui/page/bot_page/widget/msg_box.py`)

23. `ConfigDialogBase` 新增 `_current_backend: BackendType` 属性与 `apply_backend_type(backend)` 方法; 基类负责把 `debug_card` 在 SnowLuma 模式下隐藏 (所有子类共享)
24. `HttpServerConfigDialog` 重写 `apply_backend_type`: SnowLuma 模式隐藏 `cors_card` / `websocket_card`, 显示新增的 `path_card` (LineEdit, 默认 "/")
25. `HttpClientConfigDialog` 重写 `apply_backend_type`: SnowLuma 模式显示新增的 `timeout_ms_card` (LineEdit 整数, 空表示不传, 默认空)
26. `WebsocketServerConfigDialog` 重写 `apply_backend_type`: SnowLuma 模式隐藏 `force_push_event_card` / `heart_interval_card`, 显示新增的 `path_card` + `role_card` (ComboBox: Api/Event/Universal, 默认 Universal)
27. `WebsocketClientConfigDialog` 重写 `apply_backend_type`: SnowLuma 模式隐藏 `heart_interval_card`, 显示新增的 `role_card` (ComboBox: Api/Event/Universal); 沿用现有的 `reconnect_interval_card` (NapCat/SnowLuma 同义, 单位都是 ms)
28. 各 Dialog `fill_config` / `get_config` 加新字段的读写, 保持 NetworkBase 类型匹配 pydantic 模型 §2.1
29. **切换 backend_type 时已经打开的 Dialog 不强制关闭**, 用户下次打开时新 dialog 会按当前 backend 显隐

### 2.7 UI 显隐 — ChooseConfigTypeDialog (`msg_box.py`)

30. `ChooseConfigTypeDialog.__init__` 或 `apply_backend_type(backend)` 按 backend 显隐 "HTTP SSE 服务器" 选项:
    - NAPCAT: 6 类选项全可见 (HTTP Server / HTTP SSE / HTTP Client / WS Server / WS Client)
    - SNOWLUMA: 隐藏 HTTP SSE 选项, 其他 4 类可见
31. `ConnectConfigWidget` 在打开该 Dialog 前, 先调 `dialog.apply_backend_type(self._current_backend)`

### 2.8 UI 显隐 — AdvancedConfigWidget (`config.py`)

32. `AdvancedConfigWidget` 新增 `apply_backend_type(backend: BackendType)` 方法, SnowLuma 模式隐藏:
    - `parse_mult_message_card` (`parseMultMsg`, NapCat-only)
    - `local_file_to_url_card` (`enableLocalFile2Url`, NapCat-only)
    - `file_log_card` / `file_log_level_card` (NapCat 日志, SnowLuma 自治)
    - `console_log_card` / `console_level_card` (同上)
    - `backend_config_card` (整个 `packetServer` / `packetBackend` / `o3HookMode` / `bypass` 对话框入口, 全 NapCat 注入式专用)
33. SnowLuma 模式保留:
    - `auto_start_card` (Desktop 自启, 通用)
    - `offline_notice_card` (Desktop 通知, 通用)
34. 切换 NAPCAT → SNOWLUMA 时隐藏卡片保留原值, 切回即恢复可见

### 2.9 UI 编排层接线 (`src/ui/page/bot_page/__init__.py` 或 `sub_page/bot_list.py`)

35. Bot 编辑页 (外层容器) 把 `BotConfigWidget` 的 backend_type 切换信号连接到:
    - `ConnectConfigWidget.apply_backend_type`
    - `AdvancedConfigWidget.apply_backend_type`
36. Bot 编辑页首次加载 (`fill_config`) 后也要调一次 `apply_backend_type`, 保证初始可见性正确
37. 新建 Bot 流程 (`ChooseConfigTypeDialog` 前) 也要按当前选择的 backend_type 过滤可选项

### 2.11 SnowLuma WebUI HTTP 客户端 (新增)

新建模块 `src/core/runtime/snowluma_webui_client.py`, 封装所有 WebUI API 调用:

38. 客户端类 `SnowLumaWebUIClient(host: str, port: int, password: str, session_path: Path)`:
    - `wait_ready(timeout: float = 30.0) -> bool`: 轮询 `GET /api/status`, 返回是否起来
    - `login() -> str`: `POST /api/login {password}` → 拿 Bearer token, 存内部, 返回 token
    - `logout() -> None`: `POST /api/logout` 清理 server 端 session
    - `list_processes() -> list[HookProcessInfo]`: `GET /api/processes` → 返回 QQ.exe PID + status 列表
    - `load_process(pid: int) -> HookProcessInfo`: `POST /api/processes/<pid>/load` → 触发注入
    - `unload_process(pid: int) -> HookProcessInfo`: `POST /api/processes/<pid>/unload` → 卸载注入
    - `get_auth_state() -> dict`: `GET /api/auth/state` → 取 `mustChangePassword` + session 状态
    - `change_password(new: str) -> None`: `POST /api/auth/change-password {currentPassword, newPassword}` → 改密 (本期基本不用, 因为 Desktop 主导密码)
39. 数据类 `HookProcessInfo` 匹配上游 TS `HookProcessInfo` (见 `example/SnowLuma-main/packages/core/src/hook/hook-manager.ts:21-29`):
    - `pid: int`, `name: str`, `path: str`, `uin: str`, `status: str` (available/loading/connecting/loaded/online/error/disconnected), `error: str`
40. 认证语义: 每次 API 调用 (login 除外) 自动在 `Authorization: Bearer <token>` header 注入 token; 收到 401 自动 retry 一次 (重新 login + 重试)
41. 依赖 `httpx` (项目 requirements.txt 已有, NapCat 的 `/get_status` HTTP 调用也用 httpx); 所有调用走短连接, 不保持 httpx.Client 单例 (与现有 `SnowLumaStatusPoller` 对齐)
42. 错误类型: `SnowLumaWebUIError(status_code: int, message: str)` 统一封装, 上层 (`_create_snowluma_process`) 据此决定是否 kill 进程重启
43. 超时: `wait_ready` 1s 间隔, 最多 30s 共 30 轮; 其他 API 调用默认 5s timeout; `load_process` 因为涉及 native dlopen 放宽到 15s

### 2.12 SnowLuma WebUI 密码管理 (新增)

新建 Desktop 侧密码记录 `config/snowluma-session.json`:

44. 文件 schema: `{"password": "<随机强密码>", "created_at": "<ISO>", "last_rendered_at": "<ISO>"}`
45. **Desktop 完全主导密码生命周期**, 用户不在 Bot 表单管理密码 (避免与 SnowLuma WebUI 改密冲突):
    - 首次场景 (`config/snowluma-session.json` 不存在): `SnowLumaInstall` 成功后立即生成一个满足 SnowLuma 强密码规则 (≥10 位 + 大小写 + 特殊符号 + 不含空格, `example/SnowLuma-main/packages/core/src/webui/auth.ts:38-44`) 的随机密码, 记 Desktop 侧 session.json, 同时调 `render_webui_json(password=<密码>, must_change=False)` 覆盖 SnowLuma 侧 `webui.json`
    - 启动 Bot 前验证 (`_create_snowluma_process` 里): Desktop 记录的密码是 session.json 里的; 如果 `webui.json` 被用户在 WebUI 改过 (Desktop 一侧 POST /api/login 403), Desktop 重新 render_webui_json 覆盖 + kill+restart SnowLuma node.exe + 重 login; 这样用户在 WebUI 改密码的行为**被 Desktop 单向覆盖**, 不支持保留
46. `SnowLumaInstall._snowluma_session_path` 属性返回 `config/snowluma-session.json` 的 Path; 文件在 Windows 下用 `os.chmod(0o600)` (Windows 语义上只影响所有者 ACL, 但仍应写)
47. 文件损坏/手改: session.json 若 JSON 解析失败或 password 字段缺失, Desktop 视为"首次场景"走完整路径 (重生 + 重渲染)
48. **声明限制**: 用户**不能**在 SnowLuma WebUI 里自己改密码 (改了下次启动 Bot 会被 Desktop 覆盖); 这个限制要在 Desktop 设置页 / SnowLumaPage / UI 某处明示, 避免用户被默默覆盖时疑惑

### 2.13 Bot 启动/停止 完整注入流程 (新增, 重构 §2.3)

**启动流程** (在 `_create_snowluma_process` 内部, 替代 P1 的仅 spawn node.exe 简化版):

49. **Phase A 双进程起动**:
    - 查 `PathFunc.get_qq_path()`, 若无 QQ.exe 返 `FileNotFoundError`, 保留给 UI 提示
    - spawn `QProcess(QQ.exe)` 并记录 `qq_pid = process.processId()`; 该 QProcess 作为 `NapCatProcessModel.process` 的**第一个**进程, 由 manager 持有
    - spawn `QProcess(node.exe index.mjs)`, cwd=snowluma_path; 该 QProcess 作为 `NapCatProcessModel.snowluma_process` 的**第二个**进程 (新增字段)
    - BotCard 状态 = `Starting`
50. **Phase B WebUI ready + login**:
    - `SnowLumaWebUIClient.wait_ready(timeout=30)`, 失败则 emit error + kill 两个 QProcess
    - `SnowLumaWebUIClient.login()` 拿 token, 失败 (401) → render_webui_json 重写 + kill+restart node.exe + 重 ready + 重 login; 仍失败则 emit error + kill
51. **Phase C 注入**:
    - `SnowLumaWebUIClient.load_process(qq_pid)` → 等 15s 拿 `HookProcessInfo`
    - 若 `info.status == "error"` → emit error + kill 两进程 (用户看到 `SnowLuma 注入失败: <error 字段>`)
    - 否则进入 Phase D
52. **Phase D 等用户登录 QQ**:
    - BotCard 状态 = `WaitingForQRScan`
    - 启动 `SnowLumaStatusPoller` (重构后的, 见 §2.14)
    - 用户在 QQ.exe 弹出的 登录窗扫码 / 输密码 (Desktop 不干预这个窗口, 和 NapCat 行为一致)
53. 整个 Phase A→D 的 `_create_snowluma_process` 改为 `async` (或用 `QTimer.singleShot` 让出事件循环), 避免阻塞 UI 线程

**停止流程** (扩展现 `stop_process` SnowLuma 分支):

54. 反向序列:
    - `SnowLumaWebUIClient.unload_process(qq_pid)` (卸载注入; 若 WebUI 已死则静默忽略)
    - `SnowLumaWebUIClient.logout()` (清理 token; 静默忽略失败)
    - `node_process.terminate()` (5s), 不退就 `kill()`
    - `qq_process.terminate()` (5s), 不退就 `kill()` — **D11 决策: stop bot 必 kill QQ.exe**, 理由见 §10.2
    - `SnowLumaStatusPoller` stop
55. `stop_process` 里 SnowLuma 分支的两进程顺序必须是 "node 先, QQ 后", 避免 QQ 被 kill 时 SnowLuma 还在读 named pipe 导致僵尸句柄

### 2.14 SnowLumaStatusPoller 重构 (新增)

`src/core/runtime/snowluma_status_poller.py` 完全重写:

56. 轮询目标改为 `SnowLumaWebUIClient.list_processes()` 每 2s 一次 (P1 现轮询 OneBot WS `get_status`, 废弃)
57. 对自己持有的 `qq_pid` 过滤, 翻译 `HookProcessInfo.status` → Desktop 登录态:
    - `available` / `loading` / `connecting` → `Starting` (继续保持前置态)
    - `loaded` → `WaitingForQRScan`
    - `online` → `LoggedIn`
    - `error` / `disconnected` → `Disconnected`
58. 失败 (WebUI 不响应 / 401): 静默 trace log; 连续 3 次失败 emit `Disconnected` 信号
59. `state_changed` 信号 (qq_id: str, state_name: str) 与现有接口保持兼容; 上层 `ManagerNapCatQQProcess._start_snowluma_status_poller` 的调用点不改
60. P1 的现 `_SnowLumaStatusRunnable` (轮询 OneBot HTTP get_status) **删除**, poller 直接用 `SnowLumaWebUIClient` 单 tick 即完成

### 2.15 Backend 抽象重构 (新增, 与 P1 §3 妥协决策反转)

为避免在 `napcat.py` 上继续堆砌屎山, 本次同步重构进程管理层. P1 §3 / §8 的 "不重写 Backend 抽象" 决策**显式废弃**; 新建独立 driver 体系.

**新建文件**:

69. `src/core/runtime/bot_backend_driver.py`: 抽象基类 `BotBackendDriver(ABC)`, 暴露:
    - `start(config: Config) -> "ProcessHandle"`: 启动 Bot, 返回进程句柄
    - `stop(qq_id: str) -> None`: 停止 Bot, 反向清理
    - `is_running(qq_id: str) -> bool`: 探测当前是否在跑
    - `get_status_poller(qq_id: str) -> "BotStatusPoller | None"`: 取该 Bot 的状态轮询器 (NapCat 路径返回 None)
    - 公共数据类 `ProcessHandle` (统一抽象 NapCat 单进程 + SnowLuma 双进程)
70. `src/core/runtime/napcat_driver.py`: 类 `NapCatDriver(BotBackendDriver)`, 把现 `napcat.py` 中**只属于 NapCat** 的逻辑搬过来:
    - `_create_napcat_process` (现 `napcat.py:1394`)
    - `_get_env_variable` (现 `napcat.py:1370+`)
    - `_write_load_script` (现 `napcat.py:1386`)
    - 进程退出/启动失败回调里的 NapCat 分支
    - 估算 ~1800 行 (基本是 NapCat 既有逻辑搬移, 不改逻辑)
71. `src/core/runtime/snowluma_driver.py`: 类 `SnowLumaDriver(BotBackendDriver)`, 含本次 Tier D-G 的全部 SnowLuma 实现:
    - 持 `SnowLumaWebUIClient` 实例 (Tier D)
    - Phase A→D 启动序列 (Tier E)
    - 反向 stop 序列 (Tier E)
    - 单实例守护 (一期硬限制)
    - 调 `SnowLumaStatusPoller` (Tier F)
    - 调 `render_*_json` (Tier B + 密码管理 Tier G)
    - 估算 ~1100 行
72. `src/core/runtime/bot_process_manager.py`: 类 `BotProcessManager`, **替代** 原 `ManagerNapCatQQProcess`:
    - `__init__` 时创建 `NapCatDriver()` 与 `SnowLumaDriver()` 实例
    - `create_napcat_process(config)` 改名为 `start_bot(config)` (旧名作为方法 alias 保留 1 个版本以便平滑迁移; 22 文件 import 更新)
    - 内部按 `config.bot.backend_type` dispatch 到对应 driver, 不含具体实现
    - 持 `process_changed_signal` / `notification_signal` / `snowluma_login_state_signal` (signal 名字保持, 不改, 见 §10.2 D-I-7)
    - 估算 ~500 行 (从原 1931 行 napcat.py 抽出来的 dispatch + 生命周期)

**新建数据类**:

73. `BotProcessModel` 抽象基类 + `NapCatProcessModel` (持 1 个 QProcess, NapCat 注入式专用) + `SnowLumaProcessModel` (持 `qq_process: QProcess` + `node_process: QProcess` + `webui_client: SnowLumaWebUIClient` + `auth_token: str | None` 等 SnowLuma 专有字段). 各自带类型严格的 dataclass 而非 dict 字段. 决策见 §10.2 D-I-3

**删除**:

74. `src/core/runtime/napcat.py`: **整文件删除** (内容已搬到 `napcat_driver.py` + `bot_process_manager.py`). 22 文件的 `from src.core.runtime.napcat import ManagerNapCatQQProcess` 全部改为 `from src.core.runtime.bot_process_manager import BotProcessManager`. 决策见 §10.2 D-I-5

### 2.16 重构连带改动 (低关注度但不可漏)

75. `src/core/logging/crash_bundle.py:44`: 老正则 `_NAP_CAT_PROCESS_DICT_PATTERN = re.compile(r"(ManagerNapCatQQProcess\[)(\d{5,12})(\])")` 改为 `(?:ManagerNapCatQQProcess|BotProcessManager)\[`, 同时匹配新老类名以便日志脱敏不破 (D-I-8)
76. `creart` 单例注册: `CreateTargetInfo` 的 `identify="ManagerNapCatQQProcess"` 改为 `"BotProcessManager"`, 22 文件的 `it(ManagerNapCatQQProcess)` 改为 `it(BotProcessManager)`
77. UI 层 22 文件的 import 与 `it(...)` 调用全部更新 (主要是机械 rename); 改名清单见 §10.2 D-I-2 引用的 grep 结果
78. 既有测试 `script/test/test_*napcat*.py` 中只测 NapCat driver 的部分 (大约 70%) 内容搬到 `script/test/test_napcat_driver_*.py`; 跨 driver 的 manager 行为留 `script/test/test_bot_process_manager_*.py`; 改名遵循 D-I-6
79. P1 §2.6 加的 `snowluma_login_state_signal` 仍在新 `BotProcessManager` 上, 但发射点从 napcat.py 内部改为 `SnowLumaDriver.start()` 内部 → manager 转发 (signal 名不变, 见 §10.2 D-I-7)
80. `src/core/operation/backend.py:216` 的 `OperationBackend.start_napcat` (远端部署抽象) **一期不动** (D-I-4); P3 重构期一并改名为 `start_bot`
81. `.gitignore` + `script/build_scripts/collection_filters.py` 加 `config/snowluma-session.json` 排除, 防止密码明文进 git / 打包产物 (Tier G + Tier I 联合需求)

### 2.17 单实例限制 + 测试 (既有测试必须保持绿, 不写新测试)

**单实例限制** (多 SnowLuma Bot 同时运行限制, 见 §8 非目标):

61. `SnowLumaDriver.start(config)` 入口检查: 若 driver 自身的进程模型字典里已有 SnowLuma Bot 实例, raise `RuntimeError("一期仅支持 1 个 SnowLuma Bot 同时运行, 请先停止其他 SnowLuma Bot")`. `BotProcessManager.start_bot(config)` 捕获此异常 emit error 信号 + 不启动
62. 状态: 已有 NapCat Bot 同时跑不受影响 (NapCat + SnowLuma 1+1 可同时, 但 SnowLuma + SnowLuma 禁止). 单实例守护放在 `SnowLumaDriver` 内部而非 `BotProcessManager` 总入口, 让 NapCat 多实例不被误伤; 决策见 §10.2 D-I-1

**测试**:

63. `script/test/test_snowluma_config_renderer.py`: 现有 `TestRenderOnebotJson` 4 个用例 **必须更新** 以匹配新 signature (`connect: ConnectConfig` 参数), 逻辑等价; `render_webui_json` 密码默认值改动需补用例
64. `script/test/test_snowluma_process_construction.py`: 用例需要检查 `render_onebot_json` 被传的 `connect` 参数正确 + spawn QQ.exe QProcess 构造正确; 不实际调 WebUI API (mock httpx)
65. `script/test/test_snowluma_installer.py`: 现有用例不破, **新增** `snowluma-session.json` 生成与 `webui.json` 渲染的验证 (install 成功后两文件都要在)
66. `script/test/test_versioning_snowluma.py` / `test_backend_type_model.py`: **不动**
67. **不**新增测试文件 (用户明确选择手动验收 widget 显隐 + 端到端注入流程)
68. 其余 `test_*.py` 套件必须不退化

## 3. 约束 (Constraints)

- **不**改 SnowLuma 项目本身代码 (`example/SnowLuma-main/` 与上游 release 都不动)
- **不**拆分 `BotConfig` 为 `NapCatBotConfig` + `SnowLumaBotConfig` (pydantic union 迁移复杂, 收益不抵成本)
- **持久化 schema 只加不减**: `ConnectConfig` 下各 NetworkBaseConfig 子类加可选字段, 旧 bot.json 反序列化走默认值, 无需改 `_migrate_*`
- **现有 NapCat 行为零回归** (尤其 `_create_napcat_process` / `render_*` 不受影响; NapCat Bot 启动 / 表单显示与 P1 前完全一致)
- **SnowLuma P1 已支持功能的交付物保留** (`SnowLumaInstall` / `ComponentPage > SnowLuma tab` 不动); 但 `SnowLumaStatusPoller` 本次**重写** (从轮询 OneBot `get_status` 改为轮询 WebUI `/api/processes`, 见 §2.14), 这是必要的, 不算回归
- 不弹 "切换 backend_type 时清空 NapCat-only 字段" 的 AskBox (已决: 隐藏但保留持久化值)
- 不暴露 SnowLuma `runtime.json` 的 `webuiPort` 到 Bot 表单 (继续写死 5099); **不暴露 `webui.json` 密码到 Bot 表单** (Desktop 主导, 见 §2.12; 不让 SnowLuma 自治也不让用户直接管理)
- 不在 BotCard 上加新徽标 / 不做 SnowLuma 远端部署 (留 P2). 注: P1 写的 "不重写 Backend 抽象" 约束**本次反转**, 见下文及 §2.15 §10.2 D-I 系列决策
- **保留** `creart` 单例 / `it(PathFunc)` / `QObject + Signal` 模式
- **保留** `bot.runtime_target` (local/remote) 维度; SnowLuma 一期仍只支持 local
- widget 显隐一律 `setVisible(False)`, 不 `deleteLater()` 避免影响 `get_config()` 的持久化读取
- 所有 `apply_backend_type` 调用必须幂等 (同一 backend 多次调用结果一致)
- **一期仅支持 1 个 SnowLuma Bot 同时运行** (webuiPort 5099 硬编码 + SnowLuma 发布包单工作目录); 多 SnowLuma Bot 留 P2
- **Stop SnowLuma Bot 时必 kill QQ.exe** (D11 决策, 与 NapCat 行为对齐; Desktop spawn 的 QQ.exe 归 Desktop 管整个生命周期)
- **用户在 SnowLuma WebUI 里改密码的行为会被 Desktop 单向覆盖** (D2 决策, 下次启动 Bot 时 Desktop 重渲染 `webui.json`); UI 上必须明示此限制
- Desktop 侧 `config/snowluma-session.json` 不得被持久化到 git / 部署产物中 (含密码明文), `.gitignore` 与打包脚本 (`script/build_scripts/collection_filters.py` 等) 需要覆盖
- WebUI API 所有调用走 `httpx` 短连接 + 自动 401 retry; 不保持 `httpx.Client` 单例以与现有代码风格对齐
- **Backend 抽象重构 (Tier I) 是 P1 §3 / §8 妥协决策的显式反转** — P1 当初 "不重写 Backend 抽象" 是因为 SnowLuma 代码量小 (~115 行), 现在 Tier D-G 加 ~1000 行不能再塞; 本次正面拆 driver 层
- **`napcat.py` 整文件删除**, `ManagerNapCatQQProcess` 改名 `BotProcessManager`, 22 文件依赖必须同步更新 import; 不允许保留 alias / shim
- **`OperationBackend.start_napcat` (远端部署抽象, `src/core/operation/backend.py`) 一期不动** (D-I-4 决策); 它是另一层抽象 (local/remote dispatch), 与 `BotProcessManager` 正交; 留 P3 重构期一并改名
- 重构涉及类名/方法名搜索: `ManagerNapCatQQProcess` (22 文件) / `NapCatProcessModel` (类名引用) / `napcat_process_dict` (内部字段) / `create_napcat_process` (公共方法名) / `_create_napcat_process` (私有) / `_create_snowluma_process` (P1 残留) / 上述 `crash_bundle.py` 正则 / `creart` 单例注册元数据
- signal 名字 (`process_changed_signal` / `notification_signal` / `snowluma_login_state_signal` / `state_changed`) **不改**, 接收方代码无感 (D-I-7)
- 测试文件搬动: 既有 `test_*napcat*.py` 中 NapCat-only 内容搬到 `test_napcat_driver_*.py`, 跨 driver 内容搬到 `test_bot_process_manager_*.py`; 文件改名机械, 不引入新逻辑

## 4. 验收标准 (Acceptance Criteria)

### 4.1 代码可执行性

- [ ] `python -c "from src.core.runtime.snowluma_config_renderer import render_onebot_json; import inspect; assert 'connect' in inspect.signature(render_onebot_json).parameters"` 退出码 0
- [ ] `python -c "from src.core.config.config_model import HttpServersConfig, WebsocketServersConfig, WebsocketClientsConfig, HttpClientsConfig; print(HttpServersConfig.model_fields['path'].default)"` 输出 `/`
- [ ] `python -c "from src.core.config.config_model import WebsocketServersConfig; print(WebsocketServersConfig.model_fields['role'].default)"` 输出 `Universal`
- [ ] `python -c "from src.core.runtime.snowluma_webui_client import SnowLumaWebUIClient, SnowLumaWebUIError, HookProcessInfo"` 退出码 0
- [ ] `python main.py` 启动行为零回归 (打开 BotPage, 已有 NapCat Bot 仍能 Start/Stop, UI 打开不崩)

### 4.2 单元测试

- [ ] `pytest script/test/test_snowluma_config_renderer.py -q` 全绿 (新 signature 下)
- [ ] `pytest script/test/test_snowluma_process_construction.py -q` 全绿
- [ ] `pytest script/test/test_snowluma_installer.py -q` 全绿 (含新增 `snowluma-session.json` + `webui.json` 验证)
- [ ] `pytest script/test/test_versioning_snowluma.py script/test/test_backend_type_model.py -q` 全绿 (不动套件)
- [ ] 其余 `test_*` 套件不回归 (尤其 `test_*napcat*` / `test_*config*` / `test_*remote*`)

### 4.3 行为变更 (可代码 review 验证)

**字段显隐 (来自 Tier A)**:
- [ ] `BotConfigWidget` 切换 backend_type 会 emit 信号, `ConnectConfigWidget` / `AdvancedConfigWidget` 能响应
- [ ] `ChooseConfigTypeDialog` 在 NapCat 模式显示 HTTP Server / HTTP SSE / HTTP Client / WS Server / WS Client 5 类; 在 SnowLuma 模式少一项 HTTP SSE
- [ ] `HttpServerConfigDialog`: SnowLuma 模式不可见 `enableCors` / `enableWebsocket` / `debug`, 可见 `path`
- [ ] `HttpClientConfigDialog`: SnowLuma 模式不可见 `debug`, 可见 `timeoutMs`
- [ ] `WebsocketServerConfigDialog`: SnowLuma 模式不可见 `enableForcePushEvent` / `heartInterval` / `debug`, 可见 `path` / `role`
- [ ] `WebsocketClientConfigDialog`: SnowLuma 模式不可见 `heartInterval` / `debug`, 可见 `role` (`reconnectInterval` 保留可见)
- [ ] `AdvancedConfigWidget`: SnowLuma 模式仅可见 `auto_start_card` + `offline_notice_card`; 其余 7 张卡全部不可见
- [ ] 切换 backend_type 双向, `get_config()` 序列化出的 `ConnectConfig` / `AdvancedConfig` 的 NapCat-only 与 SnowLuma-only 字段值互不丢失

**Renderer 真消费 (来自 Tier B)**:
- [ ] `_create_snowluma_process` 启动前 `onebot_<QQID>.json` 内容反映 `config.connect` 的实际内容 (不再是硬编码 3000/3001/空 token)
- [ ] `render_onebot_json` 字段映射: NapCat `enable/messagePostFormat/token/reconnectInterval` → SnowLuma `enabled/messageFormat/accessToken/reconnectIntervalMs`; NapCat-only 字段 (`debug`/`enableCors`/`enableWebsocket`/`enableForcePushEvent`/`heartInterval`) 静默丢弃
- [ ] `reconnectIntervalMs <1000` 被 renderer clamp 到 1000 + `logger.warning`

**WebUI 客户端化 (来自 Tier D)**:
- [ ] `SnowLumaWebUIClient` 类提供 `wait_ready` / `login` / `logout` / `list_processes` / `load_process` / `unload_process` / `get_auth_state` / `change_password` 8 个方法
- [ ] `login` 失败后续 API 调用收到 401 时, 客户端自动 retry 一次 (重新 login + 重试)
- [ ] 任何 API 调用 timeout 默认 5s, `wait_ready` 30s 30 轮, `load_process` 15s

**密码管理 (来自 Tier G)**:
- [ ] `SnowLumaInstall.execute()` 成功后 `config/snowluma-session.json` 存在且包含 `password` / `created_at` / `last_rendered_at` 字段
- [ ] 同时 `<snowluma_path>/config/webui.json` 包含 Desktop 设置的密码 hash + salt + `mustChangePassword: false`
- [ ] 重复运行 `SnowLumaInstall` 不会改写已有密码 (sticky)

**注入流程 (来自 Tier E)**:
- [ ] `_create_snowluma_process` 内部完成 Phase A→D 完整序列, 并在 Phase B/C 失败时 emit error + kill 两个 QProcess
- [ ] `NapCatProcessModel` 持有 `process` (QQ.exe) + `snowluma_process` (node.exe) 两个 QProcess
- [ ] `stop_process` SnowLuma 分支顺序: unload → logout → kill node → kill QQ
- [ ] BotCard 状态在 SnowLuma 启动期能展示 `Starting → WaitingForQRScan → LoggedIn` (而非直接跳 `Disconnected`)

**状态轮询 (来自 Tier F)**:
- [ ] `SnowLumaStatusPoller` 重写后只调 `SnowLumaWebUIClient.list_processes()`, 不再调 OneBot HTTP `get_status`
- [ ] 7 档状态翻译表 (available/loading/connecting/loaded/online/error/disconnected → Starting/WaitingForQRScan/LoggedIn/Disconnected) 完整覆盖
- [ ] 连续 3 次 WebUI 调用失败 emit `Disconnected` 信号

**单实例限制**:
- [ ] 已有 1 个 SnowLuma Bot 在跑时, 启动第二个 SnowLuma Bot 触发 emit error + 不启动
- [ ] 1 个 SnowLuma + 1 个 NapCat 同时跑不受限制 (不同后端不冲突)

**Backend 抽象重构 (来自 Tier I)**:
- [ ] `python -c "from src.core.runtime.bot_backend_driver import BotBackendDriver; from src.core.runtime.napcat_driver import NapCatDriver; from src.core.runtime.snowluma_driver import SnowLumaDriver; from src.core.runtime.bot_process_manager import BotProcessManager"` 退出码 0
- [ ] `python -c "import src.core.runtime.napcat"` **报错** (ImportError, 旧文件已删)
- [ ] `Get-ChildItem src -Recurse -Include '*.py' | Select-String -Pattern 'ManagerNapCatQQProcess'` 只能命中 1 处: `src/core/logging/crash_bundle.py:44` (脱敏兼容正则)
- [ ] `Get-ChildItem src -Recurse -Include '*.py' | Select-String -Pattern 'from src.core.runtime.napcat import'` 零命中 (22 个导入全部迁到 bot_process_manager)
- [ ] `napcat_driver.py` 与 `snowluma_driver.py` 都正确实现 `BotBackendDriver` 抽象方法 (`start` / `stop` / `is_running` / `get_status_poller`)
- [ ] `BotProcessManager` 不含具体后端实现 (`grep -r 'subprocess\|QProcess\|psutil' src/core/runtime/bot_process_manager.py` 零命中, 这些全在 driver 层)
- [ ] `creart` 单例: `python -c "from creart import it; from src.core.runtime.bot_process_manager import BotProcessManager; assert it(BotProcessManager) is not None"` 退出码 0

### 4.4 产品验收 (Product Acceptance)

**端到端注入** (本次最关键的产品验收点):
- [ ] 创建 SnowLuma Bot, 点启动 → QQ.exe 自动弹出 + node.exe 起 + Desktop 自动登录 WebUI 拿 token + 自动调 `/api/processes/<pid>/load` 注入 + BotCard 进入 `WaitingForQRScan`; 用户在 QQ.exe 扫码登录 → BotCard 自动切换到 `LoggedIn`. **全程用户不需要打开浏览器**
- [ ] 点停止 → Desktop 调 unload + logout + kill node + kill QQ → QQ.exe 退出 + BotCard 回 `Disconnected`
- [ ] 同 Bot 启动→停止→启动 (重复 3 轮) 行为稳定, 不卡死

**字段显隐 + Renderer**:
- [ ] 创建 SnowLuma Bot, 在 ConnectConfigWidget 加一个 httpServers (port=4000, token="ACCESS-TEST", path="/api"), 启动后 `<snowluma_path>/config/onebot_<qqid>.json` 的 `networks.httpServers[0]` 应有 `port:4000 + accessToken:"ACCESS-TEST" + path:"/api" + messageFormat:"array" + enabled:true`, 且不包含 `debug`/`enableCors`/`enableWebsocket` 字段
- [ ] 启动后用 `Authorization: Bearer ACCESS-TEST` 头调 `POST http://127.0.0.1:4000/api/get_status` (注意 path=/api), 返回 200 + `{"online": true}` (端到端 OneBot 验证)
- [ ] 同 Bot 切 backend_type 到 NAPCAT, 表单不再显示 path 卡片, debug/enableCors 卡片出现; 切回 SNOWLUMA, path 卡片重现且值仍为 "/api"
- [ ] 创建 NapCat Bot, AdvancedConfigWidget 显示全部 9 张卡; 同 Bot 切到 SNOWLUMA, 仅显示 2 张 (auto_start / offline_notice); 切回 NAPCAT, 全部 9 张卡重新可见
- [ ] 切 backend_type 后 NapCat 独有字段与 SnowLuma 独有字段的持久化值零丢失 (再切回原后端, 字段值与切换前一致)

**WebUI 密码 + 错误恢复**:
- [ ] 用户在浏览器登录 SnowLuma WebUI 改密码后, 关 Desktop, 改密度过 30 分钟登录冷却, 再打开 Desktop 启动 Bot, Desktop 自动检测密码不对 + 重渲染 webui.json + 重启 node.exe + 重试登录, 最终成功启动 Bot (D2 决策: Desktop 单向覆盖用户改密)
- [ ] 系统里没有 QQ.exe 路径 (注册表里 `HKLM\...\QQ` 不存在) 时启动 SnowLuma Bot, BotCard emit error + 提示 `"未检测到 QQ.exe 安装路径, 请先安装 QQ"`

**单实例**:
- [ ] 启动 1 个 SnowLuma Bot 后, 再启动第二个 SnowLuma Bot, Desktop 弹错误提示 (一期单实例限制) + 第二个 Bot 不启动
- [ ] 1 个 NapCat + 1 个 SnowLuma 同时启动, 两者都能正常运行

## 5. 手工抽查 (Manual Spot Checks)

### 5.1 字段显隐
- 打开 Desktop → 新建 Bot → 默认 backend_type=NAPCAT, 完整表单可见
- 切换 backend_type 到 SNOWLUMA, 观察:
  - BotConfigWidget 7 张卡片全部保留
  - ConnectConfigWidget 的 HTTP SSE 标签/入口消失, 已有 HTTP SSE 卡片实例 (若有) 不可见
  - AdvancedConfigWidget 只剩 2 张卡 (auto_start + offline_notice)
- 在 SNOWLUMA 模式下点 "添加新连接" → `ChooseConfigTypeDialog` 4 类选项 (无 HTTP SSE)
- 选 HTTP Server 打开 `HttpServerConfigDialog` → 不可见 enableCors / enableWebsocket / debug, 可见 path (默认 "/")
- 选 WS Server → 不可见 enableForcePushEvent / heartInterval / debug, 可见 path / role (默认 "/" 与 "Universal")
- 选 WS Client → 不可见 heartInterval / debug, 可见 role; reconnectInterval 保留 (双后端同义)
- 保存 Bot, 查看 `runtime/config/bot.json`, SnowLuma-only 字段 (path/role/timeoutMs) 已持久化
- 切 backend_type 回 NAPCAT → 重查 bot.json, NapCat-only 字段持久化保留

### 5.2 端到端注入 (本次最关键的手工验收)
- 启动 Desktop, 新建 SnowLuma Bot (随便填 QQID, 比如 `2477817352`)
- 在 ConnectConfigWidget 加一个 httpServers: port=4000 / token="ACCESS-TEST" / path="/api"
- 点 Bot 启动按钮, 观察:
  - QQ.exe 自动从 Windows 注册表的 QQ 路径启动, 弹出 QQ 登录窗
  - SnowLuma node.exe 起来, 看 task manager 应有 2 个新进程 (QQ.exe + node.exe)
  - **不需要打开浏览器**, BotCard 状态先 Starting → 之后 WaitingForQRScan
  - Desktop 内部 log 应有 `SnowLuma WebUI ready` + `WebUI login OK` + `inject loaded for pid=<qq_pid>` 三条
- 在 QQ.exe 扫码 / 输密码登录 QQ, 观察:
  - BotCard 自动从 WaitingForQRScan 切到 LoggedIn (绿)
  - log 应显示 `bot online (uin=<登录的 uin>)`
- 用 curl 验证 OneBot 接口活: `curl -H "Authorization: Bearer ACCESS-TEST" -X POST http://127.0.0.1:4000/api/get_status` 应返回 `{"status":"ok","retcode":0,"data":{"online":true,...}}`
- 点 Bot 停止按钮, 观察:
  - Desktop log 顺序: `unload OK` → `logout OK` → `node terminated` → `qq terminated`
  - QQ.exe 进程消失, node.exe 进程消失
  - BotCard 回 Disconnected

### 5.3 错误恢复 + 单实例
- 启动 Bot 后**不**登录 QQ, 直接关 Desktop, 然后再开 Desktop, 启动同一 Bot:
  - Desktop 应能检测到旧的 QQ.exe + node.exe 仍在 (P2 范围, 一期可能直接 emit error 提示用户先 kill, OK)
- 用浏览器打开 `http://127.0.0.1:5099/`, 用 SnowLuma WebUI 内置 "改密码" 改成新密码, 关浏览器, 关 Desktop
- 重新开 Desktop 启动 Bot, Desktop 应自动检测密码失效 + 重渲染 webui.json + 重启 node + 重试 login (D2 决策), 最终 Bot 启动成功
- 启动第 2 个 SnowLuma Bot, Desktop 应弹 "一期仅支持 1 个 SnowLuma Bot 同时运行" 错误, 第 2 个 Bot 不启动
- 同时启动 1 个 NapCat Bot + 1 个 SnowLuma Bot, 两个都能正常工作 (NapCat 注入到自己的 QQ.exe, SnowLuma 注入到 Desktop spawn 的另一个 QQ.exe)

### 5.4 Backend 重构验收
- 启动 Desktop 后立即在项目根跑: `Get-ChildItem src -Recurse -Include '*.py' | Select-String -Pattern 'from src.core.runtime.napcat import'` 应 零命中 (22 个 import 全迁)
- 运行 `Test-Path d:\NapCat-Project\NapCatQQ-Desktop-V1\src\core\runtime\napcat.py` 返回 `False` (napcat.py 已删)
- 黑屏启动 Desktop, 主窗口能打开, 打开 BotPage 有已有 NapCat Bot 出现在列表里, 点 Start 能起 (NapCat 路径零回归)
- 检查日志 `runtime/log/`, 同一 Bot 的 PID 能被 `crash_bundle` 脱敏为 `[REDACTED]` (包含新老两个类名 都脱敏成功)
- 编辑器里搜 `ManagerNapCatQQProcess`, 全项目应只剩 1 处命中 (`crash_bundle.py:44` 兼容正则)

## 6. 完成语言策略 (Completion Language Policy)

只有当 §4.1+§4.2+§4.3 全部勾选完成、§5 全部手工验收通过、 phase_cleanup 报告写入 outputs 后, 才允许使用 "已完成 / 已交付 / 验收通过" 等完成性措辞. 在此之前对外用阶段性措辞 (例如 "P2 完成, 等待跑完 §4.3 UI 手工验收").

## 7. 交付真相契约 (Delivery Truth Contract)

- **可声明完成的依据**: `pytest script/test/test_snowluma_config_renderer.py script/test/test_snowluma_process_construction.py script/test/test_snowluma_installer.py script/test/test_versioning_snowluma.py script/test/test_backend_type_model.py -q` 退出码 0 **且** 其余 `test_*.py` 套件不退化 **且** §5.1+§5.2+§5.3 手工抽查全部由用户签字确认
- **不可代替的人工验收点**: 
  - §5.1 字段显隐 (UI 行为)
  - §5.2 端到端注入 (启动 Bot → QQ 登录 → curl 验证 OneBot 端口活 → 停止 Bot, 完整闭环; **本次最关键, 不通过则整个需求未完成**)
  - §5.3 密码覆盖 + 单实例限制 (错误恢复路径)
  - §5.4 Backend 重构验收 (napcat.py 已删 / 22 个 import 迁移 / 脱敏兼容 / 原 NapCat 路径零回归)
- **未达成必须明示**: 任一既有测试退化 / §5 任一 spot check 未做 / §5.2 端到端注入失败 / §5.4 重构验收未过, 均不得用 "完成" 措辞

## 8. 非目标 (Non-Goals)

- 拆分 `BotConfig` 为 `NapCatBotConfig` + `SnowLumaBotConfig` (pydantic union 迁移复杂)
- 对 `BackendType` 枚举做扩展 (P1 已定, 只含 NAPCAT / SNOWLUMA)
- 动 P1 已交付的 `SnowLumaInstall` / `SnowLumaPage` / `ComponentPage > SnowLuma tab` (注: `SnowLumaStatusPoller` 本次会重写, 不在保留范围)
- 暴露 SnowLuma `runtime.json.webuiPort` / `webui.json.passwordHash` / `webui.json.passwordSalt` 到 Bot 表单 (Desktop 主导, 用户无感)
- **多 SnowLuma Bot 同时运行** (一期仅支持 1 个, 因为 webuiPort 5099 hardcode + SnowLuma 发布包单工作目录设计; 留 P2)
- **保留用户在 SnowLuma WebUI 里改的密码** (D2 决策: Desktop 单向覆盖, 不做双向同步)
- **复用已存在的 QQ.exe 进程注入** (D12 决策: Desktop 永远 spawn 自己的 QQ.exe, 不动用户已有的; 复用机制留 P2)
- **Stop bot 时保留 QQ.exe 给用户继续用** (D11 决策: stop bot 必 kill 整个 QQ.exe; 保留行为留 P2)
- Desktop 重启后恢复已注入的 SnowLuma Bot session (graceful recovery, 留 P2; 一期 Desktop 重启即视为旧 session 失效, 用户需先停旧 Bot 再启新)
- SnowLuma 内部日志流 (`/api/logs/stream` SSE) 接入 BotCard 日志面板 (P3 backlog; 一期日志面板仍读 QProcess stdout)
- SnowLuma 自动改密 (`POST /api/auth/change-password`) — 因为 D2 选择 Desktop 主导密码, 不需要走改密 API
- SnowLuma 远端 (SSH) 部署 (留 P2)
- 在 BotCard 上加新徽标或状态显示 (P1 已处理)
- 重构 `OperationBackend` (远端部署抽象, `src/core/operation/backend.py:216` 的 `start_napcat` 一期不动, 留 P3) — 注: P1 原定的 "重写 Backend 抽象" 本次部分达成 (BotProcessManager / driver 层重构, 见 §2.15), 未达成的仅是 `OperationBackend` 远端部署那一层
- 新增测试文件 (用户明确选择手动验收 §5.1/§5.2/§5.3)
- 跨 backend 配置迁移 (比如把 NapCat Bot 一键克隆为 SnowLuma Bot), 用户需要手动新建

## 9. 自治模式 (Autonomy Mode)

`interactive_governed`. XL 计划阶段的 wave/batch 边界处不再向用户提问; 只有出现以下情况回到用户:
- 需要破坏持久化 schema 向后兼容 (加必填字段 / 删字段)
- 需要新建第二个交付目标 (比如要写新测试文件、要拆 BotConfig 子类)
- SnowLuma 上游字段命名/语义与本文档 §2.2 映射表不一致
- `_create_snowluma_process` 现有签名或调用路径需要改动超出 §2.3 范围

## 10. 推断假设 (Inferred Assumptions)

### 10.1 NapCat vs SnowLuma 字段对照 (决定显隐)

| 域            | 字段                                            | NapCat                                          | SnowLuma                                           | 本需求下 SNOWLUMA 模式显隐 |
| ------------- | ----------------------------------------------- | ----------------------------------------------- | -------------------------------------------------- | -------------------------- |
| BotConfig     | name                                            | ✅                                               | ✅                                                  | 可见                       |
| BotConfig     | QQID                                            | ✅ (启动参数)                                    | ⚠️ (仅 uin 文件名)                                  | 可见                       |
| BotConfig     | musicSignUrl                                    | ✅                                               | ✅                                                  | 可见                       |
| BotConfig     | autoRestartSchedule                             | ✅                                               | ✅                                                  | 可见                       |
| BotConfig     | offlineAutoRestart                              | ✅                                               | ⚠️ (Poller 暂无离线信号)                            | 可见                       |
| BotConfig     | runtime_target                                  | ✅                                               | 一期仅 local                                       | 可见                       |
| BotConfig     | backend_type                                    | ✅                                               | ✅                                                  | 可见                       |
| NetworkBase   | enable                                          | ✅                                               | ✅ (映射 enabled)                                   | 可见                       |
| NetworkBase   | name                                            | ✅                                               | ✅                                                  | 可见                       |
| NetworkBase   | messagePostFormat                               | ✅                                               | ✅ (映射 messageFormat)                             | 可见                       |
| NetworkBase   | token                                           | ✅                                               | ✅ (映射 accessToken)                               | 可见                       |
| NetworkBase   | debug                                           | ✅                                               | ❌                                                  | **隐藏**                   |
| HttpServer    | host                                            | ✅                                               | ✅                                                  | 可见                       |
| HttpServer    | port                                            | ✅                                               | ✅                                                  | 可见                       |
| HttpServer    | enableCors                                      | ✅                                               | ❌                                                  | **隐藏**                   |
| HttpServer    | enableWebsocket                                 | ✅                                               | ❌                                                  | **隐藏**                   |
| HttpServer    | path                                            | ❌ (NapCat 固定 `/`)                             | ✅ (前缀挂载+action 截取)                           | **仅 SnowLuma 可见**       |
| HttpSseServer | (整类)                                          | ✅                                               | ❌                                                  | **整类隐藏**               |
| HttpClient    | url                                             | ✅                                               | ✅                                                  | 可见                       |
| HttpClient    | reportSelfMessage                               | ✅                                               | ✅                                                  | 可见                       |
| HttpClient    | timeoutMs                                       | ❌ (NapCat POST 超时非配置, 走 RequestUtil 默认) | ✅ (默认 5000ms)                                    | **仅 SnowLuma 可见**       |
| WsServer      | host/port                                       | ✅                                               | ✅                                                  | 可见                       |
| WsServer      | reportSelfMessage                               | ✅                                               | ✅                                                  | 可见                       |
| WsServer      | enableForcePushEvent                            | ✅                                               | ❌                                                  | **隐藏**                   |
| WsServer      | heartInterval                                   | ✅                                               | ❌ (内置)                                           | **隐藏**                   |
| WsServer      | path                                            | ❌ (NapCat 固定 `/`)                             | ✅ (exact match, 由 ws 库实现)                      | **仅 SnowLuma 可见**       |
| WsServer      | role                                            | ❌ (NapCat 用 enableWebsocket + `/api` 路径分类) | ✅ (Api/Event/Universal, 未设时按 URL 尾部自动分类) | **仅 SnowLuma 可见**       |
| WsClient      | url                                             | ✅                                               | ✅                                                  | 可见                       |
| WsClient      | reportSelfMessage                               | ✅                                               | ✅                                                  | 可见                       |
| WsClient      | heartInterval                                   | ✅                                               | ❌                                                  | **隐藏**                   |
| WsClient      | reconnectInterval (ms)                          | ✅                                               | ✅ (映射 reconnectIntervalMs, **clamp ≥ 1000ms**)   | 可见                       |
| WsClient      | role                                            | ❌ (NapCat 无此概念)                             | ✅ (Api/Event/Universal)                            | **仅 SnowLuma 可见**       |
| plugins       | (整类)                                          | ✅                                               | ❌                                                  | **整类隐藏**               |
| Advanced      | autoStart                                       | Desktop                                         | Desktop                                            | 可见                       |
| Advanced      | offlineNotice                                   | Desktop                                         | Desktop                                            | 可见                       |
| Advanced      | parseMultMsg                                    | ✅                                               | ❌                                                  | **隐藏**                   |
| Advanced      | packetServer/packetBackend                      | ✅                                               | ❌                                                  | **隐藏**                   |
| Advanced      | enableLocalFile2Url                             | ✅                                               | ❌                                                  | **隐藏**                   |
| Advanced      | fileLog/consoleLog/fileLogLevel/consoleLogLevel | ✅                                               | ❌                                                  | **隐藏**                   |
| Advanced      | o3HookMode                                      | ✅                                               | ❌                                                  | **隐藏**                   |
| Advanced      | bypass.*                                        | ✅                                               | ❌                                                  | **隐藏**                   |

### 10.2 其他假设

- SnowLuma 上游 `wsClients.reconnectIntervalMs` 与 NapCat `WebsocketClientsConfig.reconnectInterval` 单位都是毫秒, 沿用 NapCat 字段名, 渲染时做字段名映射即可; **SnowLuma 上游强制 `Math.max(1000, value)` 下限** (`example/SnowLuma-main/packages/core/src/onebot/config.ts:299`), 因此 renderer 层也要对 <1000ms 的值做 clamp + `logger.warning`, 保证用户感知与上游行为一致 (见交付项 #12a)
- SnowLuma 上游 `path` 语义在两类 server 间**不同**:
  - **HTTP server**: `path` 是**前缀挂载点**, 后续请求路径在此基础上截取为 OneBot action name. 例如 `path=/api` + 请求 `/api/send_msg` 走 `send_msg` action; 请求 `/api` 或 `/api/` 走 GET status check; 不以 `/api/` 开头则返回 404 (`example/SnowLuma-main/packages/core/src/onebot/network/http-server-adapter.ts:99-112`)
  - **WS server**: `path` 是 **exact match**, 由 `ws` 库 `WebSocketServer({ path })` 直接负责; 客户端连接路径必须与 path 完全一致否则握手失败 (`example/SnowLuma-main/packages/core/src/onebot/network/ws-server-adapter.ts:109-113`)
  - 两者都通过 `utils.ts:46 normalizePath` 做 trailing `/` 规范化, 但匹配语义不同
- SnowLuma WS server `role` 字段在**未显式配置时**按客户端连接 URL 尾部**自动分类**: `/api` → `Api`, `/event` → `Event`, 其他 → `Universal` (`example/SnowLuma-main/packages/core/src/onebot/network/ws-server-adapter.ts:178-183`). 这与 NapCat HTTP server 启用 `enableWebsocket` 混合模式下用 `/api` 路径区分 API/Event 连接的行为**语义相近但机制不同** (NapCat 共用 HTTP 端口, SnowLuma 独立 WS 端口)
- NapCat OneBot `HttpServer.enableCors` 在 NapCat 实现里**是 dead config**: `example/NapCatQQ-main/packages/napcat-onebot/network/http-server.ts:78` 无条件 `this.app.use(cors())`, 完全忽略 `this.config.enableCors`. Desktop pydantic 保留该字段与 UI 卡只是为了向 NapCat 配置文件双向兼容 (schema 里有), 本需求不改该行为
- NapCat HTTP server 不支持 server-level path 前缀配置 (固定挂在 `/`, 按 express route 匹配具体 action), 这与 SnowLuma HTTP server 的 `path` 概念不对等; SnowLuma 独有字段在 NapCat 模式下隐藏是合理的
- NapCat OneBotConfig 顶层还有 `imageDownloadProxy: string` 与 `timeout: { baseTimeout, uploadSpeedKBps, downloadSpeedKBps, maxTimeout }` 两组字段 (见 `example/NapCatQQ-main/packages/napcat-onebot/config/config.ts:81-95`), Desktop `AdvancedConfig` 完全未暴露; SnowLuma 也完全没有对应概念. **超出本需求范围, 记录为遗留限制**
- **Desktop pydantic 与 NapCat 实际 OneBot schema 在部分字段的默认值上已有历史偏差** (非本需求引入):
  - `HttpServer.enableCors`: NapCat schema 默认 `true`, Desktop pydantic 默认 `false`
  - `HttpServer.host`: NapCat schema 默认 `127.0.0.1`, Desktop pydantic required (无默认)
  - `WebsocketServer.enableForcePushEvent`: NapCat schema 默认 `true`, Desktop pydantic 默认 `false`
  - `WebsocketClient.reconnectInterval`: NapCat schema 默认 `5000` ms, Desktop pydantic 默认 `30000` ms
  - 本需求不修订这些历史偏差 (属 NapCat 适配的遗留事项), 但在与 SnowLuma 的 1000ms 下限 clamp 交互时应注意 NapCat 原生 5000ms 默认值已高于下限
- SnowLuma `webui.json` 的 `scrypt` 参数 (`N=16384, r=8, p=1, keylen=64`, salt 16 字节) 与 Desktop `render_webui_json` 使用的参数 (`@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/runtime/snowluma_config_renderer.py:33-41`) **完全一致**, 双方兼容
- SnowLuma `runtime.json` 只包含 `webuiPort` 一个字段 (见 `example/SnowLuma-main/packages/core/src/common/runtime.ts:4-6`), Desktop `render_runtime_json` 已对齐
- SnowLuma HTTP Client `token` 用于 `X-Signature: sha1=<hmac-hex>` 签名 (`example/SnowLuma-main/packages/core/src/onebot/network/http-post-adapter.ts:88-89`); NapCat HTTP Client 行为完全相同 (`example/NapCatQQ-main/packages/napcat-onebot/network/http-client.ts:22-26`), OneBot v11 标准
- SnowLuma 上游 `messageFormat` 只认 `"array"` / `"string"`, 与 NapCat `messagePostFormat` 取值域一致
- SnowLuma 上游在 `onebot_<uin>.json.networks` 全空时会走 `makeDefaultOneBotConfig()` 兜底, 但为避免用户感到困惑, Desktop 在 renderer 层也兜底一次 (§2.2-12)
- widget 显隐通过 `setVisible(False)` 实现, `QGridLayout` / `FlowLayout` / `ExpandLayout` 都能正确处理 (PySide6 行为)
- 新建 Bot 的 `ChooseConfigTypeDialog` 是在 `ConnectConfigWidget` 触发, `ConnectConfigWidget` 必然持有当前 backend (从父容器传入), 不需要额外访问 `BotConfig`
- `render_onebot_json` signature 变化的调用者仅 `_create_snowluma_process` + `test_snowluma_config_renderer.py` 的 4-5 个用例, 改动可控
- 迁移老配置文件 (没有 `path`/`role`/`timeoutMs` 字段) 时 pydantic 走默认值, 不触发 `_migrate_legacy_*` 代码路径

**Tier I Backend 抽象重构决策记录**:

- **D-I-1 (抽象层)**: ABC 抽象类 `BotBackendDriver(ABC)`, 与 `OperationBackend(ABC)` 同风格. Protocol-based 不足 (未选 B), 函数级分发不明确 (未选 C). 提供明确接口 会为 P3 第三后端 (如 Lagrange) 接入预留扩展点
- **D-I-2 (Manager 改名)**: `ManagerNapCatQQProcess` → `BotProcessManager`, 22 文件 import 机械重命名. alias 方案 (未选 B) 并不仅太足 且为了摆脱 屁股 必须一次到位
- **D-I-3 (ProcessModel)**: 拆为 `NapCatProcessModel` + `SnowLumaProcessModel` (未选 A 简单拼字段). 原因: NapCat 有 `psutil.Process(...).children(recursive=True)` 等专有逻辑, SnowLuma 有 `qq_process` + `node_process` + `webui_client` + `auth_token` 专有字段; 拆后类型严谨
- **D-I-4 (OperationBackend)**: 一期不动 (`src/core/operation/backend.py:216` 的 `start_napcat` 是远端部署抽象, 与 `BotProcessManager` 是两层不同抽象, P3 一起重构为 `start_bot`)
- **D-I-5 (文件组织)**: 直接删 `src/core/runtime/napcat.py`, 不留 thin shim. 原因: 保留 alias / shim 只会让用户继续从旧路径 import, 垃圾代码难清理
- **D-I-6 (测试组织)**: `test_*napcat*.py` 里 NapCat-only 部分 (~70%) 搬到 `test_napcat_driver_*.py`, 跨 driver 部分搬到 `test_bot_process_manager_*.py`. 机械損动 不引入新逻辑
- **D-I-7 (Signal 名)**: `process_changed_signal` / `notification_signal` / `snowluma_login_state_signal` / `state_changed` 等 signal 名不改, 接收方代码无感. 发射点从 napcat.py 内部改为 `SnowLumaDriver.start()` / `NapCatDriver.start()` 内部 → `BotProcessManager` 转发
- **D-I-8 (crash_bundle 正则)**: `_NAP_CAT_PROCESS_DICT_PATTERN` 同时匹配新老两个类名 (`(?:ManagerNapCatQQProcess|BotProcessManager)\[`); 脱敏不破. 等 P3 迁移期超过 6 个月之后才可删旧名匹配
- **重构风险谁能撑错** (需要在 xl_plan 阶段 有 fallback 计划): 22 文件重命名 漏一个 运行时崩; creart 单例 key 变化可能与其他初始化时机货压; QProcess 生命周期跳动时可能出现泄露. 需要 git 清洁状态 + 一次 commit 一个 Tier

### 10.3 SnowLuma WebUI API 全集与注入流程 (新增, 作为 Tier D-G 实施依据)

**WebUI Server 启动**: SnowLuma `index.ts:11-33` 中 `main()` 创建 `HookManager + BridgeManager + OneBotManager`, 然后 `initWebUI(port, oneBotManager, hookManager)` 起 Hono server 监听 `runtime.json.webuiPort` (默认 5099). 注入能力依赖 `hookManager` 实例, 所以 WebUI server 必须能拿到它.

**API 全集** (来自 `webui/server.ts:90-440`, **斜体表示 Desktop 一期不调**):

| 端点                                 | 方法       | 功能                         | Desktop 一期使用             |
| ------------------------------------ | ---------- | ---------------------------- | ---------------------------- |
| `/api/login`                         | POST       | password → Bearer token      | **必用**                     |
| `/api/logout`                        | POST       | 清理 token                   | **必用** (stop bot 时)       |
| `/api/auth/state`                    | GET        | 取 mustChangePassword + 状态 | 探测密码状态用               |
| _`/api/auth/check-strength`_         | POST       | 密码强度                     | 不用                         |
| _`/api/auth/change-password`_        | POST       | 改密                         | 不用 (D2 Desktop 主导)       |
| `/api/status`                        | GET        | server 存活探测              | **必用** (wait_ready)        |
| _`/api/system`_                      | GET        | 系统信息                     | 不用                         |
| _`/api/qq-list`_                     | GET        | data/<uin>/ 历史             | 不用                         |
| _`/api/logs`_ / _`/api/logs/stream`_ | GET / SSE  | 日志                         | 不用 (P3)                    |
| `/api/processes`                     | GET        | 列 QQ.exe + status           | **必用** (状态轮询)          |
| `/api/processes/<pid>/load`          | POST       | 注入                         | **必用** (启动核心)          |
| `/api/processes/<pid>/unload`        | POST       | 卸载注入                     | **必用** (停止核心)          |
| _`/api/processes/<pid>/refresh`_     | POST       | 重连 named pipe              | 不用 (一期错误恢复直接重启)  |
| _`/api/config/<uin>`_                | GET / POST | 读写 onebot_<uin>.json       | 不用 (Desktop 直接落盘 §2.2) |
| _`/avatar/<uin>`_                    | GET        | QQ 头像代理                  | 不用 (一期 BotCard 不取头像) |

**Desktop 调用顺序** (Phase B-D 实施细节, 配合 §2.13):

```
启动:
  POST /api/login {password}           → 拿 Bearer token
  POST /api/processes/<qq_pid>/load    → 注入, 等响应 status
  GET  /api/processes (轮询每 2s)       → 找 qq_pid 的 status, 翻译为 Desktop 状态

停止:
  POST /api/processes/<qq_pid>/unload  → 卸载
  POST /api/logout                      → 清理 token
```

**HookProcessStatus 7 档** (来自 `hook/hook-manager.ts:12-19`):
- `available`: 找到 QQ.exe, 未注入
- `loading`: 注入中
- `connecting`: 注入成功, 连 named pipe 中
- `loaded`: 已注入 + 连上 pipe, **未登录** (用户该扫码了)
- `online`: QQ 已登录, bot 完全可用
- `error`: 注入或连接失败 (`error` 字段含具体原因)
- `disconnected`: 之前注入过, 现在 named pipe 掉了 (QQ.exe 退出 / hook 模块被卸载)

**认证语义** (来自 `webui/server.ts:93-115`): 所有 `/api/*` (除 `/api/login`) 必须带 `Authorization: Bearer <token>` 或 `?token=<token>` query; 401 表示 token 过期/无效, Desktop 客户端策略是自动重 login + retry 一次, 仍失败则上报 error.

**密码语义** (来自 `webui/auth.ts:38-44`):
- 强度规则: ≥10 位 + 含小写 + 含大写 + 含特殊符号 (`/[^A-Za-z0-9\s]/`) + 不含空格
- 错误密码 5 次锁定 30 分钟 (`webui/server.ts:122-125`)
- scrypt 参数: `N=16384, r=8, p=1, keylen=64`, salt 16 字节 (与 Desktop `render_webui_json` 完全一致, 见 §10.2)

**注入实现** (来自 `hook/injector.ts:108-119`):
- `injectHookProcess(pid)` 通过 native addon `loadModuleManual(pid, dllPath)` 把 `snowluma-win32-x64.dll` 远程线程注入到 QQ.exe
- DLL 路径通过 `nativeSearchDirs()` 自动探测, SnowLuma 发布包结构下应在 `<snowluma_path>/native/snowluma-win32-x64.dll`
- Desktop spawn 的 QQ.exe 必须是 win32-x64, 与 SnowLuma DLL 架构一致 (一期假设, 与 NapCat 一致)

## 11. 引用 (References)

- 上游 SnowLuma OneBot 配置源码: `example/SnowLuma-main/packages/core/src/onebot/config.ts` + `types.ts`
- 上游 SnowLuma OneBot 网络适配器: `example/SnowLuma-main/packages/core/src/onebot/network/http-server-adapter.ts` / `http-post-adapter.ts` / `ws-server-adapter.ts` / `ws-client-adapter.ts` / `utils.ts`
- 上游 SnowLuma 运行时 / WebUI 配置: `example/SnowLuma-main/packages/core/src/common/runtime.ts` + `webui/auth.ts`
- 上游 SnowLuma WebUI HTTP server (本次 Tier D 客户端 1:1 参照): `example/SnowLuma-main/packages/core/src/webui/server.ts` + `port.ts`
- 上游 SnowLuma Hook 注入 (本次 Tier E 启动器序列依据): `example/SnowLuma-main/packages/core/src/hook/hook-manager.ts` + `injector.ts`
- 上游 SnowLuma 主入口 (本次 Tier D Desktop 客户端调用顺序参照): `example/SnowLuma-main/packages/core/src/index.ts`
- Backend 抽象重构 (Tier I) 原上下文: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/runtime/napcat.py` (本次要删, 拆为 napcat_driver/snowluma_driver/bot_process_manager) + `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/operation/backend.py:205-230` (远端部署抽象, 一期不动) + `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/logging/crash_bundle.py:40-50` (脱敏正则必同步)
- 上游 NapCat OneBot 配置 schema: `example/NapCatQQ-main/packages/napcat-onebot/config/config.ts`
- 上游 NapCat OneBot 网络适配器: `example/NapCatQQ-main/packages/napcat-onebot/network/http-server.ts` / `http-client.ts` / `websocket-server.ts` / `websocket-client.ts`
- 上游 SnowLuma 真实配置样本: `C:\Users\QIAO\Desktop\SnowLuma-v1.7.5-win-x64\config\*.json`
- P1 冻结需求: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/docs/requirements/2026-05-10-snowluma-backend-adapter.md`
- P1 执行计划: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/docs/plans/2026-05-10-snowluma-backend-adapter-execution-plan.md`
- 现有 BotConfigWidget / ConnectConfigWidget / AdvancedConfigWidget: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/ui/page/bot_page/widget/config.py`
- 现有网络配置对话框: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/ui/page/bot_page/widget/msg_box.py`
- 现有 renderer: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/runtime/snowluma_config_renderer.py`
- 现有进程构造: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/runtime/napcat.py:1422-1475`
- 配置模型: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/config/config_model.py`
- P1 SnowLuma 适配 (本需求的 §2.6 父范围): `@d:/NapCat-Project/NapCatQQ-Desktop-V1/docs/requirements/2026-05-10-snowluma-backend-adapter.md#2.6`

# SnowLuma 后端并列适配 — 需求冻结

- **创建日期**：2026-05-10
- **冻结时间戳**：governed by `vibe` runtime
- **会话主题**：把 NapCatQQ-Desktop-V1 从「单后端 (NapCat 注入式)」扩展到「双后端 (NapCat 注入式 + SnowLuma 独立进程)」, UI 与配置体系上二者平权

---

## 0. 现状对照 (Background)

NapCat 与 SnowLuma 是兄弟项目, 都对外暴露 OneBot v11 WS/HTTP 协议, 但启动模型完全不同:

| 维度             | NapCat (现有)                                          | SnowLuma (本期新增)                                                          |
| ---------------- | ------------------------------------------------------ | ---------------------------------------------------------------------------- |
| 进程模型         | 注入到 QQ.exe (NTQQ 自带 NodeJS)                       | 独立 Node 进程 (发布包自带 `node.exe` v22.22.2)                              |
| 启动入口         | `NapCatWinBootMain.exe` + `NapCatWinBootHook.dll` 注入 | `node.exe ./index.mjs` (cwd = 发布包根目录)                                  |
| 是否需要 QQ 安装 | 必须 (注册表探 `HKLM\...\QQ`)                          | 不需要                                                                       |
| 是否需要管理员   | 需要 (UAC 自抬权)                                      | 不需要                                                                       |
| QQID 来源        | Desktop 启动时作为 launcher CLI 参数传入               | **不接受 CLI 参数**, 启动后由 WebUI 扫码登录得到 uin                         |
| 多账号           | 每 QQID 一个 QProcess                                  | 配置文件名按 `config/onebot_<uin>.json` 区分                                 |
| WebUI 鉴权       | token 拼到 URL                                         | password hash + salt + mustChangePassword (本期不内嵌, 让用户在浏览器里登录) |
| OneBot 端口      | NapCat 自身配置决定                                    | `config/onebot_<uin>.json` (默认 http=3000, ws=3001)                         |
| WebUI 端口       | NapCat webui-backend 决定                              | `config/runtime.json` 的 `webuiPort` (默认 5099)                             |
| 状态获取         | `status.json` 落盘                                     | OneBot WS `get_status` API (`ws://127.0.0.1:3001/`)                          |

证据:
- 真实发布包 `C:\Users\QIAO\Desktop\SnowLuma-v1.7.5-win-x64\` 的目录结构: `node.exe + launcher.bat + index.mjs + client/ + config/ + data/ + native/`
- SnowLuma `index.mjs` 中 `process.argv` 零命中, `config-DyfbYA36.js:134` 的 `loadOneBotConfig(uin)` 表明 uin 来自登录态而非 CLI

## 1. 目标 (Goal)

让 NapCatQQ-Desktop-V1 在不破坏 NapCat 既有用户兼容性的前提下, **同时支持 NapCat 与 SnowLuma 两种后端**, 用户在创建 Bot 时通过 `backend_type` 字段选择. UI 层 (BotCard / 日志 / WebUI 入口 / 设置 / 安装更新) 走同一套交互, 内部按 `backend_type` 分流到各自的进程管理与配置渲染器.

## 2. 交付物 (Deliverable)

### 2.1 配置与路径

1. `src/core/runtime/backend_type.py` (新增) — 定义 `BackendType` 枚举 (`NAPCAT` / `SNOWLUMA`), 提供 `from_str` / `display_name` 辅助
2. `src/core/config/config_model.py` (改) — `BotConfig` 加 `backend_type: BackendType = BackendType.NAPCAT` 字段; 配置加载时旧 bot 自动迁移为 `NAPCAT` (不破坏既有配置文件)
3. `src/core/runtime/paths.py` (改) — `PathFunc` 增加:
   - `snowluma_path = runtime_path / "SnowLuma"`
   - `get_snowluma_node_executable() -> Path | None` (返回 `<snowluma_path>/node.exe`, 不存在返回 `None`)
   - `get_snowluma_entry() -> Path` (`<snowluma_path>/index.mjs`)
   - `get_snowluma_config_dir() -> Path` (`<snowluma_path>/config`)
   - `get_snowluma_data_dir() -> Path` (`<snowluma_path>/data`)
   - `path_validator()` 把 `snowluma_path` 加入需要 ensure 的列表

### 2.2 配置渲染器 (Desktop 是配置 SOT, 启动前渲染到 SnowLuma 自己的 config/)

4. `src/core/runtime/snowluma_config_renderer.py` (新增) — 单一职责模块:
   - `render_runtime_json(snowluma_path: Path, webui_port: int = 5099) -> None` 写 `config/runtime.json`
   - `render_webui_json(snowluma_path: Path, password: str | None = None, *, must_change: bool = False) -> None` 写 `config/webui.json` (首次安装时随机生成 password + salt; 后续 Desktop 不覆盖, 让用户在 WebUI 自己改)
   - `render_onebot_json(snowluma_path: Path, qqid: int, *, http_port: int, ws_port: int, access_token: str, message_format: str = "array", report_self_message: bool = False) -> None` 写 `config/onebot_<qqid>.json`
   - `read_existing_onebot_json(snowluma_path: Path, qqid: int) -> dict | None` (升级路径: 读取后再合并 Desktop 的修改, 避免覆盖用户在 SnowLuma WebUI 的运行时改动)
   - **重要不变量**: 渲染只在 `bot.start` 之前执行一次; SnowLuma 进程启动后 Desktop 不再写 config (避免与 SnowLuma 自己的 `saveJson` 竞写)

### 2.3 进程管理

5. `src/core/runtime/napcat.py` (改) — `ManagerNapCatQQProcess.create_napcat_process` 在 `bot.runtime_target == local` 分支上按 `bot.backend_type` 二分:
   - `NAPCAT` 走现有 `_create_napcat_process` (零行为变更)
   - `SNOWLUMA` 走新增 `_create_snowluma_process(config: Config) -> QProcess`:
     - program: `<snowluma_path>/node.exe`
     - args: `[<snowluma_path>/index.mjs]`
     - working_directory: `<snowluma_path>`
     - environment: `QProcess.systemEnvironment()` (不写 `NAPCAT_*` 任何变量)
     - 启动前调用 `snowluma_config_renderer.render_*` 把 Desktop BotConfig 渲染落盘
   - `_handle_process_state_changed` / `_handle_process_finished` / `_handle_local_start_error` 三个回调对两种 backend_type 通用 (NapCatProcessModel 不区分后端类型, 只持 `QProcess`)
6. `src/core/runtime/napcat.py` (改) — `stop_process` 对 SnowLuma 直接 kill node.exe + 子进程 (现有 `psutil.Process(...).children(recursive=True)` 已经够用, 不需要再写)

### 2.4 登录态获取 (SnowLuma 状态机加一档)

7. `src/core/runtime/login_state.py` (改, 或并入 `ManagerNapCatQQLoginState`) — 扩展登录态枚举:
   - 现有: `Disconnected` / `LoggedIn`
   - 新增 SnowLuma 分支需要的: `WaitingForQRScan` (进程已启动但 WebUI 未登录任何 uin)
   - SnowLuma 状态获取通过 OneBot WS `get_status` 接口轮询 (`ws://127.0.0.1:<onebot_ws_port>/`), 而不是 NapCat 现有的 `status.json` 落盘读取
8. `src/core/runtime/snowluma_status_poller.py` (新增) — 仅 SnowLuma 用的状态轮询器:
   - 复用 `_REMOTE_POLLING_INTERVAL_MS = 5000` 量级
   - 失败时静默 (避免淹没日志), 仅 trace 级别记录
   - 接收到合法 `get_status` 响应后, 把 `online: bool` 翻译成 `LoggedIn` / `WaitingForQRScan`

### 2.5 安装与版本 (复用 NapCat updater 模式, 不引入新框架)

9. `src/core/network/urls.py` (改) — `Urls` 类追加:
   - `SNOWLUMA_REPO = QUrl("https://github.com/SnowLuma/SnowLuma")`
   - `SNOWLUMA_REPO_API_FALLBACK = QUrl("https://api.github.com/repos/SnowLuma/SnowLuma/releases/latest")`
   - `SNOWLUMA_REPO_API` (镜像; 若无可用镜像则与 fallback 同值, 留扩展位)
   - `SNOWLUMA_DOWNLOAD_TEMPLATE = "https://github.com/SnowLuma/SnowLuma/releases/download/{tag}/SnowLuma-{tag}-win-x64.zip"` (上游发布物名形如 `SnowLuma-v1.7.5-win-x64.zip`, 含版本号, **不是** `latest/download` 模式)
10. `src/core/versioning/service.py` (改):
    - `VersionSnapshot` 数据类追加 `snowluma_version: str | None` + `snowluma_update_log: str | None`
    - `RemoteVersionTask.execute()` 多拉一份 SnowLuma (走 `_get_version_with_fallback` + `_parse_github_response`, 与 NapCat / NCD 完全对称)
    - `LocalVersionTask` 追加 `get_snowluma_version() -> str | None` 方法 (读 `<snowluma_path>/package.json` 的 `version` 字段, 找不到时返回 None), 并在 `LocalVersionTask.execute()` 把 `snowluma_version` 写入 `VersionSnapshot`
11. `src/core/installation/installers.py` (改, **不**新建独立目录) — 在现有 `NapCatInstall` 类旁追加 `SnowLumaInstall` 类:
    - 入参: `tag` (eg `v1.7.5`)
    - 下载到 `tmp_path / f"SnowLuma-{tag}-win-x64.zip"`
    - 解压到 `snowluma_path` (覆盖文件; **保留** `config/` 与 `data/` 子目录)
    - 校验 `node.exe` 与 `index.mjs` 存在
    - 失败时清理临时 zip
    - 与 `NapCatInstall` 复用相同的 signal 命名 (`status_label_signal` / `error_finish_signal` / `progress_ring_toggle_signal` / `install_finish_signal`), 让 `SnowLumaPage` 可以走 `NapCatPage.handle_install_requested` 同款连接模式

### 2.6 UI 适配

12. `src/ui/page/add_bot_page/...` (改) — 新建 Bot 表单加 `backend_type` 单选 (NapCat / SnowLuma); 默认 `NAPCAT` 不影响存量流程
13. `src/ui/page/bot_page/widget/card.py` (改):
    - BotCard 加 `backend_type` 徽标 (颜色或文本徽标, 不引入第三方 logo)
    - `slot_web_ui_button` 按 `bot.backend_type` 分流:
      - `NAPCAT`: 现有 `http://127.0.0.1:{login_state.port}/webui?token={login_state.token}` (零行为变更)
      - `SNOWLUMA`: 读 `<snowluma_path>/config/runtime.json` 拿 `webuiPort` (默认 5099), 打开 `http://127.0.0.1:{webuiPort}/` (不带 token; 用户在 WebUI 内输入密码)
    - `slot_qr_code_button` 在 `SNOWLUMA` 分支隐藏或灰显 (扫码在 SnowLuma WebUI 内完成, 不在 Desktop 内做二维码渲染)
14. `src/ui/page/component_page/sub_page/snowluma_page.py` (新增) — 与 `napcat_page.py` 1:1 复刻的安装/更新页:
    - 继承现有 `PageBase` (`src/ui/page/component_page/widget/base.py`)
    - `app_card` 上的 `install_button` / `update_button` / `pause_button` / `cancel_button` / `open_folder_button` 全部连到 `handle_*_requested` 槽 (与 NapCatPage 同名同语义)
    - `apply_remote_version_data(version_data: VersionSnapshot)` 读 `version_data.snowluma_version` + `version_data.snowluma_update_log`
    - `apply_local_version_data(version_data: VersionSnapshot)` 读 `version_data.snowluma_version`
    - 下载用 `GithubDownloader(Urls.SNOWLUMA_DOWNLOAD_TEMPLATE.format(tag=remote_version))`; 安装用 `SnowLumaInstall()`
    - 安装/更新前若 `it(ManagerNapCatQQProcess).has_running_bot()` 为真, 弹 `AskBox` 询问是否关闭所有 Bot (与 NapCatPage 行为对齐)
    - `setObjectName("UnitSnowLumaPage")` (沿用 NapCatPage 的 `UnitNapCatPage` 命名风格)
15. `src/ui/page/component_page/sub_page/__init__.py` (改) — 导出新增的 `SnowLumaPage`
16. `src/ui/page/component_page/__init__.py` (改) — 在 `ComponentPage` 类里:
    - `__init__` 增加 `self.snowluma_page = SnowLumaPage(self)`
    - `_create_view` 调 `self.view.addWidget(self.snowluma_page)` 并在 `top_card.pivot.addItem` 加一个 `text="SnowLuma"` 的 tab
    - `_connect_signals` 把 `version_service.remote_versions_loaded` / `local_versions_loaded` 各连一条到 `self.snowluma_page.apply_*_version_data`
    - `refresh_versions` 调 `self.snowluma_page.begin_version_refresh()` + `self.snowluma_page.log_card.set_loading(True)`
    - 不引入独立的 SnowLuma 控制面板; 不动 `setup_page` (`setup_page` 是 Desktop 自身设置页, 与组件安装无关)

### 2.7 测试

17. `script/test/test_backend_type_model.py` (新增) — `BackendType` 枚举 + `BotConfig.backend_type` 默认值 + 旧配置迁移
18. `script/test/test_snowluma_config_renderer.py` (新增) — 三个 render_* 函数的输出与上游 `SnowLuma-v1.7.5-win-x64/config/*.json` 兼容 (用真实样本作为 golden)
19. `script/test/test_snowluma_process_construction.py` (新增) — `_create_snowluma_process` 构造的 QProcess (program/args/cwd/env) 符合预期, 不实际启动子进程
20. `script/test/test_snowluma_installer.py` (新增) — mock httpx + zipfile 验证 `SnowLumaInstall` 类下载、覆盖、保留 `config/data/` 行为
21. `script/test/test_versioning_snowluma.py` (新增) — `RemoteVersionTask` mock GitHub releases 响应, 验证 `VersionSnapshot.snowluma_version` 解析正确; `LocalVersionTask.get_snowluma_version()` 在 `<snowluma_path>/package.json` 存在/缺失时分别返回值 / None

## 3. 约束 (Constraints)

- **不**改 SnowLuma 项目本身代码 (`example/SnowLuma-main/` 与上游 release 都不动)
- **不**为 SnowLuma 引入第二套 UI 标签页 / SidePanel (沿用 NapCat 现有 BotPage / BotCard / SettingPage)
- **不**为 SnowLuma 内嵌 `QWebEngineView` (沿用 NapCat 的「外开浏览器」模式)
- **不**给 Desktop 自己再打包一份 Node.js (上游 release 已自带 `node.exe`, 直接复用)
- **不**做远程 (SSH) 部署 SnowLuma (一期仅本地; 远程留 P2)
- **不**做 macOS 平台支持 (上游 release 本身没原生 mac 二进制)
- **不**改 `Backend` 抽象的对外签名 (`start_napcat` 保留, 内部按 backend_type 分流; 后续若必要再重构为 `start_bot`)
- **不**改既有 NapCat 启动路径的任何行为 (零回归)
- **保留** `bot.runtime_target` (local/remote) 维度; SnowLuma 一期只支持 `local`, `remote` 分支抛 `NotImplementedError("SnowLuma 远程部署未在 P1 范围")`
- **保留** 现有 `creart` 单例 / `it(PathFunc)` / `QObject + Signal` 模式
- 渲染器只在进程启动前写 SnowLuma config; 启动后 Desktop 不再写, 避免与 SnowLuma 自己的 `saveJson` 竞写

## 4. 验收标准 (Acceptance Criteria)

### 4.1 代码可执行性

- [ ] `python -c "from src.core.runtime.backend_type import BackendType; print(list(BackendType))"` 输出包含 `NAPCAT` / `SNOWLUMA`
- [ ] `python -c "from src.core.runtime.snowluma_config_renderer import render_onebot_json"` 不报错
- [ ] `python -c "from src.core.installation.installers import SnowLumaInstall"` 不报错
- [ ] 现有 `python main.py` 启动行为零回归 (打开 BotPage, 已有 NapCat Bot 仍能 Start/Stop)

### 4.2 单元测试

- [ ] `pytest script/test/test_backend_type_model.py -q` 全绿
- [ ] `pytest script/test/test_snowluma_config_renderer.py -q` 全绿
- [ ] `pytest script/test/test_snowluma_process_construction.py -q` 全绿
- [ ] `pytest script/test/test_snowluma_installer.py -q` 全绿
- [ ] `pytest script/test/test_versioning_snowluma.py -q` 全绿
- [ ] 现有 `test_*` 套件不回归 (尤其 `test_*napcat*` / `test_*backend*` / `test_*remote*`)

### 4.3 行为变更 (可代码 review 验证)

- [ ] 在不安装 SnowLuma 的情况下打开 Desktop, 「新建 Bot」表单可见 `backend_type` 选项, 选 `SNOWLUMA` 时若 `snowluma_path/node.exe` 不存在则提示先在组件页安装
- [ ] 组件页 (`ComponentPage`) 顶部 pivot 多出一个 `SnowLuma` tab, 进入后能拉取版本 (mock 或真实) 并显示, 行为与 NapCat tab 同款 (相同的 install/update/pause/cancel/openFolder 按钮)
- [ ] 安装 SnowLuma 后, BotCard 的 WebUI 按钮在 `backend_type=SNOWLUMA` 时打开 `http://127.0.0.1:5099/`
- [ ] BotCard 在 `backend_type=NAPCAT` 时打开行为与现在完全一致 (token 拼接到 URL)
- [ ] `slot_qr_code_button` 在 `backend_type=SNOWLUMA` 时隐藏或灰显
- [ ] SnowLuma 进程退出时, `_handle_process_finished` 同样能清理 `napcat_process_dict` 并 emit `NotRunning`

### 4.4 产品验收 (Product Acceptance)

- 在已安装 NapCat 的 Desktop 上**不卸载 NapCat**, 通过组件页 (`ComponentPage`) 的 SnowLuma tab 一键下载安装 SnowLuma, 然后用「新建 Bot」创建一个 SnowLuma Bot, 启动后 BotCard 进入 `Running + WaitingForQRScan` 态, 点 WebUI 按钮浏览器跳到 `http://127.0.0.1:5099/`, 输入密码后扫码登录 QQ, BotCard 状态自动切换到 `Running + LoggedIn`
- 同一 Desktop 内同时跑「一个 NapCat Bot + 一个 SnowLuma Bot」, 两者互不干扰; 全部 Stop 后再全部 Start, 仍能跑起来
- 升级 SnowLuma 版本时, 用户在 WebUI 改过的密码与扫码登录态保留 (config/data 不被覆盖)

## 5. 手工抽查 (Manual Spot Checks)

- 启动 Desktop, 进入「新建 Bot」, 检查 backend_type 单选默认是 `NAPCAT`
- 进入组件页 (左侧导航) → 顶部 pivot 切到 SnowLuma → 触发安装, 观察 `runtime/SnowLuma/` 出现 `node.exe + index.mjs + native/...` 等真实文件
- 在不联网的情况下打开组件页 → SnowLuma tab, 版本拉取应失败但不崩 UI, 仅显示「未知」
- BotCard 上 SnowLuma 徽标与 NapCat 徽标在视觉上能区分

## 6. 完成语言策略 (Completion Language Policy)

只有当 4.1+4.2+4.3 全部勾选完成、phase_cleanup 报告写入 outputs 后, 才允许使用「全部完成 / 已交付 / 验收通过」等完成性措辞. 在此之前对外都用阶段性措辞 (例如「P2 完成, 等待跑完 4.2 单测」).

## 7. 交付真相契约 (Delivery Truth Contract)

- **可声明完成**的依据: `pytest script/test/test_backend_type_model.py script/test/test_snowluma_config_renderer.py script/test/test_snowluma_process_construction.py script/test/test_snowluma_installer.py script/test/test_versioning_snowluma.py -q` 退出码 0
- **不可代替**的人工验收点: 在真实 Windows 环境装一次 SnowLuma + 启动 SnowLuma Bot + 在 WebUI 完成扫码登录 (开发者自验)
- **未达成必须明示**: 任一新增测试未跑、跑挂、或人工验收未做, 均不得用「完成」措辞

## 8. 非目标 (Non-Goals)

- SnowLuma 远程 (SSH) 部署 (留 P2)
- macOS / Linux Desktop 平台 (上游也无 mac 原生)
- SnowLuma WebUI 内嵌到 Desktop (走外开浏览器, 与 NapCat 一致)
- Desktop 内置二维码扫码 UI for SnowLuma (扫码在 SnowLuma WebUI 内完成)
- 重写 `Backend` 抽象 (本期内部 if-else 分流即可, 抽象层重构留后续)
- SnowLuma config 与 NapCat config 双向映射 (本期 SnowLuma 是新建独立 Bot, 不做跨后端配置迁移)
- SnowLuma 自身 ffmpeg 转码工作流的 UI 暴露
- SnowLuma SDK (`@snowluma/sdk` npm 包) 的下游消费集成

## 9. 自治模式 (Autonomy Mode)

`interactive_governed`. XL 计划阶段的 wave/batch 边界处不再向用户提问; 只有出现「破坏冻结需求 / 需要新建第二个交付目标 / 上游 release 命名结构变化与本文档不符」时才回到用户.

## 10. 推断假设 (Inferred Assumptions)

- SnowLuma `index.mjs` 不解析 `process.argv` (已通过对 `SnowLuma-v1.7.5-win-x64/index.mjs` 的 grep 验证: `process.argv|argv\[|parseArgs|commander|yargs` 全部零命中)
- SnowLuma 在 `config/onebot_<uin>.json` 缺失时**不会自动创建** (`loadOneBotConfig(uin)` 路径返回值依赖文件存在); 因此 Desktop 必须在启动前渲染该文件
- SnowLuma 在 `config/webui.json` 缺失时会**自首次启动时生成** (有 `passwordSalt` + `passwordHash` + `mustChangePassword: false` 字段表明它能自治), 因此 Desktop **不必**在首次安装时强制写入 webui.json; 但 Desktop 提供「重置 WebUI 密码」入口 (清空 webui.json 让 SnowLuma 下次启动重生)
- SnowLuma 上游 GitHub release zip 的命名约定为 `SnowLuma-<tag>-win-x64.zip` (与本期参考的 `SnowLuma-v1.7.5-win-x64` 一致), 若未来命名变化需要回到本文档调整 `SNOWLUMA_DOWNLOAD_TEMPLATE`
- SnowLuma 不存在「绑定 Desktop 期望的 QQID」概念; 用户在 WebUI 扫的是哪个 QQ, SnowLuma 就用哪个 uin 落 `data/<uin>/`. Desktop 的 `BotConfig.bot.QQID` 与实际登录的 uin **可能不一致** (用户扫错号), 这种情况下 BotCard 显示「QQID 不匹配」警示, 不强制 kill 进程, 留给用户自行重新扫码或修正配置
- 现有 `Backend.start_napcat` 命名虽然带 napcat, 但其实可以承载 SnowLuma 启动 (内部分流即可); 未来如果团队认为命名不再合适, 留 P2 重构空间
- SnowLuma 一期仅 win-x64; arm64 / linux 用户即便看到「新建 Bot」时选 SnowLuma 也会因为下载阶段没有匹配资产而失败, 组件页 SnowLuma tab 应在非 win-x64 平台上禁用安装/更新按钮 (与 `NapCatPage` 平台限制保持一致的处理风格)

## 11. 引用 (References)

- 上游 SnowLuma 源码: `example/SnowLuma-main/` (gitignored, 仅本地参考)
- 上游 SnowLuma 真实发布包: `C:\Users\QIAO\Desktop\SnowLuma-v1.7.5-win-x64\`
- NapCat updater 现有实现: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/versioning/service.py:50-168` + `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/network/urls.py:54-67`
- NapCat WebUI 按钮现有实现: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/ui/page/bot_page/widget/card.py:354-372`
- NapCat 进程管理现有实现: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/runtime/napcat.py:1337-1681`
- NapCat 路径管理现有实现: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/runtime/paths.py:49-147`
- Backend 抽象: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/operation/backend.py:205-230`
- **NapCat 安装/更新页范本 (SnowLumaPage 1:1 参照)**: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/ui/page/component_page/sub_page/napcat_page.py`
- **NapCat 安装器范本 (SnowLumaInstall 1:1 参照)**: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/installation/installers.py` 的 `NapCatInstall` 类
- **ComponentPage 主容器 (注册 SnowLumaPage 入口)**: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/ui/page/component_page/__init__.py:25-114`
- **PageBase / DisplayCard 公共基类**: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/ui/page/component_page/widget/base.py`
- **LocalVersionTask 范本**: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/versioning/service.py:171-206`
- 范本: `@d:/NapCat-Project/NapCatQQ-Desktop-V1/docs/requirements/2026-05-08-ssh-distro-expansion.md`

# 后端已就绪能力速查

> 规划新功能前先查这里：已经有的直接用或扩展，别再写一份。前端能力看 frontend.md，功能域落点看 codemap.md，踩坑看 lessons.md。
> 这里只写各 crate 现在真有的导出。改名、删掉或新加一类能力时顺手改本文。拿不准就去 `lib.rs` 看 `pub use`。

ls 入口：`crates/ncd-domain/src/`、`crates/ncd-traits/src/`、`crates/ncd-host/src/`、`crates/ncd-component/src/`、`crates/ncd-appframework/src/`、`crates/ncd-runtime/src/`

分层：L1 `ncd-domain` → L2 `ncd-traits` → L3 `ncd-runtime`（Manager / Service 持有全部可变状态）→ L4 `src-tauri`（命令只转参数和错误）。中间的 host / component / appframework / deploy / config / server / network 和两个后端 crate 给 runtime 用；两个 `ncd-backend-*` 互不依赖。

---

## 先查这张表（最常被重写的小工具）

| 要做的事 | 已有的 | 在哪 |
| :--- | :--- | :--- |
| 往 `sh -c` 里拼一个值 | `shell_single_quote`（一律包单引号，里面的 `'` 写成 `'\''`）；按目标 shell 转义用 `host.shell().escape`，`HostCommand` 的参数本来就走它 | `ncd-host/src/shell.rs` |
| 看命令在不在、在哪 | `Host::command_exists`（只看 PATH）；`Host::which`（PATH 里没有再看 `/usr/local/bin`、`/usr/bin`、`/bin` 等，远端按连接记住结果，探测失败会报错） | `ncd-host/src/host.rs` |
| 改名、挪目录 | `Host::rename`，别拼 `mv` | 同上 |
| 取文件大小 | `Host::file_size`（跟着链接，给不出是 `None`）；远端 shell 里取大小、从偏移读、读末尾几行用 `remote_file_size` / `remote_read_from` / `remote_tail_lines`，单次上限 `LOG_FOLLOW_CHUNK_BYTES` | `host.rs`、`ncd-host/src/remote_file.rs` |
| Linux 装系统包 | `LinuxPackageManager`：`detect` 现探 apt / dnf / yum / apk / pacman，`install_command` / `remove_command` / `refresh_command`，一条脚本先刷再装 `refresh_and_install_script`（刷新失败也接着装），给用户复制的提示 `install_hint`，包名先过 `is_valid_package_name` | `ncd-host/src/linux_pkg.rs` |
| 装包进度、dpkg 锁 | 行解析 `parse_pkg_mgr_line`（`pkg_output.rs`）；带进度跑 `ncd_component::run_pkg_command_with_progress`；等 unattended-upgrades 放锁 `wrap_sh_script_with_dpkg_wait`（`apt_lock.rs`）；本应用内串行 `PackageManagerLock`（`ncd-runtime/src/components/package_lock.rs`） | |
| 挑口、查口 | `ncd_appframework::ports`：`PortUsage::probe` + `is_free`（本机 bind 回环再连一下，挡住在 0.0.0.0 上听的进程；远端一次读 `/proc/net/tcp{,6}`），`remote_listening_ports`；应用端实例口和对接听口 `allocate_listen_port` / `allocate_stable_port` | `ncd-appframework/src/ports.rs`、`ncd-runtime/src/app_framework/listen_port.rs` |
| known_hosts 行解析和主机匹配 | `parse_known_hosts_line`、`known_hosts_host_matches`（裸主机名只算 22 端口，别的端口只认 `[host]:port`，不分大小写，`!host` 排除）；从本机 OpenSSH 抄指纹 `ncd_server::openssh_known_hosts::seed_app_known_hosts`（跳过 `@revoked`） | `ncd-host/src/remote/host_key.rs` |
| host_id 和 server_id 互转 | `server_id_of_host`、`host_id_of_runtime_target`、`LOCAL_HOST_ID`、`REMOTE_HOST_ID_PREFIX` | `ncd-domain/src/app_framework.rs` |
| 远端默认布局、Docker Bot 目录 | `remote_paths::{desktop_default_install_paths, derive_remote_linux_paths, docker_bot_project_dir, qq_bin_candidates, snowluma_install_candidates}`；Bot 容器可能的名字 `ncd_deploy::bot_docker_container_candidates` | `ncd-domain/src/remote_paths.rs` |
| 远端 home / 布局 / 选中路径 / needs_sudo | `ComponentExecutor::inventory()` 给的 `RemoteInventoryService`：`ensure`、`host_probe`、装卸后 `invalidate`（下一次一定重探） | `ncd-runtime/src/remote/inventory.rs` |
| 读写 `app-settings.json` | `ncd_runtime::desktop::{load_app_settings, read_app_settings_file, update_app_settings, replace_app_settings_with}`，归一化只调 `AppSettings::normalize`，本机 SnowLuma 自定义 Node 用 `AppSettings::snowluma_node_override` | `ncd-runtime/src/desktop/settings.rs` |
| 下载 | `ncd_network`（共享 client、续传、镜像竞速、16 MB 以上切片）；组件里用 `DownloadHelper`（加 SHA256 和取消）；让远端自己下 `Host::download_url` / `download_url_to_host_with_progress` | `ncd-network/src/`、`ncd-component/src/download.rs` |
| 活跃任务登记 | `ActiveTasks::register` 返回 guard，任何返回路上都会摘掉 | `ncd-runtime/src/components/active_tasks.rs` |
| 退出、更新后重启 | `commands/exit.rs::shutdown_and_exit`（停本机 Bot、停本机应用端实例、关终端、清远端在场标记）；闸门计数 `local_active_bots` | `src-tauri/src/commands/exit.rs` |
| 日志快照 | `ncd_domain::LogSnapshot`（`ncd_traits` 原路径 re-export） | `ncd-domain/src/log_snapshot.rs` |

---

## ncd-domain（L1，零运行时依赖）

只依赖 serde / serde_json / thiserror / ts-rs。跨 IPC 的类型都在这里派生 ts-rs，导出到 `src-ui/core/ipc/generated/domain/`。

- 配置：`bot_config`（`BotConfig::validate` / `validate_runtime_matrix`）、`app_config`（`AppSettings`、`DesktopNotifySettings`，`normalize` 一个入口）、`runtime_scenario`、`snowluma_start_mode`、`snowluma_linux_package`
- Bot 运行：`bot_actor`（`BotActorState` 六态：Stopped / Starting / Running / Stopping / Crashed / Repairing）、`bot_status`、`daemon_state`、`napcat_events`、`bot_runtime_metrics`、`log_snapshot`
- 事件：`domain_event`（`DomainEvent`，信封版本 `DOMAIN_EVENT_ENVELOPE_VERSION`）
- 组件：`component`（`ComponentId`、依赖图 `DependencyPlan` / `DependencyNode` / `RuntimeReadiness` / `RequirementStatus` 等），`progress`、`qq_dependency`、`node_environment`
- 应用端：`app_framework`（`AppInstance`、`AppFrameworkManifest`、`OneBotLinkPlan` / `OneBotLinkMode`、`AppLinkRecord`、配置文档和错误类型、`classify_app_link` 对接拓扑、`parse_ws_url` / `rewrite_ws_loopback_port`、`app_link_connection_name`）
- 远端：`remote_inventory`、`remote_paths`、`docker`（`DockerDeploySpec::validate` 等）、`deployment_task`
- 其它：`terminal`（终端目标、会话、带 `v` 的事件）、`data_root_migrate`、`release_snapshot`、`offline_alert`、`bootstrap`、`migration`、`errors`、`ids`、`kinds`

## ncd-traits（L2，接口契约）

`ConfigStore`（+ `JsonTransaction` / `JsonWrite`）、`BotConfigRepo`、`SecretStore`、`EventBus`（+ `BroadcastEventBus` / `EventFilter`）、`BotBackend`（`runtime_backend.rs`）、`BackendConfigRenderer`、`MigrationStep`、`PathProbe`、`AppIntegration`（应用端对接的纯计划，错误 `AppFrameworkError`）。

应用端没有 `AppRuntime` trait：起停由 ncd-runtime 的 `NativeAppRuntime` 做，碰主机的框架差异在 `ncd_appframework::AppFrameworkAdapter`。

## ncd-host（主机抽象：本机 Windows / 远端 Linux）

- `Host` trait：文件（`read_file` / `write_file` / `list_dir` / `create_dir_all` / `remove_*` / `rename` / `exists` / `file_size`）、传输（`upload` / `download` / `download_url`）、`extract_archive`（zip / tar.gz / tar.xz / msi）、进程（`spawn` / `run_to_string` / `run_streaming` 逐行回调）、提权（`set_elevation_password`：远端有密码走 `sudo -S`，没有走 `sudo -n`）、`command_exists` / `which`、`open_tunnel`、`list_drives`（只有本机有盘符）、`open_pty`、连接自愈（`supports_refresh` / `invalidate_connection` / `is_healthy`）。不支持的默认返回 `Unsupported`
- 实现：`LocalWindowsHost`（tokio fs / process，zip 只开 deflate，ConPTY 在 `local/pty_windows.rs`，UAC 提权 `local/elevate.rs`）；`RemoteLinuxHost`（russh 0.45 + russh-sftp，连接复用、keepalive、`remote/linux/pty.rs`）；`RemoteWindowsHost` 是全返回 `Unsupported` 的 stub
- 命令与路径：`HostCommand`（`.elevated()`、`.timeout()`，参数按目标 shell 转义）、`HostPath`（内部一律 POSIX 风格）、`HostShell`（`BashShell` / `PowerShellShell` / `CmdShell`）
- 远端：`ConnectionConfig`、`SshCredentials`、`HostKeyPolicy` / `KnownHostsStore`、隧道 `TunnelSpec::{local_to_remote, remote_to_local}` + `TunnelHandle`（Drop 即关；`detached` 是不泵数据的句柄，给测试替身记口用）、`probe_sudo` / `SudoAccess`、`curl_url_download_command` / `wget_url_download_command` 及进度解析
- 终端：`pty.rs`（`PtyRequest` / `PtyProgram { LoginShell, Program, Script }` / `PtySession` / `PtyExit`）
- 其它：`stream_chunk`、`subprocess::hide_console_window`、`apt_lock`、`linux_pkg`、`pkg_output`、`remote_file`

真机冒烟默认 `#[ignore]`，只在 `/tmp/ncd-host-test-<pid>-<rand>/` 里动，不碰业务目录、不杀已有进程、不改 `~/.bashrc` / `/etc/*` / `~/.ssh/authorized_keys`：

    $env:NCD_TEST_SSH_HOST = "175.178.53.24"
    $env:NCD_TEST_SSH_USER = "ubuntu"
    $env:NCD_TEST_SSH_KEY  = "$env:USERPROFILE\.ssh\id_ed25519"
    cargo test -p ncd-host --test remote_linux_smoke -- --ignored --test-threads=1

麦麦远端冒烟同样按 `--test` 单独跑：`cargo test -p ncd-runtime --test maibot_remote_smoke -- --ignored`（另可设 `NCD_TEST_SSH_PORT`）。

## ncd-component（Component × Host × Action 的「装什么」）

- trait：`Component`（`requirements()` 声明依赖）、`Action`；执行上下文 `ActionCtx`（`ProgressEvent` 带 `v` 信封、取消、日志）
- 组件：`NodeJsComponent`、`UvComponent`（钉住各发行包 sha256，先攒在 `.new` 里装齐再换）、`QQComponent`（Linux rootless 解 deb / rpm 到 `<base>/opt/QQ/`，最新版本现查 `probe_linux_qq_latest`；Windows 走注册表探测）、`NoVncComponent`（装 Xvfb / fluxbox / x11vnc / novnc / websockify）、`NapCatComponent`、`SnowLumaComponent`、`NcdWatchComponent`、`DesktopSelfComponent`（只给 detect / launch_command，自更新走 ncd-update）；应用端框架组件在 ncd-appframework
- 共用：`DownloadHelper`、`download_url_to_host_with_progress`、`run_pkg_command_with_progress`、`qq_deps/`（QQ 系统依赖）、`remote_qq_entry`（改 QQ `package.json` 的 main）、`qq_linux_pid`
- 版本匹配：`VersionMatch` / `all_versions_match`（按 semver；依赖图的数据类型在 ncd-domain，这里原样转出）

## ncd-appframework（应用端框架适配器）

- `AppFrameworkAdapter`（`adapter.rs`）：清单、`component`、`integration`，以及带默认实现的一整套钩子：导入探测 `probe_project`、WebUI 账号（`read_webui_account` / `default_webui_username` / 生成和校验密码）、上游条款 `pending_terms` / `accept_terms`、`launch_command`、`terminal_profile`、对接 `apply_link` / `unlink` / `rollback_link`、配置文档 / 类型化配置 / 运行中写配置（`supports_live_config`、`config_write_min_interval`）、商店（`list_installed`、装 / 更 / 卸、启停、市场地址和解析）、运行期接口 `astrbot_runtime` / `maibot_runtime`。写配置统一 `apply_with_backup` / `restore_from_backup`（备份 → 写 → 失败还原）
- 注册：`AppFrameworkRegistry::with_builtin`（Karin / NoneBot2 / AstrBot / MaiBot）。接新框架：加子目录 + 注册一行 + `ComponentId` 变体
- 共用件：`config_doc`（`AppInstanceConfig`、修订号、读写文档）、`env_file`（保序 dotenv）、`toml_patch`（只动改了的键，保注释和不认识的键）、`adopt`（导入已有项目：记下原文件、还原、清掉桌面端留下的东西）、`node_tooling`（Node / pnpm 工具链和 PATH）、`uv_tooling`（`ensure_python`、glibc / musl 探测、剩余空间）、`terminal`（`AppTerminalProfile`、`uv_venv_profile` / `node_profile`）、`store`（框架无关的商店条目）、`ports`
- 各框架：`karin/`（配置六份 JSON + `.env`、插件）；`nonebot2/`（`.env.prod`、`[tool.nonebot]` 代管、`uv add/remove`）；`astrbot/`（Dashboard 登录态归 `AstrBotAdapter` 持有，运行期接口 `AstrBotRuntimeApi`，AI 投影写回 `ai.rs`）；`maibot/`（生成的强类型配置 `schema/`、`config/validate.rs`、WebUI 客户端、`MaiBotRuntimeApi`、`resources/` 各数据页、`release.rs` 挑适配器与宿主版本、`terms.rs`）

## ncd-runtime（L3 编排，所有可变状态在这里）

- `BotManager`（`bot_manager/`）：Bot 表、Actor、批量启停、自动重启（`auto_restart`，cron 预览）；启动预检经 `RuntimeReadinessGate`，实现是 `components/readiness_gate.rs` 的 `ComponentRuntimeGate`；构建时 `with_server_manager` / `with_remote_inventory` / `with_runtime_gate` / `with_host_resolver` / `with_snowluma` 接上协作方
- `AppManager`（`app_framework/manager.rs` + `manager/`）：结构体和共用小工具在 `manager.rs`，职责按子模块分：`lifecycle`（新建、导入、安装、启停、删除、分配实例口）、`install_dir`、`config`（写盘后联动端口、对接、重启）、`link`、`tunnel`（`ensure_desktop_tunnel` 先看能不能复用、开完再核一遍再登记，`-L` / `-R` / 正向隧道共用；应用机常驻 ssh 的建立、对账、拆除）、`store`（任务提交 `submit_store_op`）、`webui`（`open_webui`，WebUI 口和密钥按实例缓存）、`astrbot`、`maibot`（`maibot_session` 一次给出接口和会话）、`terminal`。启动时 `with_component_executor` 接上组件执行器，安装走 `install_instance`，盯任务走同一条队列。另有 `NativeAppRuntime`（起停、pid 文件、停时连进程树收）、`AppInstanceStore`（`app-instances.json`，读不出先挪开原件）、`resident_link`、`plugin_market`（市场目录缓存归 AppManager）、`export_onebot_endpoint`
- `ComponentExecutor`（`components/executor.rs`，启动时建一份）：`submit` 把依赖闭包排成任务（去重、依赖先后）、`cancel`（先交给任务队列，队列不认识的才找活跃表）、`resolve` / `readiness` / `runtime_readiness`（组件页和 Bot 启动预检共用）、`install_qq_dependencies`、本机 Node 探测 `probe_local_node_candidates` / `probe_node_binary`；持有活跃任务表和 `RemoteInventoryService`。同目录：`graph`（`render_dependency_graph` / `requirement_closure`）、`resolver`、`factory`（`build_component_for_host`，应用端按 `AppComponentHint` 实例化）、`action_policy`（组件目录、去重键、任务资源、SnowLuma 发行包名）、`system_package`（补主机命令和 QQ 依赖的任务）、`package_lock`
- `DeploymentTaskManager`（`deploy/tasks.rs`）：任务队列事实源，`submit` / `active_task_by_dedupe_key` / `cancel` / `push_progress` / `list`
- `TerminalManager`（`terminal/`）：会话表、回放、流控、`restart`、`detach_all` / `close_all`、`fill_sudo`；文件栏 `files.rs`（列目录、读写文本、建目录、改名、删、上传、下载、`export_text`）、状态条 `stats.rs`、系统终端 `external.rs`；目标到启动方案 `TerminalPlanner`（`plan.rs`、`plan/bot.rs`）、shell 集成 `integration.rs`、本机 shell 探测 `shells.rs`
- 远端：`remote/inventory.rs`、`docker_session.rs`（`DockerBotSessionRegistry`）、`runtime_sessions.rs`、`bot_log_follow.rs`、`importable_bots.rs`、`import_network.rs`
- 启动与部署：`launch/router.rs`（NC / SL × 本机 / 远端 × 原生 / Docker 路由）、`launch/plan.rs`、`bootstrap/reconcile.rs`、`native_deployment_adapter/`
- 桌面：`desktop/settings.rs`、`desktop/log.rs`、`desktop/crash_bundle.rs`；数据根 `data/relocate.rs`（整树换根）、`data/consolidate.rs`
- 其它：`metrics/`（Bot 内存和 OneBot 收发）、`notify/`（离线告警扇出：Toast / Webhook / 邮件 / OneBot）、`watch/sync.rs`（ncd-watch 配置同步）、`snowluma/`（协议同意、UI 状态）、`events.rs`（事件辅助 + 前端事件名一致性测试）、`release.rs`
- 旧路径兼容：`ncd_runtime::{config, server_manager, host_resolver, …}` 是对 ncd-config / ncd-server 的 re-export，新代码直接用域目录或原 crate

## ncd-backend-napcat / ncd-backend-snowluma

- NapCat：`napcat/webui_client/`、`napcat/login_poller/`、`endpoint_table.rs`、`offline_notifier.rs`；远端原生 `remote_native_napcat_session/`
- SnowLuma：本机 `snowluma/`（`daemon`、`session`、`status_poller/`、`webui_client/`、`proc_tree`、`qq_login_probe`、`runtime_backend`、`log_sanitize`）；远端 `remote_snowluma/`（`orchestrator`、`stack`、`layout`、`tunnel`、`inject`、`remote_bash`：远端 bash 路径按连接经 `Host::which` 记住）
- 两个 crate 都只依赖 ncd-host / component / deploy 这些下层，互不引用；共用的放下层

## ncd-deploy / ncd-config / ncd-server

- ncd-deploy：`Deployment` trait（`NativeDeployment` / `DockerDeployment` / `ExternalDeployment`）、旧轨 `DeployPlan` / `DeployBuilder`、`DockerCli`、`install_docker`、`render_compose*`、容器名 `bot_docker_container_name` / `bot_docker_container_candidates`、NapCat 和 QQ 控制台噪声过滤、`remote_coordinator`
- ncd-config：`LocalConfigStore`（`JsonTransaction` 原子写 + 自动备份）、`LocalBotConfigRepo`、Bot / App 迁移、`detect_drift`、配置渲染、`SecretStoreImpl`、`DataPaths`、`LocalPathProbe`、`LegacyDiscovery`
- ncd-server：`ServerManager`（档案、连接、健康探活、首次连接的指纹确认）、`CredentialSyncLayer`、`HostResolver`、`discover_ssh_hosts`（读 `~/.ssh/config`，跟着 Include）、`openssh_known_hosts`、`generate_ed25519`、档案迁移

## ncd-network / ncd-update / ncd-watch / ncd-log / ncd-template / ncd-test-support

- ncd-network：`shared_client`、`download_with_resume`、`download_with_mirror_race`、`download_smart`（切片）、`build_mirror_urls`、`retry_with_backoff`、GitHub API 中转签名 `proxy`
- ncd-update：`UpdateOrchestrator`（`check` / `precheck` / `install_with_graceful_shutdown` / `resume_after_update` / `record_failure` / `detect_pending_failures`）、`UpdateProvider` + `MockUpdateProvider`、`ResumeStore`。`build.rs` 嵌 manifest 声明 `asInvoker`，否则 crate 名带 update 会触发 UAC（os error 740）
- ncd-watch：远端主机侧监控二进制（探活 + Desktop 离线时 Webhook / 邮件 / 同机 OneBot），`default-features = false` 只给配置 schema，Desktop 用它写 `notify.json`
- ncd-log：会话日志六段行格式 `format_line`、`log_source_from_target`、`short_module_from_target`。业务代码用带 `target` 的 `tracing`，模块列和过滤粒度看 `src-tauri/src/desktop_log.rs` 的 `default_env_filter`；panic 这类非 tracing 场景才用 `write_session_line`。tracing 不订进 EventBus
- ncd-template：MiniJinja 模板引擎，compose 等文本生成
- ncd-test-support：`MockSecretStore`（`fail_next_*` 注入失败）、`TempWorkspace`、`BotConfigBuilder`、路径安全断言、`fixtures/legacy/`

## src-tauri（L4 薄壳）

- 插件：`tauri-plugin-opener`、`tauri-plugin-dialog`、`tauri-plugin-notification`、`tauri-plugin-single-instance`
- 权限：`capabilities/main.json` 给主窗；`capabilities/tray-panel.json` 给托盘面板，只有 `core:default` 和按 http / https 打开链接。面板要做的窗口操作放后端命令里
- 主窗和托盘面板之间的窗口通知：名字和信封版本在 `src-tauri/src/window_events.rs`，前端只在 `desktop.service.ts` 的 `windowEventService` 订
- 退出收尾：`commands/exit.rs`；AppState 挂的是 runtime 里各 Manager / 执行器的句柄，缓存和任务表由它们自己持有，别再往 AppState 上加；上面那份设置副本 `app_settings` 只经 `ncd_runtime::desktop` 的函数改

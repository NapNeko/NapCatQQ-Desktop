# Codemap:功能域 → 代码落点

> agent 接手任务时先查这张表,锁定功能域涉及的目录/文件,再按 `.claude/code-search-workflow.md`（codesearch → 闭包内 Grep/Glob → Read）精确定位。
> 粒度只到文件级(目录结构稳定),不写行号/符号(高频变动易过期)。新增功能域随手补一行。
> 本地语义索引（codesearch）流程见 `.claude/code-search-workflow.md`、`.claude/codesearch-setup.md`。
> 后端能力细节看 capabilities.md,前端分层看 frontend.md。

分层速记:Tauri command(src-tauri 薄壳) → Manager/Service(ncd-runtime 编排) → Trait(ncd-traits) → Domain(ncd-domain 模型)。前端:modules 页面 → hooks 适配 → core/services IPC壳 + core/domain 纯逻辑。

---

## 核心功能域(跨层全链路)

### Bot 生命周期(启停/状态/配置)
- **运行语义(AI)**: `.claude/kb/`（`backend_for_config` 矩阵、NC/SL WebUI 差异；slash `/load-runtime-kb`）
- IPC: `src-tauri/src/commands/bot.rs`(start_bot/stop_bot/upsert_bot_config/batch_* 等)
- 编排: `ncd-runtime/src/bot_manager.rs`(多 Bot 管理,最多 4)+ `bot_actor.rs`(6 状态机+级联取消)
- 配置: `ncd-runtime/src/{bot_config_repo_impl,backend_config_renderer,config_drift}.rs`
- 模型: `ncd-domain/src/bot_config.rs`(BotConfig+validate)+ `ncd-traits/src/bot_config_repo.rs`
- 前端: `modules/bot/`(BotPage/BotListPage/BotConfigPage/BotLogPage .next)+ `hooks/bot/*` + `core/services/bot.service.ts` + `core/domain/bot/*`

### Docker 部署
- IPC: `src-tauri/src/commands/docker.rs`(docker_probe/deploy/list_containers/container_action/logs)
- 编排: `ncd-deploy/src/docker/compose.rs`(compose 渲染)+ `deployments/docker.rs`
- 模型: `ncd-domain/src/docker.rs`(DockerProbeResult/DeployedContainer)
- 前端: `modules/docker/`(DockerPage/ContainerCard/DeployDialog/DockerToolbar)+ `modules/components/{DockerRow,FrameworkDockerDeploy}.tsx` + `hooks/docker/*` + `core/services/docker.service.ts` + `core/domain/docker/{spec,status}.ts`

### 远端 SSH 主机(连接/档案/免密)
- IPC: `src-tauri/src/commands/servers.rs`(add/update/delete_server/setup_server_key_auth/scan_local_ssh_keys)+ `host_resolve.rs`(autoconnect 单飞)+ `mod.rs`(connect_remote_host/list_remote_files)
- 编排: `ncd-runtime/src/server_manager.rs`(档案 CRUD+keyring+连接锁)+ `ssh_keygen.rs`(ed25519 生成)
- 主机层: `ncd-host/src/remote/linux.rs`(russh+SFTP 复用)+ `remote/{credentials,host_key,tunnel}.rs`
- 前端: `modules/remote/`(RemoteHostPanel/ServerCard/AddServerDialog)+ `hooks/remote/{useServerManager,useRemoteSession}.ts` + `core/services/{remote,server}.service.ts`

### 组件安装(NodeJs/QQ/NapCat/SnowLuma/noVNC/Desktop)
- IPC: `src-tauri/src/commands/components.rs`(list/detect/run/cancel_component_action)
- 组件实装: `ncd-component/src/{nodejs,qq,napcat,snowluma,novnc,desktop_self}.rs` + `download.rs`(多镜像 race+SHA256)
- 部署编排: `ncd-deploy/src/{plan,runner}.rs`(Component×Host 三元组+回滚)
- 前端: `modules/components/`(ComponentsPage/HostComponentsView/MachineComponentRow/HostSwitcher)+ `hooks/components/*`(含 componentActionStore)+ `core/services/component.service.ts` + `core/domain/components/{progress,types}.ts`

### NapCat 后端(WebUI/扫码登录)
- 子系统: `ncd-runtime/src/napcat/{webui_client,login_poller,offline_notifier,endpoint_table}.rs`
- 前端: `hooks/webui/{useNapcatLogin,useOpenWebui,napcatLoginStore}.ts` + Bot 列表里的 QrCodeDialog

### SnowLuma 后端(daemon/状态轮询)
- IPC: `src-tauri/src/commands/snowluma.rs`(list_qq_processes/open_snowluma_webui/probe_qq_login_info)
- 子系统: `ncd-runtime/src/snowluma/{daemon,status_poller,webui_client,proc_tree,session,qq_login_probe,runtime_backend}.rs`
- 远端 Native（无 launcher 脚本、无内联 mega-shell）: `ncd-runtime/src/{remote_snowluma.rs,remote_snowluma_stack.rs,remote_snowluma_orchestrator.rs,remote_snowluma_layout.rs,remote_snowluma_tunnel.rs}`（stack 分步 detach + `QQComponent` 冷启 + SSH 隧道 47099/47609）
- 前端: `hooks/webui/{useSnowlumaState,snowlumaStore}.ts`

### 事件流(后端推 → 前端聚合)
- 后端: `ncd-runtime/src/events.rs`(BroadcastEventBus + 所有 DomainEvent variant)
- 前端: `core/services/event-stream.service.ts`(DOMAIN_EVENT_NAMES 单一来源)+ `core/domain/events/*`(各 aggregator)+ `hooks/events/useDomainEvents.ts`

### 配置迁移(Python 旧版 → Rust)
- `ncd-runtime/src/{app_config_migration,bot_config_migration,migration,legacy_discovery}.rs`
- 模型: `ncd-domain/src/app_config.rs` + `ncd-traits/src/migration_step.rs`

### 自更新 + 版本检查
- 自更新: `ncd-update/src/{provider,orchestrator,channel}.rs`
- 版本: `src-tauri/src/commands/release.rs` + `ncd-runtime/src/release.rs`(GitHub API+1h缓存)+ `core/services/release.service.ts`

### 启动引导(bootstrap/数据目录/资源监控)
- IPC: `src-tauri/src/commands/mod.rs`(get_bootstrap_status/open_data_dir/export_migration_report)
- 模型: `ncd-domain/src/bootstrap.rs` + 路径探测 `ncd-runtime/src/path_probe_impl.rs`
- 前端: `modules/bootstrap/`(BootstrapPanel+OccupancyChart)+ `hooks/bootstrap/useBootstrap.ts` + `hooks/diagnostics/useResourceMonitor.ts`

### Desktop 会话日志(设置页)
- 写盘: `src-tauri/src/desktop_log.rs`(tracing layer + panic + `write_session_line`)
- 行格式: `crates/ncd-log/`(六段 legacy 兼容)
- 读盘/过滤: `ncd-runtime/src/desktop_log.rs` + IPC `src-tauri/src/commands/desktop_log.rs`
- 崩溃包: `ncd-runtime/src/crash_bundle.rs` → `<data_root>/output/crash_*.zip`
- 前端: `modules/settings/tabs/DesktopLogTab.tsx` + `hooks/diagnostics/useDesktopLogViewer.ts`(轮询 tail)

### 退出 / 轻量模式 / 托盘
- 退出门控 UI: `src-ui/app/DesktopExitGate.tsx` + `core/services/exit.service.ts`（挂于 `app/AppNext.tsx`）
- IPC: `src-tauri/src/commands/exit.rs` + `commands/tray.rs`
- 轻量: `src-tauri/src/lightweight.rs` + `lightweight_scheduler.rs`；设置 `modules/settings/tabs/AppearanceTab.tsx`
- 通知: `src-tauri/src/desktop_notify.rs` + `windows_toast.rs`；托盘摘要 `tray_summary.rs`
- 编排: `ncd-runtime/src/bot_manager.rs`（退出前停 Bot 等，见活 plan `remote-bot-detach-and-reconcile.md`）

### 网络下载(被组件/snowluma 复用)
- `ncd-network/src/*`:`client`(共享 reqwest)/`download`(续传)/`chunked`(切片)/`race`(多镜像竞速)/`mirror`(6 GitHub 镜像)/`progress`/`speed`

---

## 单侧目录速查

后端 crate 职责:`ncd-domain`(模型)·`ncd-traits`(契约)·`ncd-runtime`(编排,最重 35 文件)·`ncd-host`(主机抽象)·`ncd-component`(6 组件)·`ncd-deploy`(部署编排)·`ncd-update`(自更新)·`ncd-network`(下载)·`ncd-test-support`(Mock+fixture)。

前端原子 UI: `shared/ui/*`(Button/Card/Badge/Dialog/Tabs/Select/TextField 等)。AppShell: `shared/components/next/{CustomTitleBar,Sidebar,StatusBar,Mascot}.tsx`。主壳+路由: `app/AppNext.tsx`。

模块级 store(跨路由保留): `hooks/webui/{napcatLoginStore,snowlumaStore}.ts` · `hooks/ui/globalInfoBarStore.ts` · `hooks/components/componentActionStore.ts` · `hooks/preferences/preferencesStore.ts`。工厂 `hooks/utils/createStore.ts`。

Tauri AppState(`src-tauri/src/lib.rs`): 持有 bot_manager / server_manager / event_bus / active_tasks / host_probe_cache。

---

## 全链路切片示例(读一个就会套用)

以"Docker 部署对话框点击部署"为例,从前到后的文件串:

1. 用户在 `modules/docker/DeployDialog.tsx` 填端口点部署
2. 调 `hooks/docker/useDocker.ts` 暴露的 deploy 方法
3. → `core/services/docker.service.ts` 发 `docker_deploy` IPC(端口校验规则在 `core/domain/docker/spec.ts`)
4. → `src-tauri/src/commands/docker.rs` 的 docker_deploy command
5. → `ncd-deploy/src/docker/compose.rs` 渲染 compose
6. → `ncd-host/src/remote/linux.rs` 在远端执行 docker 命令
7. 进度/结果通过 `ncd-runtime/src/events.rs` 事件回推 → 前端 `event-stream.service.ts` → 全局 InfoBar

任何跨层功能都是这条路径的变体:modules → hooks → services(+domain 校验) → commands → 编排 crate → host/component → 事件回推。

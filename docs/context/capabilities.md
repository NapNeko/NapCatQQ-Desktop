# 后端已就绪能力速查

> 从原 .claude/CLAUDE.md §8.1-8.8 拆出。前端能力在 frontend.md,踩坑在 lessons.md。
> 规划任何新功能前先 ls 各 crate 的 src/ 看现有模块,再决定新增还是扩展。不要绕过,不要重发明。

ls 入口：`crates/ncd-runtime/src/`、`crates/ncd-domain/src/`、`crates/ncd-traits/src/`、`crates/ncd-host/src/`、`crates/ncd-component/src/`

---

## ncd-domain（Layer 1，零运行时依赖）

`ids` / `kinds` / `errors` / `models` / `report` / `bootstrap` / `app_config` / `bot_config`（含 `BotBasicConfig.snowluma_start_mode`）/ `snowluma_start_mode`。M2.1 / M2.2 搬过去。

## ncd-traits（Layer 2，接口契约）

7 个 trait：`ConfigStore` / `BotBackend` / `RemoteHost` / `SecretStore` / `EventBus` / `ProgressReporter` / `RenderError`（其他在 traits/ 子目录里）。M2.3 搬过去。

## ncd-runtime（Layer 3，原 ncd-core，M6.1 改名）

- `ConfigStore` 实现 + `JsonTransaction` / `apply_transaction` 原子写入 + 自动备份
- `BotActor` 6 状态机 + `mpsc` / `watch` / `oneshot` / `CancellationToken` 状态机 + 级联取消
- `BroadcastEventBus` + `EventFilter` 事件广播
- `BotBackend` 实现：`LocalRuntimeBackend` / `RemoteRuntimeBackend` 多态后端
- `MockRemoteHost` SSH 抽象 + 测试隔离
- `bot_config_migration.rs` schema v1.4 → v2.1 迁移
- `napcat/` 子目录（M1 内部对称）：`login_poller.rs`（2658 行）/ `webui_client.rs`（946 行）/ `offline_notifier.rs`（167 行）
- `snowluma/` 子目录（spec 落地）：`error.rs` / `launch_plan.rs` / `log_sanitize.rs`（ANSI CSI 字节状态机剥离）/ `proc_tree.rs`（sysinfo 实装 + Mock）/ `session.rs`（强密码 + scrypt + runtime.json / webui.json 渲染）/ `webui_client.rs`（8 wire payload + Reqwest 实装 + host probing + no_proxy + 401 重试 + 15s 注入超时）/ `daemon.rs`（5 状态机全局单例 + watch_exit + recent log ring buffer）/ `status_poller.rs`（500ms 启动延迟 + 2s tick + UIN 严格锁定 + 4 档状态合成）/ `runtime_backend.rs`（Phase A/C/D + zombie reaper + Windows kill_process_tree）
- 6 个 SnowLuma `DomainEvent` variant（`#[serde(rename = "snowluma_xxx")]`）+ helper 构造器 + 跨边界字面量一致性测试

M6.1 大坑（已知架构问题）：`BotManager` 持有 backend-specific 字段（`webui_client` / `offline_notifier` / `login_pollers`）。如果后续要拆 `ncd-backend-napcat` / `ncd-backend-snowluma` crate，会产生反向依赖（违反 R11）。需要先重构 BotManager 移除 backend-specific 字段（独立 spec，≥1 天）。M6.2-M6.4 暂缓。

## ncd-host（M3.1-M3.4）

- `Host` / `HostShell` / `PackageManager` trait + `HostPath` / `HostCommand`（M3.1）
- `LocalWindowsHost`：`tokio::fs` + `tokio::process` + `zip`（只 deflate）+ `tar` + `flate2`（M3.2）
- `RemoteLinuxHost`：`russh = 0.45` + `russh-sftp = 2.3` + 隧道 + keepalive + `copy_bidirectional`（M3.3）
- `RemoteWindowsHost` stub：17 个方法全 `unimplemented!` 返回 `Unsupported`（M3.4）

真机测试约束（不准破坏 175.178.53.24 的 NapCat 业务）：禁 `rm` / `mv` 业务目录；测试只在 `/tmp/ncd-host-test-<pid>-<rand>/`；不杀已有进程；`Drop` 自动清理；不改 `~/.bashrc` / `/etc/*` / `~/.ssh/authorized_keys`。测试默认 `#[ignore]`，3 个 env var 触发：

    $env:NCD_TEST_SSH_HOST = "175.178.53.24"
    $env:NCD_TEST_SSH_USER = "ubuntu"
    $env:NCD_TEST_SSH_KEY  = "$env:USERPROFILE\.ssh\id_rsa"
    cargo test -p ncd-host --test remote_linux_smoke -- --ignored --test-threads=1

## ncd-component（M4，6 个 component）

- `NodeJsComponent`（tar.xz 解压，安装目录探测 + PATH 回退）
- `LinuxQQComponent`（锁定 v3.2.25-45758 + hash 7516007c，rootless dpkg-deb -x / rpm2cpio + cpio -idm）
- `NoVncComponent`（apt / dnf 装 Xvfb / fluxbox / x11vnc / novnc / websockify）
- `NapCatComponent`（NapCat.Shell.zip + cp 到 `app_launcher/napcat/` + 写 `loadNapCat.js` + patch QQ `package.json` main）
- `SnowLumaComponent`（lite tarball + 6 镜像 fallback + tar -xzf --strip-components=1）
- `DesktopSelfComponent`（install/update 返回 Unsupported 走 ncd-update，只提供 detect/launch_command）

共享设施：`ActionCtx`（`ProgressEvent` 带 `v: u32` envelope 符合 R14）+ `DownloadHelper`（HTTP 流式 + SHA256 + 失败自动删 + 取消支持）+ `Component` trait + `Action` trait。

## ncd-deploy + ncd-update（M5）

- `DeployPlan` / `DeployStep` / `StepKind`（EnsureInstalled / ForceInstall / Update / Uninstall / Verify）+ `DeployBuilder` 链式 API + `fail_fast` / `rollback_on_failure`
- `UpdateChannel`（stable / beta / nightly）/ `AvailableUpdate` / `PrecheckReport` / `RecordedFailure`
- `UpdateProvider` trait + `MockUpdateProvider`（fail injection）+ wiremock 5 用例
- `UpdateResumePoint`（running_bots + snowluma_daemon_running）+ `ResumeStore`（JSON 持久化 + clear）
- 5 个 orchestrator 方法：`check` / `precheck` / `install_with_graceful_shutdown` / `resume_after_update` + `clear_resume` / `record_failure` + `detect_pending_failures`（JSONL）

## ncd-test-support（M6.5 接入完成）

- `MockSecretStore` 真正实装 `SecretStore` trait（`MockSecretStoreError::InjectedFailure` → `SecretError::Unavailable`），可注入到任何接口
- `fail_next_*` API（`fail_next_put` 等）实际用上
- `TempWorkspace` 跨 crate 复用（M3 / M4 / M5 / M6 测试统一），替代 `tempfile::tempdir().unwrap`
- 6 个 crate 引用（5 dev-dep + 自己 lib test）
- `fixtures/legacy/{config,bot,servers}.json` 三件套（当前字段太少，无法覆盖 `tests/config_migration.rs` 30+ 嵌套字段场景，留待补全）

## Tauri 集成

- 已注册插件：`tauri-plugin-opener`（`opener:default` + `opener:allow-open-url`）
- SnowLuma 4 Tauri commands：`list_qq_processes` / `set_snowluma_attach_pid` / `set_snowluma_password_override` / `open_snowluma_webui`
- `ncd-update` 必须有 `build.rs` 嵌入 manifest 声明 `asInvoker` 否则 Windows UAC（os error 740）

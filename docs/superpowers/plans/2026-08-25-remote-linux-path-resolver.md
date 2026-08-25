# 远端 Linux Native 路径解析器 Implementation Plan

> **For agentic workers:** 本会话 inline 执行。规格：`docs/superpowers/specs/2026-08-25-remote-linux-path-resolver-design.md`。

**Goal:** 远端 Linux Native 的启动、安装、门禁只消费 `RemoteSelectedPaths` 的派生路径，禁止再拼 `{home}/Napcat`。

**Architecture:** 路径模板与 join/反推放在 `ncd-domain::remote_paths`（backend 不能依赖 runtime）。启动走 `require_*`（缺则错）；安装才用 `desktop_default_install_paths`。库存探测写入 `snowlumaLinuxPackage`，前端不再平行推断。

**Tech Stack:** Rust workspace、ts-rs、Vitest。

## Global Constraints

- 只改远端 Linux Native；本机 Windows、Docker 不改。
- `ncd-domain` 零 I/O、零 Host。
- `servers.json` 不升 schema；新字段 `serde(default, skip_serializing_if)`。
- `{home}/Napcat` 只允许出现在 domain 默认安装函数、测试、UI 说明文案。
- 发现阶段不改 `package.json` main。

---

### Task 1: domain 解析器

**Files:**
- Create: `crates/ncd-domain/src/remote_paths.rs`
- Modify: `crates/ncd-domain/src/lib.rs`, `crates/ncd-domain/src/remote_inventory.rs`

**Produces:** `join_under`, `derive_remote_linux_paths`, `desktop_default_install_paths`, `require_qq_install_base` / `require_snowluma_dir` / `require_napcat_root`, 迁入的反推函数，`RemoteLinuxDerivedPaths`，`RemoteInventory.snowluma_linux_package`。

- [ ] 单测覆盖 spec「测试」节纯函数条目
- [ ] 实现模块并 `cargo test -p ncd-domain`

### Task 2: runtime 改调 domain

**Files:** `crates/ncd-runtime/src/remote/inventory.rs`, `crates/ncd-runtime/src/components/action_policy.rs`, `crates/ncd-runtime/src/lib.rs`

- [ ] 删除重复纯函数，re-export domain
- [ ] `select_paths` 用 `join_qq_bin`；`inventory_from_stdout` 写入 `snowluma_linux_package`
- [ ] `cargo test -p ncd-runtime --lib`

### Task 3: SnowLuma 启动用 `qq_install_base`

**Files:** `crates/ncd-backend-snowluma/src/remote_snowluma/{layout,backend,orchestrator}.rs`, `crates/ncd-runtime/tests/remote_snowluma_orchestrator.rs`

- [ ] `RemoteSnowLumaLayout.qq_install_base`
- [ ] 冷启动补丁与 spawn 使用该字段
- [ ] `cargo test -p ncd-backend-snowluma --lib`

### Task 4: NapCat 删除默认树回落

**Files:** `launch.rs`, `session.rs`, `native_deployment_adapter/remote.rs`

- [ ] 无 selected 不得拼 `{home}/Napcat`；用 `require_qq_install_base` 或库存 selected

### Task 5: 工厂 + `remote_qq_entry`

**Files:** `factory.rs`, `remote_qq_entry.rs`

- [ ] 安装缺字段才 `desktop_default_install_paths`
- [ ] package.json 路径走 domain join

### Task 6: 前端 + ts-bindings

**Files:** `remote-direct-run-deps.ts`, `runtime-gate.ts` / tests, generated types

- [ ] `inferSnowLumaLinuxPackageFromInventory` 读库存字段
- [ ] `pnpm run ts-bindings`；`pnpm run test:unit` 相关文件

### Task 7: Grep 收口

启动路径不得再有运行时 `{home}/Napcat` 拼接（默认函数 / 测试 / UI 文案除外）。

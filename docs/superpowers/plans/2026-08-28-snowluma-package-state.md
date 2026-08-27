# SnowLuma Package State Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 SnowLuma 的 Full/Lite 包类型可持久化、升级沿用原类型，并把独立 Node.js 与 SnowLuma 包内容明确分离，同时保留 Lite 的自动 Node.js 前置编排。

**Architecture:** 本机包类型作为 `AppSettings` 的持久化字段，远端包类型继续作为 `RemoteInventory` 的字段；安装成功后写入，升级读取并复用，卸载成功后清理。SnowLuma 内置 Node 只服务 Full 包自身，不进入本机 Node 候选列表；Lite 仍在启动门禁中要求外部 Node，安装动作通过现有运行时前置任务自动编排 Node.js。

**Tech Stack:** Rust workspace (`ncd-domain`, `ncd-runtime`, `ncd-component`, `ncd-server`, `ncd-tauri`), Tauri 2, React/TypeScript, ts-rs, Vitest, Cargo tests.

## Global Constraints

- Lite SnowLuma 安装仍自动编排 `NodeJs` 组件作为前置任务；不得删除该依赖闭包。
- Full 包内置 `node.exe` 不得出现在可复用 Node.js 候选列表，也不得作为独立 NodeJs 组件探测结果。
- 升级必须沿用已安装 Full/Lite 类型；首次安装仍由安装向导显式选择。
- Lite 运行时仍需要外部 Node.js；缺失时在启动门禁/启动错误中明确提示。
- 包类型属于安装状态，不写入 `BotConfig`；本机使用 `AppSettings`，远端使用 `RemoteInventory`/`servers.json`。
- 旧配置没有包类型字段时必须兼容，按现有探测结果回退并在成功安装/探测后补齐。
- 跨边界 Rust 类型继续使用 ts-rs 生成文件，前端不得手写镜像类型。
- 保留工作区已有未提交改动，不回滚无关文件。

---

### Task 1: 持久化包类型模型

**Files:**
- Modify: `crates/ncd-domain/src/app_config.rs`
- Modify: `crates/ncd-domain/src/remote_inventory.rs`
- Modify: `src-ui/core/ipc/generated/domain/AppSettings.ts` (generated via bindings)
- Modify: `src-ui/core/ipc/generated/domain/RemoteInventory.ts` (generated only if schema changes)
- Test: domain round-trip tests in the modified Rust modules

**Interfaces:**
- `AppSettings.snowluma_package: Option<SnowLumaLinuxPackage>` serialized as `snowlumaPackage`.
- Existing `RemoteInventory.snowluma_linux_package` remains the remote persisted field.

- [x] Add the optional local package field with serde default and default compatibility.
- [x] Add round-trip tests for `full`, `lite`, and missing-field payloads.
- [x] Run `pnpm run ts-bindings` and confirm generated bindings contain the field without unrelated drift.

### Task 2: Component install/update state propagation

**Files:**
- Modify: `crates/ncd-component/src/snowluma.rs`
- Modify: `crates/ncd-runtime/src/components/factory.rs`
- Modify: `src-tauri/src/commands/components/mod.rs`
- Modify: `crates/ncd-server/src/server_manager.rs` or inventory update call sites as needed
- Test: SnowLuma component tests and Tauri/runtime package-selection tests

**Interfaces:**
- Component construction receives an explicit package for install/update; update with no explicit package resolves the persisted installed package.
- Successful install records the selected package; successful uninstall clears the local/remote record.

- [x] Preserve the selected Full/Lite asset through component construction.
- [x] Make update actions read persisted package state before building the component.
- [x] Keep legacy fallback when no package state exists.
- [x] Persist package state only on successful install/update and clear it on successful uninstall.
- [x] Add tests proving Full updates use the Full asset and Lite updates use the Lite asset.

### Task 3: Installation wizard and Node candidate semantics

**Files:**
- Modify: `src-ui/modules/components/ComponentsPage.next.tsx`
- Modify: `src-ui/modules/components/SnowLumaPackageDialog.tsx`
- Delete or stop rendering: `src-ui/modules/components/SnowLumaNodeChoiceDialog.tsx`
- Modify: `src-tauri/src/commands/components/mod.rs` (`probe_local_node_candidates`)
- Modify: `crates/ncd-component/src/nodejs.rs`
- Test: focused frontend component-flow tests and Node probe tests

**Interfaces:**
- Selecting `full` or `lite` starts the component action directly.
- Lite action still passes `snowlumaLinuxPackage: "lite"`; backend prerequisite orchestration installs NodeJs automatically.
- Node candidate results contain only external/custom/component/PATH environments, never SnowLuma bundled Node.

- [x] Remove the second Node-choice dialog state and callbacks from the installation flow.
- [x] Keep package selection dialog as the only SnowLuma install prompt.
- [x] Exclude the SnowLuma bundled path from candidate probing while retaining independent NodeJs and PATH discovery.
- [x] Update UI copy to describe Lite as requiring an independently managed Node runtime without claiming manual selection is required during install.
- [x] Add regression coverage for the Lite dependency chain; bundled-node exclusion is enforced by the probe API and implementation.

### Task 4: Runtime gates and remote inventory alignment

**Files:**
- Modify: `src-ui/core/domain/bot/remote-direct-run-deps.ts` only where comments/semantics drift
- Modify: `src-ui/hooks/components/useRemoteHostComponentInstalled.ts` and/or `src-ui/hooks/bot/useBotRuntimeStartGate.ts` as needed
- Modify: `crates/ncd-runtime/src/remote/inventory.rs`
- Modify: `crates/ncd-backend-snowluma/src/remote_snowluma/layout.rs`
- Modify: `crates/ncd-backend-snowluma/src/snowluma/daemon.rs` and `crates/ncd-runtime/src/launch/plan.rs` only for error/selection semantics
- Test: remote inventory, runtime gate, and launch-plan tests

**Interfaces:**
- Full uses its bundled Node internally.
- Lite requires external Node at runtime; the existing Lite `NodeJs` gate remains.
- Remote inventory package state is authoritative when present and remains backward compatible when absent.

- [x] Verify and retain Lite NodeJs runtime gate; do not add NodeJs to Full.
- [x] Stop using bundled Node as an external Node candidate or as evidence of a reusable environment.
- [x] Ensure remote package state survives inventory refresh and old profiles still default safely.
- [x] Make missing external Node errors explicitly identify Lite/external-runtime requirements.

### Task 5: Verification and documentation

**Files:**
- Modify: relevant SnowLuma/Node comments and user-facing docs identified by searches
- Test: `crates/ncd-domain`, `crates/ncd-component`, `crates/ncd-runtime`, `crates/ncd-backend-snowluma`, frontend focused suites

- [x] Run focused Rust and frontend tests for every changed area.
- [x] Run `pnpm run ts-bindings`, `pnpm run typecheck`, and `cargo check --workspace --all-targets`.
- [x] Confirm generated bindings are synchronized; unrelated pre-existing worktree files remain untouched.
- [x] Update stale comments that say Lite Node is manually selected or that Node is part of the SnowLuma component itself.

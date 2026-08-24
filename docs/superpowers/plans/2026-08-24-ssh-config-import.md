# SSH Config Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 远端页可从本机 `~/.ssh/config` 勾选批量导入 ServerProfile。

**Architecture:** `ncd-server` 自写 OpenSSH 子集解析；`ServerManager` 结合已有档案去重；Tauri 只读命令 `discover_local_ssh_hosts`；前端导入弹窗循环调用现有 `add_server`。连接层不改。

**Tech Stack:** Rust (ncd-server) + ts-rs + Tauri command + React Dialog

## Global Constraints

- 跨边界类型用 ts-rs，前端只 re-export generated。
- Tauri command 薄壳，不写业务。
- 不读私钥内容；不把 config 当 SSH 客户端。
- 跳板 / 通配不可导入；去重键为 host+port+username（host 大小写不敏感）。
- 生产路径禁止 unwrap/expect。

---

### Task 1: OpenSSH 子集解析

**Files:**
- Create: `crates/ncd-server/src/ssh_config.rs`
- Modify: `crates/ncd-server/src/lib.rs`
- Modify: `crates/ncd-server/Cargo.toml`（`dirs`）

**Interfaces:**
- Produces: `discover_ssh_hosts(config_path, home, existing, default_user) -> Result<Vec<DiscoveredSshHost>, String>`
- Produces: `DiscoveredSshHost`（ts-rs camelCase）

- [ ] 解析器 + 单测（缺失 config、Host* 合并、Include、通配/跳板灰掉、first-match-wins、~ 展开、去重）
- [ ] `cargo test -p ncd-server --lib ssh_config`

### Task 2: ServerManager + Tauri IPC

**Files:**
- Modify: `crates/ncd-server/src/server_manager.rs`
- Modify: `crates/ncd-runtime/src/lib.rs`（re-export）
- Modify: `src-tauri/src/commands/servers.rs`
- Modify: `src-tauri/src/lib.rs`（generate_handler）
- Modify: `package.json` `ts-bindings` 加入 ncd-server

- [ ] `ServerManager::discover_local_ssh_hosts`
- [ ] `discover_local_ssh_hosts` command
- [ ] `pnpm run ts-bindings` 提交 generated

### Task 3: 导入 UI

**Files:**
- Create: `src-ui/modules/remote/ImportSshConfigDialog.tsx`
- Modify: `src-ui/modules/remote/RemoteHostPanel.next.tsx`
- Modify: `src-ui/core/services/server.service.ts`
- Modify: `docs/context/codemap.md`

- [ ] 弹窗：加载列表、勾选、灰掉原因、批量 add、汇总 InfoBar
- [ ] 远端页头部 + 空状态入口
- [ ] `pnpm run typecheck`

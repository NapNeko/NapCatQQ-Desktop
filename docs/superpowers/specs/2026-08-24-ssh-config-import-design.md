# 从本机 SSH 配置导入远端主机

日期：2026-08-24  
状态：已批准，按此实现

## 问题

远端页添加服务器只能手填。已有 `scan_local_ssh_keys` 只扫 `~/.ssh/` 标准私钥名，不读 `~/.ssh/config` 的 Host。连接层（russh）也不走 OpenSSH 配置。

## 产品

远端页提供「从 SSH 配置导入」：

1. 读用户 `%USERPROFILE%\.ssh\config`（含 `Include`）。
2. 列出 Host；可勾选的默认勾上。
3. 通配别名、`ProxyJump` / `ProxyCommand`、已按 `host+port+username` 存在的档案：列出但灰掉，并写原因。
4. 确认后逐条走现有 `add_server`。密钥只存路径，不读私钥内容。无 `IdentityFile` 则为密码认证、不存密码。
5. 导入后不测连接。

`Host *` 不单独列出，只合并进具名 Host。不读系统级 ssh_config、PuTTY、ssh-agent。不支持跳板与 `Match exec`。

## 分层

- 解析与发现：`crates/ncd-server/src/ssh_config.rs`
- 编排：`ServerManager::discover_local_ssh_hosts`
- Tauri 薄壳：`discover_local_ssh_hosts`（只读）
- 写入：现有 `add_server`，不新开写路径
- UI：`ImportSshConfigDialog` + 远端页入口

## 解析子集

关键字：`Host`、`HostName`、`User`、`Port`、`IdentityFile`、`Include`、`ProxyJump`、`ProxyCommand`。

- `Include`：相对路径相对被包含文件所在目录；`~` 展开；文件名 glob；循环跳过；缺失文件忽略。
- 每个参数取第一次命中（OpenSSH）；`IdentityFile` 可多条，取第一个已存在文件，否则第一条路径。
- `Match` 及随后关键字丢弃，直到下一个 `Host` / `Include`。
- 缺 `User` 用本机用户名（`USERNAME` / `USER`）。缺 `HostName` 用别名。缺 `Port` 为 22。

## IPC 类型

`DiscoveredSshHost`：`alias`、`host`、`port`、`username`、`identityFile`、`authMethod`、`identityFileMissing`、`selectable`、`skipReason`、`alreadyAdded`。

## 不做

跳板连接、系统级 config、导入后自动测连、改 `scan_local_ssh_keys`。

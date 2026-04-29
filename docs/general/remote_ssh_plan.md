# NapCatQQ Desktop 远程方案规划（v2 — VS Code Remote 模型）

> **修订说明**：本文档是对原 v1 规划的全面重写。  
> 原 v1 规划在推进过程中偏离了产品初衷，演变成了独立 Go Daemon + JSON-RPC 双模式架构，  
> 与"透明远程体验"的目标背道而驰。本次重写回归用户真实需求。

---

## 1. 产品目标

**一句话**：让 NapCatQQ Desktop 像 VS Code Remote SSH 一样——本地是完整的桌面 UI，远端是透明的执行环境，用户不需要区分"本地操作"和"远程操作"。

### 1.1 用户故事

1. 用户打开 Desktop，在"服务器管理"中添加一台 Linux 服务器（填 SSH 信息）
2. Desktop 自动通过 SSH 在远端安装 NapCat Core（类似 VS Code 自动安装 vscode-server）
3. 用户添加 Bot 账号时，选择运行位置：**本地** 或 **某台远程服务器**
4. 选定后，所有操作（安装/更新 NapCat、配置编辑、启动/停止、日志查看、登录/二维码）与本地体验完全一致
5. 理想状态下支持自动切换（如本地不可用时自动尝试远程）

### 1.2 非目标

- **不引入独立常驻服务/Daemon**：NapCat 本身就是远端运行的服务，不需要额外的 agent
- **不自建通信协议**：不需要 JSON-RPC / WebSocket / 自定义协议
- **不搞多模式切换**：用户不需要理解"SSH 模式"和"Agent 模式"的区别

---

## 2. 核心设计：操作抽象层

### 2.1 设计思路

现有本地模式的所有 Bot 管理操作都依赖两类底层能力：**文件 I/O** 和 **进程管理**。  
远程化的本质是：为这两类能力提供一个远程实现，上层代码（UI、Bot 管理）无感切换。

```
┌─────────────────────────────────────────────┐
│  UI 层（BotPage / ConfigPage / LogPage）     │
│  Bot 管理（operate_config / 进程管理器）      │
└──────────────┬──────────────────────────────┘
               │
       OperationBackend（抽象接口）
               │
       ┌───────┴───────┐
       │               │
  LocalBackend    RemoteBackend
  （现有逻辑）     （SSH/SFTP 实现）
```

### 2.2 抽象接口定义

`OperationBackend` 需要覆盖的操作：

| 分类      | 方法                         | 本地实现                            | 远程实现                                  |
| --------- | ---------------------------- | ----------------------------------- | ----------------------------------------- |
| **文件**  | `read_file(path)`            | `Path.read_text()`                  | SFTP get                                  |
| **文件**  | `write_file(path, content)`  | `Path.write_text()`                 | SFTP put                                  |
| **文件**  | `file_exists(path)`          | `Path.exists()`                     | SFTP stat                                 |
| **文件**  | `list_dir(path)`             | `Path.iterdir()`                    | SFTP listdir                              |
| **文件**  | `mkdir(path)`                | `Path.mkdir()`                      | SFTP mkdir                                |
| **文件**  | `remove(path)`               | `Path.unlink()` / `shutil.rmtree()` | SFTP remove                               |
| **文件**  | `upload(local, remote)`      | 不需要                              | SFTP put                                  |
| **文件**  | `download(remote, local)`    | 不需要                              | SFTP get                                  |
| **进程**  | `start_napcat(config)`       | `QProcess` 启动 .exe                | SSH: 执行启动脚本                         |
| **进程**  | `stop_napcat(qq_id)`         | `psutil.kill()`                     | SSH: kill 远端进程                        |
| **进程**  | `get_process_status(qq_id)`  | 检查 QProcess 状态                  | SSH: pgrep / PID 文件                     |
| **进程**  | `get_memory_usage(qq_id)`    | `psutil.memory_info()`              | SSH: 读 /proc 或 ps                       |
| **安装**  | `install_napcat(archive)`    | 解压 zip                            | SSH: 下载 + 解压                          |
| **安装**  | `install_qq()`               | 运行 .exe 安装器                    | SSH: 下载 deb + dpkg                      |
| **安装**  | `detect_napcat_version()`    | 读本地文件                          | SSH/SFTP: 读远端文件                      |
| **安装**  | `detect_qq_path()`           | Windows 注册表                      | SSH: 检测 Linux 安装路径                  |
| **日志**  | `read_log(qq_id)`            | QProcess stdout                     | SSH: tail 日志文件                        |
| **日志**  | `tail_log(qq_id, lines)`     | 内存 deque                          | SSH: tail -n                              |
| **WebUI** | `get_webui_url(port, token)` | `http://localhost:{port}`           | `http://{remote_host}:{port}` 或 SSH 隧道 |

### 2.3 路径抽象

本地模式使用 `PathFunc` 管理路径（Windows 风格）。远程模式需要一个对应的 `RemotePathFunc`：

| 路径            | 本地 (Windows)                  | 远程 (Linux)                                             |
| --------------- | ------------------------------- | -------------------------------------------------------- |
| NapCat 安装目录 | `{data_path}/runtime/NapCatQQ/` | `$HOME/Napcat/opt/QQ/resources/app/app_launcher/napcat/` |
| 配置目录        | `{data_path}/runtime/config/`   | `$HOME/Napcat/config/`                                   |
| 临时目录        | `{data_path}/runtime/tmp/`      | `$HOME/Napcat/tmp/`                                      |
| QQ 路径         | 注册表查询                      | `$HOME/Napcat/opt/QQ/`                                   |
| 日志目录        | NapCat stdout                   | `$HOME/Napcat/log/`                                      |
| PID 文件        | QProcess.processId()            | `$HOME/Napcat/run/napcat.pid`                            |

> 远端路径沿用 NapCat-Installer 的标准布局 `$HOME/Napcat/`，与官方安装脚本兼容。

---

## 3. 服务器生命周期

### 3.1 添加服务器

```
用户填写 SSH 信息（主机、端口、用户、认证方式）
    │
    ▼
测试 SSH 连接
    │
    ▼
探测远端环境（OS、架构、已有安装）
    │
    ▼
自动部署 NapCat Core ──── 类似 VS Code 安装 vscode-server
    │  1. 检测已有安装（复用）
    │  2. 下载 LinuxQQ（如果没有）
    │  3. 下载 NapCat（如果没有或需要更新）
    │  4. 初始化目录结构
    │  5. 写入启动脚本
    │
    ▼
服务器就绪，可以绑定 Bot
```

### 3.2 服务器状态

每台服务器记录：
- SSH 连接信息（主机、端口、用户、认证方式）
- 部署状态（未部署 / 已部署 / 部署中 / 部署失败）
- NapCat 版本、QQ 版本
- 运行中的 Bot 列表

### 3.3 服务器更新

Desktop 检测到新版 NapCat 时，提示用户更新远端（与本地更新流程对称）。

---

## 4. Bot 与服务器的绑定

### 4.1 Config 模型扩展

在现有 `Config` (即 `BotConfig + ConnectConfig + AdvancedConfig`) 基础上，增加运行位置信息：

```python
class BotConfig(BaseModel):
    name: str
    QQID: str | int
    # ... 现有字段 ...
    
    # 新增：运行位置
    runtime_target: str = "local"  # "local" 或 server_id
```

### 4.2 运行位置选择

- 添加 Bot 时，下拉选择"本地"或已添加的某台服务器
- 已有 Bot 可以迁移运行位置（配置会自动同步到目标）

### 4.3 透明操作

上层代码通过 `runtime_target` 获取对应的 `OperationBackend`：

```python
def get_backend(config: Config) -> OperationBackend:
    if config.bot.runtime_target == "local":
        return LocalBackend()
    else:
        server = server_manager.get(config.bot.runtime_target)
        return RemoteBackend(server.ssh_connection)
```

---

## 5. WebUI API 透传

NapCat 运行后会暴露 WebUI HTTP API（登录状态、二维码、在线状态等）。  
Desktop 现有代码已经在用这些 API（`napcat.py` 中的 `GetLoginStatusRunnable` 等），只是写死了 `localhost`。

### 5.1 远程方案

**方式 A：SSH 端口转发（推荐初期方案）**

Desktop 建立 SSH 隧道，将远端 NapCat WebUI 端口映射到本地：

```
SSH -L {local_port}:localhost:{remote_napcat_port} user@server
```

现有代码中 `http://localhost:{port}` 无需改动，只需把 port 换成隧道的本地端口。

**方式 B：直接连接远端（备选）**

如果远端端口可达，直接将 `localhost` 替换为 `remote_host`。  
需要对 `GetAuthStatusRunnable` / `GetLoginStatusRunnable` 等类做小改动，让 base_url 可配置。

### 5.2 WebUI 端口发现

- 本地模式：从 QProcess stdout 日志中正则匹配端口和 token
- 远程模式：从远端日志文件中读取（SSH: `grep WebUi {log_path}`），或从 NapCat 配置中读取

---

## 6. 安全基线

SSH 是唯一的远程通信通道。相比 v1 自建的 5 层安全架构，SSH 本身就是行业标准的安全远程通信方案：

### 6.1 为什么 SSH 比自建 Daemon 更安全

| 安全维度   | v1 (Daemon)                   | v2 (SSH)                             |
| ---------- | ----------------------------- | ------------------------------------ |
| 传输加密   | 自建 TLS，需自管证书          | SSH 协议内置端到端加密               |
| 身份认证   | 自建 HMAC-SHA256 挑战-响应    | SSH 私钥认证（已被全球验证数十年）   |
| 会话管理   | 自建 JWT（15分钟过期）        | SSH 会话自管理                       |
| 暴露面     | 需开放额外端口 + 自建速率限制 | 仅需 sshd 端口（服务器本就开着）     |
| 中间人攻击 | 自管证书固定                  | SSH 主机指纹校验（已有成熟信任模型） |
| 安全审计   | 自建审计日志系统              | 服务器 sshd 自带审计                 |

结论：SSH 不是“更差的安全”，而是“不需要自己发明安全”。

### 6.2 SSH 凭据策略

- 默认优先私钥认证
- 默认拒绝未知主机指纹（首次连接弹出确认对话框，类似 PuTTY）
- 密码不写入配置文件，可选存入系统 keyring（Windows Credential Manager）
- 支持 SSH agent 转发（可选）

### 6.3 权限边界

- 默认以普通用户运行，不要求 root
- 远端操作仅限 `$HOME/Napcat/` 工作区
- 所有删除操作限定在工作区路径内
- 部署脚本幂等，危险操作需确认

### 6.4 端口安全

- WebUI API 通过 SSH 隧道访问，不需要在远端开放额外端口
- 如果用户选择直连，提示安全风险

---

## 7. 分阶段实施计划

### P0：操作抽象层 + 服务器管理

目标：建立 `OperationBackend` 抽象，实现服务器增删和 SSH 连接管理。

包含：
- 定义 `OperationBackend` 抽象接口
- 实现 `LocalBackend`（封装现有逻辑）
- 实现 `RemoteBackend` 骨架（SSH/SFTP）
- 服务器配置模型和持久化
- 服务器管理 UI（添加/测试/删除服务器）

验收：能添加服务器、测试 SSH 连接、持久化服务器配置。

### P1：远端部署（MVP）

目标：Desktop 能自动在远端安装 NapCat Core。

包含：
- 远端环境探测（OS、架构、已有安装）
- 远端 QQ 安装（下载 deb + 安装）
- 远端 NapCat 安装（下载 + 解压）
- 目录结构初始化
- 部署状态追踪

验收：用户填 SSH 信息后，一键完成远端 NapCat 部署，与本地安装体验对齐。

### P2：远端 Bot 运行闭环

目标：Bot 可以选择在远端运行，操作体验与本地一致。

包含：
- `BotConfig` 增加 `runtime_target` 字段
- 远端启动/停止 NapCat 进程
- 远端配置读写（SFTP）
- 远端日志读取
- SSH 端口转发 + WebUI API 透传（登录状态、二维码）
- Bot 添加/编辑页面增加"运行位置"选择

验收：
- 用户可以在远端服务器上运行 Bot
- 配置编辑、启停、日志、登录二维码全部正常
- 本地模式完全不受影响

### P3：体验优化

目标：打磨远程体验至生产可用。

包含：
- 远端 NapCat 版本检测与更新
- Bot 运行位置迁移（本地 ↔ 远程）
- SSH 连接断线重连
- 多服务器管理
- 远程 Bot 状态在首页展示
- 部署失败回滚

### P4：高级能力（后续评估）

- 自动切换（本地不可用时切远程）
- 批量 Bot 管理
- 远端资源监控（CPU / 内存 / 磁盘）

---

## 8. 与现有代码的关系

### 8.1 保留并重构的模块

| 模块                                          | 用途         | 改造方向                        |
| --------------------------------------------- | ------------ | ------------------------------- |
| `src/desktop/core/remote/ssh_client.py`       | SSH 连接     | 作为 RemoteBackend 的底层       |
| `src/desktop/core/remote/models.py`           | 远程连接模型 | 扩展为服务器配置模型            |
| `src/desktop/core/remote/errors.py`           | 错误类型     | 保留                            |
| `src/desktop/core/remote/deployment.py`       | 部署逻辑     | 重构为 RemoteBackend 的安装方法 |
| `src/desktop/core/remote/status.py`           | 状态查询     | 重构为 RemoteBackend 的状态方法 |
| `src/desktop/core/remote/templates.py`        | 脚本模板     | 保留                            |
| `src/desktop/core/remote/remote_manager.py`   | 连接管理     | 重构为服务器管理器              |
| `src/resource/script/remote_deploy_napcat.sh` | 部署脚本     | 保留并完善                      |

### 8.2 应当移除或归档的模块

| 模块                                          | 原因                    |
| --------------------------------------------- | ----------------------- |
| `src/daemon/` (整个 Go Daemon 项目)           | 不需要独立常驻服务      |
| `src/desktop/core/remote/agent_client.py`     | Daemon 客户端，不再需要 |
| `src/desktop/core/remote/agent_backend.py`    | Daemon 后端，不再需要   |
| `src/desktop/core/remote/jsonrpc_protocol.py` | 自建协议，不再需要      |
| `src/desktop/core/remote/daemon_deployer.py`  | Daemon 部署器，不再需要 |
| `src/desktop/core/remote/daemon_config.py`    | Daemon 配置，不再需要   |

> 建议将这些文件移到 `archive/daemon-v1/` 目录，而非直接删除，以备后续参考。

### 8.3 现有本地代码的改动范围

| 文件                                          | 改动                                                         |
| --------------------------------------------- | ------------------------------------------------------------ |
| `src/desktop/core/runtime/napcat.py`          | `ManagerNapCatQQProcess` 通过 backend 间接操作，本地逻辑不变 |
| `src/desktop/core/config/operate_config.py`   | 文件读写通过 backend 抽象，本地逻辑不变                      |
| `src/desktop/core/config/config_model.py`     | `BotConfig` 增加 `runtime_target` 字段                       |
| `src/desktop/core/installation/installers.py` | 安装逻辑通过 backend 抽象                                    |
| `src/desktop/ui/page/bot_page/`               | Bot 添加/编辑页增加运行位置选择                              |
| `src/desktop/ui/page/remote_page/`            | 重构为服务器管理页（非双模式切换页）                         |

---

## 9. 风险与应对

| 风险             | 应对                                         |
| ---------------- | -------------------------------------------- |
| Linux 发行版差异 | 先聚焦 Debian/Ubuntu，部署脚本脚本化可扩展   |
| SSH 凭据安全     | 密码不落盘、私钥优先、指纹校验严格           |
| 部署失败残留     | 工作区隔离、幂等脚本、状态文件               |
| 本地模式被影响   | 通过 Backend 抽象隔离，LocalBackend 完全不变 |
| SSH 连接不稳定   | 自动重连、操作超时、清晰错误提示             |
| WebUI 端口不可达 | 默认使用 SSH 隧道，不依赖远端直接开放端口    |

---

## 10. 用户体验与零安装门槛

### 10.1 用户侧零安装

Desktop 使用 **paramiko**（纯 Python SSH 库）实现 SSH/SFTP，已打包在应用内。用户不需要：

- ✔ 安装 OpenSSH
- ✔ 懂命令行
- ✔ 配置 SSH config
- ✔ 用终端敲 `ssh user@host`

用户只需要：

- 服务器 IP、端口、用户名
- 密码 **或** 私钥文件

这与 FinalShell、MobaXterm、宝塔面板添加服务器的门槛一致。

### 10.2 操作流程

```
1. 点“添加服务器”
2. 填 IP、端口、用户名
3. 选认证方式：
   - 密码 → 输密码
   - 密钥 → 选文件（或拖拽 .pem / id_rsa）
4. 点“测试连接” → 绿勾 ✓
5. 点“部署” → 自动装好
6. 回到 Bot 页 → 添加 Bot → 选这台服务器 → 正常用
```

### 10.3 体验优化点（P3 阶段）

- 首次连接弹出指纹确认对话框（“这是你第一次连这台服务器”）
- 密码可选存到系统 keyring（Windows Credential Manager）
- 私钥文件支持拖拽
- 连接失败给人话错误提示（“密码错了”而不是 `AuthenticationException`）
- 服务器列表显示在线/离线状态

---

## 11. SSH 连接管理升级路径

现有 `ssh_client.py` 是“连接 → 执行 → 关闭”的短连接风格。VS Code Remote 模型需要升级为持久连接管理器：

| 现有能力             | 需要补的                                                 |
| -------------------- | -------------------------------------------------------- |
| 单次连接/执行/关闭   | 持久连接 + keepalive (`transport.set_keepalive(30)`)     |
| 单次 `exec_command`  | 多 channel 复用（paramiko 原生支持）                     |
| SFTP 上传/下载单文件 | SFTP 读写文本内容（用于配置）                            |
| 无端口转发           | `Transport.open_channel('direct-tcpip')` 用于 WebUI 隧道 |
| 无断线处理           | 自动重连 + 操作超时 + 状态回调                           |

改造量约 100-200 行，核心是把 `SSHClient` 从“工具类”升级为“持久连接管理器”。

paramiko 原生支持以上所有能力，不需要额外依赖。

---

## 12. 与 v1 规划的差异总结

| 维度           | v1 规划                                   | v2 规划（本文档）                                  |
| -------------- | ----------------------------------------- | -------------------------------------------------- |
| **核心理念**   | SSH 直连 + Go Daemon 双模式               | VS Code Remote 模型，透明远程                      |
| **远端服务**   | 独立 Go Daemon (WebSocket + JSON-RPC)     | 无额外服务，NapCat 本身就是远端进程                |
| **通信方式**   | SSH + 自建 WebSocket/JSON-RPC             | SSH/SFTP + NapCat 自带 WebUI API                   |
| **安全方案**   | 5 层安全架构 (JWT/HMAC/TLS/速率限制/审计) | SSH 隧道（复用成熟方案）                           |
| **用户体验**   | 用户需要理解"SSH 模式"和"Agent 模式"      | 用户只需添加服务器，其余与本地一致                 |
| **代码量**     | ~5000 行 Go + ~3000 行 Python agent 代码  | 预计 ~1500 行 Python（Backend 抽象 + Remote 实现） |
| **架构复杂度** | 高（双模式、双协议、双安全层）            | 低（单一 Backend 抽象 + SSH 传输）                 |

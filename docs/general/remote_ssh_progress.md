# 远程 SSH 方案推进进度

## 文档说明

本文件用于记录 NapCatQQ Desktop 的“本地 Desktop + 远程 Linux SSH 直连”方案推进状态。

相关规划文档见 [`docs/general/remote_ssh_plan.md`](docs/general/remote_ssh_plan.md)。

---

## 当前阶段

当前处于：**P1 MVP 集成阶段（进行中）**

### 架构演进

项目已从最初的 **SSH 直连方案** 演进为 **SSH + Agent/Daemon 双模式**：

- **SSH 模式**：保持原有能力，适合简单部署场景
- **Agent/Daemon 模式**：新增架构，适合实时监控和长期运行

### 总体判断

- P0 基础设施层已经完成
- **Agent/Daemon 核心开发完成**（JSON-RPC 2.0 协议、WebSocket 服务器、Desktop 客户端）
- 当前主线任务是把“连接 → 认证 → 控制 → 状态同步”串成闭环

---

## 已完成事项

### P0：基础设施层

- [x] 远程连接模型、SSH 凭据模型、最小安全约束
- [x] 远程 SSH 客户端与错误模型
- [x] 远程执行抽象层
- [x] Linux Core 部署骨架
- [x] 远端状态 / 日志骨架
- [x] 远程配置入口与方案规划文档

对应代码：

- [`src/core/remote/ssh_client.py`](src/core/remote/ssh_client.py)
- [`src/core/remote/models.py`](src/core/remote/models.py)
- [`src/core/remote/errors.py`](src/core/remote/errors.py)
- [`src/core/remote/execution_backend.py`](src/core/remote/execution_backend.py)
- [`src/core/remote/deployment.py`](src/core/remote/deployment.py)
- [`src/core/remote/status.py`](src/core/remote/status.py)
- [`src/core/remote/remote_manager.py`](src/core/remote/remote_manager.py)

### P1：已完成的部分

- [x] 打通本地配置导出到远端上传的第一步
- [x] 补上远端配置解包与部署脚本执行的第一版骨架
- [x] 明确最小 `status.json` 结构，并接通启动 / 停止后的状态写回骨架
- [x] 在设置页补齐第一版“初始化工作区 / 环境探测 / 连接测试”交互
- [x] 将远程功能从弹窗改为设置页独立标签页
- [x] 修复远程功能接入过程中的循环导入问题
- [x] 修复远程页使用不存在的 `FluentIcon` 枚举导致的崩溃问题

对应代码与资源：

- [`src/core/config/config_export.py`](src/core/config/config_export.py)
- [`src/core/remote/deployment.py`](src/core/remote/deployment.py)
- [`src/core/remote/status.py`](src/core/remote/status.py)
- [`src/core/remote/templates.py`](src/core/remote/templates.py)
- [`src/resource/script/remote_deploy_napcat.sh`](src/resource/script/remote_deploy_napcat.sh)
- [`src/ui/page/setup_page/sub_page/remote.py`](src/ui/page/setup_page/sub_page/remote.py)
- [`src/ui/page/setup_page/__init__.py`](src/ui/page/setup_page/__init__.py)

---

## 当前进行中

### P1 主线任务

- [-] 定义并落地远端部署脚本协议，明确安装输入、输出、返回码与幂等策略

当前状态：

- 已有第一版部署脚本模板
- 已有本地配置导出并上传到远端的能力
- 已有运行状态文件的最小落点
- **已修复**：Ubuntu 24.04+ 的 `libasound2` → `libasound2t64` 包名兼容问题
- **已修复**：curl 下载时空路径参数校验问题
- 还缺完整的部署协议固化与 UI 闭环串接

### 2025-04-11 修复记录

**问题**：部署脚本在 Ubuntu 24.04 上执行失败
- `E: 软件包 libasound2 没有可安装候选` - Ubuntu 24.04+ 中包已重命名为 `libasound2t64`
- `curl: option -o: is badly used here` - 下载函数缺少参数校验

**修复内容**（`src/resource/script/remote_deploy_napcat.sh`）：
1. `install_missing_dependencies()`：添加 t64 包动态检测逻辑，参考 example/installer 实现
2. `download_file()`：添加 URL 和 target_path 空值校验
3. `select_qq_package_url()`：添加架构不支持时的错误退出，避免空值继续执行
4. `ensure_linuxqq_rootless()`：添加 `qq_package_path` 设置后的空值检查
5. `ensure_napcat_installed()`：添加 `napcat_archive_path` 空值检查

### 2025-04-11 UI 重构 - 独立页面

**问题**：原 UI 作为设置子页，空间受限，内容拥挤

**解决方案**：将远程管理提升为独立一级页面

**修改内容**：
1. **新建页面** (`src/ui/page/remote_page/__init__.py`)：独立 RemotePage 类
2. **添加侧边栏**：在主窗口侧边栏添加"远程"导航项 (FluentIcon.GLOBE)
3. **移除设置嵌套**：从 SetupWidget 中移除 Remote，设置页恢复为 4 个标签
4. **样式表支持**：添加 PageStyleSheet.REMOTE

**新布局设计** (独立全屏页面)：
```
+-------------------------------------------------------------+
|  [🌐 远程服务器管理]                                        |
+-------------------------------------------------------------+
|                                                             |
|   +------------------+  +--------------------------------+  |
|   | 服务器连接       |  | 服务器状态面板                 |  |
|   |                  |  |                                |  |
|   |  主机 [      ]   |  |  ● 已连接                      |  |
|   |  端口 [    ]     |  |  系统: Ubuntu 24.04            |  |
|   |  用户 [      ]   |  |  架构: amd64                   |  |
|   |  认证方式 [▼]   |  |  QQ版本: 3.2.25                |  |
|   |    [密钥/密码]  |  |  NapCat: v2.6.0                |  |
|   |                  |  |                                |  |
|   | [测试连接]       |  |  快捷操作: [启动][停止][日志]  |  |
|   | [保存配置]       |  |                                |  |
|   +------------------+  +--------------------------------+  |
|                                                             |
|   +----------------------------------------------------+    |
|   | 部署流程: [探测环境][初始化][同步配置][部署 NapCat]  |    |
|   +----------------------------------------------------+    |
|                                                             |
+-------------------------------------------------------------+
```

**设计特点**：
1. **独立页面**：更大的可用空间，更好的视觉层次
2. **卡片式布局**：使用 CardWidget 分组，提升视觉清晰度
3. **三栏结构**：左侧配置、右侧状态、底部部署流程
4. **渐进披露**：高级选项折叠，认证方式条件显示

### 2025-04-11 适配标准 NapCat 安装

**背景**：需要适配通过 example/NapCat-Installer-main 和 example/NapCat-TUI-CLI-main 安装的 NapCat

**标准安装路径**（由 NapCat-Installer 创建）：
- 基础目录：`$HOME/Napcat`
- QQ 安装：`$HOME/Napcat/opt/QQ`
- NapCat：`$HOME/Napcat/opt/QQ/resources/app/app_launcher/napcat`
- 运行目录：`$HOME/Napcat/run`
- 日志目录：`$HOME/Napcat/log`

**适配内容**：
1. `src/core/remote/models.py`：`LinuxCorePaths` 默认路径从 `$HOME/NapCatCore` 改为 `$HOME/Napcat`
2. `src/resource/script/remote_deploy_napcat.sh`：
   - 更新默认路径变量
   - 添加 `detect_existing_napcat()` 和 `detect_existing_linuxqq()` 函数
   - `ensure_linuxqq_rootless()`：优先检测并使用已有 LinuxQQ 安装
   - `ensure_napcat_installed()`：优先检测并使用已有 NapCat 安装（可通过 `FORCE_NAPCAT_UPDATE=1` 强制更新）
3. `src/core/remote/status.py`：
   - `get_status()`：使用 `pgrep -f '.*/qq --no-sandbox -q [0-9]{4,}'` 检测进程（与 TUI-CLI 一致）
   - 从命令行提取 QQ 号
   - 支持从 `napcat.mjs` 读取版本号
   - `tail_log()`：支持标准日志命名格式 `napcat_{QQ号}.log`

---

## 下一步

### 高优先级（Agent/Daemon 模式）

- [x] Daemon 自动部署系统
  - 一键安装脚本（install.sh）
  - Python 自动部署器（DaemonDeployer）
  - Token 安全存储（keyring）
  - 配置管理器（DaemonConfigManager）

- [x] UI 远程管理页面重构
  - 双模式架构（SSH + Agent/Daemon）
  - 顶部模式切换下拉框
  - 动态配置面板切换
  - 统一状态面板
  - 实时日志显示

- [x] SSH 模式完整功能
  - SSH 连接配置面板
  - 认证方式切换（密码/密钥）
  - 高级选项（超时、指纹策略）
  - 连接/断开操作
  - 启动/停止/重启 NapCat
  - 部署功能

- [x] Agent 模式完整功能
  - Agent 连接配置面板
  - Token 输入
  - 一键部署集成
  - WebSocket 连接管理
  - 实时状态同步
  - 日志推送接收

- [x] 统一的处理器架构
  - BaseConnectionHandler 抽象基类
  - SSHHandler 实现
  - AgentHandler 实现
  - 统一的状态显示和操作

- [ ] Agent 连接流程优化
  - 自动重连 UI
  - 连接错误详细提示

### 高优先级（SSH 模式 - 保持）

- [ ] 继续补齐”部署”交互
  - 上传配置包
  - 上传部署脚本
  - 执行部署脚本
  - 展示部署结果

- [ ] 继续补齐”运行控制”交互
  - 启动
  - 停止
  - 重启
  - 状态刷新

### 中优先级

#### 安全强化

- [ ] Python 客户端安全库
  - HMAC-SHA256 挑战签名实现
  - JWT Token 解析与刷新
  - Token 安全存储 (Keyring/Keychain)

- [ ] TLS 证书管理
  - 自签名证书生成脚本
  - 证书固定 (Certificate Pinning)
  - Let's Encrypt 自动续期

- [ ] UI 安全提示
  - TLS 连接状态指示
  - 认证失败告警
  - 安全事件通知

#### 功能补全

- [ ] JSON-RPC 方法补全
  - `config.set` 实现
  - `file.upload/download` 实现
  - `system.info` 扩展

- [ ] 日志流优化
  - 服务端日志级别过滤
  - 客户端日志缓存
  - 历史日志查询

- [ ] 补测试与失败回滚策略
  - Agent 连接超时处理
  - 认证失败处理
  - SSH/Agent 模式切换测试

---

## 阶段判断

### 已完成

- P0：**已完成**
- **Agent/Daemon 核心**：已完成
  - Go Daemon WebSocket + JSON-RPC 2.0 服务器
  - Desktop Agent 客户端
  - NapCat 进程管理
  - 日志流推送框架

### 当前阶段

- P1：**进行中** - 双模式架构集成
  - SSH 模式：已有基础能力，需完善 UI 闭环
  - Agent 模式：核心完成，需 UI 集成

### 尚未进入

- P2：增强版本（配置回读、版本检查、多服务器管理等）
- P3：高级能力（批量编排、自动修复等）

---

## 一句话总结

项目已从”SSH 直连单一路线”演进为”SSH + Agent/Daemon 双模式架构”，JSON-RPC 2.0 协议层已完成，现阶段重点是在远程页添加 Agent 模式切换，把”连接 → 认证 → 控制 → 状态同步”串成可用闭环。

---

## 环境信息

### 项目结构（当前）

```
NapCatQQ-Desktop/
├── src/
│   ├── daemon/                 # Go Daemon (Agent 服务端)
│   │   ├── cmd/daemon/
│   │   ├── internal/
│   │   │   ├── server/         # WebSocket + JSON-RPC 服务器
│   │   │   ├── handler/        # NapCat 进程管理
│   │   │   ├── process/        # 进程监控
│   │   │   ├── log/            # 日志流
│   │   │   └── config/         # 配置管理
│   │   └── pkg/jsonrpc/        # JSON-RPC 2.0 协议
│   │       ├── types.go
│   │       ├── handler.go
│   │       └── napcat.go
│   │
│   └── desktop/                # Desktop 应用
│       ├── core/remote/        # 远程管理模块
│       │   ├── jsonrpc_protocol.py   # JSON-RPC 2.0
│       │   ├── agent_client.py       # WebSocket 客户端
│       │   ├── agent_backend.py      # ExecutionBackend
│       │   ├── ssh_client.py         # SSH 客户端
│       │   └── ...
│       └── ui/page/remote_page/     # 远程页 UI
│
└── docs/general/
    ├── remote_ssh_plan.md      # 本文件
    └── remote_ssh_progress.md  # 进度追踪
```

### 技术栈

| 层级 | Go Daemon | Desktop Python |
|------|-----------|----------------|
| 传输 | WebSocket (gorilla) | QtWebSockets |
| 协议 | JSON-RPC 2.0 | JSON-RPC 2.0 |
| 进程 | os/exec | (remote) |
| 日志 | 环形缓冲区 | Signal 槽 |

### 协议端点

**WebSocket**: `ws://host:port/ws`

**方法**：
- `auth.authenticate` - Token 认证
- `napcat.start/stop/restart/status/logs` - 进程管理
- `config.get/set` - 配置管理
- `log.subscribe/unsubscribe` - 日志订阅

**通知**：
- `status.update` - 状态变更
- `log.entry` - 新日志条目
- `process.exit` - 进程退出

### 2025-04-12 UI 重构 - 简洁设计 (参考 ApiDebugPage)

**问题**：SimpleCardWidget 阴影太强，层次感过多

**解决方案**：参考 ApiDebugPage 使用 QSplitter + QWidget 轻量布局

**改进内容** (`src/ui/page/remote_page/__init__.py`)：
1. **移除卡片阴影**：改用 QWidget + QSplitter 左右分割
2. **细分割线**：QFrame.HLine (1px 半透明灰) 分隔区域
3. **合并部署区域**：将底部部署卡片合并到右侧面板
4. **简化按钮**：减少按钮数量，更紧凑布局
5. **可调整分割**：用户可拖动分割线调整左右宽度

**新布局设计**：
```
+-------------------------------------------------------------+
|  远程服务器管理                                             |
+-------------------------------------------------------------+
|                                                             |
|   连接配置                |  服务器状态                     |
|   SSH 连接参数            |  实时信息                       |
|                           |                                 |
|   主机 [175.178...]       |  ● 已连接                       |
|   端口 [22]  认证 [key]   |  系统: Ubuntu 24.04             |
|   用户 [ubuntu]           |  架构: amd64                    |
|   私钥 [~/.ssh/...]       |  QQ: --                         |
|                           |  NapCat: --                     |
|   ▶ 高级选项              |                                 |
|   ─────────────────────   |  ─────────────────────────────  |
|                           |  快捷操作                       |
|   [测试连接]  [保存配置]   |  [启动] [停止] [日志]           |
|                           |                                 |
|                           |  ─────────────────────────────  |
|                           |  部署操作                       |
|                           |  [探测] [初始化] [同步] [部署]  |
|                           |                                 |
+-------------------------------------------------------------+
```

**设计特点**：
1. **轻量布局**：无卡片阴影，使用细线分隔
2. **左右分割**：QSplitter 可调整宽度
3. **三区域右侧面板**：状态 / 快捷操作 / 部署操作
4. **简化按钮**：移除重启按钮，部署按钮简化为"部署"
5. **状态指示**：文字前加圆点符号 (●) 表示状态

---

### 2025-04-12 开发环境完善

**目标**：确保项目可以一键构建和运行

**完成内容**：

1. **依赖配置**
   - 更新 `pyproject.toml`: 添加 `keyring` 依赖（Token 安全存储）
   - 更新 `src/daemon/go.mod`: 添加 `gopkg.in/yaml.v3` 依赖

2. **构建系统**
   - `Makefile`: 支持 `make build`、`make test`、`make run-desktop/daemon`
   - 交叉编译支持（Linux amd64/arm64/arm）
   - 开发/生产环境配置分离

3. **配置文件**
   - `config/daemon.yaml`: Daemon 配置模板
   - 开发环境默认配置（本地监听、自签名证书）
   - 生产环境配置注释

4. **开发脚本**
   - `scripts/dev-setup.ps1`: Windows 开发环境一键设置
   - `scripts/verify-env.ps1`: 环境验证脚本
   - 检查 Python/Go 依赖、项目结构、语法

5. **项目结构验证**
```
项目根目录
├── main.py                    ✅ 程序入口
├── pyproject.toml            ✅ Python 配置
├── requirements.txt          ✅ 依赖列表
├── Makefile                  ✅ 构建脚本
├── config/
│   └── daemon.yaml           ✅ 配置模板
├── src/
│   ├── desktop/core/remote/  ✅ Agent 客户端
│   └── daemon/               ✅ Go Daemon
└── scripts/
    ├── dev-setup.ps1         ✅ 环境设置
    └── verify-env.ps1        ✅ 环境验证
```

### 2025-04-12 安全架构实施

**背景**：Agent/Daemon 架构引入后，需要建立完整的安全体系，包括传输加密、认证授权、审计监控。

**安全分层设计**：

```
┌─────────────────────────────────────────────────────────────┐
│  Layer 5: 应用层安全                                          │
│  - 输入验证 (路径遍历、命令注入防护)                           │
│  - 权限分级 (RBAC)                                           │
├─────────────────────────────────────────────────────────────┤
│  Layer 4: 会话层安全                                          │
│  - JWT Token (15分钟过期)                                     │
│  - 会话绑定 (IP + 连接)                                       │
├─────────────────────────────────────────────────────────────┤
│  Layer 3: 认证层安全                                          │
│  - 挑战-响应 (HMAC-SHA256)                                    │
│  - 防重放攻击                                                │
├─────────────────────────────────────────────────────────────┤
│  Layer 2: 传输层安全                                          │
│  - TLS 1.3 (强制 wss://)                                      │
│  - 证书固定 (可选)                                            │
├─────────────────────────────────────────────────────────────┤
│  Layer 1: 网络安全                                            │
│  - 速率限制 (IP/连接)                                         │
│  - 审计日志                                                  │
└─────────────────────────────────────────────────────────────┘
```

**实现内容**：

1. **Go 安全库** (`src/daemon/pkg/security/`)
   - `challenge.go` (145行): 挑战-响应认证，HMAC-SHA256，常量时间比较
   - `jwt.go` (285行): JWT 签名/验证，权限声明，会话管理
   - `ratelimit.go` (250行): Token Bucket 限流，IP/连接限制，暴力破解防护
   - `audit.go` (350行): 安全审计日志，事件类型分级，参数脱敏
   - `validation.go` (180行): 输入验证，路径遍历防护，文件扩展名白名单

2. **安全服务器** (`src/daemon/internal/server/secure_server.go`, 500行)
   - 集成所有安全组件
   - 强制 TLS 模式
   - 权限中间件
   - 审计日志自动记录

3. **认证流程** (挑战-响应)
   ```
   Desktop                     Daemon
      │ ──1.请求挑战───────────►│
      │ ◄──2.返回随机数+时间戳───│
      │   3.计算 HMAC(Token, 挑战)
      │ ──4.发送签名───────────►│
      │ ◄──5.返回 JWT(15分钟)───│
   ```

4. **审计事件**
   - `auth.challenge.requested`
   - `auth.success` / `auth.failure.*`
   - `permission.denied`
   - `rate_limit.hit`
   - `security.path_traversal_attempt`

**权限分级**：
```
status:read   - 查看状态
status:write  - 启动/停止
config:read   - 读取配置
config:write  - 修改配置
file:read     - 下载文件
file:write    - 上传文件
admin         - 所有权限
```

**速率限制**：
- 认证: 5次/15分钟，超5次封禁15分钟
- 一般请求: 10次/秒，突发20
- 连接: 每IP最多5个并发

**文件统计**：
- Go 安全库: 1210行
- 安全服务器: 500行
- 安全设计文档: 200行
- 安全实施文档: 250行

### 2025-04-12 添加清空环境功能

**需求**：添加清空远程 NapCat 环境的功能，包括 QQ、NapCat 和运行时文件

**实现内容**：

1. **部署脚本** (`src/resource/script/remote_deploy_napcat.sh`)：
   - 添加 `clean_napcat_environment` 函数
   - 支持通过 `CLEAN_QQ` 环境变量控制是否清理 QQ

2. **部署类** (`src/core/remote/deployment.py`)：
   - 添加 `clean_environment(include_qq=True)` 方法
   - 清理内容：
     - 停止运行中的 NapCat 进程
     - 清理运行时目录（run, tmp, packages）
     - 清理日志文件
     - 清理 NapCat 安装
     - 清理 QQ 注入文件（loadNapCat.js）
     - 恢复 QQ 原始 package.json（从备份）
     - 清理启动脚本（napcat.sh）
     - 可选：清理 QQ 安装和安装包

3. **UI** (`src/ui/page/remote_page/__init__.py`)：
   - 添加"清空环境"按钮（红色文字警示）
   - 添加确认对话框，列出将要清理的内容
   - 清理完成后重置状态显示

**安全机制**：
- 确认对话框防止误操作
- 先停止进程再清理文件
- 使用 `|| true` 确保单条命令失败不影响整体流程

---

### 2025-04-12 架构重构 - Agent/Daemon 模式

**背景**：原 SSH 直连方案在进程管理、日志推送、状态同步方面存在固有局限，引入 Agent/Daemon 架构实现更可靠的远程管理。

**新架构设计**：
```
┌─────────────────────────────────────────────────────────────┐
│  Desktop (Windows)              Remote Linux Server         │
│                                                             │
│  ┌─────────────────┐            ┌──────────────────────┐   │
│  │ UI Layer        │            │  NapCat Daemon (Go)  │   │
│  │ - RemotePage    │◄──────────►│  - WebSocket Server  │   │
│  │ - Config Widget │   JSON-RPC │  - Process Manager   │   │
│  └─────────────────┘   over WS  │  - Log Streamer      │   │
│           │                      └──────────────────────┘   │
│           │                           │                     │
│  ┌────────▼──────────┐               │                     │
│  │ Agent Client      │               │                     │
│  │ - WebSocket conn  │               │                     │
│  │ - JSON-RPC encode │               ▼                     │
│  └───────────────────┘        ┌──────────────┐             │
│                               │ NapCat Process│            │
│  可选：SSH Fallback           │ (managed)     │            │
│  - 传统 SSH/SFTP 方式         └──────────────┘            │
└─────────────────────────────────────────────────────────────┘
```

**目录结构变更**：
```
src/
├── daemon/                 # 新增 Go Daemon 项目
│   ├── cmd/daemon/main.go
│   ├── internal/
│   │   ├── server/         # WebSocket + JSON-RPC 服务器
│   │   ├── handler/        # NapCat 进程管理
│   │   ├── process/        # 进程监控
│   │   ├── log/            # 日志流推送
│   │   └── config/         # 配置管理
│   └── pkg/jsonrpc/        # JSON-RPC 2.0 协议实现
│       ├── types.go        # 基础类型
│       ├── handler.go      # 请求路由
│       └── napcat.go       # NapCat 专用类型
│
└── desktop/core/remote/    # Desktop Agent 客户端
    ├── jsonrpc_protocol.py # JSON-RPC 2.0 协议
    ├── agent_client.py     # WebSocket 客户端
    └── agent_backend.py    # ExecutionBackend 实现
```

**实现内容**：

1. **Go Daemon** (`src/daemon/`)
   - JSON-RPC 2.0 over WebSocket 服务器
   - 标准方法：`auth.authenticate`, `napcat.start/stop/restart/status`, `config.get/set`, `log.subscribe/unsubscribe`
   - 标准通知：`status.update`, `log.entry`, `process.exit`
   - 进程管理：启动、停止、状态监控
   - 日志流：环形缓冲区 + 实时推送

2. **Desktop Agent 客户端** (`src/desktop/core/remote/`)
   - `jsonrpc_protocol.py` (295行)：完整 JSON-RPC 2.0 协议实现
   - `agent_client.py` (513行)：WebSocket 连接、认证、请求/响应处理
   - `agent_backend.py` (426行)：继承 `ExecutionBackend`，与 SSH/Local 统一接口

3. **协议规范**
   - 标准 JSON-RPC 2.0 格式
   - 错误码遵循规范 (-32700 ~ -32000)
   - 请求/响应/通知三态区分
   - Token 认证机制

**与旧 SSH 方案对比**：

| 能力 | SSH 直连 | Agent/Daemon |
|------|----------|--------------|
| 进程状态实时推送 | ❌ 轮询 | ✅ WebSocket 通知 |
| 日志实时流 | ❌ 轮询 tail | ✅ Server Push |
| 连接稳定性 | ⚠️ SSH 会话易断 | ✅ 自动重连 |
| 部署复杂度 | 低（无服务端） | 中（需部署 Daemon）|
| 适用场景 | 简单管理 | 实时监控、长期运行 |

### 2025-04-12 协议升级 - JSON-RPC 2.0

**背景**：原自建 JSON 协议缺乏标准化，升级至 JSON-RPC 2.0 规范。

**格式对比**：

```json
// 旧格式（自建）
{"id": "123", "method": "napcat.start", "params": {"work_dir": "/opt"}}
{"id": "123", "result": {"pid": 1234}, "error": {"code": "internal", "message": "..."}}

// JSON-RPC 2.0（新）
{"jsonrpc": "2.0", "id": "123", "method": "napcat.start", "params": {"work_dir": "/opt"}}
{"jsonrpc": "2.0", "id": "123", "result": {"pid": 1234}}
{"jsonrpc": "2.0", "id": "123", "error": {"code": -32603, "message": "..."}}

// 通知（服务端主动推送）
{"jsonrpc": "2.0", "method": "status.update", "params": {"status": {"running": true}}}
```

**优势**：
- ✅ 行业标准，文档完善
- ✅ 规范错误码体系
- ✅ 原生支持通知机制
- ✅ 批量请求支持
- ✅ 通用调试工具可用

**文件统计**：
- Go: 1239行 (pkg/jsonrpc/ + internal/server/ + internal/handler/)
- Python: 1234行 (jsonrpc_protocol.py + agent_client.py + agent_backend.py)

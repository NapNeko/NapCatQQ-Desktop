# P1 实施计划 — 远端部署 MVP

> 对应 [`docs/general/remote_ssh_plan.md`](./remote_ssh_plan.md) §7 的 **P1** 阶段。
> 本文档冻结 P1 范围、设计选型、任务分解与验收指标。

## 1. 设计冻结

### 1.1 部署粒度（用户决策 ✅）

**分两步**：`install_qq` 与 `install_napcat` 各自独立可重入。
后端能力分两步实现，UI 在 P1 仅暴露 “部署” 一个一键入口（依次跑 install_qq → install_napcat），单步重跑/强制更新交互推到 P3。

### 1.2 脚本拆分

将原单一 `remote_deploy_napcat.sh`（687 行一站式脚本）拆为 3 个独立脚本：

| 脚本 | 职责 | 触发方 |
| --- | --- | --- |
| `remote_install_linuxqq.sh` | 依赖装齐 + LinuxQQ rootless 安装 + status.json | `RemoteBackend.install_qq` |
| `remote_install_napcat.sh` | NapCat 下载/解压 + loadNapCat.js 注入 + package.json patch + launcher 脚本写入 | `RemoteBackend.install_napcat` |
| `remote_napcat_launcher.sh` | 启停/状态/日志路径查询，常驻在 `$workspace_dir/napcat.sh` | P2 进程闭环 |

> 拆分原则：
> - 每个脚本独立可重入；脚本头都要 `set -euo pipefail` + `write_status` 错误兜底。
> - 公共变量与公共工具函数（`log_info` / `download_file` / `extract_zip_to` / `escape_json_string` / `write_status` / `detect_*`）在每个脚本中保留必要副本，**避免引入 source 依赖**，便于上传后单文件运行。
> - `sync_runtime_export_config` 整段移除，P1 不再要求 `config_archive` 存在；该能力放在 P2 “远端配置同步” 重新评估。

### 1.3 探测能力增强

`LinuxCoreDeployment.probe_environment` 返回值扩展：

```python
@dataclass(slots=True)
class LinuxCoreDeploymentProbe:
    os_name: str           # `uname -s`
    architecture: str       # `uname -m` 原始值
    normalized_arch: Literal["amd64", "arm64"] | None  # 仅识别 amd64 / arm64
    distro_id: str | None   # /etc/os-release 的 ID 字段（debian / ubuntu / centos / rhel / fedora ...）
    distro_version: str | None  # /etc/os-release 的 VERSION_ID
    has_bash: bool
    has_tar: bool
    has_unzip: bool
    has_curl: bool
    has_dpkg: bool
    has_rpm2cpio: bool
    has_xvfb: bool
    has_linuxqq: bool      # `qq_executable` 是否存在
    has_napcat: bool       # `napcat.mjs` 是否存在
    installed_qq_version: str | None
    installed_napcat_version: str | None
```

### 1.4 RemoteBackend 接口落地

| 接口 | P1 实现行为 |
| --- | --- |
| `install_qq(*, progress)` | 1) `initialize_layout` 2) 上传 `remote_install_linuxqq.sh` 3) 执行脚本 4) 更新 ServerProfile 的 `qq_version`。**不会强制重装已有 QQ**（脚本内部检测后跳过）。失败抛 `RemoteDeploymentError`。 |
| `install_napcat(archive_path=None, *, progress)` | 1) `initialize_layout` 2) 上传 `remote_install_napcat.sh` 3) 执行脚本 4) 同时上传 `remote_napcat_launcher.sh` 到 `$workspace_dir/napcat.sh` 并 `chmod +x` 5) 更新 ServerProfile 的 `napcat_version`。`archive_path` 暂不使用（远端自己 curl 下载）；预留接口以便 P3 支持本地上传安装包。 |

进度回调语义（与 `ProgressCallback = Callable[[str, int], None]` 兼容）：
- `install_qq`: 0 → 100，分阶段：探测 (0–10) / 依赖安装 (10–35) / 下载 deb (35–70) / 解压安装 (70–95) / 验证 (95–100)
- `install_napcat`: 0 → 100，分阶段：探测 (0–10) / 下载 zip (10–60) / 解压注入 (60–85) / launcher 部署 (85–100)

`RemoteBackend` 负责把脚本 stdout 中带 `[PROGRESS] <percent> <message>` 标记的行翻译为 callback 调用。脚本侧统一 `log_progress() { echo "[PROGRESS] $1 $2"; }`。

### 1.5 ServerManager 编排

新增同步方法（在后台线程调用）：

```python
def deploy_server(
    self,
    server_id: str,
    *,
    progress_callback: Callable[[str, int], None] | None = None,
    force_napcat_update: bool = False,
) -> DeploymentResult
```

行为：
1. 校验当前状态 ≠ `DEPLOYING`，否则抛 `RemoteDeploymentInProgressError`
2. `set_deployment_state(server_id, DEPLOYING)`
3. `backend.connect()`
4. `backend.install_qq(progress=...)`（进度区间映射到 0–50）
5. `backend.install_napcat(progress=..., force_update=force_napcat_update)`（进度区间映射到 50–100）
6. 探测一次安装信息，更新 `napcat_version` / `qq_version`
7. 成功：`set_deployment_state(DEPLOYED)` 并返回 `DeploymentResult(ok=True, ...)`
8. 失败：`set_deployment_state(FAILED)` 并抛出原始异常

新增 Qt 信号：
- `deployment_progress = Signal(str, str, int)`  # (server_id, message, percent)
- `deployment_finished = Signal(str, bool, str)`  # (server_id, ok, message)

### 1.6 后台运行器

新增 `src/desktop/ui/page/remote_page/deployment_runner.py`：

```python
class DeploymentRunner(QRunnable):
    """后台执行 ServerManager.deploy_server，并把进度回调桥接到 Qt 信号。"""
```

签名沿用 `ConnectionTester` 模式（独立 `QObject` 信号载体），自动 deleteLater。

### 1.7 UI 改动

`RemotePage`:
- 工具栏新增 `部署` 主操作按钮（`PrimaryPushButton + FI.SEND` 风格），仅在 `selected & state ∈ {UNDEPLOYED, FAILED, DEPLOYED(允许重新部署)}` 可用
- 详情面板下方新增 “部署进度” 行：进度条 + 当前阶段文本 + 最近一条日志预览
- 部署中: `add_btn / edit_btn / test_btn / delete_btn` 灰显
- 完成: 通过 `deployment_finished` 切回 idle，弹 `success_bar` / `error_bar`

### 1.8 状态机扩展

`DeploymentState` 不新增枚举值，但语义微调：
- `UNDEPLOYED`: 从未部署 / 已被清理
- `DEPLOYING`: 正在执行 install_qq 或 install_napcat
- `DEPLOYED`: install_napcat 成功且 napcat.mjs 探测到版本
- `FAILED`: 本次部署中途失败；UI 允许重试

## 2. 任务分解 (XL Plan)

### Wave A — 后端能力（独立可测，先做）

| # | 单元 | 文件 | 备注 |
| --- | --- | --- | --- |
| A1 | 探测能力增强 | `core/remote/deployment.py` | 扩展 `LinuxCoreDeploymentProbe` + `probe_environment` |
| A2 | 脚本拆分 | `desktop/resource/script/remote_install_linuxqq.sh` `remote_install_napcat.sh` `remote_napcat_launcher.sh` | 新增 3 文件；保留旧 `remote_deploy_napcat.sh` 作 archive 引用 |
| A3 | 模板构建函数 | `core/remote/templates.py` | `build_install_linuxqq_script` / `build_install_napcat_script` / `build_launcher_script` |
| A4 | 部署器拆分 | `core/remote/deployment.py` | 新增 `install_linuxqq(progress)` / `install_napcat(progress, force_update)` / `upload_launcher_script` 方法 |
| A5 | 进度协议 | `core/remote/deployment.py` | `_run_script_with_progress` 解析脚本 `[PROGRESS] N message` 输出 |

### Wave B — Backend & 服务编排（依赖 Wave A）

| # | 单元 | 文件 |
| --- | --- | --- |
| B1 | RemoteBackend.install_qq 实现 | `core/operation/remote_backend.py` |
| B2 | RemoteBackend.install_napcat 实现 | 同上 |
| B3 | ServerManager.deploy_server | `core/remote/server_manager.py` |
| B4 | Qt 信号 deployment_progress/finished | 同上 |

### Wave C — UI（依赖 Wave B）

| # | 单元 | 文件 |
| --- | --- | --- |
| C1 | DeploymentRunner | `ui/page/remote_page/deployment_runner.py` |
| C2 | RemotePage 部署按钮 + 进度区 | `ui/page/remote_page/__init__.py` |

### Wave D — 测试 & 验收

| # | 单元 | 文件 |
| --- | --- | --- |
| D1 | probe 单元测试 | `script/test/test_remote_deploy_probe.py` |
| D2 | install_linuxqq / install_napcat 用例（mock SSH） | `script/test/test_remote_deploy_runner.py` |
| D3 | ServerManager.deploy_server 编排测试 | `script/test/test_server_manager_deploy.py` |
| D4 | 验收文档 | `docs/general/remote_ssh_p1_acceptance.md` |

## 3. 验收指标

### 3.1 自动化

```
python -m pytest script/test/test_remote_deploy_probe.py \
                 script/test/test_remote_deploy_runner.py \
                 script/test/test_server_manager_deploy.py
```

预期：全部通过，覆盖：
- 探测：`/etc/os-release` 缺失回退、arm64 / amd64 归一、已有 QQ/NapCat 检测、版本解析
- install_linuxqq：脚本上传顺序、`[PROGRESS]` 解析、失败时 stderr 透传
- install_napcat：`force_update` 参数透传、launcher 脚本一并部署
- ServerManager.deploy_server：状态机 UNDEPLOYED → DEPLOYING → DEPLOYED；中途失败 → FAILED；进度回调总进度 0→100；并发部署被拒

### 3.2 手动验收

| 步骤 | 预期 |
| --- | --- |
| 1. 选中一台 UNDEPLOYED 服务器, 点击 “部署” | 工具栏其它按钮灰显, 进度条出现 |
| 2. 等待 install_qq 阶段 | 进度从 0→50, 阶段文本依次为 “探测环境” → “安装系统依赖” → “下载 LinuxQQ” → “解压安装” → “验证” |
| 3. 等待 install_napcat 阶段 | 进度从 50→100, 阶段文本 “下载 NapCat” → “注入” → “部署 launcher” |
| 4. 部署成功 | 状态变 DEPLOYED, 详情显示 napcat_version / qq_version, success_bar 提示 |
| 5. 模拟失败（断网 / 远端拒绝） | 状态变 FAILED, error_bar 显示具体阶段 + stderr 摘要; 工具栏恢复可用 |
| 6. 已 DEPLOYED 服务器再次点击 “部署” | 二次确认对话框, 用户确认后重跑（脚本幂等, 实际只补缺失部分） |

### 3.3 安全基线复核

- [ ] 部署脚本不含任何凭据信息（仅运行参数）
- [ ] 远端命令通过 `_quote_remote_argument` 转义, 无 shell 注入面
- [ ] 部署失败不会留下 `*.tmp` 残留（脚本 trap 兜底）
- [ ] `force_update` 参数仅由 UI 显式触发, 默认 False, 防止误删用户已有 NapCat 配置

## 4. 接口稳定性承诺

P1 锁定下列接口签名，P2 / P3 不会破坏性修改：

- `RemoteBackend.install_qq(*, progress=None)` 签名
- `RemoteBackend.install_napcat(archive_path=None, *, progress=None, force_update=False)` 签名
- `ServerManager.deploy_server(server_id, *, progress_callback=None, force_napcat_update=False)` 签名
- `ServerManager.deployment_progress(str, str, int)` / `deployment_finished(str, bool, str)` 信号
- `[PROGRESS] N message` 脚本进度协议

## 5. 不在 P1 范围

- BotConfig.runtime_target 字段（P2）
- 远端进程启停（P2）
- 远端配置同步（P2）
- WebUI 隧道（P2）
- 部署取消/中断（P3）
- keyring 集成（P3）
- 部署失败回滚清理（P3，复用现有 `clean_environment`）

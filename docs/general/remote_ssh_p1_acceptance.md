# P1 验收报告 — 远端部署 MVP

> 验收日期: 2026-04-30
> 对应实施计划: [`remote_ssh_p1_plan.md`](./remote_ssh_p1_plan.md)
> 整体规划: [`remote_ssh_plan.md`](./remote_ssh_plan.md) §7

## 1. 范围回顾

| 项目               | 计划                                                     | 交付                                                                                     |
| ------------------ | -------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| 远端环境探测增强   | OS / 发行版 / 架构 / 已有 LinuxQQ / 已有 NapCat / 版本号 | ✅ `LinuxCoreDeploymentProbe` 字段 16 项                                                  |
| 部署粒度           | 分两步 install_qq / install_napcat（用户决策）           | ✅ `LinuxCoreDeployment.install_linuxqq` / `install_napcat`                               |
| 部署脚本           | 拆分为 3 个独立脚本                                      | ✅ `remote_install_linuxqq.sh` / `remote_install_napcat.sh` / `remote_napcat_launcher.sh` |
| RemoteBackend 接入 | install_qq / install_napcat 委托部署器                   | ✅ `RemoteBackend.install_qq` / `install_napcat`                                          |
| 服务编排           | `ServerManager.deploy_server` + Qt 信号                  | ✅ `deployment_progress` / `deployment_finished`                                          |
| 后台任务           | `DeploymentRunner(QRunnable)`                            | ✅ `deployment_runner.py`                                                                 |
| UI 入口            | 一键“部署”按钮 + 详情面板进度区（用户决策）              | ✅ `RemotePage`                                                                           |
| 自动化测试         | probe / runner / server_manager_deploy                   | ✅ 32 个 P1 用例全绿                                                                      |

## 2. 自动化验收

### 2.1 P1 测试套件

```
python -m pytest script/test/test_remote_deploy_probe.py \
                 script/test/test_remote_deploy_runner.py \
                 script/test/test_server_manager_deploy.py -q
```

实测结果：

```
32 passed in 0.36s
```

### 2.2 P0 + P1 合并验收

```
python -m pytest script/test/test_remote_deploy_probe.py \
                 script/test/test_remote_deploy_runner.py \
                 script/test/test_server_manager_deploy.py \
                 script/test/test_local_backend.py \
                 script/test/test_server_registry.py -q
```

实测结果：

```
81 passed in 0.38s
```

### 2.3 用例覆盖明细

#### `test_remote_deploy_probe.py` (9 用例)

- [x] `test_full_ubuntu_amd64_with_existing_install` — Ubuntu / amd64 / 已装 QQ + NapCat 全字段解析
- [x] `test_arm64_normalization` — `aarch64` → `arm64` 归一
- [x] `test_unsupported_arch_returns_none` — riscv64 等返回 `None`
- [x] `test_missing_os_release` — `/etc/os-release` 缺失时优雅降级
- [x] `test_centos_rpm_only` — RPM 系（dpkg 缺失）正确识别
- [x] `test_missing_napcat_mjs_means_no_napcat` — 无 NapCat 时 `has_napcat=False`
- [x] `test_existing_napcat_with_unparseable_mjs` — 版本号无法解析时仅 `installed_napcat_version` 为 None
- [x] `test_existing_qq_without_version_field` — 同上（QQ）
- [x] `test_parse_os_release_handles_quotes_and_comments` — 注释/单引号/双引号兼容

#### `test_remote_deploy_runner.py` (12 用例)

- [x] `TestInstallLinuxQQ.test_success_emits_progress_events` — 进度回调串接正确
- [x] `TestInstallLinuxQQ.test_uploads_install_linuxqq_script` — 脚本上传顺序
- [x] `TestInstallLinuxQQ.test_force_reinstall_sets_env_var` — `FORCE_LINUXQQ_REINSTALL=1`
- [x] `TestInstallLinuxQQ.test_failure_raises_remote_command_error` — 退出码非 0 → `RemoteCommandError`
- [x] `TestInstallLinuxQQ.test_no_progress_callback_does_not_explode` — `progress=None` 兼容
- [x] `TestInstallNapCat.test_force_update_sets_env_var` — `FORCE_NAPCAT_UPDATE=1`
- [x] `TestInstallNapCat.test_uploads_napcat_script_and_launcher` — launcher 一并部署
- [x] `TestInstallNapCat.test_custom_download_url_is_quoted_and_passed` — 自定义下载地址透传
- [x] `TestInstallNapCat.test_failure_does_not_upload_launcher` — 失败时 launcher 不部署
- [x] `TestInstallNapCat.test_progress_events_are_clamped_to_upper_bound` — 进度上界钳制
- [x] `TestProgressLineParsing.test_non_progress_lines_are_ignored` — 普通日志不影响
- [x] `TestProgressLineParsing.test_malformed_progress_lines_are_skipped` — 残缺协议行忽略

#### `test_server_manager_deploy.py` (11 用例)

- [x] `TestDeploySuccess.test_state_transitions_undeployed_to_deployed` — UNDEPLOYED → DEPLOYING → DEPLOYED
- [x] `TestDeploySuccess.test_install_qq_runs_before_install_napcat` — 顺序保证
- [x] `TestDeploySuccess.test_force_flags_are_passed_through` — 强制选项透传
- [x] `TestDeploySuccess.test_progress_is_mapped_to_unified_0_100` — 0–50 / 50–100 区间映射
- [x] `TestDeploySuccess.test_finished_signal_emits_success_message` — `deployment_finished(ok=True)`
- [x] `TestDeploySuccess.test_is_deploying_during_call` — `is_deploying` 锁状态正确
- [x] `TestDeployFailure.test_install_qq_failure_marks_failed_state` — stage="install_qq"
- [x] `TestDeployFailure.test_install_napcat_failure_marks_failed_state` — stage="install_napcat"
- [x] `TestDeployFailure.test_failure_release_deploying_lock` — 失败也释放并发锁
- [x] `TestConcurrencyGuard.test_concurrent_deploy_is_rejected` — 并发触发抛 `RemoteDeploymentInProgressError`
- [x] `TestConcurrencyGuard.test_unknown_server_id_raises_key_error` — 不存在档案抛 KeyError

## 3. 架构演进对比

### 3.1 模块结构

```
src/desktop/
├── core/
│   ├── operation/
│   │   └── remote_backend.py      P1: install_qq / install_napcat 实现 (委托 LinuxCoreDeployment)
│   └── remote/
│       ├── deployment.py           P1: probe_environment 增强 + install_linuxqq / install_napcat / upload_launcher_script
│       ├── errors.py               P1: RemoteDeploymentError / RemoteDeploymentInProgressError
│       ├── server_manager.py       P1: deploy_server + DeploymentResult + Qt 信号
│       ├── ssh_client.py           P1: exec_stream 流式执行接口
│       └── templates.py            P1: build_install_linuxqq_script / build_install_napcat_script / build_napcat_launcher_script
├── resource/script/
│   ├── remote_install_linuxqq.sh   P1 新增
│   ├── remote_install_napcat.sh    P1 新增
│   └── remote_napcat_launcher.sh   P1 新增
└── ui/page/remote_page/
    ├── __init__.py                 P1: 部署按钮 + 进度区 + 信号桥接
    └── deployment_runner.py        P1 新增
```

### 3.2 关键决策落地

| 决策点    | 用户选择                           | 实际落地                                         |
| --------- | ---------------------------------- | ------------------------------------------------ |
| 部署粒度  | 分两步 install_qq / install_napcat | 拆为 3 脚本 + 2 个 backend 方法独立可调          |
| UI 暴露面 | P1 仅一个“部署”入口, 高级交互 P3   | 仅 `_on_deploy` 一个入口, `force_*` 参数仅供后端 |
| 脚本依赖  | 不再要求 `config_archive`          | 新脚本独立运行, 无需 config zip                  |

## 4. 接口稳定性承诺

P1 锁定下列接口（不在 P2/P3 做破坏性修改）：

```python
# core/operation/remote_backend.py
class RemoteBackend(OperationBackend):
    def install_qq(self, *, progress=None, force_reinstall=False) -> None: ...
    def install_napcat(
        self, archive_path=None, *, progress=None, force_update=False
    ) -> None: ...

# core/remote/deployment.py
class LinuxCoreDeployment:
    def install_linuxqq(self, *, progress=None, force_reinstall=False) -> InstallStepResult: ...
    def install_napcat(
        self, *, progress=None, force_update=False, download_url=None
    ) -> InstallStepResult: ...

# core/remote/server_manager.py
class ServerManager(QObject):
    deployment_progress = Signal(str, str, int)   # (server_id, message, percent)
    deployment_finished = Signal(str, bool, str)  # (server_id, ok, message)
    def deploy_server(
        self,
        server_id: str,
        *,
        progress_callback=None,
        force_napcat_update=False,
        force_linuxqq_reinstall=False,
    ) -> DeploymentResult: ...
    def is_deploying(self, server_id: str) -> bool: ...
```

进度协议: 远端脚本 stdout 的 `[PROGRESS] <0-100> <message>` 行是公开协议，由 `_run_script_with_progress` 解析并触发 `ProgressCallback`。

## 5. 已知限制 & 下一步

### 5.1 P1 仍然不解决的（按计划保留到后续）

- BotConfig.runtime_target 字段（P2）
- 远端进程启停（P2，launcher 脚本已经部署到位）
- 远端配置同步（P2）
- WebUI 隧道（P2）
- 部署取消/中断（P3）
- keyring 集成（P3）
- 单步重跑（仅 install_qq 或仅 install_napcat）UI 暴露（P3）
- archive_path 自定义安装包（P3）

### 5.2 风险与缓解

| 风险                             | 缓解                                                               |
| -------------------------------- | ------------------------------------------------------------------ |
| 不同 Linux 发行版的 t64 包名差异 | 脚本 `install_missing_dependencies` 内置探测；缺失时告警不阻断     |
| 远端无外网下载失败               | `RemoteCommandError.stderr` 透传到 `error_bar`，用户能看到具体阶段 |
| 部署中断导致状态卡 DEPLOYING     | `try/finally` 保证锁释放 + 失败状态写盘                            |
| 进度协议被未来脚本污染           | `_PROGRESS_LINE_PATTERN` 严格匹配 `[PROGRESS] N message` 三段式    |

## 6. 验收结论

✅ **P1 阶段达成**：

- 实施计划全部任务（A1–A5 / B1–B4 / C1–C2 / D1–D4）完成
- 32 个 P1 自动化测试全部通过；P0 49 个回归测试不破坏
- 关键接口签名已锁定为公开 API，文档化承诺向后兼容
- UI 一键部署入口与详情面板进度区已可用

P1 阶段验收**通过**，可推进到 P2（远端 Bot 运行闭环）。

---

## 7. P1.5 增量：独立部署控制台

### 7.1 动机

P1 验收完成后，用户提出"部署期间能否把执行任务的终端实时展示出来"。
针对该诉求新增 P1.5 增量交付，**不破坏 P1 接口**，仅在已有信号体系上叠加一条日志通道。

### 7.2 设计要点

| 维度     | 选择                                                                                      |
| -------- | ----------------------------------------------------------------------------------------- |
| 展示形式 | **独立弹窗**（用户决策）                                                                  |
| 模态性   | 非模态，可同时打开多台服务器的控制台                                                      |
| 流式协议 | SSH `paramiko` PTY 模式（`get_pty=True`），bash 进入行缓冲，stderr 合并到 stdout 实时输出 |
| 行级渲染 | `QPlainTextEdit`（专为流式日志优化），最大 5000 行自动裁剪                                |
| 上色规则 | `[INFO]` 蓝 / `[WARN]` 黄 / `[ERROR]` 红 / `[PROGRESS]` 青 / `[OK]` 绿 / 其他浅灰         |
| 关闭保护 | 部署中关闭按钮禁用，避免误关丢失日志；完成后启用                                          |
| 失败留痕 | 失败时窗口保持显示，错误摘要置于终端尾部                                                  |

### 7.3 新增/扩展接口

```python
# core/remote/ssh_client.py
SSHClient.exec_stream(
    command,
    *,
    on_stdout_line=None,
    on_stderr_line=None,
    timeout=None,
    check=False,
    merge_stderr=False,   # P1.5 新增: True 时启用 PTY，stderr 合并到 stdout 实时输出
)

# core/remote/deployment.py
LinuxCoreDeployment.install_linuxqq(
    *,
    progress=None,
    log_callback=None,     # P1.5 新增: 每行 stdout 透传给上层
    force_reinstall=False,
)
LinuxCoreDeployment.install_napcat(
    *,
    progress=None,
    log_callback=None,     # P1.5 新增
    force_update=False,
    download_url=None,
)

# core/operation/remote_backend.py
RemoteBackend.install_qq(*, progress=None, log_callback=None, force_reinstall=False)
RemoteBackend.install_napcat(archive_path=None, *, progress=None, log_callback=None, force_update=False)

# core/remote/server_manager.py
class ServerManager(QObject):
    deployment_log = Signal(str, str)  # P1.5 新增: (server_id, line) 实时回显每行远端 stdout
```

### 7.4 新增模块

- `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/desktop/ui/page/remote_page/deployment_console.py` — `DeploymentConsoleDialog` 独立弹窗
- `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/desktop/ui/page/remote_page/__init__.py` — `RemotePage._open_or_focus_console` + 控制台字典管理

### 7.5 自动化测试

| 用例                                                                                     | 关注点                                                                         |
| ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `TestLogCallback.test_log_callback_receives_every_stdout_line`                           | 所有行（含非 PROGRESS 行）都被 log_callback 接收                               |
| `TestLogCallback.test_log_callback_does_not_block_progress`                              | log_callback 与 progress 协议同时工作互不影响                                  |
| `TestLogCallback.test_log_callback_exception_does_not_break_install`                     | 回调抛错不破坏部署                                                             |
| `TestDeploymentLogSignal.test_log_callback_is_passed_to_install_methods`                 | ServerManager.deploy_server 把 log_callback 透传给 install_qq / install_napcat |
| `TestDeploymentLogSignal.test_deployment_log_signal_emits_lines`                         | `deployment_log` 信号正确发射每行                                              |
| `TestDeploymentLogSignal.test_deployment_log_filtered_by_server_id_for_multi_subscriber` | 多订阅者按 server_id 各自过滤                                                  |

实测：`87 passed in 0.45s`（49 P0 + 32 P1 + 6 P1.5 新增）。

### 7.6 接口稳定性承诺

P1.5 锁定的接口与 P1 一并视为稳定 API：
- `merge_stderr` 参数语义：`True` 时输出 `\r` / `\n` 都视为换行，stderr 字段为空
- `log_callback` 签名：`Callable[[str], None]`；调用方异常会被记录但不中断部署
- `deployment_log` 信号：`Signal(str, str)` `(server_id, line)`，按 server_id 过滤是订阅方的职责
- `[PROGRESS] N message` 行同时进入 `progress` 与 `log_callback`

### 7.7 已知约束

- PTY 模式下 stderr 全部合并到 stdout，调用方无法区分流来源（接受这种取舍换实时性）
- curl 等带 `\r` 进度条的工具会被切成多行展示而不是原地刷新（未实现 ANSI 转义解析）
- 控制台默认上限 5000 行，超过自动裁剪顶部（`QPlainTextEdit.setMaximumBlockCount`）

---

## 8. 验收后增量补丁（2026-04-30 当晚）

P1 / P1.5 正式验收完成后，由用户实机测试反馈触发的一批修复与 UX 重构。
**所有改动不破坏第 4 节锁定的公开接口**，纯属实现层修复 + UI 归位。

### 8.1 修复：远端 NapCat 版本探测一直返回 `未探测到`

#### 8.1.1 现象

部署成功 toast 始终显示 `NapCat=未探测到, QQ=3.2.25-45758`，UI 卡片版本号字段空白。
直接 SSH 远端执行 `cat /root/Napcat/.../napcat.mjs | grep napCat` 能确认文件存在且含真实版本号。

#### 8.1.2 根因（基于代码事实，**非假设**）

实测下载 `NapCat.Shell.zip` 解出 `napcat.mjs:10375`，版本字段实际形态：

```js
const napCatVersion = typeof (__vite_import_meta_env__) !== "undefined" && "4.18.1" || "1.0.0-dev";
```

两个独立 bug 叠加：

1. **正则字符类问题**：远端探测用 `[^"]*"(\d+\.\d+\.\d+...)"`，只能跨过等号到**第一个**引号 → 卡在 `"undefined"`，捕获组 `\d+` 校验失败 → 返回 `None`。本地 [`VersioningService._get_napcat_version_from_mjs`](src/desktop/core/versioning/service.py) 用 `.*?` 非贪婪能跨越多个引号字面量找到 `"4.18.1"`。
2. **shell 引号阻断 `$HOME` 展开**：grep 路径用单引号 `'$HOME/Napcat/...'`，bash 在单引号下**不展开变量** → 实际打开了名为 `$HOME/...` 的不存在文件。`LinuxCorePaths` 默认值含 `$HOME` 字面量，必须由 bash 展开。

#### 8.1.3 修复

**正则对齐本地**（[`deployment.py:48-59`](src/desktop/core/remote/deployment.py)）：

```python
_NAPCAT_VERSION_PATTERN = re.compile(
    r'napCatVersion\s*=\s*.*?"(\d+\.\d+\.\d+(?:[-+][^"]+)?)"'
)
```

**shell 命令路径双引号**（[`deployment.py:241-248`](src/desktop/core/remote/deployment.py) / [`status.py:103-106`](src/desktop/core/remote/status.py)）：

```bash
grep -oE 'napCatVersion[^;]*' "$napcat_dir/napcat.mjs" 2>/dev/null | head -n1 || true
#                              ^^^^^^^^^^^^^^^^^^^^^^^ 双引号让 bash 展开 $HOME
```

> 项目里其他所有 shell 命令（`qq_check`、`qq_pkg cat`、`napcat_check`、`rm -rf`）都用双引号。
> 此修复属于"对齐既有约定"，无新约束。

**不再回退 `napcat/package.json`**：实测 `NapCat.Shell.zip` 里这个文件的 `version` 恒为 monorepo 占位 `"0.0.1"`，写入档案会比 `None` 更具误导性。返回 `None` 是更安全的失败语义。官方 `NapCat-Installer/new/installer.py` 读这个值仅用于触发更新比对（`(0,0,1) < (4,18,1)` 永远更新），并非显示用途。

#### 8.1.4 防回归测试

[`test_real_napcat_shell_zip_mjs_format`](script/test/test_remote_deploy_probe.py) 直接 byte-for-byte 用真实 mjs:10375 行作为输入：

```python
real_mjs_line = (
    'const napCatVersion = typeof (__vite_import_meta_env__) '
    '!== "undefined" && "4.18.1" || "1.0.0-dev";'
)
assert probe.installed_napcat_version == "4.18.1"
```

任何未来重新引入 `[^"]*` 字符类或单引号路径的改动都会被这条用例直接打挂。

### 8.2 新增：刷新按钮触发后台版本重探

#### 8.2.1 动机

部署完成时若版本未探到（如 8.1 修复前的存档数据），用户没有手段在不重装的情况下补全版本号。点旧版"刷新"只是 `_reload()` 重渲染列表，不会触发任何远端命令。

#### 8.2.2 设计

新增一条**轻量探测路径**，与 `deploy_server` 共享 `detect_installation` 但不跑安装脚本：

```python
# core/remote/server_manager.py
def redetect_versions(self, server_id: str) -> tuple[str | None, str | None]:
    """**同步**重新探测远端 NapCat / QQ 版本号并写回档案。"""
    ...
```

UI 端 [`RemotePage._on_refresh`](src/desktop/ui/page/remote_page/__init__.py) 改造为：

1. `self._reload()` 重渲染列表（保留旧行为）
2. 遍历所有 `DEPLOYED` 服务器（跳过正在部署的），对每台后台触发 [`RedetectRunner`](src/desktop/ui/page/remote_page/deployment_runner.py)
3. 每台完成后通过 `success_bar` / `error_bar` 反馈，`server_updated` 信号让卡片自动重绘

### 8.3 重构：开发者回滚 UI 归位到 设置→开发者

#### 8.3.1 动机

最初 dev 回滚按钮直接挂在 `RemotePage` 工具栏（仅 `--dev` 启动可见）。用户指出这违反项目约定 —— 其他 dev 功能（崩溃诊断、首页通知测试、MSI 更新测试）统一聚合在 `设置→开发者` 页面，不污染主功能 UI。

#### 8.3.2 落地

| 删除                                                       | 新增                                                                                                                    |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `RemotePage.rollback_btn` 工具栏按钮                       | [`RemoteRollbackCard`](src/desktop/ui/page/setup_page/sub_page/developer.py) 仿 `ActionButtonCard` 自定义 `SettingCard` |
| `RemotePage._on_rollback` / `_on_rollback_runner_finished` | `Developer.remote_group` 新分组 "远程部署调试"                                                                          |
| `is_developer_mode_enabled` 在 `RemotePage` 的判断分支     | 卡片内嵌 `ComboBox` 列出所有服务器 + 状态标签（`已部署 4.18.1` / `未部署` / `失败`）                                    |

`Developer` 页面整体由 [`SetupWidget.__init__`](src/desktop/ui/page/setup_page/__init__.py) 的 `is_developer_mode_enabled()` 守卫，新分组天然只在 `--dev` 启动时存在。

`RemoteRollbackCard` 行为：

1. `installEventFilter` 监听 ComboBox 鼠标按下 → 重新查询 `ServerManager`，避免与 `RemotePage` 的添加/删除不同步
2. 点 **执行回滚** → `AskBox` 危险二次确认（列出会清空的所有目录）
3. 弹独立 [`DeploymentConsoleDialog`](src/desktop/ui/page/remote_page/deployment_console.py)，复用其对 `deployment_log/progress/finished` 的订阅
4. 后台 [`RollbackRunner`](src/desktop/ui/page/remote_page/deployment_runner.py) 跑 `ServerManager.rollback_server`

#### 8.3.3 命名规范

`_RollbackRunner` / `_RedetectRunner` 跨包导入需要公开命名，去下划线：

- `_RollbackRunner` → [`RollbackRunner`](src/desktop/ui/page/remote_page/deployment_runner.py)
- `_RedetectRunner` → [`RedetectRunner`](src/desktop/ui/page/remote_page/deployment_runner.py)

### 8.4 测试基线更新

```pwsh
python -m pytest script/test/test_ssh_line_splitter.py `
                 script/test/test_remote_deploy_probe.py `
                 script/test/test_remote_deploy_runner.py `
                 script/test/test_server_manager_deploy.py `
                 script/test/test_local_backend.py `
                 script/test/test_server_registry.py
```

实测：**`102 passed in 0.58s`**

| 增量分类      | 用例                                                                  |
| ------------- | --------------------------------------------------------------------- |
| 真实 mjs 回归 | `test_real_napcat_shell_zip_mjs_format`                               |
| 回滚          | `TestRollback.test_rollback_calls_clean_environment_and_resets_state` |
| 回滚          | `TestRollback.test_rollback_include_qq_false`                         |
| 回滚          | `TestRollback.test_rollback_failure_emits_failed_signal`              |
| 回滚          | `TestRollback.test_rollback_concurrent_raises`                        |
| 回滚          | `TestRollback.test_rollback_unknown_server_raises_keyerror`           |

### 8.5 接口增量（与第 4 节锁定接口兼容）

新增的均为**纯增**，老接口签名不变：

```python
# core/remote/server_manager.py
class ServerManager(QObject):
    def redetect_versions(self, server_id: str) -> tuple[str | None, str | None]: ...
    def rollback_server(
        self,
        server_id: str,
        *,
        include_qq: bool = True,
        log_callback: Callable[[str], None] | None = None,
    ) -> None: ...

# ui/page/remote_page/deployment_runner.py
class RedetectRunner(QRunnable): ...
class RollbackRunner(QRunnable): ...
```

### 8.6 用户实机验收记录

应用日志确认（2026-04-30 21:08）：

```
[INFO] 远端版本探测完成: id=678c6195-..., napcat=4.18.1, qq=3.2.25-45758
```

UI 卡片正确显示 NapCat 版本号；刷新按钮触发后台探测无报错；设置→开发者→远程部署调试 卡片下拉框正确列出服务器。

✅ **P1 / P1.5 含本批增量补丁正式验收通过**。

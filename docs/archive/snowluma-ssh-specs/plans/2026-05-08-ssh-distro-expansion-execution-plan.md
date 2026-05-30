# SSH 远程支持边界扩展 — 执行计划

- **关联需求**：`docs/requirements/2026-05-08-ssh-distro-expansion.md`
- **内部执行级别 (Internal Grade)**：**L** — 单 agent 串行执行；任务无可独立并发的子项
- **runtime**：`interactive_governed`（root_governed lane）

---

## Wave 结构

| Wave | 名称 | 依赖 | 串行 / 并行 |
|---|---|---|---|
| W1 | 数据层：distro 矩阵 | — | 串行 |
| W2 | probe 增强 + 兼容性评估 | W1 | 串行 |
| W3 | deploy_server preflight 串接 + 友好文案 | W2 | 串行 |
| W4 | 脚本层 dep 调整（epel bootstrap） | W2 | 串行（与 W3 文件无重叠，但保守串行） |
| W5 | UI 文案 / placeholder 调整 | — | 串行（与 W1-W4 无依赖） |
| W6 | 单元测试 | W1-W5 | 串行 |
| W7 | 验证 + phase_cleanup | W6 | 串行 |

> 内部 grade 选 L 而非 XL：所有 wave 都改少量 Python 文件 + 1 个 shell 脚本，
> 实际改动量 < 600 行；多 agent fan-out 只会增加合并风险，不会缩短关键路径。

---

## W1 — `src/core/remote/distro_matrix.py`

**Owner boundary**：仅新建该文件，不改其他文件。

**输出**：

```python
@dataclass(frozen=True, slots=True)
class DistroEntry:
    distro_id: str            # /etc/os-release ID
    family: Literal["debian", "rhel"]
    package_manager: Literal["apt-get", "dnf", "yum"]
    qq_installer: Literal["dpkg", "rpm"]
    display_name: str
    support_tier: Literal["primary", "compatible", "experimental"]

KNOWN_DISTROS: tuple[DistroEntry, ...] = (
    DistroEntry("ubuntu", "debian", "apt-get", "dpkg", "Ubuntu", "primary"),
    DistroEntry("debian", "debian", "apt-get", "dpkg", "Debian", "compatible"),
    DistroEntry("centos", "rhel", "dnf", "rpm", "CentOS / CentOS Stream", "compatible"),
    DistroEntry("rhel", "rhel", "dnf", "rpm", "RHEL", "compatible"),
    DistroEntry("rocky", "rhel", "dnf", "rpm", "Rocky Linux", "compatible"),
    DistroEntry("almalinux", "rhel", "dnf", "rpm", "AlmaLinux", "compatible"),
    DistroEntry("fedora", "rhel", "dnf", "rpm", "Fedora", "experimental"),
)

def lookup_distro(distro_id: str | None) -> DistroEntry | None: ...
def lookup_by_id_like(id_like: str | None) -> DistroEntry | None: ...
```

**验证命令**：
```pwsh
python -c "from src.core.remote.distro_matrix import KNOWN_DISTROS, lookup_distro; print(len(KNOWN_DISTROS), lookup_distro('ubuntu').family)"
```

**期望输出**：`7 debian`

---

## W2 — `src/core/remote/deployment.py`

**Owner boundary**：仅修改 `LinuxCoreDeploymentProbe` 与新增 `evaluate_compatibility()`。

**改动点**：

1. `_parse_os_release` 增加解析 `ID_LIKE`（用于 Rocky 这类带 `ID=rocky ID_LIKE="rhel centos"` 的发行版）
2. `LinuxCoreDeploymentProbe` 加 `id_like: str | None = None`
3. 新增 `@dataclass CompatibilityReport`：`{compat_status, distro_entry, family, reasons: list[str]}`
4. 新增 `LinuxCoreDeploymentProbe.evaluate_compatibility() -> CompatibilityReport`，规则：
   - `is_supported_arch=False` → `unsupported`，reason 含原始 arch
   - 已知 distro 命中且有对应 installer → `supported`
   - distro 未命中但 `has_dpkg or has_rpm2cpio` → `unknown_but_runnable`
   - 否则 → `unsupported`

**验证命令**：
```pwsh
python -c "from src.core.remote.deployment import LinuxCoreDeploymentProbe as P; p=P(os_name='Linux',architecture='x86_64',normalized_arch='amd64',distro_id='ubuntu',distro_version='24.04',has_bash=True,has_tar=True,has_unzip=True,has_curl=True,has_dpkg=True,has_rpm2cpio=False,has_xvfb=True,has_linuxqq=False,has_napcat=False,installed_qq_version=None,installed_napcat_version=None); print(p.evaluate_compatibility().compat_status)"
```

**期望输出**：`supported`

---

## W3 — `src/core/remote/server_manager.py` + `friendly_errors.py`

**Owner boundary**：
- `server_manager.deploy_server`：在 `_emit_progress("准备 SSH 连接", 0)` 后、
  `install_qq` 之前插入 preflight；
- `friendly_errors.py`：新增 `_format_remote_deployment` 内 stage="preflight" 的人话文案
  （或在 `to_friendly` 注册表里追加专门 handler）

**插入逻辑**：

```python
backend.connect()

# ----- Stage 0: 兼容性体检 -----
self.deployment_log.emit(server_id, "[PREFLIGHT] 正在探测远端环境...")
probe = backend.deployment.probe_environment()
report = probe.evaluate_compatibility()
self.deployment_log.emit(
    server_id,
    f"[PREFLIGHT] distro={probe.distro_id or 'unknown'} "
    f"version={probe.distro_version or '-'} arch={probe.normalized_arch or probe.architecture} "
    f"family={report.family or '-'} status={report.compat_status}",
)
if report.compat_status == "unsupported":
    raise RemoteDeploymentError(
        "preflight",
        f"远端系统暂不支持自动部署: {'; '.join(report.reasons)}",
    )
if report.compat_status == "unknown_but_runnable":
    self.deployment_log.emit(
        server_id,
        "[PREFLIGHT] 警告: 未识别的发行版，但探测到可用的包管理器，将尝试通用流程。",
    )
```

**friendly_errors 改动**：`_format_remote_deployment` 已经走通用拼接，stage="preflight"
会得到形如「远端 preflight 失败: ...」的输出；对此**追加**专属 handler：

```python
def _format_remote_deployment(exc):
    stage = getattr(exc, "stage", "") or "部署"
    if stage == "preflight":
        text = str(exc).strip()
        return text or "远端环境兼容性体检未通过"
    ...
```

---

## W4 — `src/resource/script/remote_install_linuxqq.sh`

**改动点**：

`install_missing_dependencies` 的 dnf 分支前置 `epel-release` bootstrap：

```bash
sudo dnf install -y epel-release || true
sudo dnf install --allowerasing -y \
    curl unzip xorg-x11-server-Xvfb xauth procps-ng jq python3 ...
```

**保留**：apt-get 路径里的 `rpm2cpio cpio` 不变（Debian/Ubuntu 上它们由 `rpm2cpio`
独立包提供，apt 找不到包时 `|| true` 会兜住，不会硬失败）。这一兜底语义已经
在 line 190 `|| true` 中存在，无需额外动。

**验证命令**：bash 语法 lint
```pwsh
bash -n src\resource\script\remote_install_linuxqq.sh
```

---

## W5 — UI 文案

**改动点**：
1. `src/ui/page/remote_page/__init__.py:90` — 替换提示文本
2. `src/ui/page/remote_page/server_edit_dialog.py:219` — placeholder 改为 `"root / 用户名"`

**新文案**：

```
"远程服务器功能已支持 Debian/Ubuntu 与 RHEL 系（CentOS / Rocky / AlmaLinux / Fedora），"
"架构覆盖 amd64 与 arm64。\n\n"
"主要在 Ubuntu 24 上做过完整实测，其他发行版做过分发逻辑覆盖；首次部署时若遇到"
"特定发行版相关的问题，请提交 Issue 帮助我们扩展实测矩阵。\n\n"
"此功能会连接你的服务器并执行安装、更新、回滚等远端操作，存在一定危险性。"
"项目已经尽可能完善校验与保护，但仍请你根据自身情况谨慎使用。"
```

---

## W6 — 单元测试

**测试 1**：`script/test/test_remote_distro_matrix.py`
- KNOWN_DISTROS 长度 ≥ 7
- `lookup_distro("ubuntu").family == "debian"`
- `lookup_distro("rocky").qq_installer == "rpm"`
- `lookup_distro("alpine") is None`
- 每条 entry 的 `qq_installer` 必须与 `family` 一致（debian↔dpkg, rhel↔rpm）

**测试 2**：`script/test/test_remote_preflight.py`
- 用 `unittest.mock` mock 一个 `RemoteBackend`，让 `deployment.probe_environment()`
  返回 supported / unsupported / unknown_but_runnable 三种 probe；
- 断言 supported 路径会走到 install_qq；
- 断言 unsupported 路径**不**调 install_qq 且抛 `RemoteDeploymentError(stage="preflight")`；
- 断言三个分支都会 emit `[PREFLIGHT]` 开头的 deployment_log 行。

**测试 3**（轻量）：`script/test/test_remote_distro_matrix.py` 末尾追加
`test_evaluate_compatibility_*` 直接构造 `LinuxCoreDeploymentProbe` 实例
覆盖 4 个分支。

**验证命令**：
```pwsh
.venv\Scripts\python.exe -m pytest script\test\test_remote_distro_matrix.py script\test\test_remote_preflight.py -q
```

---

## W7 — 验证 & phase_cleanup

1. 跑完整远程相关测试基线：
```pwsh
.venv\Scripts\python.exe -m pytest script\test -q -k "remote or server_manager or deployment or distro or preflight"
```
2. grep 兜底验证：
```pwsh
findstr /s /n /i "仅支持 Ubuntu" src\
findstr /s /n /i "Ubuntu 24" src\
```
3. 写 `outputs/runtime/vibe-sessions/<run-id>/cleanup-receipt.json`
4. 写 `outputs/runtime/vibe-sessions/<run-id>/phase-*.json` 三份（W1-W2 / W3-W5 / W6-W7）

---

## 完成语言规则

- 所有 wave **未通过**对应验证命令前，对外不能用「全部完成」「交付完成」
- W6 测试退出码非 0 → 必须报告失败 stage 与 stderr
- W7 grep 仍命中 "仅支持 Ubuntu" → 不能声明 W5 已完成

## 回滚规则

- W1-W2 失败：删除 `distro_matrix.py`，恢复 `deployment.py` 中改动（git checkout）
- W3 失败：仅恢复 `server_manager.py` / `friendly_errors.py`
- W4 失败：恢复 `remote_install_linuxqq.sh`
- W6 失败：保留代码但报告未完成，不写 cleanup-receipt 的 success 字段

## 阶段清理预期

- W1-W7 完成后写：
  - `outputs/runtime/vibe-sessions/<run-id>/skeleton-receipt.json`（启动时已可写）
  - `outputs/runtime/vibe-sessions/<run-id>/intent-contract.json`
  - `outputs/runtime/vibe-sessions/<run-id>/phase-W1-W2.json`
  - `outputs/runtime/vibe-sessions/<run-id>/phase-W3-W5.json`
  - `outputs/runtime/vibe-sessions/<run-id>/phase-W6-W7.json`
  - `outputs/runtime/vibe-sessions/<run-id>/cleanup-receipt.json`
- 临时脚本 / 调试 print **不**应残留在 `src/`、`script/test/`

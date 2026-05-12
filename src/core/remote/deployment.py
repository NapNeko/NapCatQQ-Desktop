# -*- coding: utf-8 -*-
"""Linux Core 部署器 (P1 拆分版) . 

P1 阶段重写要点: 
- `probe_environment` 大幅增强: 识别 OS / 发行版 / 架构 / 已有 LinuxQQ / 已有 NapCat / 版本号
- 拆出 `install_linuxqq` / `install_napcat` 独立方法, 分别负责对应阶段
- 通过 `[PROGRESS] N message` 进度协议把脚本运行进度回传到 ProgressCallback
- 部署 launcher 脚本到 ``$workspace_dir/napcat.sh`` (P2 用, 但 P1 部署期一并落地) 

历史 API (一站式 `upload_deploy_script` / `run_deploy_script` / `export_and_upload_current_config`) 
保留以兼容已有调用方, 标记为 deprecated. 
"""

from __future__ import annotations

import re
import tempfile
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath
from typing import Literal

from src.core.config.config_export import ExportExecutionPlan, apply_config_export, scan_current_config_export
from src.core.logging import LogSource, LogType, logger

from .distro_matrix import DistroEntry, DistroFamily, lookup_by_id_like, lookup_distro
from .errors import RemoteCommandError, SSHConnectionError
from .execution_backend import ExecutionBackend, RemoteExecutionBackend
from .models import LinuxCorePaths, RemoteCommandResult
from .ssh_client import SSHClient
from .templates import (
    build_install_linuxqq_script,
    build_install_napcat_script,
    build_linux_deploy_script,
    build_napcat_launcher_script,
)


# ==================== 类型与常量 ====================
NormalizedArch = Literal["amd64", "arm64"]

# (message, percent_0_to_100) 进度回调
ProgressCallback = Callable[[str, int], None]

# (line) 原始日志行回调; P1.5 用于把脚本 stdout 实时投递给"部署控制台"
LogLineCallback = Callable[[str], None]

# 解析脚本 stdout 中的 `[PROGRESS] N message`
_PROGRESS_LINE_PATTERN = re.compile(r"^\[PROGRESS\]\s+(\d{1,3})\s+(.*)$")
# NapCat 版本号探测: 与本地 [`VersioningService._get_napcat_version_from_mjs`]
# (src/core/versioning/service.py) 完全一致.
#
# 真实 napcat.mjs 中的形态(经实测下载 NapCat.Shell.zip 验证):
#   const napCatVersion = typeof (__vite_import_meta_env__) !== "undefined" && "4.18.1" || "1.0.0-dev";
#
# 关键点: 等号到目标版本 ``"4.18.1"`` 之间隔了 ``"undefined"`` 字符串字面量,
# 因此正则**必须用 .*? 非贪婪**, 不能用 ``[^"]*`` (后者无法跨越中间引号).
_NAPCAT_VERSION_PATTERN = re.compile(
    r'napCatVersion\s*=\s*.*?"(\d+\.\d+\.\d+(?:[-+][^"]+)?)"'
)
_QQ_VERSION_PATTERN = re.compile(r'"version"\s*:\s*"([^"]+)"')


CompatStatus = Literal["supported", "unknown_but_runnable", "unsupported"]


@dataclass(slots=True)
class CompatibilityReport:
    """远端环境兼容性评估结果 (扩展 SSH 支持边界后引入).

    - ``compat_status``: 兜底分级, ``unsupported`` 时 [`ServerManager.deploy_server`]
      (src/core/remote/server_manager.py) 会以 ``stage="preflight"`` 提前失败
    - ``distro_entry``: 命中 [`KNOWN_DISTROS`](src/core/remote/distro_matrix.py)
      时为对应 entry; 通过 ID_LIKE 兜底命中也会写这里
    - ``family``: 推断到的发行版家族 (``debian`` / ``rhel``); 未命中时为 ``None``
    - ``reasons``: 给用户看的人话原因列表 (中文短句), 已脱敏
    """

    compat_status: CompatStatus
    distro_entry: DistroEntry | None
    family: DistroFamily | None
    reasons: tuple[str, ...] = ()


@dataclass(slots=True)
class LinuxCoreDeploymentProbe:
    """Linux Core 环境探测结果 (P1 增强版) . """

    os_name: str
    architecture: str
    normalized_arch: NormalizedArch | None
    distro_id: str | None
    distro_version: str | None
    has_bash: bool
    has_tar: bool
    has_unzip: bool
    has_curl: bool
    has_dpkg: bool
    has_rpm2cpio: bool
    has_dnf: bool
    has_xvfb: bool
    has_linuxqq: bool
    has_napcat: bool
    installed_qq_version: str | None
    installed_napcat_version: str | None
    id_like: str | None = None

    @property
    def is_supported_arch(self) -> bool:
        """是否落在 P1 支持的架构白名单内. """
        return self.normalized_arch in ("amd64", "arm64")

    @property
    def has_package_installer(self) -> bool:
        return self.has_dpkg or self.has_rpm2cpio

    def evaluate_compatibility(self) -> CompatibilityReport:
        """基于探测结果与 [`distro_matrix`](src/core/remote/distro_matrix.py)
        给出 deploy preflight 用的兼容性评估.

        判定优先级:

        1. 架构白名单 (amd64 / arm64) 不通过 -> ``unsupported``
        2. 没有任何 LinuxQQ 解包能力 (``has_dpkg`` 与 ``has_rpm2cpio`` 均 False)
           -> ``unsupported``
        3. ``distro_id`` / ``id_like`` 命中 [`KNOWN_DISTROS`]
           且 ``qq_installer`` 与探测到的解包工具一致 -> ``supported``
        4. 命中但 ``qq_installer`` 与探测工具不匹配 (例如声称 rhel 但远端没有
           rpm2cpio) -> ``unsupported`` 并解释原因
        5. 未命中 (``distro_id=None`` 或 distro 不在白名单) 但探测到 dpkg / rpm2cpio
           -> ``unknown_but_runnable``
        """
        reasons: list[str] = []

        if not self.is_supported_arch:
            reasons.append(f"不支持的 CPU 架构: {self.architecture or 'unknown'} (仅支持 amd64 / arm64)")
            return CompatibilityReport("unsupported", None, None, tuple(reasons))

        if not (self.has_dpkg or self.has_rpm2cpio):
            reasons.append(
                "远端缺少 LinuxQQ 解包工具: 既没有 dpkg, 也没有 rpm2cpio + cpio"
            )
            return CompatibilityReport("unsupported", None, None, tuple(reasons))

        entry = lookup_distro(self.distro_id) or lookup_by_id_like(self.id_like)

        if entry is not None:
            installer_ok = (
                (entry.qq_installer == "dpkg" and self.has_dpkg)
                or (entry.qq_installer == "rpm" and self.has_rpm2cpio)
            )
            if installer_ok:
                return CompatibilityReport("supported", entry, entry.family, ())
            reasons.append(
                f"已识别为 {entry.display_name}, 但远端缺少 "
                f"{'dpkg' if entry.qq_installer == 'dpkg' else 'rpm2cpio + cpio'}, "
                "无法解包对应 LinuxQQ 安装包"
            )
            return CompatibilityReport("unsupported", entry, entry.family, tuple(reasons))

        # 未识别但有 installer: 让 deploy 走 best-effort
        reasons.append(
            f"未识别的发行版 (distro_id={self.distro_id or 'unknown'}), "
            "但探测到可用的包管理器, 将以通用流程尝试部署"
        )
        return CompatibilityReport("unknown_but_runnable", None, None, tuple(reasons))


@dataclass(slots=True)
class RemoteConfigSyncResult:
    """远端配置同步结果. """

    remote_archive_path: str
    archive_name: str
    app_exported: bool
    bot_exported: bool
    exported_bot_count: int
    exported_files: tuple[str, ...]
    warnings: tuple[str, ...]


@dataclass(slots=True)
class RemoteDeployScriptResult:
    """远端部署脚本执行结果. """

    remote_script_path: str
    script_result: RemoteCommandResult


@dataclass(slots=True)
class InstallStepResult:
    """单个安装阶段 (install_linuxqq / install_napcat) 的执行结果. """

    step: Literal["install_linuxqq", "install_napcat"]
    remote_script_path: str
    exit_status: int
    stdout: str
    stderr: str = ""
    progress_events: list[tuple[int, str]] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return self.exit_status == 0


# ==================== 部署器 ====================
class LinuxCoreDeployment:
    """Linux Core 部署器. 

    P1 之后建议使用 [`install_linuxqq`](src/core/remote/deployment.py)
    与 [`install_napcat`](src/core/remote/deployment.py) 分两步部署, 
    历史一站式 API (`upload_deploy_script` / `run_deploy_script`) 仅保留兼容入口. 
    """

    def __init__(self, backend: ExecutionBackend, paths: LinuxCorePaths | None = None) -> None:
        self.backend = backend
        self.paths = paths or LinuxCorePaths()

    # ==================== 探测 ====================
    def probe_environment(self) -> LinuxCoreDeploymentProbe:
        """探测远端 Linux 环境. 

        与 P0 阶段相比, 新增:
        - ``/etc/os-release`` 解析得到发行版 ID / VERSION_ID
        - ``uname -m`` 归一化成 ``amd64`` / ``arm64``
        - 已有 LinuxQQ / NapCat 检测与版本号读取
        - 包管理器与 ``xvfb-run`` 等关键工具的存在性
        """
        os_result = self.backend.run("uname -s")
        arch_result = self.backend.run("uname -m")

        os_release_result = self.backend.run(
            "test -f /etc/os-release && cat /etc/os-release || true"
        )
        distro_id, distro_version, id_like = self._parse_os_release(os_release_result.stdout)

        has_bash = self.backend.run("command -v bash >/dev/null 2>&1").ok
        has_tar = self.backend.run("command -v tar >/dev/null 2>&1").ok
        has_unzip = self.backend.run("command -v unzip >/dev/null 2>&1").ok
        has_curl = self.backend.run("command -v curl >/dev/null 2>&1").ok
        has_dpkg = self.backend.run("command -v dpkg >/dev/null 2>&1").ok
        has_rpm2cpio = self.backend.run(
            "command -v rpm2cpio >/dev/null 2>&1 && command -v cpio >/dev/null 2>&1"
        ).ok
        has_dnf = self.backend.run("command -v dnf >/dev/null 2>&1").ok
        has_xvfb = self.backend.run("command -v xvfb-run >/dev/null 2>&1").ok

        # 已有 LinuxQQ
        qq_check = self.backend.run(
            f'test -x "{self.paths.qq_executable}" && test -f "{self.paths.qq_package_json_path}" '
            f"&& echo yes || echo no"
        )
        has_linuxqq = qq_check.ok and qq_check.stdout.strip() == "yes"

        installed_qq_version: str | None = None
        if has_linuxqq:
            qq_pkg_result = self.backend.run(
                f'cat "{self.paths.qq_package_json_path}" 2>/dev/null || true'
            )
            if qq_pkg_result.ok and qq_pkg_result.stdout.strip():
                m = _QQ_VERSION_PATTERN.search(qq_pkg_result.stdout)
                if m:
                    installed_qq_version = m.group(1).strip() or None

        # 已有 NapCat
        napcat_check = self.backend.run(
            f'test -f "{self.paths.napcat_dir}/napcat.mjs" && echo yes || echo no'
        )
        has_napcat = napcat_check.ok and napcat_check.stdout.strip() == "yes"

        installed_napcat_version: str | None = None
        if has_napcat:
            installed_napcat_version = self._detect_napcat_version()

        raw_arch = arch_result.stdout.strip()
        normalized_arch = self._normalize_arch(raw_arch)

        return LinuxCoreDeploymentProbe(
            os_name=os_result.stdout.strip(),
            architecture=raw_arch,
            normalized_arch=normalized_arch,
            distro_id=distro_id,
            distro_version=distro_version,
            has_bash=has_bash,
            has_tar=has_tar,
            has_unzip=has_unzip,
            has_curl=has_curl,
            has_dpkg=has_dpkg,
            has_rpm2cpio=has_rpm2cpio,
            has_dnf=has_dnf,
            has_xvfb=has_xvfb,
            has_linuxqq=has_linuxqq,
            has_napcat=has_napcat,
            installed_qq_version=installed_qq_version,
            installed_napcat_version=installed_napcat_version,
            id_like=id_like,
        )

    def _detect_napcat_version(self) -> str | None:
        """探测远端 NapCat 安装版本号. 

        与本地 [`VersioningService._get_napcat_version_from_mjs`]
        (src/core/versioning/service.py) **保持完全一致** 的解析逻辑:

        - 字段名: ``napCatVersion``
        - 正则: ``r'napCatVersion\\s*=\\s*.*?"(\\d+\\.\\d+\\.\\d+(?:[-+][^"]+)?)"'``
        - 用 ``.*?`` 非贪婪以跨越中间的 ``"undefined"`` 等引号字面量

        实现差异: 远端不能把 4.4MB 的 ``napcat.mjs`` 通过 SSH 整文件回传, 改用
        ``grep -oE 'napCatVersion[^;]*'`` 在 shell 端**先抓出该字段所在的小窗口**
        (从 ``napCatVersion`` 到下一个分号), Python 端再用 ``.*?`` 正则提取版本号. 

        **注意**: 故意**不**回退到 ``napcat/package.json``, 因为 NapCat.Shell.zip
        中的 ``package.json`` 的 ``version`` 字段恒为 ``"0.0.1"`` (monorepo 占位),
        会得到误导性结果. 返回 None 比返回 ``0.0.1`` 更安全. 

        返回不带 ``v`` 前缀的纯版本号字符串(如 ``4.18.1``); 探测失败时返回 None. 
        """
        # 抓 ``napCatVersion`` 到下一个分号的局部窗口 (典型形如:
        # napCatVersion = typeof (...) !== "undefined" && "4.18.1" || "1.0.0-dev")
        # 路径用**双引号**让 bash 展开 ``$HOME`` (LinuxCorePaths 默认值含 $HOME);
        # grep 的正则模式继续用单引号防止 shell 解释 ``$``. 
        mjs_grep = self.backend.run(
            f'grep -oE \'napCatVersion[^;]*\' "{self.paths.napcat_dir}/napcat.mjs" '
            "2>/dev/null | head -n1 || true"
        )
        if not mjs_grep.ok or not mjs_grep.stdout.strip():
            return None

        match = _NAPCAT_VERSION_PATTERN.search(mjs_grep.stdout)
        if match is None:
            return None
        return match.group(1).strip() or None

    @staticmethod
    def _parse_os_release(text: str) -> tuple[str | None, str | None, str | None]:
        """解析 ``/etc/os-release`` 内容, 提取 ID / VERSION_ID / ID_LIKE.

        ID_LIKE 字段在派生发行版上至关重要: Rocky / Alma / Mint 这类发行版
        ``ID`` 各异但 ``ID_LIKE`` 会列出父发行版 (rhel / centos / debian / ubuntu),
        [`distro_matrix.lookup_by_id_like`](src/core/remote/distro_matrix.py)
        据此把它们落到正确的家族上.
        """
        if not text or not text.strip():
            return None, None, None
        distro_id: str | None = None
        distro_version: str | None = None
        id_like: str | None = None
        for raw_line in text.splitlines():
            line = raw_line.strip()
            if not line or "=" not in line or line.startswith("#"):
                continue
            key, _, value = line.partition("=")
            key = key.strip()
            value = value.strip().strip('"').strip("'")
            if key == "ID" and not distro_id:
                distro_id = value or None
            elif key == "VERSION_ID" and not distro_version:
                distro_version = value or None
            elif key == "ID_LIKE" and not id_like:
                id_like = value or None
        return distro_id, distro_version, id_like

    @staticmethod
    def _normalize_arch(raw: str) -> NormalizedArch | None:
        if raw in ("x86_64", "amd64"):
            return "amd64"
        if raw in ("aarch64", "arm64"):
            return "arm64"
        return None

    # ==================== 目录初始化 ====================
    def initialize_layout(self) -> list[RemoteCommandResult]:
        """初始化远端目录布局. """
        results: list[RemoteCommandResult] = []
        for path in (
            self.paths.workspace_dir,
            self.paths.runtime_dir,
            self.paths.config_dir,
            self.paths.log_dir,
            self.paths.tmp_dir,
            self.paths.package_dir,
        ):
            results.append(self.backend.ensure_directory(path))
        return results

    # ==================== P1 安装: 分两步 ====================
    def install_linuxqq(
        self,
        *,
        progress: ProgressCallback | None = None,
        log_callback: LogLineCallback | None = None,
        progress_log_callback: LogLineCallback | None = None,
        force_reinstall: bool = False,
        local_package_cache_dir: Path | None = None,
        should_cancel: Callable[[], bool] | None = None,
    ) -> InstallStepResult:
        """在远端安装 LinuxQQ rootless. 

        **本机下载兜底**: 当远端无法直连 ``dldir1.qq.com`` (腾讯 CDN) 时,
        若 ``local_package_cache_dir`` 非空则改为在 Desktop 本机下载 LinuxQQ 安装包
        (根据远端架构和包格式选择 deb/rpm), 再通过 SFTP 上传到
        ``${package_dir}/linuxqq_*.deb`` 或 ``*.rpm``; 远端脚本里
        ``verify_qq_package`` + 缓存复用分支会自动跳过下载并复用本地包.
        远端能直连 QQ CDN 时该路径不会触发, 行为完全不变.
        见 [`local_linuxqq_fallback`](src/core/remote/local_linuxqq_fallback.py).

        Args:
            progress: 进度协议回调, 由 ``[PROGRESS] N message`` 行触发
            log_callback: 原始日志行回调, 用于把脚本 stdout 实时透传给"部署控制台"
            progress_log_callback: ``\\r`` 终止的瞬时刷新行 (dnf/apt/curl 进度条) 回调,
                设置后这类行 *不* 再走 ``log_callback``, 由调用方自行决定如何渲染
                (典型: UI 原地覆盖上一行) 
            force_reinstall: 强制重装(会先备份 NapCat 配置再 ``rm -rf $install_base_dir/opt`` 重新解压)
            local_package_cache_dir: 本机预下载缓存目录 (一般为
                ``it(PathFunc).tmp_path``). ``None`` 时**关闭**本机兜底,
                直接交给远端脚本下载.
            should_cancel: 取消检查协议; 命中时招
                :class:`RemoteDeploymentCancelledError`.
        """
        logger.info(
            (
                f"开始远端 LinuxQQ 安装: workspace={self.paths.workspace_dir}, "
                f"force_reinstall={force_reinstall}, "
                f"local_fallback={'enabled' if local_package_cache_dir else 'disabled'}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )
        self.initialize_layout()

        # 远端 QQ CDN 连通性预检 + 必要时本机预下载并上传 package (在脚本上传/执行之前).
        if local_package_cache_dir is not None:
            self._maybe_prefetch_linuxqq_via_local(
                local_package_cache_dir=local_package_cache_dir,
                force_reinstall=force_reinstall,
                should_cancel=should_cancel,
                log_callback=log_callback,
            )

        script_content = build_install_linuxqq_script(self._build_script_variables())
        remote_script_path = self._upload_script(script_content, "remote_install_linuxqq.sh")

        env_prefix = "FORCE_LINUXQQ_REINSTALL=1 " if force_reinstall else ""
        command = f'{env_prefix}bash "{remote_script_path}"'
        result, progress_events = self._run_script_with_progress(
            command,
            progress,
            log_callback,
            progress_log_callback=progress_log_callback,
        )

        step_result = InstallStepResult(
            step="install_linuxqq",
            remote_script_path=remote_script_path,
            exit_status=result.exit_status,
            stdout=result.stdout,
            stderr=result.stderr,
            progress_events=progress_events,
        )
        if not step_result.ok:
            raise RemoteCommandError(
                command=command,
                exit_status=result.exit_status,
                stderr=self._summarize_failure(result),
            )

        logger.info(
            f"远端 LinuxQQ 安装完成: events={len(progress_events)}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        return step_result

    # ==================== LinuxQQ 本机下载兜底 ====================
    def _maybe_prefetch_linuxqq_via_local(
        self,
        *,
        local_package_cache_dir: Path,
        force_reinstall: bool,
        log_callback: LogLineCallback | None,
        should_cancel: Callable[[], bool] | None = None,
    ) -> None:
        """远端连不上 QQ CDN 时, 在本机下载 LinuxQQ 包并 SFTP 上传到远端.

        三层短路 (按代价从低到高依次判定):

        1. **远端已有合法包缓存 + 非强制重装**: 直接复用, 不动本机也不上传
        2. **远端能直连 QQ CDN**: 让脚本走自己的下载路径
        3. **以上都不命中**: 探测远端架构/包格式 -> 本机下载对应包 -> SFTP 上传

        任何一步失败都**不抛**, 仅在 log_callback 里 emit ``[WARN]``: 让远端脚本
        按原路径继续尝试, 给用户最后兜底的报错文案.
        """
        from .local_linuxqq_fallback import (
            backend_can_reach_qq_cdn,
            get_remote_package_filename,
            prefetch_linuxqq_package_locally,
        )

        # 探测远端架构和包格式, 确定要下载哪个包
        arch, pkg_format = self._detect_remote_arch_and_format(log_callback)
        if arch is None or pkg_format is None:
            # 无法确定远端环境, 退回让远端脚本自处理
            return

        remote_filename = get_remote_package_filename(arch, pkg_format)
        remote_package_path = PurePosixPath(
            self.paths.package_dir, remote_filename
        ).as_posix()

        # 短路 1: 远端已有合法包 + 非强制重装 -> 让脚本直接复用
        if not force_reinstall:
            check = self.backend.run(
                f'test -f "{remote_package_path}"', check=False
            )
            if check.exit_status == 0:
                # 用脚本同款校验: 文件 > 1MB 即视为有效 (完整性由脚本 verify_qq_package 保证)
                size_check = self.backend.run(
                    f'stat -c "%s" "{remote_package_path}" 2>/dev/null || echo 0',
                    check=False,
                )
                size_str = (size_check.stdout or "0").strip()
                try:
                    file_size = int(size_str)
                except ValueError:
                    file_size = 0
                if file_size > 1_048_576:
                    if log_callback is not None:
                        log_callback(
                            f"[INFO] 远端已存在 LinuxQQ 包缓存 ({file_size} bytes), "
                            f"跳过本机预下载: {remote_package_path}"
                        )
                    return
                # 文件存在但过小 (损坏) -> 删除并继续兜底
                self.backend.run(f'rm -f "{remote_package_path}"', check=False)
                if log_callback is not None:
                    log_callback(
                        f"[WARN] 远端 LinuxQQ 包过小 ({file_size} bytes), "
                        f"已删除并重新下载: {remote_package_path}"
                    )

        # 短路 2: 远端能直连 QQ CDN -> 让脚本自己处理
        try:
            reachable = backend_can_reach_qq_cdn(self.backend, log_callback=log_callback)
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"QQ CDN 连通性探测失败, 走本机兜底: {type(exc).__name__}: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            reachable = False
        if reachable:
            return

        # 短路 3: 走本机下载 + SFTP 上传
        if log_callback is not None:
            log_callback(
                "[WARN] 远端无法直连 QQ CDN (dldir1.qq.com), 切换到本机下载 + SFTP 上传兜底"
            )
        try:
            local_target = local_package_cache_dir / remote_filename
            prefetch_linuxqq_package_locally(
                target_path=local_target,
                arch=arch,
                pkg_format=pkg_format,
                log_callback=log_callback,
                should_cancel=should_cancel,
            )
            # 上传前再检查一次取消
            if should_cancel is not None and should_cancel():
                from .errors import RemoteDeploymentCancelledError as _Cancelled

                raise _Cancelled()
            self.backend.ensure_directory(self.paths.package_dir)
            self.backend.upload_file(local_target, remote_package_path)
            if log_callback is not None:
                log_callback(
                    f"[INFO] 本机 LinuxQQ 包已上传到远端: {remote_package_path}"
                )
        except Exception as exc:  # noqa: BLE001
            from .errors import RemoteDeploymentCancelledError as _Cancelled2

            if isinstance(exc, _Cancelled2):
                raise
            logger.warning(
                f"LinuxQQ 本机兜底失败, 退回远端脚本自下载: {type(exc).__name__}: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            if log_callback is not None:
                log_callback(
                    f"[WARN] 本机下载兜底失败 ({type(exc).__name__}), "
                    "退回远端脚本自行下载 (可能超时)"
                )

    def _detect_remote_arch_and_format(
        self, log_callback: LogLineCallback | None
    ) -> tuple[str | None, str | None]:
        """探测远端架构和包格式, 用于确定要下载哪个 LinuxQQ 包.

        Returns:
            (arch, pkg_format) 或 (None, None) 如果无法确定.
            arch: ``"amd64"`` 或 ``"arm64"``
            pkg_format: ``"dpkg"`` 或 ``"rpm"``
        """
        # 探测架构
        arch_result = self.backend.run("uname -m", check=False)
        raw_arch = (arch_result.stdout or "").strip()
        arch: str | None = None
        if raw_arch in ("x86_64", "amd64"):
            arch = "amd64"
        elif raw_arch in ("aarch64", "arm64"):
            arch = "arm64"
        else:
            if log_callback is not None:
                log_callback(
                    f"[WARN] 无法确定远端架构 (uname -m={raw_arch!r}), "
                    "跳过本机 LinuxQQ 预下载"
                )
            return None, None

        # 探测包格式
        dpkg_check = self.backend.run("command -v dpkg", check=False)
        rpm_check = self.backend.run("command -v rpm2cpio", check=False)
        pkg_format: str | None = None
        if dpkg_check.exit_status == 0:
            pkg_format = "dpkg"
        elif rpm_check.exit_status == 0:
            pkg_format = "rpm"
        else:
            if log_callback is not None:
                log_callback(
                    "[WARN] 远端无 dpkg 也无 rpm2cpio, 跳过本机 LinuxQQ 预下载"
                )
            return None, None

        return arch, pkg_format

    #: ``remote_install_napcat.sh`` 在 SHA512 校验失败时使用的 dedicated 退出码 (P5 F1.4).
    INSTALL_NAPCAT_VERIFY_EXIT_CODE: int = 36

    def install_napcat(
        self,
        *,
        progress: ProgressCallback | None = None,
        log_callback: LogLineCallback | None = None,
        progress_log_callback: LogLineCallback | None = None,
        force_update: bool = False,
        download_url: str | None = None,
        expected_sha512: str | None = None,
        local_archive_cache: Path | None = None,
        should_cancel: Callable[[], bool] | None = None,
    ) -> InstallStepResult:
        """在远端安装/更新 NapCat. 

        默认仅在远端不存在 NapCat 时下载; 设置 ``force_update=True`` 强制重新下载并解压. 
        部署完成后会自动把 launcher 脚本上传到 ``$workspace_dir/napcat.sh``. 

        **本机下载兜底**: 当远端无法直连 ``github.com`` 时 (典型: 国内云服务商 + 出方向受限),
        若 ``local_archive_cache`` 非空则改为在 Desktop 本机下载 NapCat.Shell.zip
        (走 [`Urls.MIRROR_SITE`](src/core/network/urls.py) 镜像列表), 再通过 SFTP
        上传到 ``${package_dir}/NapCat.Shell.zip``; 远端脚本里 ``[ -f archive ]`` 分支
        会自动跳过下载并复用本地包. 远端能直连 GitHub 时该路径不会触发, 行为完全不变.
        见 [`local_napcat_fallback`](src/core/remote/local_napcat_fallback.py).

        Args:
            progress: 进度协议回调, 由 ``[PROGRESS] N message`` 行触发
            log_callback: 原始日志行回调, 用于把脚本 stdout 实时透传给"部署控制台"
            progress_log_callback: ``\\r`` 终止的瞬时刷新行 (dnf/apt/curl 进度条) 回调,
                设置后这类行 *不* 再走 ``log_callback``, 由调用方自行决定如何渲染
                (典型: UI 原地覆盖上一行) 
            force_update: 强制重新下载并解压 NapCat
            download_url: 自定义下载地址(覆盖 ``NAPCAT_DOWNLOAD_URL``); **设置后本机兜底
                自动禁用**, 因为自定义 URL 可能就是为了绕过 GitHub, 应当尊重调用方意图
            expected_sha512: P5 F1.4: NapCat.Shell.zip 的期望 SHA512 (128 位 hex);
                提供时通过 ``NAPCAT_EXPECTED_SHA512`` 环境变量传给远端脚本, 校验失败
                远端会以退出码 36 中断, 本方法把该退出码转为
                ``RemoteCommandError`` (调用方按 stage="install_napcat_verify" 区分).
                ``None`` 时跳过校验, 远端仅记 warning 不阻断 (兼容老客户端).
            local_archive_cache: 本机预下载缓存路径 (一般为
                ``it(PathFunc).tmp_path / 'NapCat.Shell.zip'``). ``None`` 时**关闭**本机兜底,
                直接交给远端脚本下载; 测试环境想强制兜底则注入 tmp 路径.

        Raises:
            RemoteCommandError: 远端脚本退出码非 0; 当 ``exit_status==36`` 时表示
                SHA512 校验失败, ``stderr`` 已包含期望与实际值, 上层应转为
                ``RemoteDeploymentError(stage="install_napcat_verify")`` 以保留语义.
        """
        logger.info(
            (
                f"开始远端 NapCat 安装: napcat_dir={self.paths.napcat_dir}, "
                f"force_update={force_update}, "
                f"sha512_verify={'enabled' if expected_sha512 else 'skipped'}, "
                f"local_fallback={'enabled' if local_archive_cache else 'disabled'}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )
        self.initialize_layout()

        # 远端 GitHub 连通性预检 + 必要时本机预下载并上传 archive (在脚本上传/执行之前).
        # download_url 非空时跳过本机兜底, 尊重调用方"我有自己的镜像 URL"的意图.
        if local_archive_cache is not None and not download_url:
            self._maybe_prefetch_napcat_archive_via_local(
                local_archive_cache=local_archive_cache,
                expected_sha512=expected_sha512,
                force_update=force_update,
                should_cancel=should_cancel,
                log_callback=log_callback,
            )

        script_content = build_install_napcat_script(self._build_script_variables())
        remote_script_path = self._upload_script(script_content, "remote_install_napcat.sh")

        env_parts: list[str] = []
        if force_update:
            env_parts.append("FORCE_NAPCAT_UPDATE=1")
        if download_url:
            env_parts.append(f'NAPCAT_DOWNLOAD_URL={self._shell_quote(download_url)}')
        if expected_sha512:
            normalized_hash = expected_sha512.strip().lower()
            env_parts.append(f"NAPCAT_EXPECTED_SHA512={self._shell_quote(normalized_hash)}")
        env_prefix = (" ".join(env_parts) + " ") if env_parts else ""
        command = f'{env_prefix}bash "{remote_script_path}"'

        result, progress_events = self._run_script_with_progress(
            command,
            progress,
            log_callback,
            progress_log_callback=progress_log_callback,
        )

        step_result = InstallStepResult(
            step="install_napcat",
            remote_script_path=remote_script_path,
            exit_status=result.exit_status,
            stdout=result.stdout,
            stderr=result.stderr,
            progress_events=progress_events,
        )
        if not step_result.ok:
            raise RemoteCommandError(
                command=command,
                exit_status=result.exit_status,
                stderr=self._summarize_failure(result),
            )

        # NapCat 安装成功后, 部署 launcher 脚本到 $workspace_dir/napcat.sh
        self.upload_launcher_script()

        logger.info(
            f"远端 NapCat 安装完成: events={len(progress_events)}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        return step_result

    # ==================== 本机下载兜底 (P? F?.?) ====================
    REMOTE_NAPCAT_ARCHIVE_NAME: str = "NapCat.Shell.zip"
    """远端 ``${package_dir}`` 下 NapCat 归档的固定文件名;
    必须与 [`remote_install_napcat.sh`](src/resource/script/remote_install_napcat.sh)
    里 ``napcat_archive_path`` 的默认值同名, 否则脚本不会复用预上传归档."""

    def _maybe_prefetch_napcat_archive_via_local(
        self,
        *,
        local_archive_cache: Path,
        expected_sha512: str | None,
        force_update: bool,
        log_callback: LogLineCallback | None,
        should_cancel: Callable[[], bool] | None = None,
    ) -> None:
        """远端连不上 GitHub 时, 在本机下载 NapCat.Shell.zip 并 SFTP 上传到远端.

        三层短路 (按代价从低到高依次判定):

        1. **远端已有归档 + 非强制更新**: 直接复用, 不动本机也不上传
        2. **远端能直连 GitHub**: 让脚本走自己的下载路径 (镜像/CDN 由 curl 处理)
        3. **以上都不命中**: 本机下载 (走 [`Urls.MIRROR_SITE`](src/core/network/urls.py))
           -> SFTP 上传到 ``${package_dir}/${REMOTE_NAPCAT_ARCHIVE_NAME}``

        任何一步失败都**不抛**, 仅在 log_callback 里 emit ``[WARN]``: 让远端脚本
        按原路径继续尝试, 给用户最后兜底的报错文案 (而不是在预检阶段就把流程挂掉).

        Args:
            local_archive_cache: 本机缓存路径, 由调用方决定 (生产 = ``PathFunc.tmp_path``,
                测试 = tmp_path)
            expected_sha512: 与 ``install_napcat`` 同字段; 用于本机下载产物校验
            force_update: ``True`` 时跳过"远端已有归档"短路, 强制重下并覆盖上传
            log_callback: 部署控制台行回调
        """
        # 延迟 import, 把 httpx / Urls 等 UI 链路依赖隔离到真正需要兜底的代码路径
        from .local_napcat_fallback import (
            backend_can_reach_github,
            prefetch_napcat_archive_locally,
        )

        remote_archive_path = PurePosixPath(
            self.paths.package_dir, self.REMOTE_NAPCAT_ARCHIVE_NAME
        ).as_posix()

        # 短路 1: 远端已有归档 + zip 完整性 OK + 非强制更新 -> 让脚本直接复用即可.
        # 关键: 必须先用 ``unzip -t`` 验证完整性, 否则上一次失败 (例如 curl 中断) 留下的
        # **损坏残件** 会被复用, 让 ``remote_install_napcat.sh`` 在解压阶段炸退出码 9
        # ("End-of-central-directory signature not found"). 损坏时自动 ``rm -f`` 并落入
        # 后续短路, 让 GitHub 直连 / 本机兜底重新拉一个干净的归档.
        if not force_update:
            check = self.backend.run(
                f'test -f "{remote_archive_path}"', check=False
            )
            if check.exit_status == 0:
                verify = self.backend.run(
                    f'unzip -t "{remote_archive_path}" >/dev/null 2>&1',
                    check=False,
                )
                if verify.exit_status == 0:
                    if log_callback is not None:
                        log_callback(
                            f"[INFO] 远端已存在 NapCat 归档缓存 (zip 完整), 跳过本机预下载: {remote_archive_path}"
                        )
                    return
                # 文件存在但损坏 (典型: 上一次下载中断的残件) -> 删除并继续兜底流程
                self.backend.run(f'rm -f "{remote_archive_path}"', check=False)
                if log_callback is not None:
                    log_callback(
                        f"[WARN] 远端 NapCat 归档损坏 (zip 校验失败), 已删除并重新下载: {remote_archive_path}"
                    )

        # 短路 2: 远端能直连 GitHub -> 让脚本自己处理, 行为完全不变
        try:
            reachable = backend_can_reach_github(self.backend, log_callback=log_callback)
        except Exception as exc:  # noqa: BLE001 - 探测失败一律视为"该走兜底"
            logger.warning(
                f"GitHub 连通性探测失败, 走本机兜底: {type(exc).__name__}: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            reachable = False
        if reachable:
            return

        # 短路 3: 走本机下载 + SFTP 上传; 任何异常都退回让远端脚本自处理
        # (例外: RemoteDeploymentCancelledError 是用户取消语义, 必须透出给上层 server_manager)
        if log_callback is not None:
            log_callback(
                "[WARN] 远端无法直连 GitHub, 切换到本机下载 + SFTP 上传兜底"
            )
        try:
            prefetch_napcat_archive_locally(
                target_path=local_archive_cache,
                expected_sha512=expected_sha512,
                log_callback=log_callback,
                should_cancel=should_cancel,
            )
            # 上传前再检查一次取消, 避免下载完了用户已经点了取消还要白白做 SFTP
            if should_cancel is not None and should_cancel():
                from .errors import RemoteDeploymentCancelledError

                raise RemoteDeploymentCancelledError()
            self.backend.ensure_directory(self.paths.package_dir)
            self.backend.upload_file(local_archive_cache, remote_archive_path)
            if log_callback is not None:
                log_callback(
                    f"[INFO] 本机 NapCat 归档已上传到远端: {remote_archive_path}"
                )
        except Exception as exc:  # noqa: BLE001 - 任何失败都不让 install_napcat 直接挂
            from .errors import RemoteDeploymentCancelledError

            if isinstance(exc, RemoteDeploymentCancelledError):
                # 用户主动取消: 不要降级为 "退回远端脚本", 直接透出让 server_manager 走 cancelled 分支
                raise
            logger.warning(
                f"本机预下载/上传 NapCat 归档失败, 退回远端脚本下载: {type(exc).__name__}: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            if log_callback is not None:
                log_callback(
                    f"[WARN] 本机兜底失败, 退回远端脚本自下载 (仍可能因 GitHub 不通而失败): {exc}"
                )

    def upload_launcher_script(self, remote_path: str | None = None) -> str:
        """上传 launcher 脚本并赋予可执行权限. """
        target_path = remote_path or self.paths.launcher_script
        script_content = build_napcat_launcher_script(self._build_script_variables())

        with tempfile.TemporaryDirectory(prefix="napcat-launcher-") as temp_dir:
            local_path = Path(temp_dir) / "napcat.sh"
            local_path.write_text(script_content, encoding="utf-8", newline="\n")
            self.backend.ensure_directory(str(PurePosixPath(target_path).parent))
            self.backend.upload_file(local_path, target_path)
        self.backend.run(f'chmod +x "{target_path}"', check=True)

        logger.info(
            f"远端 launcher 脚本已部署: target={target_path}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        return target_path

    # ==================== 历史一站式 API (保留兼容入口)  ====================
    def upload_package(self, local_archive: str | Path, remote_filename: str | None = None) -> str:
        """上传安装包到远端包目录. """
        local_file = Path(local_archive)
        filename = remote_filename or local_file.name
        remote_path = PurePosixPath(self.paths.package_dir, filename).as_posix()
        logger.info(
            (
                "准备上传远端安装包: "
                f"local={local_file}, remote={remote_path}, size={local_file.stat().st_size}"
            ),
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        self.backend.ensure_directory(self.paths.package_dir)
        self.backend.upload_file(local_file, remote_path)
        logger.info(
            f"远端安装包上传完成: remote={remote_path}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        return remote_path

    def upload_config_archive(self, local_archive: str | Path, remote_filename: str = "config-export.zip") -> str:
        """上传配置包到远端临时目录. """
        local_file = Path(local_archive)
        remote_path = PurePosixPath(self.paths.tmp_dir, remote_filename).as_posix()
        logger.info(
            (
                "准备上传远端配置包: "
                f"local={local_file}, remote={remote_path}, size={local_file.stat().st_size}"
            ),
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        self.backend.ensure_directory(self.paths.tmp_dir)
        self.backend.upload_file(local_archive, remote_path)
        logger.info(
            f"远端配置包上传完成: remote={remote_path}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        return remote_path

    def export_and_upload_current_config(
        self,
        *,
        export_app_config: bool = True,
        export_bot_config: bool = True,
        remote_filename: str = "config-export.zip",
    ) -> RemoteConfigSyncResult:
        """导出当前本地配置并上传到远端 (v1 历史 API, 用于配置同步场景) . """
        logger.info(
            "开始导出并上传当前本地配置到远端",
            log_type=LogType.FILE_FUNC,
            log_source=LogSource.CORE,
        )
        with tempfile.TemporaryDirectory(prefix="napcat-remote-export-") as temp_dir:
            scan_result = scan_current_config_export(Path(temp_dir))
            execution_result = apply_config_export(
                ExportExecutionPlan(
                    scan_result=scan_result,
                    export_app_config=export_app_config,
                    export_bot_config=export_bot_config,
                )
            )
            logger.info(
                (
                    "本地配置导出完成: "
                    f"archive={execution_result.archive_path}, app_exported={execution_result.app_exported}, "
                    f"bot_exported={execution_result.bot_exported}, exported_bot_count={execution_result.exported_bot_count}, "
                    f"warnings={list(execution_result.warnings)}"
                ),
                log_type=LogType.FILE_FUNC,
                log_source=LogSource.CORE,
            )
            remote_archive_path = self.upload_config_archive(
                execution_result.archive_path,
                remote_filename=remote_filename,
            )

        logger.info(
            f"本地配置已上传到远端: remote_archive={remote_archive_path}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        return RemoteConfigSyncResult(
            remote_archive_path=remote_archive_path,
            archive_name=remote_filename,
            app_exported=execution_result.app_exported,
            bot_exported=execution_result.bot_exported,
            exported_bot_count=execution_result.exported_bot_count,
            exported_files=execution_result.exported_files,
            warnings=execution_result.warnings,
        )

    def upload_deploy_script(self, remote_filename: str = "deploy_napcat.sh") -> str:
        """上传一站式部署脚本 (v1 历史 API, P1 起请使用 install_linuxqq / install_napcat) . """
        script_content = build_linux_deploy_script(
            {
                **self._build_script_variables(),
                "config_archive": PurePosixPath(self.paths.tmp_dir, "config-export.zip").as_posix(),
            }
        )

        with tempfile.TemporaryDirectory(prefix="napcat-remote-script-") as temp_dir:
            local_script_path = Path(temp_dir) / remote_filename
            local_script_path.write_text(script_content, encoding="utf-8", newline="\n")
            remote_script_path = PurePosixPath(self.paths.tmp_dir, remote_filename).as_posix()
            logger.info(
                (
                    "准备上传远端部署脚本: "
                    f"local={local_script_path}, remote={remote_script_path}, size={local_script_path.stat().st_size}"
                ),
                log_type=LogType.NETWORK,
                log_source=LogSource.CORE,
            )
            self.backend.ensure_directory(self.paths.tmp_dir)
            self.backend.upload_file(local_script_path, remote_script_path)

        self.backend.run(f'chmod +x "{remote_script_path}"', check=True)
        logger.info(
            f"远端部署脚本上传完成并已授予执行权限: remote={remote_script_path}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        return remote_script_path

    def run_deploy_script(self, remote_script_path: str | None = None) -> RemoteDeployScriptResult:
        """执行远端一站式部署脚本 (v1 历史 API) . """
        script_path = remote_script_path or PurePosixPath(self.paths.tmp_dir, "deploy_napcat.sh").as_posix()
        logger.info(
            f"准备执行远端部署脚本: remote={script_path}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        result = self.backend.run(f'bash "{script_path}"', check=False)
        logger.info(
            (
                "远端部署脚本执行完成: "
                f"remote={script_path}, exit_status={result.exit_status}, "
                f"stdout_len={len(result.stdout)}, stderr_len={len(result.stderr)}"
            ),
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        if result.stdout.strip():
            logger.info(
                f"远端部署脚本标准输出:\n{result.stdout}",
                log_type=LogType.NETWORK,
                log_source=LogSource.CORE,
            )
        if result.stderr.strip():
            logger.warning(
                f"远端部署脚本标准错误:\n{result.stderr}",
                log_type=LogType.NETWORK,
                log_source=LogSource.CORE,
            )
        return RemoteDeployScriptResult(remote_script_path=script_path, script_result=result)

    # ==================== 清理 ====================
    def clean_environment(self, include_qq: bool = True) -> RemoteCommandResult:
        """清理 NapCat 环境. 

        Args:
            include_qq: 是否同时清理 QQ 安装

        Returns:
            命令执行结果
        """
        logger.info(
            f"开始清理 NapCat 环境: include_qq={include_qq}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )

        # 1. 停止运行中的进程
        logger.info("停止运行中的 NapCat 进程", log_source=LogSource.CORE)
        self.backend.run('pkill -f "qq --no-sandbox" 2>/dev/null || true', check=False)

        # 2. 清理运行时目录
        logger.info("清理运行时目录", log_source=LogSource.CORE)
        self.backend.run(f'rm -rf "{self.paths.runtime_dir}" 2>/dev/null || true', check=False)
        self.backend.run(f'rm -rf "{self.paths.tmp_dir}" 2>/dev/null || true', check=False)

        # 3. 清理日志
        logger.info("清理日志文件", log_source=LogSource.CORE)
        self.backend.run(f'rm -f "{self.paths.log_dir}"/*.log 2>/dev/null || true', check=False)

        # 4. 清理 NapCat 安装
        logger.info("清理 NapCat 安装", log_source=LogSource.CORE)
        self.backend.run(f'rm -rf "{self.paths.napcat_dir}" 2>/dev/null || true', check=False)

        # 5. 清理 QQ 注入文件
        logger.info("清理 QQ 注入文件", log_source=LogSource.CORE)
        self.backend.run(
            f'rm -f "{self.paths.qq_base_path}/resources/app/loadNapCat.js" 2>/dev/null || true',
            check=False,
        )

        # 6. 恢复 QQ 原始配置 (从备份) 
        backup_path = f"{self.paths.qq_package_json_path}.backup"
        result = self.backend.run(
            f'test -f "{backup_path}" && mv "{backup_path}" "{self.paths.qq_package_json_path}" 2>/dev/null || true',
            check=False,
        )
        if result.ok:
            logger.info("已恢复 QQ 原始配置", log_source=LogSource.CORE)

        # 7. 清理启动脚本
        self.backend.run(f'rm -f "{self.paths.launcher_script}" 2>/dev/null || true', check=False)

        # 8. 可选: 清理 QQ 安装
        if include_qq:
            logger.info("清理 QQ 安装", log_source=LogSource.CORE)
            self.backend.run(f'rm -rf "{self.paths.qq_base_path}" 2>/dev/null || true', check=False)
            # 清理下载的安装包
            self.backend.run(
                f'rm -f "{self.paths.package_dir}"/*.deb "{self.paths.package_dir}"/*.rpm 2>/dev/null || true',
                check=False,
            )

        logger.info(
            "NapCat 环境清理完成",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        return RemoteCommandResult(
            command="clean_environment",
            exit_status=0,
            stdout="",
            stderr="",
        )

    # ==================== 内部辅助 ====================
    def _build_script_variables(self) -> dict[str, str]:
        """构建脚本注入用的标准变量字典. """
        return {
            "workspace_dir": self.paths.workspace_dir,
            "runtime_dir": self.paths.runtime_dir,
            "config_dir": self.paths.config_dir,
            "log_dir": self.paths.log_dir,
            "tmp_dir": self.paths.tmp_dir,
            "package_dir": self.paths.package_dir,
            "status_file": self.paths.status_file,
            "pid_file": self.paths.pid_file,
            "log_file": self.paths.log_file,
            "install_base_dir": self.paths.install_base_dir,
            "qq_base_path": self.paths.qq_base_path,
            "target_folder": self.paths.target_folder,
            "qq_executable": self.paths.qq_executable,
            "qq_package_json_path": self.paths.qq_package_json_path,
            "launcher_script": self.paths.launcher_script,
        }

    def _upload_script(self, content: str, filename: str) -> str:
        """上传脚本到远端 tmp 目录, 赋予执行权限, 返回远端路径. """
        with tempfile.TemporaryDirectory(prefix="napcat-script-") as temp_dir:
            local_path = Path(temp_dir) / filename
            local_path.write_text(content, encoding="utf-8", newline="\n")
            remote_script_path = PurePosixPath(self.paths.tmp_dir, filename).as_posix()
            self.backend.ensure_directory(self.paths.tmp_dir)
            self.backend.upload_file(local_path, remote_script_path)
        self.backend.run(f'chmod +x "{remote_script_path}"', check=True)
        return remote_script_path

    def _run_script_with_progress(
        self,
        command: str,
        progress: ProgressCallback | None,
        log_callback: LogLineCallback | None = None,
        progress_log_callback: LogLineCallback | None = None,
    ) -> tuple[RemoteCommandResult, list[tuple[int, str]]]:
        """执行脚本并解析 ``[PROGRESS] N message`` 行. 

        - ``progress``: 仅在匹配到 PROGRESS 协议行时触发
        - ``log_callback``: 每行 ``\\n`` 终止的最终 stdout(含合并的 stderr) 都会触发一次,
          用于"部署控制台"实时回显
        - ``progress_log_callback``: 每行 ``\\r`` 终止的瞬时刷新行 (dnf/apt/curl 进度条等)
          触发一次. **不为 None 时**, 这类瞬时行 *不会* 再发往 ``log_callback``,
          适合 UI 用 "原地覆盖上一行" 的方式渲染, 避免上千行刷屏污染部署控制台. 

        非 [`SSHClient`](src/core/remote/ssh_client.py) 的执行后端不支持流式读取,
        会退化为同步执行后再一次性解析进度行(仍能正确触发回调, 只是失去"实时性"). 
        """
        progress_events: list[tuple[int, str]] = []

        def _on_line(line: str) -> None:
            # 1. 实时回显原始行
            if log_callback is not None:
                try:
                    log_callback(line)
                except Exception as exc:  # noqa: BLE001 - 回调失败不阻断部署
                    logger.warning(
                        f"LogLineCallback 抛出异常: {exc}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )

            # 2. 解析 PROGRESS 协议
            match = _PROGRESS_LINE_PATTERN.match(line.strip())
            if match is None:
                return
            try:
                percent = max(0, min(100, int(match.group(1))))
            except ValueError:
                return
            message = match.group(2).strip()
            progress_events.append((percent, message))
            if progress is not None:
                try:
                    progress(message, percent)
                except Exception as exc:  # noqa: BLE001 - 回调失败不阻断部署
                    logger.warning(
                        f"ProgressCallback 抛出异常: {exc}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )

        def _on_progress_line(line: str) -> None:
            # ``\r`` 终止的瞬时刷新行: dnf/apt/curl 进度条更新, 不参与 PROGRESS 协议
            # 解析 (脚本侧的 [PROGRESS] 始终 \n 终止), 仅作 UI 实时回显. 
            if progress_log_callback is None:
                return
            try:
                progress_log_callback(line)
            except Exception as exc:  # noqa: BLE001 - 回调失败不阻断部署
                logger.warning(
                    f"ProgressLogCallback 抛出异常: {exc}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )

        # 优先使用 SSHClient.exec_stream 实现实时进度
        if isinstance(self.backend, RemoteExecutionBackend):
            ssh_client: SSHClient = self.backend._ssh_client  # noqa: SLF001 - 同包私有访问
            # 部署脚本耗时常远超普通命令(apt-get / curl 大文件等), 使用专用的 script_timeout
            script_timeout = ssh_client.credentials.script_timeout
            # 仅当上层提供了 progress_log_callback 时才把 \r 行单独路由,
            # 否则保持旧行为 (\r 行仍走 on_stdout_line, 进入 captured_stdout) 
            on_progress = _on_progress_line if progress_log_callback is not None else None
            try:
                result = ssh_client.exec_stream(
                    command,
                    on_stdout_line=_on_line,
                    on_stdout_progress=on_progress,
                    check=False,
                    merge_stderr=True,
                    timeout=script_timeout,
                )
            except SSHConnectionError:
                raise
            return result, progress_events

        # 非 SSH 后端: 退化到同步执行 + 事后解析
        result = self.backend.run(command, check=False)
        for line in result.stdout.splitlines():
            _on_line(line)
        return result, progress_events

    @staticmethod
    def _summarize_failure(result: RemoteCommandResult) -> str:
        """从命令结果中归纳一段适合呈现给用户的失败摘要. """
        stderr_text = result.stderr.strip()
        if stderr_text:
            return stderr_text.splitlines()[-1].strip()
        stdout_lines = [
            line for line in result.stdout.splitlines() if line.startswith("[ERROR]")
        ]
        if stdout_lines:
            return stdout_lines[-1].strip()
        return f"远端命令退出码 {result.exit_status}"

    @staticmethod
    def _shell_quote(value: str) -> str:
        return "'" + value.replace("'", "'\"'\"'") + "'"

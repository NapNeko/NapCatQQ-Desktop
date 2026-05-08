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
    has_xvfb: bool
    has_linuxqq: bool
    has_napcat: bool
    installed_qq_version: str | None
    installed_napcat_version: str | None

    @property
    def is_supported_arch(self) -> bool:
        """是否落在 P1 支持的架构白名单内. """
        return self.normalized_arch in ("amd64", "arm64")

    @property
    def has_package_installer(self) -> bool:
        return self.has_dpkg or self.has_rpm2cpio


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
        distro_id, distro_version = self._parse_os_release(os_release_result.stdout)

        has_bash = self.backend.run("command -v bash >/dev/null 2>&1").ok
        has_tar = self.backend.run("command -v tar >/dev/null 2>&1").ok
        has_unzip = self.backend.run("command -v unzip >/dev/null 2>&1").ok
        has_curl = self.backend.run("command -v curl >/dev/null 2>&1").ok
        has_dpkg = self.backend.run("command -v dpkg >/dev/null 2>&1").ok
        has_rpm2cpio = self.backend.run(
            "command -v rpm2cpio >/dev/null 2>&1 && command -v cpio >/dev/null 2>&1"
        ).ok
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
            has_xvfb=has_xvfb,
            has_linuxqq=has_linuxqq,
            has_napcat=has_napcat,
            installed_qq_version=installed_qq_version,
            installed_napcat_version=installed_napcat_version,
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
    def _parse_os_release(text: str) -> tuple[str | None, str | None]:
        """解析 ``/etc/os-release`` 内容, 提取 ID 与 VERSION_ID. """
        if not text or not text.strip():
            return None, None
        distro_id: str | None = None
        distro_version: str | None = None
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
        return distro_id, distro_version

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
        force_reinstall: bool = False,
    ) -> InstallStepResult:
        """在远端安装 LinuxQQ rootless. 

        Args:
            progress: 进度协议回调, 由 ``[PROGRESS] N message`` 行触发
            log_callback: 原始日志行回调, 用于把脚本 stdout 实时透传给"部署控制台"
            force_reinstall: 强制重装(会先备份 NapCat 配置再 ``rm -rf $install_base_dir/opt`` 重新解压)
        """
        logger.info(
            f"开始远端 LinuxQQ 安装: workspace={self.paths.workspace_dir}, force_reinstall={force_reinstall}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        self.initialize_layout()

        script_content = build_install_linuxqq_script(self._build_script_variables())
        remote_script_path = self._upload_script(script_content, "remote_install_linuxqq.sh")

        env_prefix = "FORCE_LINUXQQ_REINSTALL=1 " if force_reinstall else ""
        command = f'{env_prefix}bash "{remote_script_path}"'
        result, progress_events = self._run_script_with_progress(command, progress, log_callback)

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

    #: ``remote_install_napcat.sh`` 在 SHA512 校验失败时使用的 dedicated 退出码 (P5 F1.4).
    INSTALL_NAPCAT_VERIFY_EXIT_CODE: int = 36

    def install_napcat(
        self,
        *,
        progress: ProgressCallback | None = None,
        log_callback: LogLineCallback | None = None,
        force_update: bool = False,
        download_url: str | None = None,
        expected_sha512: str | None = None,
    ) -> InstallStepResult:
        """在远端安装/更新 NapCat. 

        默认仅在远端不存在 NapCat 时下载; 设置 ``force_update=True`` 强制重新下载并解压. 
        部署完成后会自动把 launcher 脚本上传到 ``$workspace_dir/napcat.sh``. 

        Args:
            progress: 进度协议回调, 由 ``[PROGRESS] N message`` 行触发
            log_callback: 原始日志行回调, 用于把脚本 stdout 实时透传给"部署控制台"
            force_update: 强制重新下载并解压 NapCat
            download_url: 自定义下载地址(覆盖 ``NAPCAT_DOWNLOAD_URL``)
            expected_sha512: P5 F1.4: NapCat.Shell.zip 的期望 SHA512 (128 位 hex);
                提供时通过 ``NAPCAT_EXPECTED_SHA512`` 环境变量传给远端脚本, 校验失败
                远端会以退出码 36 中断, 本方法把该退出码转为
                ``RemoteCommandError`` (调用方按 stage="install_napcat_verify" 区分).
                ``None`` 时跳过校验, 远端仅记 warning 不阻断 (兼容老客户端).

        Raises:
            RemoteCommandError: 远端脚本退出码非 0; 当 ``exit_status==36`` 时表示
                SHA512 校验失败, ``stderr`` 已包含期望与实际值, 上层应转为
                ``RemoteDeploymentError(stage="install_napcat_verify")`` 以保留语义.
        """
        logger.info(
            (
                f"开始远端 NapCat 安装: napcat_dir={self.paths.napcat_dir}, "
                f"force_update={force_update}, "
                f"sha512_verify={'enabled' if expected_sha512 else 'skipped'}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )
        self.initialize_layout()

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

        result, progress_events = self._run_script_with_progress(command, progress, log_callback)

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
    ) -> tuple[RemoteCommandResult, list[tuple[int, str]]]:
        """执行脚本并解析 ``[PROGRESS] N message`` 行. 

        - ``progress``: 仅在匹配到 PROGRESS 协议行时触发
        - ``log_callback``: 每行 stdout(含合并的 stderr) 都会触发一次, 用于"部署控制台"实时回显

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

        # 优先使用 SSHClient.exec_stream 实现实时进度
        if isinstance(self.backend, RemoteExecutionBackend):
            ssh_client: SSHClient = self.backend._ssh_client  # noqa: SLF001 - 同包私有访问
            # 部署脚本耗时常远超普通命令(apt-get / curl 大文件等), 使用专用的 script_timeout
            script_timeout = ssh_client.credentials.script_timeout
            try:
                result = ssh_client.exec_stream(
                    command,
                    on_stdout_line=_on_line,
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

# -*- coding: utf-8 -*-
"""SnowLuma 远端部署器 (W4).

与 :class:`src.core.remote.deployment.LinuxCoreDeployment` 同模式但独立, 因为
SnowLuma 的远端工作流是:

1. **probe_environment** — 委托 NC ``LinuxCoreDeployment.probe_environment``,
   复用 distro 识别 / arch 归一化 / LinuxQQ 检测等设施
2. **install_linuxqq** — 委托 NC ``install_linuxqq``, 但用 SnowLuma 自己的
   ``workspace_dir`` (``$HOME/snowluma-remote/workspace``), 让 LinuxQQ 落到
   SnowLuma 私有目录, 与 NC 安装互不干扰 (D8 ``ServerProfile`` per-flavor 互斥)
3. **install_snowluma_framework** — SL 独有: SFTP 上传 lite tarball + 渲染
   ``install_snowluma.sh.j2`` 跑图形栈安装与解压
4. **upload_daemon_launcher_script** / **upload_bot_launcher_script** — 上传
   :mod:`.templates` 渲染的 launcher 脚本到 ``workspace_dir``

设计取舍 (与 NC 对照):

- 路径模型: :class:`.paths.SnowLumaRemotePaths` 与 :class:`.models.LinuxCorePaths`
  彼此独立; 内部派生一个最小化 NC ``LinuxCorePaths`` 让 NC 部署器把 LinuxQQ
  落到 SL workspace 下 (``${sl_workspace}/opt/QQ/...``)
- helper 复用: ``_upload_script`` / ``_run_script_with_progress`` /
  ``_summarize_failure`` 这类底层 IO 直接调 NC ``LinuxCoreDeployment`` 的
  内部方法 (同包跨模块约定, 注释标注耦合点)
- 错误模型: 复用 NC ``RemoteCommandError`` / ``RemoteDeploymentError``;
  新增 :class:`SnowLumaFrameworkNotBundledError` 表达 "Desktop 未捆绑 lite tarball"
"""

from __future__ import annotations

import tempfile
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath
from typing import Literal

from src.core.logging import LogSource, LogType, logger

from ..deployment import (
    InstallStepResult as _NcInstallStepResult,
    LinuxCoreDeployment,
    LinuxCoreDeploymentProbe,
    LogLineCallback,
    ProgressCallback,
)
from ..errors import RemoteCommandError, RemoteError
from ..execution_backend import ExecutionBackend
from ..models import LinuxCorePaths, RemoteCommandResult
from .bundled import LITE_TARBALL_FILENAME, find_bundled_lite_tarball, read_bundled_version
from .paths import SnowLumaRemotePaths
from .templates import (
    build_install_snowluma_script,
    build_snowluma_bot_launcher,
    build_snowluma_daemon_launcher,
)


# 远端落点文件名 (与 Desktop 内置 tarball 同名, SFTP put 后供 install_snowluma.sh
# ``tar -xzf $WORKSPACE_DIR/<name>`` 使用)
_LITE_TARBALL_REMOTE_FILENAME: str = LITE_TARBALL_FILENAME


# ==================== 错误 ====================
class SnowLumaFrameworkNotBundledError(RemoteError):
    """Desktop 未捆绑 SnowLuma.Framework lite tarball.

    触发场景:

    - 用户拿到的是手工 build 的 Desktop, 没跑过 ``build_snowluma_framework_lite.py``
    - PyInstaller datas 钩子未被 spec 引用
    - lite tarball 路径在解压后丢失

    UI 应给出引导: "请运行 ``script/build_scripts/build_snowluma_framework_lite.py``
    重新构建 Desktop, 或下载官方发布版".
    """


# ==================== 步骤结果 ====================
SnowLumaInstallStep = Literal[
    "install_linuxqq",
    "install_snowluma_framework",
]


@dataclass(slots=True)
class SnowLumaInstallStepResult:
    """SL 单个安装阶段的执行结果 (与 NC ``InstallStepResult`` 同结构, step 不同).

    Attributes:
        step: 步骤名; 用于日志关联 + UI 进度条标题
        remote_script_path: 远端实际执行的脚本路径 (用于失败时定位)
        exit_status: 退出码 (0 = ok)
        stdout: 远端 stdout 全文
        stderr: 远端 stderr 全文
        progress_events: ``[(percent, message), ...]`` 解析自 ``[PROGRESS]`` 行
    """

    step: SnowLumaInstallStep
    remote_script_path: str
    exit_status: int
    stdout: str
    stderr: str = ""
    progress_events: list[tuple[int, str]] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return self.exit_status == 0


# ==================== 部署器 ====================
class SnowLumaDeployment:
    """SnowLuma 远端部署器.

    Args:
        backend: SSH 执行后端 (复用 NC :class:`ExecutionBackend`)
        paths: SnowLuma 远端目录布局; 不传则用默认 ``$HOME/snowluma-remote/...``

    Examples:
        >>> from src.core.remote.snowluma import SnowLumaRemotePaths
        >>> from src.core.remote.snowluma.deployment import SnowLumaDeployment
        >>> deployer = SnowLumaDeployment(backend, SnowLumaRemotePaths.from_base())
        >>> probe = deployer.probe_environment()
        >>> if probe.is_supported_arch:
        ...     deployer.install_linuxqq(progress=on_progress)
        ...     deployer.install_snowluma_framework(progress=on_progress)
        ...     deployer.upload_daemon_launcher_script()
        ...     deployer.upload_bot_launcher_script()
    """

    def __init__(
        self,
        backend: ExecutionBackend,
        paths: SnowLumaRemotePaths | None = None,
    ) -> None:
        self.backend = backend
        self.paths = paths or SnowLumaRemotePaths.from_base()
        # NC 部署器, base_dir 设为 SL workspace, 让 LinuxQQ 落到 SL 私有目录
        self._nc_paths = self._build_nc_paths(self.paths)
        self._nc_deployment = LinuxCoreDeployment(backend, self._nc_paths)

    @staticmethod
    def _build_nc_paths(sl_paths: SnowLumaRemotePaths) -> LinuxCorePaths:
        """从 SL 路径派生一份最小化 NC ``LinuxCorePaths``.

        NC 部署器只用 ``workspace_dir`` / ``runtime_dir`` / ``log_dir`` /
        ``tmp_dir`` / ``package_dir`` (后两者是 LinuxQQ 安装时的暂存区);
        ``config_dir`` 是 NapCat 的注入点, SL 不装 NapCat 但需要让校验通过.

        关键约定: ``workspace_dir`` 与 SL 共用, 让 LinuxQQ 解压到
        ``${sl_workspace}/opt/QQ/...``; runtime/log/tmp 也在 SL workspace 下.
        """
        ws = sl_paths.workspace_dir
        return LinuxCorePaths(
            workspace_dir=ws,
            runtime_dir=sl_paths.runtime_dir,
            # NapCat 注入点 (SL 不装 NapCat, 此字段仅满足 dataclass 校验)
            config_dir=f"{ws}/opt/QQ/resources/app/app_launcher/napcat/config",
            log_dir=sl_paths.log_dir,
            tmp_dir=f"{ws}/tmp",
            package_dir=f"{ws}/packages",
        )

    # ==================== 委托 NC ====================
    def probe_environment(self) -> LinuxCoreDeploymentProbe:
        """探测远端环境 (委托 NC).

        Returns:
            :class:`LinuxCoreDeploymentProbe`. SL 关心的额外字段:

            - ``has_linuxqq``: SL workspace 下是否已装 LinuxQQ
            - ``normalized_arch``: 必须是 ``amd64`` / ``arm64`` 才支持
            - ``has_dpkg`` / ``has_dnf``: SL 支持 apt (Debian/Ubuntu) 或 dnf (RHEL/CentOS/Fedora)
        """
        return self._nc_deployment.probe_environment()

    def initialize_layout(self) -> list[RemoteCommandResult]:
        """初始化 SL 远端目录 (workspace / config / runtime / log + NC 兼容子树)."""
        results: list[RemoteCommandResult] = []
        for path in (
            self.paths.base_dir,
            self.paths.workspace_dir,
            self.paths.snowluma_framework_dir,
            self.paths.config_dir,
            self.paths.runtime_dir,
            self.paths.log_dir,
            self._nc_paths.tmp_dir,
            self._nc_paths.package_dir,
        ):
            results.append(self.backend.ensure_directory(path))
        return results

    def install_linuxqq(
        self,
        *,
        progress: ProgressCallback | None = None,
        log_callback: LogLineCallback | None = None,
        progress_log_callback: LogLineCallback | None = None,
        force_reinstall: bool = False,
        local_package_cache_dir: Path | None = None,
        should_cancel: Callable[[], bool] | None = None,
    ) -> SnowLumaInstallStepResult:
        """委托 NC 安装 LinuxQQ 到 ``${sl_workspace}/opt/QQ/...``.

        Args 参数语义与 :meth:`LinuxCoreDeployment.install_linuxqq` 完全一致;
        只是返回类型从 NC ``InstallStepResult`` 转成 SL ``SnowLumaInstallStepResult``
        以保持 step 字段的语义 (``"install_linuxqq"``).

        Raises:
            RemoteCommandError: NC 脚本退出码非 0
            RemoteDeploymentError: NC 包装的部署阶段错误
        """
        # 显式先建 SL 目录, 防止 NC 在自己 initialize_layout 里没建出 SL 顶层
        self.initialize_layout()

        nc_result: _NcInstallStepResult = self._nc_deployment.install_linuxqq(
            progress=progress,
            log_callback=log_callback,
            progress_log_callback=progress_log_callback,
            force_reinstall=force_reinstall,
            local_package_cache_dir=local_package_cache_dir,
            should_cancel=should_cancel,
        )
        return self._convert_nc_step(nc_result, step="install_linuxqq")

    @staticmethod
    def _convert_nc_step(
        nc_result: _NcInstallStepResult,
        *,
        step: SnowLumaInstallStep,
    ) -> SnowLumaInstallStepResult:
        return SnowLumaInstallStepResult(
            step=step,
            remote_script_path=nc_result.remote_script_path,
            exit_status=nc_result.exit_status,
            stdout=nc_result.stdout,
            stderr=nc_result.stderr,
            progress_events=list(nc_result.progress_events),
        )

    # ==================== SL 独有: install_snowluma_framework ====================
    def install_snowluma_framework(
        self,
        *,
        progress: ProgressCallback | None = None,
        log_callback: LogLineCallback | None = None,
        progress_log_callback: LogLineCallback | None = None,
        enable_nodesource: bool = True,
        vnc_port: int = 5900,
        novnc_port: int = 6081,
        webui_port: int = 5099,
        display_num: int = 0,
        lite_tarball_override: Path | None = None,
        local_node_cache_dir: Path | None = None,
        should_cancel: Callable[[], bool] | None = None,
    ) -> SnowLumaInstallStepResult:
        """SFTP 上传 lite tarball + 跑 ``install_snowluma.sh`` 装图形栈/node/解压.

        前置条件:

        - Desktop 已捆绑 ``snowluma_framework_lite.tar.gz`` (W3 产物);
          否则 raise :class:`SnowLumaFrameworkNotBundledError`
        - 远端已装 LinuxQQ (建议在 :meth:`install_linuxqq` 之后调本方法);
          但本方法不强制校验, 由调用方控制顺序

        **本机下载兜底 (Node.js)**: 当远端无法直连 npmmirror / nodejs.org 等镜像站时,
        若 ``local_node_cache_dir`` 非空则改为在 Desktop 本机下载 Node.js tarball,
        再通过 SFTP 上传到 ``${workspace_dir}/packages/node-vX.Y.Z-linux-{arch}.tar.xz``;
        脚本里 ``NODE_PRELOADED`` 分支会自动跳过网络下载并复用预上传包.
        见 [`local_node_fallback`](src/core/remote/snowluma/local_node_fallback.py).

        Args:
            progress: ``[PROGRESS] N message`` 协议回调
            log_callback: 行级 stdout 回调 (供"部署控制台"实时回显)
            progress_log_callback: ``\\r`` 终止瞬时刷新行 (apt/curl 进度条) 回调
            enable_nodesource: ``True`` 允许 OQ2 三级 fallback 走 nodesource L3;
                ``False`` 强制只走 apt nodejs (air-gapped 部署或测试用)
            vnc_port / novnc_port / webui_port / display_num: 远端监听端口
                (与 launcher 脚本默认值对齐)
            lite_tarball_override: 测试用; 指定不走 :func:`find_bundled_lite_tarball`
                的备用 tarball 路径
            local_node_cache_dir: 本机 Node tarball 预下载缓存目录 (一般为
                ``it(PathFunc).tmp_path``). ``None`` 时关闭本机兜底.
            should_cancel: 取消检查协议.

        Returns:
            :class:`SnowLumaInstallStepResult` (``step="install_snowluma_framework"``).

        Raises:
            SnowLumaFrameworkNotBundledError: Desktop 未捆绑 lite tarball
            RemoteCommandError: 远端脚本退出码非 0
        """
        bundled_version = read_bundled_version() or "unknown"
        logger.info(
            (
                f"开始远端 SnowLuma.Framework 安装: workspace={self.paths.workspace_dir}, "
                f"bundled_version={bundled_version}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

        # 1. 定位本地 lite tarball
        local_tarball = lite_tarball_override or find_bundled_lite_tarball()
        if local_tarball is None:
            raise SnowLumaFrameworkNotBundledError(
                "Desktop 未捆绑 SnowLuma.Framework lite tarball, 远端部署不可用; "
                "请运行 script/build_scripts/build_snowluma_framework_lite.py 后重启 Desktop"
            )
        if not local_tarball.is_file():
            raise SnowLumaFrameworkNotBundledError(
                f"指定的 lite tarball 不存在: {local_tarball}"
            )

        # 2. 初始化目录 + SFTP 上传 tarball 到 ${workspace_dir}/{filename}
        self.initialize_layout()
        remote_tarball_path = PurePosixPath(
            self.paths.workspace_dir, _LITE_TARBALL_REMOTE_FILENAME
        ).as_posix()
        self.backend.upload_file(local_tarball, remote_tarball_path)
        logger.info(
            f"SFTP 上传 lite tarball 完成: {local_tarball} -> {remote_tarball_path} "
            f"({local_tarball.stat().st_size} bytes)",
            LogType.NETWORK,
            LogSource.CORE,
        )

        # 2.5. Node.js tarball 本机下载兜底: 远端镜像不可达时预上传到 packages/
        if local_node_cache_dir is not None:
            self._maybe_prefetch_node_tarball(
                local_node_cache_dir=local_node_cache_dir,
                should_cancel=should_cancel,
                log_callback=log_callback,
            )

        # 3. 渲染脚本 + 上传 + 执行
        script_content = build_install_snowluma_script(
            self.paths,
            framework_archive_name=_LITE_TARBALL_REMOTE_FILENAME,
            enable_nodesource=enable_nodesource,
            vnc_port=vnc_port,
            novnc_port=novnc_port,
            webui_port=webui_port,
            display_num=display_num,
        )

        # NC 内部 helper 复用 (同包跨模块约定, 比单独抽 module-level 更内聚)
        remote_script_path = self._nc_deployment._upload_script(  # noqa: SLF001
            script_content, "remote_install_snowluma.sh"
        )
        command = f'bash "{remote_script_path}"'

        result, progress_events = self._nc_deployment._run_script_with_progress(  # noqa: SLF001
            command,
            progress,
            log_callback,
            progress_log_callback=progress_log_callback,
        )

        step_result = SnowLumaInstallStepResult(
            step="install_snowluma_framework",
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
                stderr=self._nc_deployment._summarize_failure(result),  # noqa: SLF001
            )

        logger.info(
            f"远端 SnowLuma.Framework 安装完成: events={len(progress_events)}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        return step_result

    # ==================== launcher 脚本部署 ====================
    def upload_daemon_launcher_script(
        self,
        *,
        display_num: int = 0,
        vnc_port: int = 5900,
        novnc_port: int = 6081,
        webui_port: int = 5099,
    ) -> str:
        """渲染并上传 daemon launcher 脚本到 ``paths.daemon_launcher_script``.

        Returns:
            远端落点路径.
        """
        return self._upload_launcher(
            content=build_snowluma_daemon_launcher(
                self.paths,
                display_num=display_num,
                vnc_port=vnc_port,
                novnc_port=novnc_port,
                webui_port=webui_port,
            ),
            filename="snowluma_daemon_launcher.sh",
            target_path=self.paths.daemon_launcher_script,
        )

    def upload_bot_launcher_script(
        self,
        *,
        display_num: int = 0,
    ) -> str:
        """渲染并上传 bot launcher 脚本到 ``paths.bot_launcher_script``."""
        return self._upload_launcher(
            content=build_snowluma_bot_launcher(self.paths, display_num=display_num),
            filename="snowluma_bot_launcher.sh",
            target_path=self.paths.bot_launcher_script,
        )

    def _upload_launcher(self, *, content: str, filename: str, target_path: str) -> str:
        """通用 launcher 脚本上传: 写本地临时文件 → SFTP → chmod +x."""
        with tempfile.TemporaryDirectory(prefix="snowluma-launcher-") as tmp_dir:
            local_path = Path(tmp_dir) / filename
            local_path.write_text(content, encoding="utf-8", newline="\n")
            self.backend.upload_file(local_path, target_path)
        self.backend.run(f'chmod +x "{target_path}"', check=True)
        return target_path

    # ==================== Node.js tarball 本机下载兜底 ====================
    def _maybe_prefetch_node_tarball(
        self,
        *,
        local_node_cache_dir: Path,
        should_cancel: Callable[[], bool] | None,
        log_callback: Callable[[str], None] | None,
    ) -> None:
        """远端镜像不可达时, 在本机下载 Node.js tarball 并 SFTP 上传到远端.

        三层短路:

        1. **远端已有 node >= 22** (系统 PATH 或 ``$WORKSPACE_DIR/node/bin/node``):
           脚本 L1 会直接跳过, 不需要预上传
        2. **远端能直连 Node 镜像站**: 让脚本 L4 自己处理
        3. **以上都不命中**: 探测远端架构 -> 本机下载 -> SFTP 上传到
           ``${workspace_dir}/packages/node-vX.Y.Z-linux-{arch}.tar.xz``

        任何一步失败都不抛, 让脚本按原路径继续尝试.
        """
        from .local_node_fallback import (
            backend_can_reach_node_mirrors,
            get_remote_node_tarball_filename,
            prefetch_node_tarball_locally,
        )
        from ..errors import RemoteDeploymentCancelledError as _Cancelled

        # 短路 1: 远端已有 node >= 22 (系统 PATH 或便携式)
        node_check = self.backend.run(
            f'{{ command -v node && node -v; }} 2>/dev/null || '
            f'{{ [ -x "{self.paths.workspace_dir}/node/bin/node" ] && '
            f'"{self.paths.workspace_dir}/node/bin/node" -v; }} 2>/dev/null || echo ""',
            check=False,
        )
        node_output = (node_check.stdout or "").strip()
        # 解析版本号: 输出可能是 "/usr/bin/node\nv22.18.0" 或 "v22.18.0"
        import re
        ver_match = re.search(r"v(\d+)\.", node_output)
        if ver_match:
            major = int(ver_match.group(1))
            if major >= 22:
                if log_callback is not None:
                    log_callback(
                        f"[INFO] 远端已有 node >= 22 (v{ver_match.group(0)}...), "
                        "跳过 Node tarball 预上传"
                    )
                return

        # 探测远端架构
        arch = self._detect_remote_arch(log_callback)
        if arch is None:
            return

        # 短路 2: 远端能直连 Node 镜像站
        try:
            reachable = backend_can_reach_node_mirrors(
                self.backend, arch=arch, log_callback=log_callback
            )
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"Node 镜像连通性探测失败, 走本机兜底: {type(exc).__name__}: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            reachable = False
        if reachable:
            return

        # 短路 3: 本机下载 + SFTP 上传
        if log_callback is not None:
            log_callback(
                "[WARN] 远端无法直连 Node 镜像站, 切换到本机下载 + SFTP 上传兜底"
            )
        try:
            filename = get_remote_node_tarball_filename(arch)
            local_target = local_node_cache_dir / filename
            prefetch_node_tarball_locally(
                target_path=local_target,
                arch=arch,
                log_callback=log_callback,
                should_cancel=should_cancel,
            )
            # 上传前检查取消
            if should_cancel is not None and should_cancel():
                raise _Cancelled()
            # 上传到 ${workspace_dir}/packages/ (脚本 NODE_PRELOADED 路径)
            packages_dir = f"{self.paths.workspace_dir}/packages"
            self.backend.ensure_directory(packages_dir)
            remote_path = PurePosixPath(packages_dir, filename).as_posix()
            self.backend.upload_file(local_target, remote_path)
            if log_callback is not None:
                log_callback(
                    f"[INFO] 本机 Node tarball 已上传到远端: {remote_path}"
                )
        except Exception as exc:  # noqa: BLE001
            if isinstance(exc, _Cancelled):
                raise
            logger.warning(
                f"Node tarball 本机兜底失败, 退回远端脚本自下载: {type(exc).__name__}: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            if log_callback is not None:
                log_callback(
                    f"[WARN] Node tarball 本机下载兜底失败 ({type(exc).__name__}), "
                    "退回远端脚本自行下载 (可能超时)"
                )

    def _detect_remote_arch(
        self, log_callback: Callable[[str], None] | None
    ) -> str | None:
        """探测远端架构, 返回 ``"amd64"`` 或 ``"arm64"``; 无法确定时返回 None."""
        from .local_node_fallback import ArchType  # noqa: F811 - type hint only

        arch_result = self.backend.run("uname -m", check=False)
        raw_arch = (arch_result.stdout or "").strip()
        if raw_arch in ("x86_64", "amd64"):
            return "amd64"
        elif raw_arch in ("aarch64", "arm64"):
            return "arm64"
        else:
            if log_callback is not None:
                log_callback(
                    f"[WARN] 无法确定远端架构 (uname -m={raw_arch!r}), "
                    "跳过 Node tarball 预上传"
                )
            return None

    # ==================== 清理 (W10b-Maintenance) ====================
    def clean_environment(self, include_qq: bool = True) -> RemoteCommandResult:
        """清理 SnowLuma 远端环境.

        与 :meth:`LinuxCoreDeployment.clean_environment` 对应, 但清的是 SL 的产物:

        1. 优雅停 daemon (``daemon_launcher.sh stop``) → 让 launcher 自己 reap
           xvfb / x11vnc / websockify / fluxbox / node 进程组
        2. 强 pkill 残留 (优雅停失败时的 fallback)
        3. 杀 LinuxQQ 进程 (``qq --no-sandbox``)
        4. 清运行时 / 日志 / 临时文件
        5. 清 SnowLuma framework 安装 (``snowluma_framework_dir``)
        6. 清两个 launcher 脚本 + 密钥文件
        7. 可选清 LinuxQQ 安装 + 已下载安装包

        Args:
            include_qq: 是否同时清理 LinuxQQ 安装 (与 NC 接口对齐)

        Returns:
            形式上的 ``RemoteCommandResult`` (exit_status=0, command="clean_environment");
            实际单条命令的失败已由 ``check=False`` 静默吞掉, 调用方按返回值判定整体成功.
        """
        logger.info(
            f"开始清理 SnowLuma 环境: include_qq={include_qq}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )

        # 1. 优雅停 daemon (launcher 内部会 reap 所有 5 个辅助进程 + node);
        # 脚本可能根本不存在 (从未部署成功), 用 test -x 包一层
        logger.info("尝试优雅停 SnowLuma daemon", log_source=LogSource.CORE)
        self.backend.run(
            f'test -x "{self.paths.daemon_launcher_script}" && '
            f'bash "{self.paths.daemon_launcher_script}" stop 2>/dev/null || true',
            check=False,
        )

        # 2. 强 pkill 残留辅助进程 (有些 launcher 死的早, reap 不全).
        # SnowLuma 自身 node 进程通过 index.mjs 路径模糊匹配
        logger.info("强制清理残留 daemon 进程", log_source=LogSource.CORE)
        self.backend.run(
            "pkill -f 'snowluma/index.mjs' 2>/dev/null || true; "
            "pkill -f 'Xvfb.*:0' 2>/dev/null || true; "
            "pkill -f 'x11vnc.*-rfbport' 2>/dev/null || true; "
            "pkill -f 'websockify.*--daemon' 2>/dev/null || true; "
            "pkill -f 'fluxbox' 2>/dev/null || true",
            check=False,
        )

        # 3. 杀 LinuxQQ
        logger.info("停止 LinuxQQ 进程", log_source=LogSource.CORE)
        self.backend.run('pkill -f "qq --no-sandbox" 2>/dev/null || true', check=False)

        # 4. 清运行时 / 日志 / tmp
        logger.info("清理运行时与日志", log_source=LogSource.CORE)
        for d in (
            self.paths.runtime_dir,
            self._nc_paths.tmp_dir,
        ):
            self.backend.run(f'rm -rf "{d}" 2>/dev/null || true', check=False)
        # 仅清日志文件不删 log_dir 目录 (与 NC 同), 让下次启动自然重建
        self.backend.run(
            f'rm -f "{self.paths.log_dir}"/*.log 2>/dev/null || true',
            check=False,
        )

        # 5. 清 SnowLuma framework
        logger.info("清理 SnowLuma framework 安装", log_source=LogSource.CORE)
        self.backend.run(
            f'rm -rf "{self.paths.snowluma_framework_dir}" 2>/dev/null || true',
            check=False,
        )

        # 6. 清 launcher 脚本 + 密钥 + dbus env 残留
        logger.info("清理 launcher 脚本与密钥文件", log_source=LogSource.CORE)
        for f in (
            self.paths.daemon_launcher_script,
            self.paths.bot_launcher_script,
            self.paths.vnc_secret,
            self.paths.webui_secret,
        ):
            self.backend.run(f'rm -f "{f}" 2>/dev/null || true', check=False)

        # 7. 可选: 清 LinuxQQ (走 NC 路径; NC paths 的 qq_base_path 已指向 SL workspace)
        if include_qq:
            logger.info("清理 LinuxQQ 安装", log_source=LogSource.CORE)
            self.backend.run(
                f'rm -rf "{self._nc_paths.qq_base_path}" 2>/dev/null || true',
                check=False,
            )
            # 清下载缓存 (deb/rpm)
            self.backend.run(
                f'rm -f "{self._nc_paths.package_dir}"/*.deb '
                f'"{self._nc_paths.package_dir}"/*.rpm 2>/dev/null || true',
                check=False,
            )
            # 顺便清掉便携式 node (是 install_snowluma.sh L4 fallback 装的;
            # 包含跟当前 launcher 不匹配的旧版本时, 重新部署再下一次更稳)
            self.backend.run(
                f'rm -rf "{self.paths.workspace_dir}/node" '
                f'"{self.paths.workspace_dir}/node.tar.xz" 2>/dev/null || true',
                check=False,
            )

        logger.info(
            "SnowLuma 环境清理完成",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        return RemoteCommandResult(
            command="clean_environment",
            exit_status=0,
        )

    # ==================== 验证 ====================
    def verify_deployment(self) -> tuple[bool, list[str]]:
        """检查关键文件是否齐全; 返回 ``(ok, missing_paths)``.

        断言项:

        - ``${snowluma_framework_dir}/index.mjs`` (W3 修正: SnowLuma release lite 扫平结构)
        - ``${snowluma_framework_dir}/native/`` 至少一个 ``*.node``
        - ``${vnc_secret}`` / ``${webui_secret}`` 存在 (mode 600 由脚本保证)
        - ``${daemon_launcher_script}`` / ``${bot_launcher_script}`` 可执行
        """
        checks: list[tuple[str, str]] = [
            (f"{self.paths.snowluma_framework_dir}/index.mjs", "SL daemon 入口"),
            (self.paths.vnc_secret, "VNC 密钥"),
            (self.paths.webui_secret, "WebUI 密钥"),
            (self.paths.daemon_launcher_script, "daemon launcher"),
            (self.paths.bot_launcher_script, "bot launcher"),
        ]
        missing: list[str] = []
        for remote_path, label in checks:
            result = self.backend.run(f'test -e "{remote_path}"', check=False)
            if result.exit_status != 0:
                missing.append(f"{label} ({remote_path})")

        # native 二进制至少有一个 (W3 修正: release lite 中 native/ 位于顶层)
        native_check = self.backend.run(
            f'ls "{self.paths.snowluma_framework_dir}/native/"*.node '
            "2>/dev/null | head -n1",
            check=False,
        )
        if not native_check.stdout.strip():
            missing.append(
                f"SL native (.node) 二进制 ({self.paths.snowluma_framework_dir}/native/)"
            )

        return len(missing) == 0, missing


__all__ = [
    "SnowLumaDeployment",
    "SnowLumaInstallStep",
    "SnowLumaInstallStepResult",
    "SnowLumaFrameworkNotBundledError",
]

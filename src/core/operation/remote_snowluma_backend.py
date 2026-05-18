# -*- coding: utf-8 -*-
"""[`RemoteSnowLumaBackend`](src/core/operation/remote_snowluma_backend.py): SnowLuma flavor 的 [`OperationBackend`](src/core/operation/backend.py) 实现 (W10b-Driver).

与 [`RemoteBackend`](src/core/operation/remote_backend.py) (NC flavor) 平级设计:

- **文件 / 字节 IO**: 完全复用 [`SSHClient`](src/core/remote/ssh_client.py), 与 NC 路径
  完全等价 (远端 Linux 文件系统是同一棵).
- **进程方法**: 走 [`SnowLumaLauncherCommands`](src/core/remote/snowluma/launcher.py)
  + [`RemoteSnowLumaDaemon`](src/core/remote/snowluma/daemon.py) 而非 NC 的 ``napcat.sh``.
  Bot 启动序: ``daemon.ensure_running()`` → 首次自动 ``open_snowluma_vnc`` 弹扫码 →
  ``snowluma_bot_launcher.sh start <qq_id> [<uin>]`` → 探测 ``status_bot_<qq_id>.json``.
- **WebUI**: SL daemon 全局只有 1 个 SnowLuma WebUI (5099 → 47099 隧道) 管理所有 Bot,
  而 NC 是 per-bot 独立 WebUI. 这里 ``get_webui_endpoint(qq_id)`` 始终返同一端点
  (qq_id 入参仅用于符合协议).
- **安装 / 部署**: 不复用 [`LinuxCoreDeployment`](src/core/remote/deployment.py);
  SL 部署链路由 [`ServerManager._deploy_snowluma_flavor`](src/core/remote/server_manager.py)
  独立编排, 这里 raise ``NotImplementedError`` 提示调用方走对的入口.

调用方约定:

- 由 [`ServerManager.get_backend`](src/core/remote/server_manager.py) 在 ``profile.backend_flavor==SNOWLUMA``
  时实例化并缓存; 同一服务器多个 SL Bot 共享同一 backend.
- ``connect()`` / ``close()`` 控 SSH 生命周期, ``close()`` 会触发 daemon 全量 release
  (无论 ref_count 多少), 让 SSH transport 安全释放.
"""

from __future__ import annotations

import re
from pathlib import Path
from typing import TYPE_CHECKING

from src.core.logging import LogSource, LogType, logger
from src.core.remote.execution_backend import RemoteExecutionBackend
from src.core.remote.models import SSHCredentials
from src.core.remote.snowluma import (
    RemoteSnowLumaDaemon,
    SnowLumaLauncherCommands,
    SnowLumaRemotePaths,
    SnowLumaRemoteRuntimeService,
)
from src.core.remote.snowluma.deployment import SnowLumaDeployment
from src.core.remote.ssh_client import SSHClient

from .backend import (
    FileEntry,
    InstallationInfo,
    OperationBackend,
    ProcessStatus,
    ProgressCallback,
    WebUIEndpoint,
)

if TYPE_CHECKING:
    from src.core.config.config_model import Config


class RemoteSnowLumaBackend(OperationBackend):
    """SnowLuma flavor 远端操作后端.

    与 :class:`src.core.operation.remote_backend.RemoteBackend` 协议等价但实现差异:

    Attributes:
        credentials: SSH 凭证 (与 NC 共用 :class:`SSHCredentials`)
        sl_paths: SL 远端目录布局 (区别于 NC ``LinuxCorePaths``)
        ssh_client: 共享的 :class:`SSHClient` 实例; ``daemon`` / ``_exec_backend`` 都引用它
    """

    def __init__(
        self,
        credentials: SSHCredentials,
        sl_paths: SnowLumaRemotePaths,
        *,
        webui_password_override: str = "",
    ) -> None:
        self.credentials = credentials
        self.sl_paths = sl_paths
        # W10b-WebUI: per-server WebUI 密码 override (来自 ``ServerProfile``).
        # 空串表示走 fallback (App 级 cfg.snowluma_webui_password_override 或
        # 远端 webui.secret 文件内容).
        self._webui_password_override: str = (webui_password_override or "").strip()
        # daemon 启动前 _ensure_remote_webui_password 渲染好的 effective password;
        # ``get_webui_endpoint`` 用此值作 token (UI 复制到剪贴板).
        self._cached_webui_password: str | None = None
        self.ssh_client = SSHClient(credentials)
        self._exec_backend = RemoteExecutionBackend(self.ssh_client)
        self._runtime = SnowLumaRemoteRuntimeService(self._exec_backend, sl_paths)
        self._launcher_cmds = SnowLumaLauncherCommands(sl_paths)
        # 惰性: 首次 start_napcat 时构造 (daemon 内部 _ensure_tunnel_manager 也是惰性)
        self._daemon: RemoteSnowLumaDaemon | None = None
        # 首次启 SL Bot 自动打开 noVNC 扫码; 同一 backend 后续 Bot 不再重复弹浏览器
        # (用户能在 daemon 占用期重新点 "打开 VNC" 按钮自行触发, 见 W10a vnc_launcher)
        self._novnc_browser_opened: bool = False
        # W10b-Maintenance: 惰性部署器, 用于 ServerManager.rollback_server() 调
        # ``backend.deployment.clean_environment(include_qq)`` 走 SL 专用清理路径.
        # 与 NC ``RemoteBackend._deployment`` 同模式; 测试可注入 mock.
        self._deployment: SnowLumaDeployment | None = None
        # 远端服务器物理总内存 (字节); session 内不变, 首次 get_process_status / start_napcat
        # 期间惰性探测后永久缓存. 与 NC ``RemoteBackend._server_total_memory_bytes`` 同模式.
        self._server_total_memory_bytes: int | None = None

    @property
    def paths(self) -> SnowLumaRemotePaths:
        """与 :class:`RemoteBackend.paths` 同名属性, 但类型不同 (``SnowLumaRemotePaths``)."""
        return self.sl_paths

    @property
    def daemon(self) -> RemoteSnowLumaDaemon:
        """惰性构造的 daemon 控制器; UI 层(BotCard) 也可读取此属性接 ``state_changed`` 信号."""
        if self._daemon is None:
            self._daemon = RemoteSnowLumaDaemon(self.ssh_client, self.sl_paths)
        return self._daemon

    @property
    def deployment(self) -> SnowLumaDeployment:
        """惰性构造的 SnowLuma 部署器; 与 :attr:`RemoteBackend.deployment` 同语义.

        ServerManager 通过 ``backend.deployment.clean_environment(include_qq)`` 走
        SL 专用清理 (清 framework / launcher / runtime, 而非 NC 的 napcat 目录).
        """
        if self._deployment is None:
            self._deployment = SnowLumaDeployment(self._exec_backend, self.sl_paths)
        return self._deployment

    # ==================== 生命周期 ====================
    def connect(self) -> None:
        if not self.ssh_client.is_connected:
            self.ssh_client.connect()

    def close(self) -> None:
        # 关 SSH 之前, 把 daemon 引用计数清空 (远端 launcher stop + 隧道 stop).
        # daemon.release() 在 ref_count=0 时是 no-op, 多调几次安全;
        # 但为了避免无限循环, 上限 16 次 (单服务器最多 4 SL Bot, 留足余量).
        if self._daemon is not None:
            for _ in range(16):
                if self._daemon.ref_count == 0:
                    break
                try:
                    self._daemon.release()
                except Exception as exc:  # noqa: BLE001
                    logger.warning(
                        f"RemoteSnowLumaBackend.close: daemon.release() 异常 (静默忽略): {exc}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )
                    break
        self.ssh_client.close()

    @property
    def is_connected(self) -> bool:
        return self.ssh_client.is_connected

    def _ensure_connected(self) -> None:
        if not self.ssh_client.is_connected:
            self.ssh_client.connect()

    # ==================== 文件 IO (与 RemoteBackend 完全一致) ====================
    def read_file(self, path: str) -> str:
        self._ensure_connected()
        return self.ssh_client.read_text(path)

    def write_file(self, path: str, content: str) -> None:
        self._ensure_connected()
        self.ssh_client.write_text(path, content)

    def file_exists(self, path: str) -> bool:
        self._ensure_connected()
        return self.ssh_client.remote_exists(path)

    def list_dir(self, path: str) -> list[FileEntry]:
        self._ensure_connected()
        return [
            FileEntry(name=name, is_dir=is_dir, size=size)
            for name, is_dir, size in self.ssh_client.remote_listdir(path)
        ]

    def mkdir(self, path: str, *, parents: bool = True, exist_ok: bool = True) -> None:
        self._ensure_connected()
        if parents and exist_ok:
            self.ssh_client.ensure_remote_directory(path)
            return
        quoted = self.ssh_client._quote_remote_argument(path)  # noqa: SLF001
        if not parents and not exist_ok:
            self._exec_backend.run(f"mkdir -- {quoted}", check=True)
            return
        if parents and not exist_ok:
            self._exec_backend.run(
                f"mkdir -p -- {quoted} && test -z \"$(ls -A {quoted})\"",
                check=True,
            )
            return
        self._exec_backend.run(f"mkdir -- {quoted} 2>/dev/null || test -d {quoted}", check=True)

    def remove(self, path: str, *, recursive: bool = False) -> None:
        self._ensure_connected()
        self.ssh_client.remote_remove(path, recursive=recursive)

    def upload(self, local_path: str | Path, remote_path: str) -> None:
        self._ensure_connected()
        self.ssh_client.upload_file(local_path, remote_path)

    def download(self, remote_path: str, local_path: str | Path) -> None:
        self._ensure_connected()
        self.ssh_client.download_file(remote_path, local_path)

    # ==================== 字节级 IO ====================
    def file_size(self, path: str) -> int:
        self._ensure_connected()
        return self.ssh_client.remote_file_size(path)

    def read_bytes(self, path: str, *, offset: int = 0, length: int | None = None) -> bytes:
        self._ensure_connected()
        if offset < 0:
            raise ValueError(f"offset 不能为负数: {offset}")
        return self.ssh_client.read_bytes(path, offset=offset, length=length)

    def append_bytes(self, path: str, data: bytes) -> None:
        self._ensure_connected()
        self.ssh_client.append_bytes(path, data)

    def rename(self, src: str, dst: str) -> None:
        self._ensure_connected()
        self.ssh_client.remote_rename(src, dst)

    # ==================== 进程: 启停 / 状态 ====================
    def start_napcat(self, qq_id: str, config: "Config") -> ProcessStatus:
        """SL Bot 启动: ensure daemon → 打开扫码 (首次) → bot launcher start → 探测 status.

        与 NC 路径的语义差别:

        - 启动前 daemon 必须 READY (daemon 内部含 Xvfb / fluxbox / x11vnc / websockify / node).
          ``daemon.ensure_running()`` 引用计数 +1, 由 :meth:`stop_napcat` / :meth:`close` 释放.
        - 首次启动时**自动打开** noVNC 扫码页 (浏览器); 用户用手机 QQ 扫码登录后, daemon
          内的 SnowLuma framework 才会接管 ``qq.exe`` 注入.
        - ``qq_id`` 与 ``uin`` 当前简化为同值 (SL 单 Bot 场景下 qq_id 即用户 QQ 号);
          多账号场景 backlog: 让 UI 显式传 ``uin``.

        Raises:
            FileNotFoundError: bot launcher 脚本未部署 (调用方应引导用户走 SL 部署)
            RemoteCommandError: launcher start 退出码非 0 / status 探测失败
            Exception (来自 daemon.ensure_running): daemon 启动超时 / launcher start 失败
        """
        self._ensure_connected()
        self._verify_launcher_present()

        # 0. 渲染远端 webui.json (W10b-WebUI): daemon 启动前用 Desktop 已知密码覆盖
        # 远端 ``${snowluma}/config/webui.json``, 让 SL framework 启动时跳过自动 generate
        # + mustChangePassword 流程, 与本地 ``render_daemon_globals`` 同模式.
        # 失败不阻断 (fallback 到 daemon 自治, 但 token 拿不到, UI 会提示用户手填).
        try:
            self._ensure_remote_webui_password()
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"渲染远端 webui.json 失败, 将走 SL daemon 自治模式 (用户可能需要手动登录): {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )

        # 1. 启 daemon (ref_count +1); 失败时 daemon 内部已回滚, 这里直接透出
        ready_info = self.daemon.ensure_running()

        # 2. 首次自动打开 noVNC 扫码 (后续 Bot 共用; 失败不阻断启动)
        # 修复 (2026-05-12): ``open_snowluma_vnc`` 返回 ``(ok, message)``, 之前忽略返回值
        # 导致失败也设 ``_novnc_browser_opened=True``, 后续重试就不再弹了. 现在按返回值决定.
        if not self._novnc_browser_opened:
            try:
                from src.core.remote.snowluma.vnc_launcher import open_snowluma_vnc

                ok, message = open_snowluma_vnc(
                    self._exec_backend, self.sl_paths, ready_info.tunnels.novnc
                )
                if ok:
                    self._novnc_browser_opened = True
                    logger.info(
                        f"已自动打开 SnowLuma noVNC 扫码页: {message}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )
                else:
                    # 不设 _novnc_browser_opened=True; 下次启 Bot 时再试.
                    # 用户可手动点 BotCard 上的 "打开 VNC" 按钮 (走同样的 open_snowluma_vnc 路径).
                    logger.warning(
                        f"自动打开 noVNC 扫码页失败 (用户可手动访问 "
                        f"{ready_info.tunnels.novnc.local_url}): {message}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )
            except Exception as exc:  # noqa: BLE001
                logger.warning(
                    f"自动打开 noVNC 扫码页异常 (用户可手动访问 "
                    f"{ready_info.tunnels.novnc.local_url}): {type(exc).__name__}: {exc}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )

        # 3. 调远端 bot launcher start
        # uin 与 qq_id 同值 (SL 单账号约定); SL launcher 端会再做数字校验
        uin = qq_id if qq_id and qq_id.isdigit() else None
        cmd = self._launcher_cmds.bot_start_cmd(qq_id, uin=uin)
        try:
            result = self._exec_backend.run(cmd, timeout=60.0, check=True)
        except Exception:
            # launcher 失败 → 主动 release daemon ref, 避免 ref_count 漏扣
            try:
                self.daemon.release()
            except Exception:  # noqa: BLE001
                pass
            raise

        # 4. 探测 status_bot_<qq_id>.json
        status = self._runtime.get_bot_status(qq_id)
        if not status.running:
            from src.core.remote.errors import RemoteCommandError

            # 同样需要 release daemon ref
            try:
                self.daemon.release()
            except Exception:  # noqa: BLE001
                pass
            raise RemoteCommandError(
                command=cmd,
                exit_status=0,
                stderr=(
                    f"bot launcher 表示成功但 status_bot_{qq_id}.json 显示未运行: "
                    f"{result.stdout!r}"
                ),
            )

        # 5. 主动调远端 SL WebUI ``load_process`` 触发注入 (修复 2026-05-12):
        # 旧版只 spawn QQ 没调 SL inject, 用户必须去 WebUI 手动点 "Load" 才能让 SL framework
        # 接管 QQ. 现复用本地 :meth:`SnowLumaDriver._do_phase_c_inject` 协议, 通过已有 SSH
        # 隧道 (``127.0.0.1:webui.local_port`` → remote ``5099`` SL WebUI) 调
        # ``load_process(status.pid)``, 与本地路径行为对齐.
        #
        # 失败时**不阻断 Bot 启动** (QQ 已在远端跑, 用户可去 WebUI 手动注入兜底), 仅 emit
        # warning + 把 ``inject_error`` 塞进 ``ProcessStatus.extra`` 让 UI 判断是否提示用户.
        inject_status: str | None = None
        inject_error: str | None = None
        if status.pid:
            try:
                inject_status = self._inject_remote_qq(
                    status.pid, ready_info.tunnels.webui.local_port
                )
                logger.info(
                    (
                        f"远端 SnowLuma 自动注入完成 (QQID: {qq_id}, pid={status.pid}, "
                        f"hook_status={inject_status})"
                    ),
                    LogType.NETWORK,
                    LogSource.CORE,
                )
            except Exception as exc:  # noqa: BLE001
                inject_error = str(exc)
                logger.warning(
                    (
                        f"远端 SnowLuma 自动注入失败 (QQID: {qq_id}, pid={status.pid}); "
                        f"Bot 已启动但未注入, 用户可在 SnowLuma WebUI 手动 Load: {exc}"
                    ),
                    LogType.NETWORK,
                    LogSource.CORE,
                )
        else:
            inject_error = "status_bot 无 pid 字段, 跳过自动注入"
            logger.warning(
                (
                    f"远端 SnowLuma Bot {qq_id} 启动后 status 无 pid, 无法自动注入; "
                    "用户需手动去 WebUI 触发 Load"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )

        return ProcessStatus(
            qq_id=qq_id,
            running=True,
            pid=status.pid,
            started_at=status.started_at,
            # 远端 ``qq.exe`` 主进程 + Electron helper 子进程 RSS 累加
            # (与 NC RemoteBackend 同一份 BFS 实现, 见
            # ``src.core.remote.process_tree.fetch_process_tree_rss_bytes``)
            memory_rss_bytes=self._fetch_rss_bytes(status.pid) if status.pid else None,
            server_total_memory_bytes=self._resolve_server_total_memory_bytes(),
            extra={
                "uin": status.uin,
                "novnc_url": ready_info.tunnels.novnc.local_url,
                "webui_url": ready_info.tunnels.webui.local_url,
                "launcher_stdout": result.stdout,
                "inject_status": inject_status,
                "inject_error": inject_error,
                "elapsed_seconds": status.elapsed_seconds,
            },
        )

    def _inject_remote_qq(self, pid: int, webui_local_port: int) -> str:
        """通过 SSH 隧道调远端 SnowLuma WebUI ``load_process`` 触发 QQ 注入.

        与本地 :meth:`src.core.runtime.snowluma_driver.SnowLumaDriver._do_phase_c_inject`
        等价, 但走 SSH 隧道 (``127.0.0.1:webui_local_port`` → remote ``5099``); 远端 SL
        framework 接到请求后在 Linux 上对指定 pid 做 native dlopen + ptrace attach 注入.

        密码来源优先级 (与 :meth:`_resolve_remote_webui_password` 一致):

        1. ``self._cached_webui_password`` (``_ensure_remote_webui_password`` 刚渲染过的
           effective password, 与 daemon 实际登录密码一致)
        2. fallback 实时 resolve (``_webui_password_override`` → ``cfg`` → 远端 webui.secret)

        Args:
            pid: 远端 qq 主进程 PID (来自 ``status_bot_<qq_id>.json``).
            webui_local_port: SSH 隧道本地端口 (典型 47099); SL WebUI 通过此端口暴露.

        Returns:
            注入后 :class:`HookProcessInfo.status` 字段 (``"loaded"`` / ``"online"`` /
            ``"available"`` 等), 供调用方记 log 与 UI 提示.

        Raises:
            RuntimeError: WebUI API 失败 / 密码不可用 / SL 返 ``status="error"``.
                调用方应捕获后 emit warning, 不应阻断 Bot 启动 (QQ 已在远端跑).
        """
        # 延迟 import 避免循环依赖 (与 ``vnc_launcher`` 同模式); ``SnowLumaWebUIClient``
        # 与本地 driver 共用, 走 ``httpx`` 短连接 + Bearer token + 自动 401 重 login.
        from src.core.runtime.snowluma_webui_client import (
            SnowLumaWebUIClient,
            SnowLumaWebUIError,
        )

        if self._cached_webui_password:
            password = self._cached_webui_password
        else:
            try:
                password = self._resolve_remote_webui_password()
            except Exception as exc:  # noqa: BLE001
                raise RuntimeError(
                    f"无法解析远端 SnowLuma WebUI 密码, 无法自动注入: {exc}"
                ) from exc

        client = SnowLumaWebUIClient(
            host="127.0.0.1",
            port=webui_local_port,
            password=password,
        )
        try:
            info = client.load_process(pid)
        except SnowLumaWebUIError as exc:
            raise RuntimeError(
                f"SnowLuma WebUI load_process API 调用失败 (pid={pid}): {exc.message}"
            ) from exc

        if info.status == "error":
            raise RuntimeError(
                f"SnowLuma 注入失败 (pid={pid}): {info.error or '<no error>'}"
            )
        return info.status

    def stop_napcat(self, qq_id: str) -> None:
        """SL Bot 停止: bot launcher stop → daemon.release() (ref_count -1).

        launcher 端对未运行的 Bot 仍返回 0, 因此本方法是幂等的; 但 daemon.release()
        在 ref_count 已为 0 时也是 no-op (多调安全).

        Raises:
            FileNotFoundError: bot launcher 脚本未部署
            RemoteCommandError: launcher stop 退出码非 0 (例如 qq_id 校验失败)
        """
        self._ensure_connected()
        self._verify_launcher_present()

        cmd = self._launcher_cmds.bot_stop_cmd(qq_id)
        try:
            self._exec_backend.run(cmd, timeout=30.0, check=True)
        finally:
            # 即使 stop 命令报错 (远端 daemon 崩溃 / SSH 闪断), 也要 release ref
            # 让本 backend 的引用计数回归正确; 否则 close() 时会循环 release.
            if self._daemon is not None:
                try:
                    self._daemon.release()
                except Exception as exc:  # noqa: BLE001
                    logger.warning(
                        f"stop_napcat 后 daemon.release 异常 (静默忽略): {exc}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )

    def get_process_status(self, qq_id: str) -> ProcessStatus:
        """读取 ``status_bot_<qq_id>.json`` 返回当前运行状态.

        若 Bot 处于 running 状态且 pid 有效, 同步走 ``ps -e -o pid=,ppid=,rss=``
        累加进程树 RSS, 与 NC ``RemoteBackend.get_process_status`` 行为对齐;
        否则 ``memory_rss_bytes=None`` (UI 显示 "未知" 而非 "0").
        """
        self._ensure_connected()
        status = self._runtime.get_bot_status(qq_id)
        rss = (
            self._fetch_rss_bytes(status.pid)
            if status.running and status.pid
            else None
        )
        return ProcessStatus(
            qq_id=qq_id,
            running=status.running,
            pid=status.pid,
            started_at=status.started_at,
            memory_rss_bytes=rss,
            server_total_memory_bytes=self._resolve_server_total_memory_bytes(),
            extra={
                "uin": status.uin,
                "elapsed_seconds": status.elapsed_seconds,
            },
        )

    def get_memory_usage(self, qq_id: str) -> int | None:
        """SL Bot 远端 RSS 总和 (字节); 与 NC backend 同语义.

        实现路径: ``get_process_status`` → ``_fetch_rss_bytes(pid)`` → 全局
        ``ps -e -o pid=,ppid=,rss=`` BFS 累加 ``qq.exe`` 主进程及所有 helper 子进程.
        """
        status = self.get_process_status(qq_id)
        return status.memory_rss_bytes

    def _fetch_rss_bytes(self, pid: int) -> int | None:
        """读取远端 ``pid`` 进程树 RSS 总和 (字节); 与 NC backend ``_fetch_rss_bytes`` 同语义.

        实现委托给 :func:`src.core.remote.process_tree.fetch_process_tree_rss_bytes`,
        让 SL daemon 下的 ``qq.exe`` + Electron helper 子进程都能被累加 (单进程
        ``ps -o rss=`` 会大量低估实际占用).
        """
        from src.core.remote.process_tree import fetch_process_tree_rss_bytes

        return fetch_process_tree_rss_bytes(self._exec_backend, pid)

    # ==================== W10b-WebUI: 远端 WebUI 密码注入 ====================
    def _resolve_remote_webui_password(self) -> str:
        """解析远端 SL WebUI 有效密码; 优先级与本地 ``resolve_effective_password`` 对齐.

        Fallback 链:

        1. ``self._webui_password_override`` (per-server 字段, 由 ``ServerProfile`` 注入)
        2. ``cfg.snowluma_webui_password_override`` (App 级全局)
        3. 远端 ``webui.secret`` 文件内容 (install_snowluma.sh 生成的随机 16 字节 hex)

        Returns:
            非空字符串. 远端 ``webui.secret`` 缺失则抛 ``RuntimeError`` (上层应 fallback
            到 SL daemon 自治模式).
        """
        if self._webui_password_override:
            return self._webui_password_override

        # 延迟导入避免 UI / runtime 循环依赖
        from src.core.config import cfg

        app_override = (cfg.get(cfg.snowluma_webui_password_override) or "").strip()
        if app_override:
            return app_override

        # Fallback: 远端 webui.secret (install_snowluma.sh 生成)
        self._ensure_connected()
        secret = self.ssh_client.read_text(self.sl_paths.webui_secret).strip()
        if not secret:
            raise RuntimeError(
                f"远端 webui.secret 为空 ({self.sl_paths.webui_secret}); "
                "请检查 install_snowluma.sh 是否成功生成密码文件"
            )
        return secret

    def _ensure_remote_webui_password(self) -> str:
        """daemon 启动前渲染远端 ``${snowluma}/config/webui.json`` (scrypt hash + salt).

        让 SL framework 启动时检测到 webui.json 已存在 → 跳过 ``initial credentials``
        生成 + ``mustChangePassword`` 流程, 直接用 Desktop 注入的密码登录.

        效果:

        - 用户点 Desktop "WebUI" 按钮 → 剪贴板得到 ``effective_password``
        - 浏览器打开 SL WebUI 登录页 → 粘贴密码 → **直接登录成功** (无 mustChange 弹窗)

        无密码源 (override 都空 + webui.secret 也读不到) 时抛, 上层捕获后 fallback 到
        SL daemon 自治 (用户需手动从远端 daemon.log 拿 ``initial credentials``).

        Returns:
            渲染好的明文密码; 同时缓存到 ``self._cached_webui_password`` 供
            ``get_webui_endpoint`` 复用.
        """
        import json as _json

        from src.core.runtime.snowluma_config_renderer import build_webui_json_payload

        password = self._resolve_remote_webui_password()
        payload = build_webui_json_payload(password=password, must_change=False)
        content = _json.dumps(payload, ensure_ascii=False, indent=2)

        target = f"{self.sl_paths.config_dir}/webui.json"
        self._ensure_connected()
        # SSHClient.write_text 自动 mkdir -p 父目录
        self.ssh_client.write_text(target, content)

        self._cached_webui_password = password
        logger.info(
            (
                f"已注入远端 SnowLuma WebUI 密码 (scrypt hash 写入 {target}, "
                f"password_source={'override' if self._webui_password_override else 'cfg/secret'})"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )
        return password

    def _resolve_server_total_memory_bytes(self) -> int | None:
        """惰性探测 + 缓存远端物理总内存 (字节); 与 NC ``RemoteBackend`` 同模式.

        Session 内 ``MemTotal`` 不变, 首次成功后永久缓存; 探测失败时不缓存,
        下次轮询再试. UI 通过 ``ProcessStatus.server_total_memory_bytes`` 读到
        正确的服务器 RAM (而非 Desktop 本机 RAM).
        """
        if self._server_total_memory_bytes is not None:
            return self._server_total_memory_bytes
        from src.core.remote.process_tree import fetch_remote_total_memory_bytes

        try:
            total = fetch_remote_total_memory_bytes(self._exec_backend)
        except Exception:  # noqa: BLE001 - 探测失败不应阻断状态查询
            return None
        if total is not None and total > 0:
            self._server_total_memory_bytes = total
        return total

    # ==================== WebUI / 隧道 ====================
    def get_webui_endpoint(self, qq_id: str) -> WebUIEndpoint | None:
        """返回 SL daemon 全局 WebUI 端点 (qq_id 仅用于符合协议, 实际所有 Bot 共用).

        WebUI 是 daemon 维度的, 一台 SL 服务器只有一个; 不会因为多 Bot 而开多个隧道.

        Returns:
            ``WebUIEndpoint(base_url=隧道本地 URL, token=webui.secret 内容)``;
            daemon 未运行 / 隧道未建立时返 ``None``.
        """
        if self._daemon is None:
            logger.trace(
                f"get_webui_endpoint: daemon 未构造 (SL backend 未调 start_napcat); qq_id={qq_id}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return None
        bundle = self._daemon.tunnel_manager.get_endpoints()
        if bundle is None:
            logger.trace(
                f"get_webui_endpoint: tunnel bundle 为 None (daemon not READY / 隧道未起); qq_id={qq_id}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return None
        # W10b-WebUI: 优先用 ``_ensure_remote_webui_password`` 已缓存的 effective password.
        # 这是 daemon 启动前 Desktop 注入到 ``webui.json`` 的明文, 与 daemon 实际登录密码
        # 一致. 缓存空 (start_napcat 之前 / 注入失败) 才回退实时 resolve.
        if self._cached_webui_password:
            password = self._cached_webui_password
        else:
            try:
                password = self._resolve_remote_webui_password()
            except Exception as exc:  # noqa: BLE001
                logger.warning(
                    f"解析远端 WebUI 密码失败, endpoint token 留空: {exc}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                password = ""
        endpoint = WebUIEndpoint(
            base_url=bundle.webui.local_url,
            token=password,
        )
        logger.trace(
            f"get_webui_endpoint: base_url={endpoint.base_url}, has_token={bool(password)}, qq_id={qq_id}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        return endpoint

    def close_webui_tunnel(self, qq_id: str) -> None:  # noqa: ARG002
        """SL 隧道由 daemon 引用计数管, 不在 per-bot 级别关闭. 协议兼容用 (no-op)."""
        return None

    def open_vnc(self) -> tuple[bool, str]:
        """打开远端 SnowLuma noVNC 扫码页 (BotCard "打开 VNC" 按钮入口).

        前置条件: daemon 必须已构造且处于 READY 态 (即至少一个 Bot 已启动 / reattach 成功).
        实现复用 :func:`src.core.remote.snowluma.vnc_launcher.open_snowluma_vnc`,
        仅在调用方一侧把 daemon / 隧道存活性校验前置, 失败时给出可读 message.

        注意:
        - 不调 ``daemon.ensure_running``: 避免无脑加 ref_count 导致用户后续 stop_bot
          时 daemon 不释放. 如果 daemon 没构造, 提示用户先启动 Bot.
        - URL 含明文 vnc 密码, 仅经 ``webbrowser.open`` 交给系统浏览器, 不返回上层.

        Returns:
            ``(ok, message)``;

            - ``ok=True`` 时 message 是脱敏端点描述 (``http://127.0.0.1:<port>``);
            - ``ok=False`` 时 message 是用户可读错误描述.
        """
        if self._daemon is None:
            return False, "SnowLuma daemon 未启动, 请先启动 Bot 或等待 reattach 完成"

        bundle = self._daemon.tunnel_manager.get_endpoints()
        if bundle is None:
            return False, "SnowLuma 隧道尚未建立, 请稍后重试"

        # 项目内模块导入: 局部 import 避免顶层耦合 vnc_launcher
        from src.core.remote.snowluma.vnc_launcher import open_snowluma_vnc

        return open_snowluma_vnc(self._exec_backend, self.sl_paths, bundle.novnc)

    # ==================== 配置同步 ====================
    def write_bot_runtime_config(self, config: "Config") -> tuple[str, str]:
        """把当前 SL Bot 的 ``onebot_<uin>.json`` 同步到远端 ``$config_dir``.

        与 :meth:`RemoteBackend.write_bot_runtime_config` (NC 路径) 对称, 但只产出
        一个文件 (SL 没有独立的 napcat_<uin>.json):

        - 通过 :func:`build_onebot_payload` 复用 renderer 字段映射, 与本地
          ``<snowluma_path>/config/onebot_<uin>.json`` 写盘内容完全一致.
        - 通过 SFTP 写入 ``self.sl_paths.onebot_json(uin)`` (默认
          ``$HOME/snowluma-remote/workspace/snowluma/config/onebot_<uin>.json``).

        调用方约定与 NC 路径一致 (见 :meth:`RemoteBackend.write_bot_runtime_config`).

        Args:
            config: 完整 [`Config`](src/core/config/config_model.py) 对象.

        Returns:
            ``(remote_onebot_path, "")`` 元组; 第二项保持 NC 协议兼容 (NC 返
            ``(onebot11, napcat)`` 两元素, SL 只有 onebot 一项).
        """
        # 延迟导入避免与 [`snowluma_config_renderer`](src/core/runtime/snowluma_config_renderer.py)
        # / [`operate_config`](src/core/config/operate_config.py) 互相依赖.
        import json as _json

        from src.core.runtime.snowluma_config_renderer import build_onebot_payload

        self._ensure_connected()

        qq_id_str = str(config.bot.QQID).strip()
        if not qq_id_str.isdigit():
            raise ValueError(f"SL write_bot_runtime_config 收到非法 QQID: {config.bot.QQID!r}")

        payload = build_onebot_payload(
            int(qq_id_str),
            connect=config.connect,
            music_sign_url=config.bot.musicSignUrl,
        )
        onebot_remote = self.sl_paths.onebot_json(qq_id_str)
        # 父目录由 ``write_text`` 内部 ensure_remote_directory 自动创建
        self.ssh_client.write_text(
            onebot_remote,
            _json.dumps(payload, ensure_ascii=False, indent=2),
        )
        return onebot_remote, ""

    def delete_bot_runtime_config(self, qq_id: str) -> None:
        """删除指定 SL Bot 在远端的 ``onebot_<uin>.json``.

        与 :meth:`RemoteBackend.delete_bot_runtime_config` 对称; 文件不存在时静默成功,
        整个操作幂等.
        """
        self._ensure_connected()
        qq_id_str = str(qq_id).strip()
        if not qq_id_str.isdigit():
            # 与 NC 路径一致: 非法 qq_id 静默跳过, 避免 ``rm -f`` 出现 shell 注入风险
            logger.warning(
                f"SL delete_bot_runtime_config 收到非法 qq_id (已跳过): {qq_id!r}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return
        remote_path = self.sl_paths.onebot_json(qq_id_str)
        quoted = self.ssh_client._quote_remote_argument(remote_path)  # noqa: SLF001
        # ``rm -f`` 已经是幂等; 不抛错即可
        self._exec_backend.run(f"rm -f -- {quoted}", check=False)

    # ==================== 安装 / 部署 ====================
    def install_napcat(
        self,
        archive_path: str | Path | None = None,  # noqa: ARG002
        *,
        progress: ProgressCallback | None = None,  # noqa: ARG002
        log_callback=None,  # noqa: ARG002
        progress_log_callback=None,  # noqa: ARG002
        force_update: bool = False,  # noqa: ARG002
        expected_sha256: str | None = None,  # noqa: ARG002
        local_archive_cache: Path | None = None,  # noqa: ARG002
        should_cancel=None,  # noqa: ARG002
    ) -> None:
        """SL backend 不实现 install_napcat;  SL 部署链路独立.

        Raises:
            NotImplementedError: 调用方应改走
                [`ServerManager.deploy_server`](src/core/remote/server_manager.py)
                的 SL 分支 (``_deploy_snowluma_flavor``).
        """
        raise NotImplementedError(
            "RemoteSnowLumaBackend 不支持 install_napcat; SL 部署请走 "
            "ServerManager.deploy_server (内部会 dispatch 到 _deploy_snowluma_flavor)"
        )

    def install_qq(self, *, progress: ProgressCallback | None = None) -> None:  # noqa: ARG002
        """SL backend 不实现 install_qq; LinuxQQ 与 SL Framework 一并由 SL 部署链路安装."""
        raise NotImplementedError(
            "RemoteSnowLumaBackend 不支持 install_qq; LinuxQQ 已在 SL 部署期一并安装"
        )

    # ==================== 探测 ====================
    def detect_napcat_version(self) -> str | None:
        """SL 不装 NapCat, 返 None."""
        return None

    def detect_qq_path(self) -> str | None:
        """SL 远端 qq 二进制位于 ``$workspace_dir/opt/QQ/qq``; UI 不强依赖此值, 返 None."""
        return None

    def _detect_snowluma_framework_version(self) -> str | None:
        """读取远端 ``${snowluma_framework_dir}/package.json`` 的 version 字段."""
        pkg_path = f"{self.sl_paths.snowluma_framework_dir}/package.json"
        result = self._exec_backend.run(
            f'test -f "{pkg_path}" && cat "{pkg_path}" || true',
            check=False,
        )
        if not result.ok or not result.stdout.strip():
            return None
        match = re.search(r'"version"\s*:\s*"([^"]+)"', result.stdout)
        if match is None:
            return None
        return match.group(1).strip() or None

    def _detect_qq_version(self) -> str | None:
        """读取远端 LinuxQQ ``package.json`` 的 version 字段 (SL workspace 下)."""
        # SL 的 LinuxQQ 安装在 ${workspace_dir}/opt/QQ/ (与 NC 同构)
        pkg_path = f"{self.sl_paths.workspace_dir}/opt/QQ/resources/app/package.json"
        result = self._exec_backend.run(
            f'test -f "{pkg_path}" && cat "{pkg_path}" || true',
            check=False,
        )
        if not result.ok or not result.stdout.strip():
            return None
        match = re.search(r'"version"\s*:\s*"([^"]+)"', result.stdout)
        if match is None:
            return None
        return match.group(1).strip() or None

    def detect_installation(self) -> InstallationInfo:
        """聚合探测: 读取远端 SnowLuma.Framework 版本 + LinuxQQ 版本.

        对于 SL 后端, ``napcat_version`` 字段复用为 SnowLuma.Framework 版本
        (上层 ``redetect_versions`` 会根据 flavor 写入正确的 profile 字段).
        """
        self._ensure_connected()
        return InstallationInfo(
            napcat_version=self._detect_snowluma_framework_version(),
            qq_version=self._detect_qq_version(),
            qq_install_path=None,
        )

    # ==================== 日志 ====================
    def read_log(self, qq_id: str) -> str:
        """读取 SL Bot 完整日志.

        SL 日志路径 = ``self.sl_paths.log_bot(qq_id)``; 不存在时返空串.
        """
        self._ensure_connected()
        # cat + 静默 fallback; 与 RemoteRuntimeService.tail_bot_log 模式一致
        path = self.sl_paths.log_bot(qq_id)
        result = self._exec_backend.run(
            f'cat "{path}" 2>/dev/null || true',
            check=False,
        )
        return result.stdout if result.ok else ""

    def tail_log(self, qq_id: str, *, lines: int = 200) -> str:
        """读 SL Bot 日志末尾 N 行."""
        self._ensure_connected()
        return self._runtime.tail_bot_log(qq_id, lines=lines)

    # ==================== 资源采样 ====================
    def sample_resources(self):
        """单次远端资源采样, 与 NC ``RemoteBackend.sample_resources`` 行为一致."""
        from src.core.remote.resource_monitor import SAMPLE_COMMAND, parse_sample_output

        self._ensure_connected()
        try:
            result = self._exec_backend.run(SAMPLE_COMMAND, timeout=15)
        except Exception:  # noqa: BLE001
            return None
        if not result.ok:
            return None
        return parse_sample_output(result.stdout)

    # ==================== 内部辅助 ====================
    def _verify_launcher_present(self) -> None:
        """启停前确认 SL bot launcher 已部署到位.

        与 NC ``_verify_launcher_present`` 同语义, 但检查的是
        ``self.sl_paths.bot_launcher_script`` (``$workspace_dir/snowluma_bot_launcher.sh``).
        """
        if not self.ssh_client.remote_exists(self.sl_paths.bot_launcher_script):
            raise FileNotFoundError(
                f"远端 SnowLuma bot launcher 脚本缺失: {self.sl_paths.bot_launcher_script}; "
                "请先在服务器管理页执行远端部署 (SnowLuma flavor)"
            )


__all__ = ["RemoteSnowLumaBackend"]

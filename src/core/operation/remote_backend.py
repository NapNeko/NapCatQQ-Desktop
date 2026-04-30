# -*- coding: utf-8 -*-
"""[`RemoteBackend`](src/core/operation/remote_backend.py): 通过 SSH/SFTP 远程实现 [`OperationBackend`](src/core/operation/backend.py)。

依赖现有 [`core/remote/`](src/core/remote/__init__.py) 子系统:
- [`SSHClient`](src/core/remote/ssh_client.py): 底层 SSH/SFTP 通道
- [`RemoteRuntimeService`](src/core/remote/status.py): 远端运行态查询与启停命令
- [`LinuxCorePaths`](src/core/remote/models.py): 远端目录布局

P0 阶段实现范围:
- 文件类 8 个方法: 完整 SFTP/SSH 实现
- 检测类: ``detect_napcat_version`` / ``detect_qq_path`` / ``detect_installation``
- 进程查询: ``get_process_status`` / ``get_memory_usage``
- 日志: ``read_log`` / ``tail_log`` (映射到 [`RemoteRuntimeService.tail_log`](src/core/remote/status.py))

P1 阶段补全:
- 安装写入: ``install_napcat`` / ``install_qq`` (远端部署 MVP, 委托 [`LinuxCoreDeployment`](src/core/remote/deployment.py))

P2 阶段补全:
- P2.3: 进程启停 ``start_napcat`` / ``stop_napcat`` (走远端 launcher 脚本)
- P2.5: WebUI ``get_webui_endpoint`` (含 SSH 隧道生命周期管理) -- 仍未实现
"""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import TYPE_CHECKING

from src.core.remote.deployment import LinuxCoreDeployment
from src.core.remote.execution_backend import RemoteExecutionBackend
from src.core.remote.models import LinuxCorePaths, SSHCredentials
from src.core.remote.ssh_client import SSHClient
from src.core.remote.status import RemoteRuntimeService
from src.core.remote.tunnel import LocalPortForwarder

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


_QQ_VERSION_PACKAGE_PATTERN = re.compile(r'"version"\s*:\s*"([^"]+)"')
_PS_RSS_PATTERN = re.compile(r"^\s*(\d+)\s*$")

# WebUI 日志中 NapCat 打印的入口 URL 形如:
#   [info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=abc
#   [info] [NapCat] [WebUi] WebUi Local Panel Url: http://127.0.0.1:6099/webui?token=abc
#   [info] [NapCat] [WebUi] WebUi User Panel Url: http://[::]:6102/webui?token=abc   (IPv6 dual-stack)
#   或 host 为 0.0.0.0 / 公网 IPv4 / 带方括号的 IPv6.
# 我们只关心 ``port`` 与 ``token`` —— SSH 隧道始终走 ``127.0.0.1:port``,
# NapCat 监听 ``[::]`` 时 (Linux 默认 ``bindv6only=0``) 同样能从 ``127.0.0.1`` 命中.
# host 段需同时支持:
#   - 普通域名 / IPv4: ``127.0.0.1`` / ``0.0.0.0`` / ``host.example.com``
#   - 方括号 IPv6: ``[::]`` / ``[::1]`` / ``[2001:db8::1]``
# token 仅允许 URL 安全字符, 防止把行尾标点 / 引号吃进来.
_WEBUI_LOG_PATTERN = re.compile(
    r"https?://(?:\[[^\]\s]+\]|[^\s:/\[\]]+):(?P<port>\d+)/webui\?token=(?P<token>[A-Za-z0-9_\-.]+)"
)


class RemoteBackend(OperationBackend):
    """远端操作后端。

    通过 SSH/SFTP 在 Linux 远端执行 NapCat 操作,
    路径全部按 POSIX 风格解析(支持 ``$HOME`` 前缀, 由
    [`SSHClient._resolve_sftp_path`](src/core/remote/ssh_client.py) 内部展开)。
    """

    def __init__(self, credentials: SSHCredentials, paths: LinuxCorePaths | None = None) -> None:
        self.credentials = credentials
        self.paths = paths or LinuxCorePaths()
        self.ssh_client = SSHClient(credentials)
        self._exec_backend = RemoteExecutionBackend(self.ssh_client)
        self._runtime = RemoteRuntimeService(self._exec_backend, self.paths)
        # P1: 部署器委托给 LinuxCoreDeployment, 共用同一个 RemoteExecutionBackend
        self._deployment = LinuxCoreDeployment(self._exec_backend, self.paths)
        # P2.5: WebUI SSH 隧道按 qq_id 缓存; 远端端口漂移或 close() 时统一清理
        self._webui_tunnels: dict[str, LocalPortForwarder] = {}

    @property
    def deployment(self) -> LinuxCoreDeployment:
        """暴露底层部署器供 ServerManager / 测试直接使用。"""
        return self._deployment

    # ==================== 生命周期 ====================
    def connect(self) -> None:
        if not self.ssh_client.is_connected:
            self.ssh_client.connect()

    def close(self) -> None:
        # P2.5: 关闭 SSH 之前先关掉所有 WebUI 隧道, 避免后台线程操作
        # 已经被释放的 paramiko Transport.
        for qq_id, forwarder in list(self._webui_tunnels.items()):
            try:
                forwarder.stop()
            except Exception:  # noqa: BLE001
                pass
            self._webui_tunnels.pop(qq_id, None)
        self.ssh_client.close()

    @property
    def is_connected(self) -> bool:
        return self.ssh_client.is_connected

    # ==================== 文件 ====================
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
        # ``mkdir -p`` 同时满足 parents=True / exist_ok=True 语义,
        # 远端环境对 parents=False / exist_ok=False 的细分场景需求很弱,
        # 当用户明确要求严格语义时才走非 -p 模式。
        self._ensure_connected()
        if parents and exist_ok:
            self.ssh_client.ensure_remote_directory(path)
            return

        quoted = self.ssh_client._quote_remote_argument(path)  # noqa: SLF001 - 复用既有引用渲染
        if not parents and not exist_ok:
            self._exec_backend.run(f"mkdir -- {quoted}", check=True)
            return
        if parents and not exist_ok:
            self._exec_backend.run(
                f"mkdir -p -- {quoted} && test -z \"$(ls -A {quoted})\"",
                check=True,
            )
            return
        # parents=False, exist_ok=True
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

    # ==================== 进程 ====================
    def start_napcat(self, qq_id: str, config: "Config") -> ProcessStatus:
        """通过远端 launcher 脚本启动指定 Bot.

        路径: ``bash $launcher_script start <qq_id>``,
        其中 ``$launcher_script`` = [`LinuxCorePaths.launcher_script`](src/core/remote/models.py)
        (默认 ``$HOME/Napcat/napcat.sh``), 在 P1 部署阶段已上传到位.

        Args:
            qq_id: 待启动 QQ 号
            config: 完整 [`Config`](src/core/config/config_model.py); 启动前会通过
                [`write_bot_runtime_config`](src/core/operation/remote_backend.py)
                同步到远端 ``$config_dir/{onebot11,napcat}_<qqid>.json``,
                确保 NapCat 进程读取的是最新配置 (P2.4).

        Returns:
            启动后的 [`ProcessStatus`](src/core/operation/backend.py); 启动成功时
            ``running=True``, ``pid`` 为远端 PID.

        Raises:
            RemoteCommandError: launcher 脚本退出码非 0 (启动失败 / qq_id 校验不通过).
            SSHConnectionError: SSH 通讯异常.
        """
        self._ensure_connected()
        self._verify_launcher_present()

        # P2.4: 启动前确保远端配置最新; 失败时不阻断启动 (NapCat 仍可能用旧配置或缺省值跑起来),
        # 只记录 warning 让用户感知.
        try:
            self.write_bot_runtime_config(config)
        except Exception as exc:  # noqa: BLE001 - 配置同步失败不应阻断启动
            from src.core.logging import LogSource, LogType
            from src.core.logging import logger as _logger

            _logger.warning(
                f"远端 Bot 启动前配置同步失败(QQID={qq_id}): {type(exc).__name__}: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )

        # launcher 内部 sleep 8s + 启动 xvfb-run, 单次调用最长不超过 ~30s,
        # 但 SSH command_timeout 默认 20s 不够用; 显式给 60s 保险.
        command = (
            f'bash "{self.paths.launcher_script}" start {self._shell_quote_qq(qq_id)}'
        )
        result = self._exec_backend.run(command, timeout=60.0, check=True)
        # launcher 0 退出意味着进程探活成功, 直接复用 status 查询拿 PID.
        status = self._runtime.get_status_for_bot(qq_id)
        if not status.running:
            # launcher 表示成功但状态查询找不到 -> 极少见的竞态; 抛出原始 stdout 供排查
            from src.core.remote.errors import RemoteCommandError

            raise RemoteCommandError(
                command=command,
                exit_status=0,
                stderr=f"launcher reported success but status_for_bot says not running: {result.stdout!r}",
            )
        return ProcessStatus(
            qq_id=qq_id,
            running=True,
            pid=status.pid,
            started_at=None,
            memory_rss_bytes=self._fetch_rss_bytes(status.pid) if status.pid else None,
            extra={
                "raw_qq": status.qq,
                "version": status.version,
                "log_file": status.log_file,
                "launcher_stdout": result.stdout,
            },
        )

    def stop_napcat(self, qq_id: str) -> None:
        """通过远端 launcher 脚本停止指定 Bot.

        路径: ``bash $launcher_script stop <qq_id>``. launcher 对未运行的 Bot 仍返回 0,
        因此本方法是幂等的. SSH 异常或 launcher 校验 qq_id 失败时仍会抛错.
        """
        self._ensure_connected()
        self._verify_launcher_present()

        command = (
            f'bash "{self.paths.launcher_script}" stop {self._shell_quote_qq(qq_id)}'
        )
        # launcher 内部 sleep 3 + kill, 一般 < 10s; 给 30s 余量.
        self._exec_backend.run(command, timeout=30.0, check=True)

    def get_process_status(self, qq_id: str) -> ProcessStatus:
        """读取指定 Bot 的远端运行状态(P2 多 Bot 版).

        基于 [`RemoteRuntimeService.get_status_for_bot`](src/core/remote/status.py)
        实现, 通过 ``pgrep -f 'qq --no-sandbox -q <qq_id>'`` + cmdline 二次校验过滤目标进程,
        并合并 ``status_<qq_id>.json`` 中的辅助字段.
        """
        self._ensure_connected()
        status = self._runtime.get_status_for_bot(qq_id)
        return ProcessStatus(
            qq_id=qq_id,
            running=status.running,
            pid=status.pid,
            started_at=None,
            memory_rss_bytes=self._fetch_rss_bytes(status.pid) if status.pid else None,
            extra={
                "raw_qq": status.qq,
                "version": status.version,
                "log_file": status.log_file,
            },
        )

    def get_memory_usage(self, qq_id: str) -> int | None:
        status = self.get_process_status(qq_id)
        return status.memory_rss_bytes

    # ==================== 配置同步 (P2.4) ====================
    def write_bot_runtime_config(self, config: "Config") -> tuple[str, str]:
        """把当前 Bot 的 NapCat 配置同步到远端 ``$config_dir``.

        与本地 [`update_config`](src/core/config/operate_config.py) 写盘逻辑对齐:
        - 渲染 [`OneBotConfig`](src/core/config/config_model.py) -> ``onebot11_<qqid>.json``
        - 渲染 [`NapCatConfig`](src/core/config/config_model.py) -> ``napcat_<qqid>.json``
        - 通过 SFTP 写入 ``self.paths.config_dir`` (默认
          ``$HOME/Napcat/opt/QQ/resources/app/app_launcher/napcat/config``)

        该方法是幂等的, 调用方在以下两个时机使用:
        1. ``operate_config.update_config`` 检测到 Bot 绑定到远端时
        2. ``RemoteBackend.start_napcat`` 启动前 (兜底, 防止配置漂移)

        Args:
            config: 完整 [`Config`](src/core/config/config_model.py) 对象

        Returns:
            (remote_onebot_path, remote_napcat_path) 元组, 便于上层日志追踪.
        """
        # 延迟导入避免与 [`config_export`](src/core/config/config_export.py) /
        # [`operate_config`](src/core/config/operate_config.py) 互相依赖.
        from src.core.config.config_model import NapCatConfig, OneBotConfig

        self._ensure_connected()

        qq_id = self._shell_quote_qq(str(config.bot.QQID))

        onebot_payload = OneBotConfig(
            network=config.connect,
            musicSignUrl=config.bot.musicSignUrl,
            enableLocalFile2Url=config.advanced.enableLocalFile2Url,
            parseMultMsg=config.advanced.parseMultMsg,
        ).model_dump(mode="json")
        napcat_payload = NapCatConfig(
            fileLog=config.advanced.fileLog,
            consoleLog=config.advanced.consoleLog,
            fileLogLevel=config.advanced.fileLogLevel,
            consoleLogLevel=config.advanced.consoleLogLevel,
            packetBackend=config.advanced.packetBackend,
            packetServer=config.advanced.packetServer,
            o3HookMode=config.advanced.o3HookMode,
            bypass=config.advanced.bypass,
        ).model_dump(mode="json")

        onebot_remote = f"{self.paths.config_dir}/onebot11_{qq_id}.json"
        napcat_remote = f"{self.paths.config_dir}/napcat_{qq_id}.json"

        # 父目录由 ``write_text`` 内部 ensure_remote_directory 自动创建
        self.ssh_client.write_text(
            onebot_remote,
            json.dumps(onebot_payload, ensure_ascii=False, indent=4),
        )
        self.ssh_client.write_text(
            napcat_remote,
            json.dumps(napcat_payload, ensure_ascii=False, indent=4),
        )
        return onebot_remote, napcat_remote

    def delete_bot_runtime_config(self, qq_id: str) -> None:
        """删除指定 Bot 在远端的 NapCat 配置文件.

        与 [`delete_config`](src/core/config/operate_config.py) 对齐.
        文件不存在时静默成功, 整个操作幂等.
        """
        self._ensure_connected()
        qq_id = self._shell_quote_qq(str(qq_id))
        for filename in (f"onebot11_{qq_id}.json", f"napcat_{qq_id}.json"):
            remote_path = f"{self.paths.config_dir}/{filename}"
            quoted = self.ssh_client._quote_remote_argument(remote_path)  # noqa: SLF001
            # ``rm -f`` 已经是幂等; 不抛错即可
            self._exec_backend.run(f"rm -f -- {quoted}", check=False)

    # ==================== 安装 ====================
    def install_napcat(
        self,
        archive_path: str | Path | None = None,
        *,
        progress: ProgressCallback | None = None,
        log_callback=None,
        force_update: bool = False,
    ) -> None:
        """P1: 远端安装/更新 NapCat。

        ``archive_path`` 当前未使用 (远端脚本自行 ``curl`` 下载官方 release),
        预留接口以便 P3 支持 Desktop 本地上传安装包到内网无外网场景。

        ``force_update=True`` 强制重新下载并解压 NapCat;
        默认情况下脚本会复用远端已有 NapCat 安装。

        ``log_callback`` (P1.5): 每行远端脚本输出都会触发一次, 用于"部署控制台"实时回显。
        """
        if archive_path is not None:
            # P1 不实现自定义包路径, 但仍允许调用方传参（直接忽略并 logger.warning 比抛错更友好）
            from src.core.logging import LogSource, LogType, logger as _logger

            _logger.warning(
                f"RemoteBackend.install_napcat 暂未支持 archive_path 参数(P3 处理): {archive_path}",
                LogType.NETWORK,
                LogSource.CORE,
            )
        self._ensure_connected()
        self._deployment.install_napcat(
            progress=progress,
            log_callback=log_callback,
            force_update=force_update,
        )

    def install_qq(
        self,
        *,
        progress: ProgressCallback | None = None,
        log_callback=None,
        force_reinstall: bool = False,
    ) -> None:
        """P1: 远端安装 LinuxQQ rootless。

        ``force_reinstall=True`` 强制重装(会先备份 NapCat 配置再 ``rm -rf $install_base_dir/opt`` 后重新解压)。
        ``log_callback`` (P1.5): 每行远端脚本输出都会触发一次, 用于"部署控制台"实时回显。
        """
        self._ensure_connected()
        self._deployment.install_linuxqq(
            progress=progress,
            log_callback=log_callback,
            force_reinstall=force_reinstall,
        )

    def detect_napcat_version(self) -> str | None:
        """探测远端 NapCat 版本号。

        委托到 [`LinuxCoreDeployment._detect_napcat_version`](src/core/remote/deployment.py),
        支持现代 ``napCatVersion = "..."`` / 历史 ``const version = "..."`` 与 ``package.json`` 兜底。
        """
        self._ensure_connected()
        return self._deployment._detect_napcat_version()  # noqa: SLF001 - 同包私有方法

    def detect_qq_path(self) -> str | None:
        """探测远端 QQ 安装路径 ``{paths.qq_base_path}``; 若不存在返回 None。"""
        self._ensure_connected()
        if not self.ssh_client.remote_exists(self.paths.qq_base_path):
            return None
        return self.paths.qq_base_path

    def detect_installation(self) -> InstallationInfo:
        return InstallationInfo(
            napcat_version=self.detect_napcat_version(),
            qq_version=self._detect_qq_version(),
            qq_install_path=self.detect_qq_path(),
        )

    # ==================== 日志 ====================
    def read_log(self, qq_id: str) -> str:
        """读取远端 ``napcat_{qq_id}.log`` 全部内容。"""
        self._ensure_connected()
        log_path = f"{self.paths.log_dir}/napcat_{qq_id}.log"
        result = self._exec_backend.run(f'test -f "{log_path}" && cat "{log_path}" || true')
        return result.stdout if result.ok else ""

    def tail_log(self, qq_id: str, *, lines: int = 200) -> str:
        """读取远端 ``napcat_{qq_id}.log`` 尾部 ``lines`` 行。"""
        self._ensure_connected()
        log_path = f"{self.paths.log_dir}/napcat_{qq_id}.log"
        tail = self._runtime.tail_log(log_path, lines=lines)
        return tail.content

    # ==================== WebUI ====================
    def get_webui_endpoint(self, qq_id: str) -> WebUIEndpoint | None:
        """探测远端 NapCat WebUI 端口/Token, 通过 SSH 隧道暴露给本地 (P2.5).

        实现路径:
        1. 在远端 ``napcat_<qq_id>.log`` 中 ``grep`` ``WebUi User Panel Url`` 行
        2. 提取 ``port`` / ``token``, 失败时返回 None
        3. 复用或新建 [`LocalPortForwarder`](src/core/remote/tunnel.py),
           把远端 ``127.0.0.1:port`` 暴露为 ``127.0.0.1:<local_port>``
        4. 返回 [`WebUIEndpoint`](src/core/operation/backend.py),
           ``base_url=http://127.0.0.1:<local_port>``, 使本地 NapCat WebUI HTTP 调用
           (``GetAuthStatusRunnable`` / ``GetLoginStatusRunnable``) 无需改动即可对接.

        端口漂移处理: 若同一 ``qq_id`` 检测到的远端端口与已有隧道不一致 (例如 NapCat 重启),
        旧隧道会被关闭并重新建立.

        Returns:
            ``WebUIEndpoint`` 或 None (未发现 URL / 未启动). SSH 异常会向上抛.
        """
        self._ensure_connected()
        port_token = self._extract_webui_port_and_token(qq_id)
        if port_token is None:
            return None

        remote_port, token = port_token
        forwarder = self._webui_tunnels.get(qq_id)
        if forwarder is not None and forwarder.remote_port != remote_port:
            # 端口已变 (例如远端 NapCat 重启), 关掉旧隧道再开
            try:
                forwarder.stop()
            except Exception:  # noqa: BLE001
                pass
            self._webui_tunnels.pop(qq_id, None)
            forwarder = None

        if forwarder is None:
            forwarder = self.ssh_client.open_local_tunnel(
                remote_port,
                remote_host="127.0.0.1",
                label=f"webui-{qq_id}",
            )
            self._webui_tunnels[qq_id] = forwarder

        local_port = forwarder.local_port
        if local_port is None:
            return None
        return WebUIEndpoint(
            base_url=f"http://127.0.0.1:{local_port}",
            token=token,
        )

    def close_webui_tunnel(self, qq_id: str) -> None:
        """主动关闭指定 Bot 的 WebUI 隧道 (Bot 停止时调用)."""
        forwarder = self._webui_tunnels.pop(qq_id, None)
        if forwarder is None:
            return
        try:
            forwarder.stop()
        except Exception:  # noqa: BLE001
            pass

    def _extract_webui_port_and_token(self, qq_id: str) -> tuple[int, str] | None:
        """在远端 ``napcat_<qq_id>.log`` 中 grep WebUI URL.

        优先返回最新一次出现的 URL; NapCat 启动时会打印一次, token 会随每次重启变化.

        匹配策略 (与 [`_WEBUI_LOG_PATTERN`](src/core/operation/remote_backend.py)
        协同, 兼容多版本 NapCat 输出):
        - grep 仅按 ``/webui?token=`` 子串过滤, 不再绑死 ``WebUi User Panel Url`` 标签
          (新版本可能用 ``Local Panel Url`` / ``AccessUrl`` 等)
        - 正则只抽取 ``port`` + ``token``, 宿主任意 (``127.0.0.1`` / ``0.0.0.0`` / 公网 IP),
          因为 SSH 隧道总是走远端 ``127.0.0.1``.
        """
        # 强校验 qq_id, 避免命令注入 (即使该方法被内部调用也不放过)
        safe_qq_id = self._shell_quote_qq(qq_id)
        log_path = f"{self.paths.log_dir}/napcat_{safe_qq_id}.log"
        # ``tac`` 在 Alpine 等精简镜像上不一定可用, 改用 ``tail`` + ``grep`` 取最后一条匹配.
        # ``grep -F`` 走固定字符串匹配, 不需要转义 ``?``.
        cmd = (
            f'test -f "{log_path}" && '
            f'grep -F "/webui?token=" "{log_path}" | tail -n 1 || true'
        )
        result = self._exec_backend.run(cmd)
        if not result.ok or not result.stdout.strip():
            self._log_webui_extract_miss(qq_id, log_path, reason="empty grep result")
            return None
        match = _WEBUI_LOG_PATTERN.search(result.stdout)
        if match is None:
            self._log_webui_extract_miss(
                qq_id,
                log_path,
                reason=f"pattern miss; sample={result.stdout.strip()[:200]!r}",
            )
            return None
        try:
            port = int(match.group("port"))
        except ValueError:
            return None
        token = match.group("token").strip()
        if port <= 0 or not token:
            return None
        return port, token

    @staticmethod
    def _log_webui_extract_miss(qq_id: str, log_path: str, *, reason: str) -> None:
        """在 WebUI URL 提取失败时打一条 trace, 便于排查无二维码 / 无 WebUI 问题.

        刻意走 ``trace`` 等级避免轮询期间噪音过大; 用户在 dev 模式下能看到.
        """
        from src.core.logging import LogSource, LogType
        from src.core.logging import logger as _logger

        _logger.trace(
            f"远端 WebUI URL 提取失败(QQID={qq_id}, log={log_path}): {reason}",
            LogType.NETWORK,
            LogSource.CORE,
        )

    # ==================== 内部辅助 ====================
    def _ensure_connected(self) -> None:
        if not self.ssh_client.is_connected:
            self.ssh_client.connect()

    def _verify_launcher_present(self) -> None:
        """启动/停止前确认 launcher 脚本已部署到位.

        若服务器从未走过 P1 部署 (``self._deployment.install_napcat``) 或
        部署后 launcher 被外部清理, 直接抛出明确错误而非让 ``bash`` 自己报
        ``No such file``, 便于上层 UI 提示用户重新部署.
        """
        if not self.ssh_client.remote_exists(self.paths.launcher_script):
            raise FileNotFoundError(
                f"远端 launcher 脚本缺失: {self.paths.launcher_script}; "
                "请先在服务器管理页执行远端部署"
            )

    @staticmethod
    def _shell_quote_qq(qq_id: str) -> str:
        """对 ``qq_id`` 做严格的 shell 注入防御.

        launcher 脚本端会再做一次 ``^[0-9]{4,12}$`` 正则校验, 这里出现非数字
        即提前抛错, 避免误把畸形 qq_id 通过 SSH 发出.
        """
        normalized = str(qq_id).strip()
        if not normalized.isdigit() or not (4 <= len(normalized) <= 12):
            raise ValueError(f"非法 qq_id (必须为 4-12 位数字): {qq_id!r}")
        return normalized

    def _fetch_rss_bytes(self, pid: int) -> int | None:
        """读取远端 ``ps -o rss=`` 获取 RSS, 返回字节数。"""
        result = self._exec_backend.run(f"ps -o rss= -p {pid} 2>/dev/null || true")
        if not result.ok:
            return None
        match = _PS_RSS_PATTERN.match(result.stdout.strip())
        if match is None:
            return None
        # ``ps`` 输出单位为 KiB
        return int(match.group(1)) * 1024

    def _detect_qq_version(self) -> str | None:
        """读取远端 QQ ``package.json`` 的 version 字段。"""
        result = self._exec_backend.run(
            f'test -f "{self.paths.qq_package_json_path}" && cat "{self.paths.qq_package_json_path}" || true'
        )
        if not result.ok or not result.stdout.strip():
            return None
        match = _QQ_VERSION_PACKAGE_PATTERN.search(result.stdout)
        if match is None:
            return None
        return match.group(1).strip() or None

# -*- coding: utf-8 -*-
"""Agent 执行后端（JSON-RPC 2.0）。

将 Agent 模式集成到现有的 ExecutionBackend 体系，
为远程管理提供第三种模式选择（本地、SSH、Agent）。
"""

from __future__ import annotations

import asyncio
import logging
from pathlib import Path
from typing import Any

from PySide6.QtCore import QObject, Signal

from .execution_backend import ExecutionBackend
from .agent_client import AgentClient, AgentConnectionConfig
from .jsonrpc_protocol import NapCatMethod, NapCatStatus
from .models import RemoteCommandResult

logger = logging.getLogger(__name__)


class AgentBackend(ExecutionBackend, QObject):
    """基于 Agent 的执行后端。

    通过 WebSocket 与远程 Go Daemon 通信，实现对 NapCat 的管理。

    Signals:
        connected: 连接建立时发射
        disconnected: 连接断开时发射
        authenticated: 认证成功时发射
        status_changed: NapCat 状态变化时发射 (NapCatStatus)
        log_received: 收到日志时发射 (str)
        error: 发生错误时发射 (str)
    """

    connected = Signal()
    disconnected = Signal()
    authenticated = Signal()
    status_changed = Signal(object)  # NapCatStatus
    log_received = Signal(str)  # log line
    error = Signal(str)

    def __init__(
        self,
        config: AgentConnectionConfig,
        parent: QObject | None = None,
    ) -> None:
        QObject.__init__(self, parent)
        self._config = config
        self._client = AgentClient(self)
        self._setup_signal_forwarding()

        # Remote path cache
        self._remote_paths: dict[str, str] = {}

    def _setup_signal_forwarding(self) -> None:
        """设置信号转发。"""
        self._client.connected.connect(self.connected.emit)
        self._client.disconnected.connect(self.disconnected.emit)
        self._client.authenticated.connect(self.authenticated.emit)
        self._client.error.connect(self.error.emit)
        self._client.status_changed.connect(self.status_changed.emit)
        self._client.log_received.connect(
            lambda entry: self.log_received.emit(entry.message)
        )

    @property
    def client(self) -> AgentClient:
        """底层 Agent 客户端。"""
        return self._client

    @property
    def is_connected(self) -> bool:
        """是否已连接。"""
        return self._client.is_connected

    @property
    def is_authenticated(self) -> bool:
        """是否已认证。"""
        return self._client.is_authenticated

    def connect(self) -> None:
        """建立连接。"""
        self._client.connect_to_agent(self._config)

    def disconnect(self) -> None:
        """断开连接。"""
        self._client.disconnect_from_agent()

    # ExecutionBackend interface implementation

    def run(
        self,
        command: str,
        *,
        timeout: float | None = None,
        check: bool = False,
    ) -> RemoteCommandResult:
        """执行命令。

        注意：Agent 模式下命令执行受限，主要用于状态查询和控制操作。
        大多数命令会被转换为对应的 Agent 方法调用。

        Args:
            command: 要执行的命令
            timeout: 超时时间
            check: 是否检查返回码

        Returns:
            RemoteCommandResult: 执行结果
        """
        # Run async operation in sync context
        try:
            loop = asyncio.get_event_loop()
        except RuntimeError:
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)

        try:
            result = loop.run_until_complete(
                self._run_async(command, timeout)
            )
            if check and not result.ok:
                raise RuntimeError(
                    f"Command failed with exit code {result.exit_status}: {result.stderr}"
                )
            return result
        except Exception as e:
            logger.exception(f"Command execution failed: {e}")
            return RemoteCommandResult(
                command=command,
                exit_status=-1,
                stdout="",
                stderr=str(e),
            )

    async def _run_async(self, command: str, timeout: float | None) -> RemoteCommandResult:
        """异步执行命令。"""
        # Map common commands to Agent methods
        cmd_lower = command.lower().strip()

        # Status check commands
        if any(kw in cmd_lower for kw in ["status", "ps", "pidof"]):
            try:
                status = await self._client.get_status()
                output = f"Running: {status.running}"
                if status.pid:
                    output += f"\nPID: {status.pid}"
                if status.qq:
                    output += f"\nQQ: {status.qq}"
                if status.version:
                    output += f"\nVersion: {status.version}"

                return RemoteCommandResult(
                    command=command,
                    exit_status=0 if status.running else 1,
                    stdout=output,
                    stderr="",
                )
            except Exception as e:
                return RemoteCommandResult(
                    command=command,
                    exit_status=-1,
                    stdout="",
                    stderr=str(e),
                )

        # Start command
        if any(kw in cmd_lower for kw in ["start", "./napcat", "node "]):
            try:
                result = await self._client.start_napcat()
                return RemoteCommandResult(
                    command=command,
                    exit_status=0,
                    stdout=f"NapCat started: {result}",
                    stderr="",
                )
            except Exception as e:
                return RemoteCommandResult(
                    command=command,
                    exit_status=-1,
                    stdout="",
                    stderr=str(e),
                )

        # Stop command
        if any(kw in cmd_lower for kw in ["stop", "kill", "pkill"]):
            try:
                result = await self._client.stop_napcat()
                return RemoteCommandResult(
                    command=command,
                    exit_status=0,
                    stdout=f"NapCat stopped: {result}",
                    stderr="",
                )
            except Exception as e:
                return RemoteCommandResult(
                    command=command,
                    exit_status=-1,
                    stdout="",
                    stderr=str(e),
                )

        # Restart command
        if "restart" in cmd_lower:
            try:
                result = await self._client.restart_napcat()
                return RemoteCommandResult(
                    command=command,
                    exit_status=0,
                    stdout=f"NapCat restarted: {result}",
                    stderr="",
                )
            except Exception as e:
                return RemoteCommandResult(
                    command=command,
                    exit_status=-1,
                    stdout="",
                    stderr=str(e),
                )

        # For other commands, return not supported
        logger.warning(f"Command not supported in Agent mode: {command}")
        return RemoteCommandResult(
            command=command,
            exit_status=-1,
            stdout="",
            stderr=f"Command not supported in Agent mode: {command}",
        )

    def ensure_directory(self, path: str) -> RemoteCommandResult:
        """确保目录存在。

        注意：Agent 模式下目录管理由远程 Daemon 自动处理。

        Args:
            path: 目录路径

        Returns:
            RemoteCommandResult: 操作结果
        """
        # Agent 模式下目录由远程 Daemon 管理，直接返回成功
        logger.debug(f"Directory ensure requested (Agent mode auto-manages): {path}")
        return RemoteCommandResult(
            command=f"mkdir -p {path}",
            exit_status=0,
            stdout=f"Directory managed by Agent: {path}",
            stderr="",
        )

    def upload_file(self, local_path: str | Path, target_path: str) -> None:
        """上传文件。

        通过 Agent 的文件上传接口实现。

        Args:
            local_path: 本地文件路径
            target_path: 目标路径
        """
        local = Path(local_path)
        if not local.exists():
            raise FileNotFoundError(f"Local file not found: {local}")

        try:
            loop = asyncio.get_event_loop()
        except RuntimeError:
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)

        try:
            # Read file content
            content = local.read_bytes()

            # Upload via Agent
            loop.run_until_complete(
                self._client.request(
                    NapCatMethod.FILE_UPLOAD,
                    {
                        "path": target_path,
                        "content": content.hex(),  # Send as hex string
                        "encoding": "hex",
                    }
                )
            )
            logger.info(f"File uploaded: {local} -> {target_path}")

        except Exception as e:
            logger.exception(f"File upload failed: {e}")
            raise

    def download_file(self, source_path: str, local_path: str | Path) -> None:
        """下载文件。

        通过 Agent 的文件下载接口实现。

        Args:
            source_path: 源文件路径
            local_path: 本地保存路径
        """
        local = Path(local_path)

        try:
            loop = asyncio.get_event_loop()
        except RuntimeError:
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)

        try:
            # Download via Agent
            response = loop.run_until_complete(
                self._client.request(
                    NapCatMethod.FILE_DOWNLOAD,
                    {"path": source_path}
                )
            )

            if not response.is_success:
                raise RuntimeError(
                    response.error.message if response.error else "Download failed"
                )

            # Write file
            result = response.result or {}
            content_hex = result.get("content", "")
            encoding = result.get("encoding", "hex")

            if encoding == "hex":
                content = bytes.fromhex(content_hex)
            else:
                content = content_hex.encode("utf-8")

            local.parent.mkdir(parents=True, exist_ok=True)
            local.write_bytes(content)
            logger.info(f"File downloaded: {source_path} -> {local}")

        except Exception as e:
            logger.exception(f"File download failed: {e}")
            raise

    # NapCat specific methods

    async def start_napcat_async(self, work_dir: str | None = None) -> dict[str, Any]:
        """异步启动 NapCat。

        Args:
            work_dir: 工作目录

        Returns:
            启动结果
        """
        return await self._client.start_napcat(work_dir)

    async def stop_napcat_async(self) -> dict[str, Any]:
        """异步停止 NapCat。

        Returns:
            停止结果
        """
        return await self._client.stop_napcat()

    async def restart_napcat_async(self, work_dir: str | None = None) -> dict[str, Any]:
        """异步重启 NapCat。

        Args:
            work_dir: 工作目录

        Returns:
            重启结果
        """
        return await self._client.restart_napcat(work_dir)

    async def get_status_async(self) -> NapCatStatus:
        """异步获取 NapCat 状态。

        Returns:
            当前状态
        """
        return await self._client.get_status()

    async def subscribe_logs_async(self, level: str | None = None) -> None:
        """异步订阅日志流。

        Args:
            level: 日志级别过滤
        """
        await self._client.subscribe_logs(level)

    async def unsubscribe_logs_async(self) -> None:
        """异步取消订阅日志流。"""
        await self._client.unsubscribe_logs()

    def on_status_changed(self, callback: Any) -> None:
        """注册状态变化回调。

        Args:
            callback: 回调函数，接收 NapCatStatus 参数
        """
        self.status_changed.connect(callback)

    def on_log_received(self, callback: Any) -> None:
        """注册日志接收回调。

        Args:
            callback: 回调函数，接收 str 参数（日志内容）
        """
        self.log_received.connect(callback)

    def wait_for_connected(self, timeout: float = 30.0) -> bool:
        """等待连接建立。

        Args:
            timeout: 超时时间（秒）

        Returns:
            是否在超时前连接成功
        """
        import time
        start = time.time()
        while time.time() - start < timeout:
            if self.is_connected and self.is_authenticated:
                return True
            time.sleep(0.1)
        return False

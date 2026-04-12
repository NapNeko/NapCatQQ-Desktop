# -*- coding: utf-8 -*-
"""SSH 客户端封装。"""

from __future__ import annotations

import shlex
import socket
from pathlib import Path, PurePosixPath
from typing import Any

from src.core.logging import LogSource, LogType, logger

from .errors import RemoteCommandError, SSHAuthenticationError, SSHConnectionError, SSHHostKeyError
from .models import RemoteCommandResult, SSHCredentials

try:
    import paramiko
except ImportError:  # pragma: no cover - 依赖缺失时在运行期给出清晰错误
    paramiko = None


class SSHClient:
    """受限默认配置的 SSH 客户端。

    默认策略：
    - 拒绝未知主机指纹
    - 关闭 agent 与自动搜索本地密钥
    - 默认不使用 PTY，避免引入额外 shell 行为差异
    """

    def __init__(self, credentials: SSHCredentials) -> None:
        self.credentials = credentials
        self._client: "paramiko.SSHClient | None" = None

    def connect(self) -> None:
        """建立 SSH 连接。"""
        self.credentials.validate()
        self._ensure_paramiko_available()

        client = paramiko.SSHClient()
        self._apply_host_key_policy(client)

        connect_kwargs: dict[str, Any] = {
            "hostname": self.credentials.host,
            "port": self.credentials.port,
            "username": self.credentials.username,
            "timeout": self.credentials.connect_timeout,
            "auth_timeout": self.credentials.connect_timeout,
            "banner_timeout": self.credentials.connect_timeout,
            "allow_agent": self.credentials.allow_agent,
            "look_for_keys": self.credentials.look_for_keys,
        }
        if self.credentials.auth_method == "password":
            connect_kwargs["password"] = self.credentials.password
        else:
            connect_kwargs["key_filename"] = str(self.credentials.private_key_file)
            if self.credentials.private_key_passphrase:
                connect_kwargs["passphrase"] = self.credentials.private_key_passphrase

        logger.info(
            (
                "准备建立 SSH 连接: "
                f"host={self.credentials.host}, port={self.credentials.port}, "
                f"username={self.credentials.username}, auth_method={self.credentials.auth_method}, "
                f"host_key_policy={self.credentials.host_key_policy}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

        try:
            client.connect(**connect_kwargs)
        except paramiko.BadHostKeyException as exc:
            raise SSHHostKeyError(f"SSH 主机指纹校验失败: {exc}") from exc
        except paramiko.AuthenticationException as exc:
            raise SSHAuthenticationError(f"SSH 认证失败: {exc}") from exc
        except (paramiko.SSHException, socket.timeout, TimeoutError, OSError) as exc:
            raise SSHConnectionError(f"SSH 连接失败: {exc}") from exc

        self._client = client
        logger.info(
            f"SSH 连接已建立: host={self.credentials.host}, username={self.credentials.username}",
            LogType.NETWORK,
            LogSource.CORE,
        )

    def close(self) -> None:
        """关闭 SSH 连接。"""
        if self._client is None:
            return

        self._client.close()
        self._client = None
        logger.info(
            f"SSH 连接已关闭: host={self.credentials.host}, username={self.credentials.username}",
            LogType.NETWORK,
            LogSource.CORE,
        )

    def run(self, command: str, *, timeout: float | None = None, get_pty: bool = False, check: bool = False) -> RemoteCommandResult:
        """执行远程命令。"""
        client = self._require_client()
        effective_timeout = timeout or self.credentials.command_timeout

        logger.trace(
            (
                "执行远程命令: "
                f"host={self.credentials.host}, timeout={effective_timeout}, get_pty={get_pty}, command={command}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

        try:
            _stdin, stdout, stderr = client.exec_command(command, timeout=effective_timeout, get_pty=get_pty)
            exit_status = stdout.channel.recv_exit_status()
            result = RemoteCommandResult(
                command=command,
                exit_status=exit_status,
                stdout=stdout.read().decode("utf-8", errors="replace"),
                stderr=stderr.read().decode("utf-8", errors="replace"),
            )
        except (paramiko.SSHException, socket.timeout, TimeoutError, OSError) as exc:
            raise SSHConnectionError(f"远程命令执行异常: {exc}") from exc

        if check and not result.ok:
            raise RemoteCommandError(command=result.command, exit_status=result.exit_status, stderr=result.stderr)
        return result

    def ensure_remote_directory(self, remote_path: str) -> RemoteCommandResult:
        """确保远端目录存在。"""
        return self.run(f"mkdir -p -- {self._quote_remote_argument(remote_path)}", check=True)

    def upload_file(self, local_path: str | Path, remote_path: str) -> None:
        """上传单个文件。"""
        client = self._require_client()
        local_file = Path(local_path)
        if not local_file.exists():
            raise FileNotFoundError(f"待上传文件不存在: {local_file}")

        self.ensure_remote_directory(PurePosixPath(remote_path).parent.as_posix())

        try:
            with client.open_sftp() as sftp:
                sftp.put(str(local_file), remote_path)
        except (OSError, paramiko.SSHException) as exc:
            raise SSHConnectionError(f"上传文件失败: {exc}") from exc

    def download_file(self, remote_path: str, local_path: str | Path) -> None:
        """下载单个文件。"""
        client = self._require_client()
        local_file = Path(local_path)
        local_file.parent.mkdir(parents=True, exist_ok=True)

        try:
            with client.open_sftp() as sftp:
                sftp.get(remote_path, str(local_file))
        except (OSError, paramiko.SSHException) as exc:
            raise SSHConnectionError(f"下载文件失败: {exc}") from exc

    def __enter__(self) -> "SSHClient":
        self.connect()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

    @staticmethod
    def _ensure_paramiko_available() -> None:
        if paramiko is None:
            raise SSHConnectionError("当前环境未安装 paramiko，无法启用远程 SSH 能力")

    def _apply_host_key_policy(self, client: "paramiko.SSHClient") -> None:
        client.load_system_host_keys()
        if self.credentials.host_key_policy == "reject":
            client.set_missing_host_key_policy(paramiko.RejectPolicy())
            return
        if self.credentials.host_key_policy == "warning":
            client.set_missing_host_key_policy(paramiko.WarningPolicy())
            return
        client.set_missing_host_key_policy(paramiko.AutoAddPolicy())

    def _require_client(self) -> "paramiko.SSHClient":
        if self._client is None:
            raise SSHConnectionError("SSH 尚未连接，请先调用 connect()")
        return self._client

    @staticmethod
    def _quote_remote_argument(value: str) -> str:
        """为远端 shell 渲染参数。"""
        if value.startswith("$HOME"):
            return f'"{value}"'
        return shlex.quote(value)

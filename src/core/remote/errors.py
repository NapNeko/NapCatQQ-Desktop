# -*- coding: utf-8 -*-
"""远程管理相关异常定义。"""


class RemoteError(RuntimeError):
    """远程管理基础异常。"""


class SSHConnectionError(RemoteError):
    """SSH 连接失败。"""


class SSHAuthenticationError(SSHConnectionError):
    """SSH 认证失败。"""


class SSHHostKeyError(SSHConnectionError):
    """SSH 主机指纹校验失败。"""


class RemoteCommandError(RemoteError):
    """远程命令执行失败。"""

    def __init__(self, command: str, exit_status: int, stderr: str = "") -> None:
        self.command = command
        self.exit_status = exit_status
        self.stderr = stderr
        super().__init__(self._build_message())

    def _build_message(self) -> str:
        message = f"远程命令执行失败(exit_status={self.exit_status})"
        if self.command:
            message += f": {self.command}"
        if self.stderr.strip():
            message += f" | stderr={self.stderr.strip()}"
        return message

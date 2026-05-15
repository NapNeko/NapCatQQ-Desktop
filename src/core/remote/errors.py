# -*- coding: utf-8 -*-
"""远程管理相关异常定义. """


class RemoteError(RuntimeError):
    """远程管理基础异常. """


class SSHConnectionError(RemoteError):
    """SSH 连接失败. """


class SSHAuthenticationError(SSHConnectionError):
    """SSH 认证失败. """


class SSHHostKeyError(SSHConnectionError):
    """SSH 主机指纹校验失败. """


class RemoteCommandError(RemoteError):
    """远程命令执行失败. """

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


class RemoteDeploymentError(RemoteError):
    """远端部署流程异常(P1). 

    包装 [`RemoteCommandError`](src/core/remote/errors.py) / [`SSHConnectionError`](src/core/remote/errors.py)
    等底层异常, 附带阶段标签便于上层定位失败步骤. 
    """

    def __init__(self, stage: str, message: str, *, cause: Exception | None = None) -> None:
        self.stage = stage
        self.cause = cause
        super().__init__(f"[{stage}] {message}")


class RemoteDeploymentInProgressError(RemoteError):
    """远端正在部署, 拒绝并发部署请求(P1). """


class RemoteDeploymentCancelledError(RemoteDeploymentError):
    """用户在控制台点击 "取消部署" 主动中止流程时抛出.

    与 :class:`RemoteDeploymentError` 的区别:
    - ``stage`` 固定为 ``"cancelled"``, 上层 UI / friendly_errors 据此走"已取消"专属文案
    - ServerManager 在 except 时把档案状态重置为 ``UNDEPLOYED`` 而**不是** ``FAILED``,
      因为"取消"语义上不属于"失败"--下次用户点部署应该是干净起点
    """

    def __init__(self, message: str = "部署已被用户取消", *, cause: Exception | None = None) -> None:
        super().__init__("cancelled", message, cause=cause)

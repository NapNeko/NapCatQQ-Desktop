# -*- coding: utf-8 -*-
"""远程管理聚合服务。"""

from __future__ import annotations

from pathlib import Path

from src.core.logging import LogSource, LogType, logger

from .deployment import LinuxCoreDeployment, LinuxCoreDeploymentProbe
from .execution_backend import RemoteExecutionBackend
from .models import LinuxCorePaths, SSHCredentials
from .ssh_client import SSHClient
from .status import RemoteLogTail, RemoteNapCatStatus, RemoteRuntimeService


class RemoteManager:
    """远程 Linux Core 管理入口。

    封装 SSH 连接、部署、状态查询与日志读取，
    作为 UI/业务层调用远程能力的统一入口。
    """

    def __init__(self, credentials: SSHCredentials, paths: LinuxCorePaths | None = None) -> None:
        self.credentials = credentials
        self.paths = paths or LinuxCorePaths()
        self.ssh_client = SSHClient(credentials)
        self.backend = RemoteExecutionBackend(self.ssh_client)
        self.deployment = LinuxCoreDeployment(self.backend, self.paths)
        self.runtime = RemoteRuntimeService(self.backend, self.paths)

    def connect(self) -> None:
        """建立远程连接。"""
        logger.info(
            f"远程管理器开始连接服务器: host={self.credentials.host}, username={self.credentials.username}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        self.ssh_client.connect()

    def close(self) -> None:
        """关闭远程连接。"""
        self.ssh_client.close()

    def probe_environment(self) -> LinuxCoreDeploymentProbe:
        """探测远端环境。"""
        return self.deployment.probe_environment()

    def initialize_workspace(self) -> None:
        """初始化远端工作目录。"""
        self.deployment.initialize_layout()

    def upload_package(self, local_archive: str | Path, remote_filename: str | None = None) -> str:
        """上传安装包。"""
        return self.deployment.upload_package(local_archive, remote_filename)

    def upload_config_archive(self, local_archive: str | Path, remote_filename: str = "config-export.zip") -> str:
        """上传配置导出包。"""
        return self.deployment.upload_config_archive(local_archive, remote_filename)

    def get_status(self) -> RemoteNapCatStatus:
        """获取远端运行状态。"""
        return self.runtime.get_status()

    def tail_log(self, log_path: str | None = None, *, lines: int = 200) -> RemoteLogTail:
        """读取远端日志。"""
        return self.runtime.tail_log(log_path, lines=lines)

    def start(self, command: str) -> None:
        """启动远端服务。"""
        self.runtime.start(command)

    def stop(self) -> None:
        """停止远端服务。"""
        self.runtime.stop()

    def restart(self, command: str) -> None:
        """重启远端服务。"""
        self.runtime.restart(command)

    def __enter__(self) -> "RemoteManager":
        self.connect()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

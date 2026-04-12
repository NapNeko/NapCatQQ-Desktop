# -*- coding: utf-8 -*-
"""远程/本地统一执行后端抽象。"""

from __future__ import annotations

import subprocess
from abc import ABC, abstractmethod
from pathlib import Path

from .models import RemoteCommandResult
from .ssh_client import SSHClient


class ExecutionBackend(ABC):
    """统一执行后端接口。"""

    @abstractmethod
    def run(self, command: str, *, timeout: float | None = None, check: bool = False) -> RemoteCommandResult:
        """执行命令。"""

    @abstractmethod
    def ensure_directory(self, path: str) -> RemoteCommandResult:
        """确保目录存在。"""

    @abstractmethod
    def upload_file(self, local_path: str | Path, target_path: str) -> None:
        """上传文件。"""

    @abstractmethod
    def download_file(self, source_path: str, local_path: str | Path) -> None:
        """下载文件。"""


class LocalExecutionBackend(ExecutionBackend):
    """本地执行后端。

    该后端用于为未来的双模式统一接口做兼容层，
    目前主要服务于接口抽象与测试隔离。
    """

    def run(self, command: str, *, timeout: float | None = None, check: bool = False) -> RemoteCommandResult:
        completed = subprocess.run(
            command,
            shell=True,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        result = RemoteCommandResult(
            command=command,
            exit_status=completed.returncode,
            stdout=completed.stdout,
            stderr=completed.stderr,
        )
        if check and not result.ok:
            raise subprocess.CalledProcessError(
                completed.returncode,
                command,
                output=completed.stdout,
                stderr=completed.stderr,
            )
        return result

    def ensure_directory(self, path: str) -> RemoteCommandResult:
        Path(path).mkdir(parents=True, exist_ok=True)
        return RemoteCommandResult(command=f"mkdir -p {path}", exit_status=0)

    def upload_file(self, local_path: str | Path, target_path: str) -> None:
        source = Path(local_path)
        target = Path(target_path)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(source.read_bytes())

    def download_file(self, source_path: str, local_path: str | Path) -> None:
        source = Path(source_path)
        target = Path(local_path)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(source.read_bytes())


class RemoteExecutionBackend(ExecutionBackend):
    """基于 SSH 的远程执行后端。"""

    def __init__(self, ssh_client: SSHClient) -> None:
        self._ssh_client = ssh_client

    def run(self, command: str, *, timeout: float | None = None, check: bool = False) -> RemoteCommandResult:
        return self._ssh_client.run(command, timeout=timeout, check=check)

    def ensure_directory(self, path: str) -> RemoteCommandResult:
        return self._ssh_client.ensure_remote_directory(path)

    def upload_file(self, local_path: str | Path, target_path: str) -> None:
        self._ssh_client.upload_file(local_path, target_path)

    def download_file(self, source_path: str, local_path: str | Path) -> None:
        self._ssh_client.download_file(source_path, local_path)

# -*- coding: utf-8 -*-
"""[`RemoteBackend`](src/desktop/core/operation/remote_backend.py): 通过 SSH/SFTP 远程实现 [`OperationBackend`](src/desktop/core/operation/backend.py)。

依赖现有 [`core/remote/`](src/desktop/core/remote/__init__.py) 子系统:
- [`SSHClient`](src/desktop/core/remote/ssh_client.py): 底层 SSH/SFTP 通道
- [`RemoteRuntimeService`](src/desktop/core/remote/status.py): 远端运行态查询与启停命令
- [`LinuxCorePaths`](src/desktop/core/remote/models.py): 远端目录布局

P0 阶段实现范围:
- 文件类 8 个方法: 完整 SFTP/SSH 实现
- 检测类: ``detect_napcat_version`` / ``detect_qq_path`` / ``detect_installation``
- 进程查询: ``get_process_status`` / ``get_memory_usage``
- 日志: ``read_log`` / ``tail_log`` (映射到 [`RemoteRuntimeService.tail_log`](src/desktop/core/remote/status.py))

P1 阶段补全(目前抛 NotImplementedError):
- 安装写入: ``install_napcat`` / ``install_qq`` (远端部署 MVP, 复用 [`LinuxCoreDeployment`](src/desktop/core/remote/deployment.py))

P2 阶段补全:
- 进程启停: ``start_napcat`` / ``stop_napcat`` (基于 [`Config`](src/desktop/core/config/config_model.py) 渲染启动命令)
- WebUI: ``get_webui_endpoint`` (含 SSH 隧道生命周期管理)
"""

from __future__ import annotations

import re
from pathlib import Path
from typing import TYPE_CHECKING

from src.desktop.core.remote.execution_backend import RemoteExecutionBackend
from src.desktop.core.remote.models import LinuxCorePaths, SSHCredentials
from src.desktop.core.remote.ssh_client import SSHClient
from src.desktop.core.remote.status import RemoteRuntimeService

from .backend import (
    FileEntry,
    InstallationInfo,
    OperationBackend,
    ProcessStatus,
    ProgressCallback,
    WebUIEndpoint,
)

if TYPE_CHECKING:
    from src.desktop.core.config.config_model import Config


_QQ_VERSION_PACKAGE_PATTERN = re.compile(r'"version"\s*:\s*"([^"]+)"')
_PS_RSS_PATTERN = re.compile(r"^\s*(\d+)\s*$")

_P1_DEFER_MESSAGE = (
    "RemoteBackend 当前方法已在 OperationBackend 接口中定义, "
    "实现排期: P1 阶段(远端部署 MVP), 复用 LinuxCoreDeployment 落地"
)
_P2_DEFER_MESSAGE = (
    "RemoteBackend 当前方法已在 OperationBackend 接口中定义, "
    "实现排期: P2 阶段(远端 Bot 运行闭环 + WebUI 透传)"
)


class RemoteBackend(OperationBackend):
    """远端操作后端。

    通过 SSH/SFTP 在 Linux 远端执行 NapCat 操作,
    路径全部按 POSIX 风格解析(支持 ``$HOME`` 前缀, 由
    [`SSHClient._resolve_sftp_path`](src/desktop/core/remote/ssh_client.py) 内部展开)。
    """

    def __init__(self, credentials: SSHCredentials, paths: LinuxCorePaths | None = None) -> None:
        self.credentials = credentials
        self.paths = paths or LinuxCorePaths()
        self.ssh_client = SSHClient(credentials)
        self._exec_backend = RemoteExecutionBackend(self.ssh_client)
        self._runtime = RemoteRuntimeService(self._exec_backend, self.paths)

    # ==================== 生命周期 ====================
    def connect(self) -> None:
        if not self.ssh_client.is_connected:
            self.ssh_client.connect()

    def close(self) -> None:
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
        raise NotImplementedError(_P2_DEFER_MESSAGE)

    def stop_napcat(self, qq_id: str) -> None:
        raise NotImplementedError(_P2_DEFER_MESSAGE)

    def get_process_status(self, qq_id: str) -> ProcessStatus:
        """通过 [`RemoteRuntimeService.get_status`](src/desktop/core/remote/status.py) 获取远端运行态。

        Args:
            qq_id: 期望匹配的 QQ 号; 若与远端探测到的 QQ 号不一致, 仍按远端实际状态返回, 由调用方做对账。
        """
        self._ensure_connected()
        status = self._runtime.get_status()
        return ProcessStatus(
            qq_id=qq_id,
            running=status.running and (status.qq is None or status.qq == qq_id),
            pid=status.pid,
            started_at=None,
            memory_rss_bytes=self._fetch_rss_bytes(status.pid) if status.pid else None,
            extra={"raw_qq": status.qq, "version": status.version, "log_file": status.log_file},
        )

    def get_memory_usage(self, qq_id: str) -> int | None:
        status = self.get_process_status(qq_id)
        return status.memory_rss_bytes

    # ==================== 安装 ====================
    def install_napcat(
        self,
        archive_path: str | Path | None = None,
        *,
        progress: ProgressCallback | None = None,
    ) -> None:
        raise NotImplementedError(_P1_DEFER_MESSAGE)

    def install_qq(self, *, progress: ProgressCallback | None = None) -> None:
        raise NotImplementedError(_P1_DEFER_MESSAGE)

    def detect_napcat_version(self) -> str | None:
        """从 ``{napcat_dir}/napcat.mjs`` 中 ``const version = "..."`` 读取。"""
        self._ensure_connected()
        result = self._exec_backend.run(
            f"grep 'const version = ' '{self.paths.napcat_dir}/napcat.mjs' 2>/dev/null | "
            r"sed -n 's/.*const version = \"\([^\"]*\)\".*/\1/p' || true"
        )
        if not result.ok:
            return None
        version = result.stdout.strip()
        return version or None

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
        raise NotImplementedError(_P2_DEFER_MESSAGE)

    # ==================== 内部辅助 ====================
    def _ensure_connected(self) -> None:
        if not self.ssh_client.is_connected:
            self.ssh_client.connect()

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

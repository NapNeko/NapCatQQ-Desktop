# -*- coding: utf-8 -*-
"""Linux Core 部署骨架。"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from .execution_backend import ExecutionBackend
from .models import LinuxCorePaths, RemoteCommandResult


@dataclass(slots=True)
class LinuxCoreDeploymentProbe:
    """Linux Core 环境探测结果。"""

    os_name: str
    architecture: str
    has_bash: bool
    has_tar: bool
    has_unzip: bool


class LinuxCoreDeployment:
    """Linux Core 部署器。

    当前阶段先提供远端目录初始化、环境探测和安装包上传能力，
    后续再继续扩展成完整的部署/升级/回滚流程。
    """

    def __init__(self, backend: ExecutionBackend, paths: LinuxCorePaths | None = None) -> None:
        self.backend = backend
        self.paths = paths or LinuxCorePaths()

    def probe_environment(self) -> LinuxCoreDeploymentProbe:
        """探测 Linux 基础环境。"""
        os_result = self.backend.run("uname -s")
        arch_result = self.backend.run("uname -m")
        bash_result = self.backend.run("command -v bash >/dev/null 2>&1")
        tar_result = self.backend.run("command -v tar >/dev/null 2>&1")
        unzip_result = self.backend.run("command -v unzip >/dev/null 2>&1")

        return LinuxCoreDeploymentProbe(
            os_name=os_result.stdout.strip(),
            architecture=arch_result.stdout.strip(),
            has_bash=bash_result.ok,
            has_tar=tar_result.ok,
            has_unzip=unzip_result.ok,
        )

    def initialize_layout(self) -> list[RemoteCommandResult]:
        """初始化远端目录布局。"""
        results: list[RemoteCommandResult] = []
        for path in (
            self.paths.workspace_dir,
            self.paths.runtime_dir,
            self.paths.config_dir,
            self.paths.log_dir,
            self.paths.tmp_dir,
            self.paths.package_dir,
        ):
            results.append(self.backend.ensure_directory(path))
        return results

    def upload_package(self, local_archive: str | Path, remote_filename: str | None = None) -> str:
        """上传安装包到远端包目录。"""
        local_file = Path(local_archive)
        filename = remote_filename or local_file.name
        remote_path = PurePosixPath(self.paths.package_dir, filename).as_posix()
        self.backend.ensure_directory(self.paths.package_dir)
        self.backend.upload_file(local_file, remote_path)
        return remote_path

    def upload_config_archive(self, local_archive: str | Path, remote_filename: str = "config-export.zip") -> str:
        """上传配置包到远端临时目录。"""
        remote_path = PurePosixPath(self.paths.tmp_dir, remote_filename).as_posix()
        self.backend.ensure_directory(self.paths.tmp_dir)
        self.backend.upload_file(local_archive, remote_path)
        return remote_path

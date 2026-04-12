# -*- coding: utf-8 -*-
"""Linux Core 部署骨架。"""

from __future__ import annotations

import tempfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from src.core.config.config_export import ExportExecutionPlan, apply_config_export, scan_current_config_export

from .execution_backend import ExecutionBackend
from .models import LinuxCorePaths, RemoteCommandResult
from .templates import build_linux_deploy_script


@dataclass(slots=True)
class LinuxCoreDeploymentProbe:
    """Linux Core 环境探测结果。"""

    os_name: str
    architecture: str
    has_bash: bool
    has_tar: bool
    has_unzip: bool


@dataclass(slots=True)
class RemoteConfigSyncResult:
    """远端配置同步结果。"""

    remote_archive_path: str
    archive_name: str
    app_exported: bool
    bot_exported: bool
    exported_bot_count: int
    exported_files: tuple[str, ...]
    warnings: tuple[str, ...]


@dataclass(slots=True)
class RemoteDeployScriptResult:
    """远端部署脚本执行结果。"""

    remote_script_path: str
    script_result: RemoteCommandResult


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

    def export_and_upload_current_config(
        self,
        *,
        export_app_config: bool = True,
        export_bot_config: bool = True,
        remote_filename: str = "config-export.zip",
    ) -> RemoteConfigSyncResult:
        """导出当前本地配置并上传到远端。

        该方法负责衔接本地配置导出服务与远端上传流程，
        是 P1 MVP 阶段“本地配置 -> 远端配置包”闭环的第一步。
        """
        with tempfile.TemporaryDirectory(prefix="napcat-remote-export-") as temp_dir:
            scan_result = scan_current_config_export(Path(temp_dir))
            execution_result = apply_config_export(
                ExportExecutionPlan(
                    scan_result=scan_result,
                    export_app_config=export_app_config,
                    export_bot_config=export_bot_config,
                )
            )
            remote_archive_path = self.upload_config_archive(
                execution_result.archive_path,
                remote_filename=remote_filename,
            )

        return RemoteConfigSyncResult(
            remote_archive_path=remote_archive_path,
            archive_name=remote_filename,
            app_exported=execution_result.app_exported,
            bot_exported=execution_result.bot_exported,
            exported_bot_count=execution_result.exported_bot_count,
            exported_files=execution_result.exported_files,
            warnings=execution_result.warnings,
        )

    def upload_deploy_script(self, remote_filename: str = "deploy_napcat.sh") -> str:
        """上传远端部署脚本。"""
        script_content = build_linux_deploy_script(
            {
                "workspace_dir": self.paths.workspace_dir,
                "runtime_dir": self.paths.runtime_dir,
                "config_dir": self.paths.config_dir,
                "log_dir": self.paths.log_dir,
                "tmp_dir": self.paths.tmp_dir,
                "package_dir": self.paths.package_dir,
                "config_archive": PurePosixPath(self.paths.tmp_dir, "config-export.zip").as_posix(),
                "status_file": self.paths.status_file,
            }
        )

        with tempfile.TemporaryDirectory(prefix="napcat-remote-script-") as temp_dir:
            local_script_path = Path(temp_dir) / remote_filename
            local_script_path.write_text(script_content, encoding="utf-8")
            remote_script_path = PurePosixPath(self.paths.tmp_dir, remote_filename).as_posix()
            self.backend.ensure_directory(self.paths.tmp_dir)
            self.backend.upload_file(local_script_path, remote_script_path)

        self.backend.run(f'chmod +x "{remote_script_path}"', check=True)
        return remote_script_path

    def run_deploy_script(self, remote_script_path: str | None = None) -> RemoteDeployScriptResult:
        """执行远端部署脚本。"""
        script_path = remote_script_path or PurePosixPath(self.paths.tmp_dir, "deploy_napcat.sh").as_posix()
        result = self.backend.run(f'bash "{script_path}"', check=False)
        return RemoteDeployScriptResult(remote_script_path=script_path, script_result=result)

# -*- coding: utf-8 -*-
"""Linux Core 部署骨架。"""

from __future__ import annotations

import tempfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from src.desktop.core.config.config_export import ExportExecutionPlan, apply_config_export, scan_current_config_export
from src.desktop.core.logging import LogSource, LogType, logger

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
        logger.info(
            (
                "准备上传远端安装包: "
                f"local={local_file}, remote={remote_path}, size={local_file.stat().st_size}"
            ),
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        self.backend.ensure_directory(self.paths.package_dir)
        self.backend.upload_file(local_file, remote_path)
        logger.info(
            f"远端安装包上传完成: remote={remote_path}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        return remote_path

    def upload_config_archive(self, local_archive: str | Path, remote_filename: str = "config-export.zip") -> str:
        """上传配置包到远端临时目录。"""
        local_file = Path(local_archive)
        remote_path = PurePosixPath(self.paths.tmp_dir, remote_filename).as_posix()
        logger.info(
            (
                "准备上传远端配置包: "
                f"local={local_file}, remote={remote_path}, size={local_file.stat().st_size}"
            ),
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        self.backend.ensure_directory(self.paths.tmp_dir)
        self.backend.upload_file(local_archive, remote_path)
        logger.info(
            f"远端配置包上传完成: remote={remote_path}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
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
        logger.info(
            "开始导出并上传当前本地配置到远端",
            log_type=LogType.FILE_FUNC,
            log_source=LogSource.CORE,
        )
        with tempfile.TemporaryDirectory(prefix="napcat-remote-export-") as temp_dir:
            scan_result = scan_current_config_export(Path(temp_dir))
            execution_result = apply_config_export(
                ExportExecutionPlan(
                    scan_result=scan_result,
                    export_app_config=export_app_config,
                    export_bot_config=export_bot_config,
                )
            )
            logger.info(
                (
                    "本地配置导出完成: "
                    f"archive={execution_result.archive_path}, app_exported={execution_result.app_exported}, "
                    f"bot_exported={execution_result.bot_exported}, exported_bot_count={execution_result.exported_bot_count}, "
                    f"warnings={list(execution_result.warnings)}"
                ),
                log_type=LogType.FILE_FUNC,
                log_source=LogSource.CORE,
            )
            remote_archive_path = self.upload_config_archive(
                execution_result.archive_path,
                remote_filename=remote_filename,
            )

        logger.info(
            f"本地配置已上传到远端: remote_archive={remote_archive_path}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
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
                "pid_file": self.paths.pid_file,
                "log_file": self.paths.log_file,
                "install_base_dir": self.paths.install_base_dir,
                "qq_base_path": self.paths.qq_base_path,
                "target_folder": self.paths.target_folder,
                "qq_executable": self.paths.qq_executable,
                "qq_package_json_path": f"{self.paths.qq_base_path}/resources/app/package.json",
                "launcher_script": self.paths.launcher_script,
            }
        )

        with tempfile.TemporaryDirectory(prefix="napcat-remote-script-") as temp_dir:
            local_script_path = Path(temp_dir) / remote_filename
            local_script_path.write_text(script_content, encoding="utf-8", newline="\n")
            remote_script_path = PurePosixPath(self.paths.tmp_dir, remote_filename).as_posix()
            logger.info(
                (
                    "准备上传远端部署脚本: "
                    f"local={local_script_path}, remote={remote_script_path}, size={local_script_path.stat().st_size}"
                ),
                log_type=LogType.NETWORK,
                log_source=LogSource.CORE,
            )
            self.backend.ensure_directory(self.paths.tmp_dir)
            self.backend.upload_file(local_script_path, remote_script_path)

        self.backend.run(f'chmod +x "{remote_script_path}"', check=True)
        logger.info(
            f"远端部署脚本上传完成并已授予执行权限: remote={remote_script_path}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        return remote_script_path

    def run_deploy_script(self, remote_script_path: str | None = None) -> RemoteDeployScriptResult:
        """执行远端部署脚本。"""
        script_path = remote_script_path or PurePosixPath(self.paths.tmp_dir, "deploy_napcat.sh").as_posix()
        logger.info(
            f"准备执行远端部署脚本: remote={script_path}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        result = self.backend.run(f'bash "{script_path}"', check=False)
        logger.info(
            (
                "远端部署脚本执行完成: "
                f"remote={script_path}, exit_status={result.exit_status}, "
                f"stdout_len={len(result.stdout)}, stderr_len={len(result.stderr)}"
            ),
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        if result.stdout.strip():
            logger.info(
                f"远端部署脚本标准输出:\n{result.stdout}",
                log_type=LogType.NETWORK,
                log_source=LogSource.CORE,
            )
        if result.stderr.strip():
            logger.warning(
                f"远端部署脚本标准错误:\n{result.stderr}",
                log_type=LogType.NETWORK,
                log_source=LogSource.CORE,
            )
        return RemoteDeployScriptResult(remote_script_path=script_path, script_result=result)

    def clean_environment(self, include_qq: bool = True) -> RemoteCommandResult:
        """清理 NapCat 环境。

        Args:
            include_qq: 是否同时清理 QQ 安装

        Returns:
            命令执行结果
        """
        logger.info(
            f"开始清理 NapCat 环境: include_qq={include_qq}",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )

        # 1. 停止运行中的进程
        logger.info("停止运行中的 NapCat 进程", log_source=LogSource.CORE)
        self.backend.run('pkill -f "qq --no-sandbox" 2>/dev/null || true', check=False)

        # 2. 清理运行时目录
        logger.info("清理运行时目录", log_source=LogSource.CORE)
        self.backend.run(f'rm -rf "{self.paths.runtime_dir}" 2>/dev/null || true', check=False)
        self.backend.run(f'rm -rf "{self.paths.tmp_dir}" 2>/dev/null || true', check=False)

        # 3. 清理日志
        logger.info("清理日志文件", log_source=LogSource.CORE)
        self.backend.run(f'rm -f "{self.paths.log_dir}"/*.log 2>/dev/null || true', check=False)

        # 4. 清理 NapCat 安装
        logger.info("清理 NapCat 安装", log_source=LogSource.CORE)
        self.backend.run(f'rm -rf "{self.paths.napcat_dir}" 2>/dev/null || true', check=False)

        # 5. 清理 QQ 注入文件
        logger.info("清理 QQ 注入文件", log_source=LogSource.CORE)
        self.backend.run(f'rm -f "{self.paths.qq_base_path}/resources/app/loadNapCat.js" 2>/dev/null || true', check=False)

        # 6. 恢复 QQ 原始配置（从备份）
        backup_path = f"{self.paths.qq_package_json_path}.backup"
        result = self.backend.run(f'test -f "{backup_path}" && mv "{backup_path}" "{self.paths.qq_package_json_path}" 2>/dev/null || true', check=False)
        if result.ok:
            logger.info("已恢复 QQ 原始配置", log_source=LogSource.CORE)

        # 7. 清理启动脚本
        self.backend.run(f'rm -f "{self.paths.launcher_script}" 2>/dev/null || true', check=False)

        # 8. 可选：清理 QQ 安装
        if include_qq:
            logger.info("清理 QQ 安装", log_source=LogSource.CORE)
            self.backend.run(f'rm -rf "{self.paths.qq_base_path}" 2>/dev/null || true', check=False)
            # 清理下载的安装包
            self.backend.run(f'rm -f "{self.paths.package_dir}"/*.deb "{self.paths.package_dir}"/*.rpm 2>/dev/null || true', check=False)

        logger.info(
            "NapCat 环境清理完成",
            log_type=LogType.NETWORK,
            log_source=LogSource.CORE,
        )
        return RemoteCommandResult(
            command="clean_environment",
            exit_status=0,
            stdout="",
            stderr="",
        )

# -*- coding: utf-8 -*-
"""远程管理模块. 

注意: 
此处使用延迟导入, 避免 [`src.core.config`](src/core/config/__init__.py)
在仅引用 [`LinuxCorePaths`](src/core/remote/models.py) 或 [`SSHCredentials`](src/core/remote/models.py)
时触发整个远程子系统加载, 进而与 [`src.core.config.config_export`](src/core/config/config_export.py)
形成循环依赖. 
"""

from __future__ import annotations

from importlib import import_module
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .deployment import (
        InstallStepResult,
        LinuxCoreDeployment,
        LinuxCoreDeploymentProbe,
        RemoteConfigSyncResult,
        RemoteDeployScriptResult,
    )
    from .errors import (
        RemoteCommandError,
        RemoteDeploymentError,
        RemoteDeploymentInProgressError,
        RemoteError,
        SSHAuthenticationError,
        SSHConnectionError,
        SSHHostKeyError,
    )
    from .execution_backend import ExecutionBackend, LocalExecutionBackend, RemoteExecutionBackend
    from .models import LinuxCorePaths, RemoteCommandResult, SSHCredentials
    from .remote_manager import RemoteManager
    from .server_manager import DeploymentResult, ServerManager
    from .servers import DeploymentState, ServerProfile, ServerRegistry
    from .ssh_client import SSHClient
    from .status import RemoteLogTail, RemoteNapCatStatus, RemoteRuntimeService
    from .thread_pool import (
        dispatch_remote_ssh,
        remote_ssh_pool,
        reset_remote_ssh_pool,
        shutdown_remote_ssh_pool,
    )
    from .tunnel import LocalPortForwarder
    from .templates import (
        build_install_linuxqq_script,
        build_install_napcat_script,
        build_linux_deploy_script,
        build_napcat_launcher_script,
        load_linux_deploy_script,
    )


_EXPORT_MAP = {
    "DeploymentResult": ".server_manager",
    "DeploymentState": ".servers",
    "ExecutionBackend": ".execution_backend",
    "InstallStepResult": ".deployment",
    "LinuxCoreDeployment": ".deployment",
    "LinuxCoreDeploymentProbe": ".deployment",
    "LinuxCorePaths": ".models",
    "LocalExecutionBackend": ".execution_backend",
    "LocalPortForwarder": ".tunnel",
    "RemoteCommandError": ".errors",
    "RemoteCommandResult": ".models",
    "RemoteConfigSyncResult": ".deployment",
    "RemoteDeployScriptResult": ".deployment",
    "RemoteDeploymentError": ".errors",
    "RemoteDeploymentInProgressError": ".errors",
    "RemoteError": ".errors",
    "RemoteExecutionBackend": ".execution_backend",
    "RemoteLogTail": ".status",
    "RemoteManager": ".remote_manager",
    "RemoteNapCatStatus": ".status",
    "RemoteRuntimeService": ".status",
    "ServerManager": ".server_manager",
    "ServerProfile": ".servers",
    "ServerRegistry": ".servers",
    "SSHAuthenticationError": ".errors",
    "SSHClient": ".ssh_client",
    "SSHCredentials": ".models",
    "SSHConnectionError": ".errors",
    "SSHHostKeyError": ".errors",
    "dispatch_remote_ssh": ".thread_pool",
    "remote_ssh_pool": ".thread_pool",
    "reset_remote_ssh_pool": ".thread_pool",
    "shutdown_remote_ssh_pool": ".thread_pool",
    "build_install_linuxqq_script": ".templates",
    "build_install_napcat_script": ".templates",
    "build_linux_deploy_script": ".templates",
    "build_napcat_launcher_script": ".templates",
    "load_linux_deploy_script": ".templates",
}

__all__ = list(_EXPORT_MAP.keys())


def __getattr__(name: str) -> Any:
    module_name = _EXPORT_MAP.get(name)
    if module_name is None:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")

    module = import_module(module_name, __name__)
    value = getattr(module, name)
    globals()[name] = value
    return value

# -*- coding: utf-8 -*-
"""远程管理模块。

注意：
此处使用延迟导入，避免 [`src.core.config`](src/core/config/__init__.py)
在仅引用 [`LinuxCorePaths`](src/core/remote/models.py) 或 [`SSHCredentials`](src/core/remote/models.py)
时触发整个远程子系统加载，进而与 [`src.core.config.config_export`](src/core/config/config_export.py)
形成循环依赖。
"""

from __future__ import annotations

from importlib import import_module
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    # JSON-RPC 2.0 Agent 模式
    from .agent_backend import AgentBackend
    from .agent_client import AgentClient, AgentConnectionConfig
    from .daemon_config import DaemonConfigManager, DaemonConnection
    from .daemon_deployer import DaemonDeployer, DeployResult, ServerInfo
    from .jsonrpc_protocol import (
        ErrorCode,
        JsonRpcError,
        JsonRpcNotification,
        JsonRpcRequest,
        JsonRpcResponse,
        LogEntry,
        NapCatMethod,
        NapCatNotification,
        NapCatStatus,
        SystemInfo,
    )
    # SSH/Local 模式
    from .deployment import LinuxCoreDeployment, LinuxCoreDeploymentProbe, RemoteConfigSyncResult, RemoteDeployScriptResult
    from .errors import (
        RemoteCommandError,
        RemoteError,
        SSHAuthenticationError,
        SSHConnectionError,
        SSHHostKeyError,
    )
    from .execution_backend import ExecutionBackend, LocalExecutionBackend, RemoteExecutionBackend
    from .models import LinuxCorePaths, RemoteCommandResult, SSHCredentials
    from .remote_manager import RemoteManager
    from .ssh_client import SSHClient
    from .status import RemoteLogTail, RemoteNapCatStatus, RemoteRuntimeService
    from .templates import build_linux_deploy_script, load_linux_deploy_script


_EXPORT_MAP = {
    # JSON-RPC 2.0 Agent 模式
    "AgentBackend": ".agent_backend",
    "AgentClient": ".agent_client",
    "AgentConnectionConfig": ".agent_client",
    "DaemonConfigManager": ".daemon_config",
    "DaemonConnection": ".daemon_config",
    "DaemonDeployer": ".daemon_deployer",
    "DeployResult": ".daemon_deployer",
    "ServerInfo": ".daemon_deployer",
    "ErrorCode": ".jsonrpc_protocol",
    "JsonRpcError": ".jsonrpc_protocol",
    "JsonRpcNotification": ".jsonrpc_protocol",
    "JsonRpcRequest": ".jsonrpc_protocol",
    "JsonRpcResponse": ".jsonrpc_protocol",
    "LogEntry": ".jsonrpc_protocol",
    "NapCatMethod": ".jsonrpc_protocol",
    "NapCatNotification": ".jsonrpc_protocol",
    "NapCatStatus": ".jsonrpc_protocol",
    "SystemInfo": ".jsonrpc_protocol",
    # SSH/Local 模式
    "ExecutionBackend": ".execution_backend",
    "LinuxCoreDeployment": ".deployment",
    "LinuxCoreDeploymentProbe": ".deployment",
    "LinuxCorePaths": ".models",
    "LocalExecutionBackend": ".execution_backend",
    "RemoteCommandError": ".errors",
    "RemoteCommandResult": ".models",
    "RemoteConfigSyncResult": ".deployment",
    "RemoteDeployScriptResult": ".deployment",
    "RemoteError": ".errors",
    "RemoteExecutionBackend": ".execution_backend",
    "RemoteLogTail": ".status",
    "RemoteManager": ".remote_manager",
    "RemoteNapCatStatus": ".status",
    "RemoteRuntimeService": ".status",
    "SSHAuthenticationError": ".errors",
    "SSHClient": ".ssh_client",
    "SSHCredentials": ".models",
    "SSHConnectionError": ".errors",
    "SSHHostKeyError": ".errors",
    "build_linux_deploy_script": ".templates",
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

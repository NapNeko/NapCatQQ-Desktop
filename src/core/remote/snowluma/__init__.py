# -*- coding: utf-8 -*-
"""SnowLuma 远端管理子包.

按 ``docs/plans/2026-05-11-snowluma-remote-management-execution-plan.md`` 落地,
为 SnowLuma 后端提供与 NapCat (`src/core/remote/`) 同构但独立的远端能力:

- :class:`SnowLumaRemotePaths` (:mod:`.paths`) — 远端目录布局
- :func:`build_install_snowluma_script` / :func:`build_snowluma_daemon_launcher` /
  :func:`build_snowluma_bot_launcher` (:mod:`.templates`) — shell 脚本渲染器

本子包**不**修改 NapCat 路径任何代码; 仅依赖 :mod:`src.core.remote` 中的中性设施
(:class:`SSHClient` / :class:`ExecutionBackend` / :class:`LocalPortForwarder` /
:func:`dispatch_remote_ssh` 等). 与 NapCat 路径见同一份 SSH 资源池, 互不相犯.

延迟导入策略与父包 :mod:`src.core.remote` 一致, 避免在仅需路径常量时拉起完整脚本
模板与 Qt 资源加载链路.
"""

from __future__ import annotations

from importlib import import_module
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .daemon import (
        RemoteDaemonReadyInfo,
        RemoteDaemonStartFailed,
        RemoteDaemonStartTimeout,
        RemoteDaemonState,
        RemoteSnowLumaDaemon,
    )
    from .deployment import (
        SnowLumaDeployment,
        SnowLumaFrameworkNotBundledError,
        SnowLumaInstallStep,
        SnowLumaInstallStepResult,
    )
    from .launcher import SnowLumaLauncherCommands
    from .paths import SnowLumaRemotePaths
    from .status import (
        SnowLumaRemoteBotState,
        SnowLumaRemoteBotStatus,
        SnowLumaRemoteDaemonState,
        SnowLumaRemoteDaemonStatus,
        SnowLumaRemoteRuntimeService,
    )
    from .templates import (
        build_install_snowluma_script,
        build_snowluma_bot_launcher,
        build_snowluma_daemon_launcher,
    )
    from .tunnels import (
        SNOWLUMA_PREFERRED_NOVNC_LOCAL_PORT,
        SNOWLUMA_PREFERRED_WEBUI_LOCAL_PORT,
        SNOWLUMA_REMOTE_NOVNC_PORT,
        SNOWLUMA_REMOTE_WEBUI_PORT,
        SnowLumaTunnelBundle,
        SnowLumaTunnelEndpoint,
        SnowLumaTunnelError,
        SnowLumaTunnelManager,
    )
    from .vnc_launcher import (
        build_snowluma_novnc_url,
        open_snowluma_vnc,
        open_url_in_default_browser,
        read_remote_vnc_password,
    )


_EXPORT_MAP: dict[str, str] = {
    "SnowLumaRemotePaths": ".paths",
    "build_install_snowluma_script": ".templates",
    "build_snowluma_daemon_launcher": ".templates",
    "build_snowluma_bot_launcher": ".templates",
    "SnowLumaDeployment": ".deployment",
    "SnowLumaInstallStep": ".deployment",
    "SnowLumaInstallStepResult": ".deployment",
    "SnowLumaFrameworkNotBundledError": ".deployment",
    "RemoteSnowLumaDaemon": ".daemon",
    "RemoteDaemonState": ".daemon",
    "RemoteDaemonReadyInfo": ".daemon",
    "RemoteDaemonStartTimeout": ".daemon",
    "RemoteDaemonStartFailed": ".daemon",
    "SnowLumaLauncherCommands": ".launcher",
    "SnowLumaRemoteBotState": ".status",
    "SnowLumaRemoteBotStatus": ".status",
    "SnowLumaRemoteDaemonState": ".status",
    "SnowLumaRemoteDaemonStatus": ".status",
    "SnowLumaRemoteRuntimeService": ".status",
    "SnowLumaTunnelManager": ".tunnels",
    "SnowLumaTunnelBundle": ".tunnels",
    "SnowLumaTunnelEndpoint": ".tunnels",
    "SnowLumaTunnelError": ".tunnels",
    "SNOWLUMA_PREFERRED_WEBUI_LOCAL_PORT": ".tunnels",
    "SNOWLUMA_PREFERRED_NOVNC_LOCAL_PORT": ".tunnels",
    "SNOWLUMA_REMOTE_WEBUI_PORT": ".tunnels",
    "SNOWLUMA_REMOTE_NOVNC_PORT": ".tunnels",
    "build_snowluma_novnc_url": ".vnc_launcher",
    "read_remote_vnc_password": ".vnc_launcher",
    "open_url_in_default_browser": ".vnc_launcher",
    "open_snowluma_vnc": ".vnc_launcher",
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

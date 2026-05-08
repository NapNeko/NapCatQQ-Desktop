# -*- coding: utf-8 -*-
"""操作抽象层 (OperationBackend) . 

对应 [`docs/general/remote_ssh_plan.md`](../../../../docs/general/remote_ssh_plan.md) §2 设计的"操作抽象层", 
统一覆盖文件 / 进程 / 安装 / 日志 / WebUI 五大类操作. 

- [`LocalBackend`](src/core/operation/local_backend.py) 在 Windows 桌面环境实现这些操作. 
- [`RemoteBackend`](src/core/operation/remote_backend.py) 通过 SSH/SFTP 在 Linux 远端实现. 

上层 UI 与 Bot 管理代码通过 [`OperationBackend`](src/core/operation/backend.py)
与具体环境解耦, 从而做到 "本地 / 远程透明切换". 
"""

from __future__ import annotations

from importlib import import_module
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .backend import (
        FileEntry,
        InstallationInfo,
        OperationBackend,
        ProcessStatus,
        ProgressCallback,
        WebUIEndpoint,
    )
    from .batch_dispatcher import (
        BatchAction,
        BatchDispatcher,
        BatchOutcome,
        Executor,
    )
    from .local_backend import LocalBackend
    from .remote_backend import RemoteBackend
    from .resolver import (
        BackendResolutionError,
        reset_local_backend_singleton,
        resolve_backend_for_bot,
    )


_EXPORT_MAP = {
    "FileEntry": ".backend",
    "InstallationInfo": ".backend",
    "OperationBackend": ".backend",
    "ProcessStatus": ".backend",
    "ProgressCallback": ".backend",
    "WebUIEndpoint": ".backend",
    "BatchAction": ".batch_dispatcher",
    "BatchDispatcher": ".batch_dispatcher",
    "BatchOutcome": ".batch_dispatcher",
    "Executor": ".batch_dispatcher",
    "LocalBackend": ".local_backend",
    "RemoteBackend": ".remote_backend",
    "BackendResolutionError": ".resolver",
    "resolve_backend_for_bot": ".resolver",
    "reset_local_backend_singleton": ".resolver",
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

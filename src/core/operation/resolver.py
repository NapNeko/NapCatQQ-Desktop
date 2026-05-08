# -*- coding: utf-8 -*-
"""[`OperationBackend`](src/core/operation/backend.py) 解析层 (P2.2).

把 [`Config.bot.runtime_target`](src/core/config/config_model.py) 路由到
[`LocalBackend`](src/core/operation/local_backend.py) 或某台远端服务器
的 [`RemoteBackend`](src/core/operation/remote_backend.py) 实例.

设计要点:
- 该层是 backend 抽象与具体调用方之间的"路由网关", 上层(UI / Bot 管理 /
  进程管理器)只需获得一个 `OperationBackend`, 不直接依赖 `ServerManager`.
- 缺失服务器档案 / 远端连接异常时, **不静默降级到本地**: 远端 Bot 一旦绑定到
  不存在的 server_id, 误回退本地会导致用户看到"配置在本地"的假象, 进一步引发
  本地下次启动空 QQ 客户端等更严重的副作用. 这里改为抛
  [`BackendResolutionError`](src/core/operation/resolver.py),
  由 UI / 进程管理器决定如何向用户解释错误.
- LocalBackend 默认全局复用单例(无连接成本); RemoteBackend 由 ServerManager
  内部缓存按 server_id 复用 SSH 连接.

参考:
- [`docs/general/remote_ssh_plan.md`](../../../../docs/general/remote_ssh_plan.md) §4.3
- [`ServerManager.get_backend`](src/core/remote/server_manager.py)
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from src.core.config.config_model import RUNTIME_TARGET_LOCAL

from .backend import OperationBackend
from .local_backend import LocalBackend

if TYPE_CHECKING:
    from src.core.config.config_model import BotConfig, Config
    from src.core.remote.server_manager import ServerManager

    from .remote_backend import RemoteBackend


class BackendResolutionError(RuntimeError):
    """无法为指定 Bot 解析 [`OperationBackend`](src/core/operation/backend.py).

    触发场景:
    - ``runtime_target`` 引用的 server_id 在 [`ServerRegistry`](src/core/remote/servers.py)
      中不存在 (服务器被删除 / 配置文件被外部篡改).
    - ``runtime_target`` 引用的服务器尚未完成部署 (deployment_state != DEPLOYED)
      -- 该子类型可由调用方根据 ``stage`` 字段区分.
    """

    def __init__(self, message: str, *, stage: str = "unknown", target: str | None = None) -> None:
        super().__init__(message)
        self.stage = stage
        self.target = target


# ==================== LocalBackend 单例 ====================
# LocalBackend 不持有连接, 全局共享一个实例即可避免反复构造.
_LOCAL_BACKEND_SINGLETON: LocalBackend | None = None


def _get_local_backend() -> LocalBackend:
    """惰性创建 [`LocalBackend`](src/core/operation/local_backend.py) 单例."""
    global _LOCAL_BACKEND_SINGLETON
    if _LOCAL_BACKEND_SINGLETON is None:
        _LOCAL_BACKEND_SINGLETON = LocalBackend()
    return _LOCAL_BACKEND_SINGLETON


def reset_local_backend_singleton() -> None:
    """清空 LocalBackend 单例缓存. 仅供测试用.

    生产代码不应调用; 单例的 lifecycle 与进程一致.
    """
    global _LOCAL_BACKEND_SINGLETON
    _LOCAL_BACKEND_SINGLETON = None


# ==================== 解析入口 ====================
def resolve_backend_for_bot(
    config: "Config | BotConfig",
    *,
    server_manager: "ServerManager | None" = None,
) -> OperationBackend:
    """根据 ``config.bot.runtime_target`` 返回合适的 backend 实例.

    Args:
        config: 完整 [`Config`](src/core/config/config_model.py) 或单独的
            [`BotConfig`](src/core/config/config_model.py); 仅依赖 ``runtime_target``.
        server_manager: 可选 [`ServerManager`](src/core/remote/server_manager.py) 注入,
            缺省时通过 ``creart`` 单例获取. 测试场景应显式注入以避免 creart 副作用.

    Returns:
        - 本地 Bot: [`LocalBackend`](src/core/operation/local_backend.py) 单例
        - 远端 Bot: [`ServerManager.get_backend`](src/core/remote/server_manager.py)
          管理的 [`RemoteBackend`](src/core/operation/remote_backend.py) 实例

    Raises:
        BackendResolutionError: 远端 server_id 不存在或 ServerManager 不可用.
    """
    bot_config = _extract_bot_config(config)
    target = bot_config.runtime_target

    if target == RUNTIME_TARGET_LOCAL:
        return _get_local_backend()

    return _resolve_remote_backend(target, server_manager=server_manager)


def _extract_bot_config(config: "Config | BotConfig") -> "BotConfig":
    """从 ``Config`` 或 ``BotConfig`` 中提取 ``BotConfig``.

    避免硬依赖具体类型, 走 duck-typing 检测 ``bot`` 属性.
    """
    if hasattr(config, "bot"):
        return config.bot  # type: ignore[union-attr]
    return config  # type: ignore[return-value]


def _resolve_remote_backend(
    server_id: str,
    *,
    server_manager: "ServerManager | None",
) -> "RemoteBackend":
    """通过 [`ServerManager`](src/core/remote/server_manager.py) 获取远端 backend."""
    manager = server_manager or _get_server_manager_singleton()
    if manager is None:
        raise BackendResolutionError(
            f"无法解析远端运行位置: ServerManager 单例不可用 (server_id={server_id})",
            stage="server_manager_missing",
            target=server_id,
        )

    try:
        return manager.get_backend(server_id)
    except KeyError as exc:
        raise BackendResolutionError(
            f"无法解析远端运行位置: 服务器档案不存在 (server_id={server_id})",
            stage="server_not_found",
            target=server_id,
        ) from exc


def _get_server_manager_singleton() -> "ServerManager | None":
    """通过 ``creart.it`` 获取 [`ServerManager`](src/core/remote/server_manager.py) 单例.

    测试或最早期初始化阶段 creart 可能尚未就绪, 此时返回 None,
    由调用方处理(典型路径是抛 [`BackendResolutionError`](src/core/operation/resolver.py)).
    """
    try:
        from creart import it

        from src.core.remote.server_manager import ServerManager
    except ImportError:
        return None

    try:
        return it(ServerManager)
    except Exception:  # noqa: BLE001 - creart 内部异常签名不稳定, 统一吞掉
        return None


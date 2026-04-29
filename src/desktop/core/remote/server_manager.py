# -*- coding: utf-8 -*-
"""[`ServerManager`](src/desktop/core/remote/server_manager.py): 多服务器管理服务。

职责:
- 包装 [`ServerRegistry`](src/desktop/core/remote/servers.py) 的增删改查
- 内存中缓存活跃服务器的 SSH 密码(不持久化, 参考 §6.2)
- 惰性创建并复用 [`RemoteBackend`](src/desktop/core/operation/remote_backend.py) 实例
- Qt 信号通知 UI(server_added / server_updated / server_removed)
- 同步 ``test_connection`` 接口供后台线程调用; UI 应通过 [`QRunnable`](https://doc.qt.io/qt-6/qrunnable.html) 调度
- 启动时一次性迁移旧版单服务器配置 (``cfg.remote_*``) 到新版 ``servers.json``

存储路径默认: ``{PathFunc.config_dir_path}/servers.json``。
"""

from __future__ import annotations

from abc import ABC
from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtCore import QObject, Signal

from src.desktop.core.logging import LogSource, LogType, logger

from .errors import RemoteError
from .models import SSHCredentials
from .servers import DeploymentState, ServerProfile, ServerRegistry
from .ssh_client import SSHClient

if TYPE_CHECKING:
    from src.desktop.core.operation.remote_backend import RemoteBackend


def _default_storage_path() -> Path:
    """返回默认 ``servers.json`` 持久化路径。

    取自 [`PathFunc.config_dir_path`](src/desktop/core/runtime/paths.py),
    若 creart 单例不可用(例如纯单元测试场景), 回退到当前工作目录。
    """
    try:
        from creart import it

        from src.desktop.core.runtime.paths import PathFunc

        return Path(it(PathFunc).config_dir_path) / "servers.json"
    except Exception:  # noqa: BLE001 - 测试或最早期初始化时允许回退
        return Path.cwd() / "servers.json"


class ServerManager(QObject):
    """多服务器管理服务。

    Signals:
        server_added (str): 新增服务器, 参数为 ``server_id``
        server_updated (str): 服务器档案被更新
        server_removed (str): 服务器被删除
        server_state_changed (str, str): 部署状态变化, ``(server_id, new_state_value)``
    """

    server_added = Signal(str)
    server_updated = Signal(str)
    server_removed = Signal(str)
    server_state_changed = Signal(str, str)

    def __init__(self, storage_path: str | Path | None = None, parent: QObject | None = None) -> None:
        super().__init__(parent)
        path = Path(storage_path) if storage_path is not None else _default_storage_path()
        had_storage_file = path.exists()
        self._registry = ServerRegistry(path)
        # 服务器 ID -> 内存中的 SSH 密码(不落盘)
        self._password_cache: dict[str, str] = {}
        # 服务器 ID -> RemoteBackend 实例(惰性创建并缓存)
        self._backend_cache: dict[str, "RemoteBackend"] = {}

        # 仅在首次启动(磁盘上无 servers.json)时尝试迁移旧版单服务器配置
        if not had_storage_file:
            self._migrate_legacy_single_server_config()

    # ==================== 仓库直通 ====================
    @property
    def registry(self) -> ServerRegistry:
        return self._registry

    @property
    def storage_path(self) -> Path:
        return self._registry.storage_path

    def list_servers(self) -> list[ServerProfile]:
        return self._registry.list()

    def get_server(self, server_id: str) -> ServerProfile | None:
        return self._registry.get(server_id)

    # ==================== CRUD ====================
    def add_server(self, profile: ServerProfile, *, password: str | None = None) -> None:
        """添加新服务器档案; 若是密码认证, 调用方应同时传入 password 暂存到内存。"""
        self._registry.add(profile)
        if password and profile.credentials.auth_method == "password":
            self._password_cache[profile.id] = password
        logger.info(
            f"已新增服务器档案: id={profile.id}, name={profile.name}, host={profile.credentials.host}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        self.server_added.emit(profile.id)

    def update_server(self, profile: ServerProfile, *, password: str | None = None) -> None:
        """覆盖现有档案, 同时失效 backend 缓存; 密码可选更新。"""
        self._registry.update(profile)
        self._backend_cache.pop(profile.id, None)
        if password is not None:
            if password and profile.credentials.auth_method == "password":
                self._password_cache[profile.id] = password
            else:
                self._password_cache.pop(profile.id, None)
        logger.info(
            f"已更新服务器档案: id={profile.id}, name={profile.name}, host={profile.credentials.host}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        self.server_updated.emit(profile.id)

    def remove_server(self, server_id: str) -> bool:
        """删除档案与所有关联缓存。"""
        ok = self._registry.remove(server_id)
        if ok:
            self._password_cache.pop(server_id, None)
            backend = self._backend_cache.pop(server_id, None)
            if backend is not None:
                try:
                    backend.close()
                except Exception:  # noqa: BLE001
                    pass
            logger.info(f"已删除服务器档案: id={server_id}", LogType.NETWORK, LogSource.CORE)
            self.server_removed.emit(server_id)
        return ok

    def set_deployment_state(self, server_id: str, state: DeploymentState) -> None:
        """更新指定服务器的部署状态并持久化。"""
        profile = self._registry.get(server_id)
        if profile is None:
            return
        profile.deployment_state = state
        self._registry.update(profile)
        self.server_state_changed.emit(server_id, state.value)

    # ==================== 凭据辅助 ====================
    def set_password(self, server_id: str, password: str | None) -> None:
        """设置或清除内存中的密码。"""
        if password:
            self._password_cache[server_id] = password
        else:
            self._password_cache.pop(server_id, None)

    def has_password(self, server_id: str) -> bool:
        return server_id in self._password_cache

    def get_runtime_credentials(self, server_id: str) -> SSHCredentials | None:
        """返回注入了内存密码的运行期凭据副本; 服务器不存在时返回 None。"""
        profile = self._registry.get(server_id)
        if profile is None:
            return None
        return self._inject_runtime_password(profile)

    # ==================== Backend 工厂 ====================
    def get_backend(self, server_id: str) -> "RemoteBackend":
        """获取或惰性创建 ``RemoteBackend`` 实例; 服务器不存在时抛 KeyError。"""
        cached = self._backend_cache.get(server_id)
        if cached is not None:
            return cached

        profile = self._registry.get(server_id)
        if profile is None:
            raise KeyError(f"服务器档案不存在: {server_id}")

        # 延迟导入避免与 [`core.operation`](src/desktop/core/operation/__init__.py) 之间的循环
        from src.desktop.core.operation.remote_backend import RemoteBackend

        cred = self._inject_runtime_password(profile)
        backend = RemoteBackend(cred, profile.paths)
        self._backend_cache[server_id] = backend
        return backend

    # ==================== 连接测试 ====================
    def test_connection(
        self,
        profile: ServerProfile,
        *,
        password: str | None = None,
    ) -> tuple[bool, str]:
        """**同步**测试 SSH 连接(应在后台线程调用)。

        Args:
            profile: 服务器档案; 不会修改其内部凭据
            password: 临时密码; 若 None 且为密码认证, 则尝试 ``password_cache``

        Returns:
            (成功标志, 用户可读消息)
        """
        cred = profile.credentials
        if cred.auth_method == "password":
            effective_password = password or self._password_cache.get(profile.id)
            if not effective_password:
                return False, "密码认证模式下必须提供密码"
            cred = replace(cred, password=effective_password)

        client = SSHClient(cred)
        try:
            client.connect()
            try:
                result = client.run('echo "ok"')
            finally:
                client.close()
        except RemoteError as exc:
            return False, str(exc)
        except Exception as exc:  # noqa: BLE001 - 测试场景需要给用户清晰反馈
            return False, f"连接失败: {exc}"

        if not result.ok:
            return False, f"远端命令返回非零状态: {result.stderr or result.stdout}"
        return True, "SSH 连接测试成功"

    # ==================== 资源清理 ====================
    def shutdown(self) -> None:
        """关闭所有缓存的 backend 连接, 应用退出前调用。"""
        for server_id, backend in list(self._backend_cache.items()):
            try:
                backend.close()
            except Exception:  # noqa: BLE001
                logger.warning(f"关闭 RemoteBackend 失败: id={server_id}", LogType.NETWORK, LogSource.CORE)
        self._backend_cache.clear()

    # ==================== 内部 ====================
    def _inject_runtime_password(self, profile: ServerProfile) -> SSHCredentials:
        """对密码认证模式, 从内存缓存注入密码生成凭据副本; 否则原样返回。"""
        cred = profile.credentials
        if cred.auth_method != "password":
            return cred
        password = self._password_cache.get(profile.id)
        if not password:
            return cred  # 由调用方在 SSHClient.connect() 时报 SSHAuthenticationError
        return replace(cred, password=password)

    def _migrate_legacy_single_server_config(self) -> None:
        """从 ``cfg.remote_*`` 单服务器配置迁移到 ``servers.json``。

        触发条件: 磁盘上原本不存在 ``servers.json``。
        若 ``cfg.remote_host`` 为空, 视为干净安装, 直接跳过。
        """
        try:
            from src.desktop.core.config import cfg
        except ImportError:
            return

        try:
            credentials = cfg.build_ssh_credentials()
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"读取旧版 cfg.remote_* 配置失败, 跳过迁移: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return

        if not credentials.host.strip():
            return  # 用户从未配置过远程服务器, 无需迁移

        try:
            paths = cfg.build_linux_core_paths()
        except Exception:  # noqa: BLE001
            from .models import LinuxCorePaths

            paths = LinuxCorePaths()

        profile = ServerProfile.create(
            name=f"已迁移服务器 ({credentials.host})",
            credentials=credentials,
            notes="从旧版 v1 单服务器配置自动迁移; 如使用密码认证, 请重新输入密码",
            paths=paths,
        )
        try:
            self._registry.add(profile)
        except ValueError:
            return  # 极端情况下 UUID 冲突, 忽略
        logger.info(
            (
                "已从旧版单服务器配置迁移到 servers.json: "
                f"id={profile.id}, host={credentials.host}, auth={credentials.auth_method}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )


class ServerManagerCreator(AbstractCreator, ABC):
    """``ServerManager`` 单例创建器, 与现有 creart 风格保持一致。"""

    targets = (
        CreateTargetInfo(
            module="src.desktop.core.remote.server_manager",
            identify="ServerManager",
            humanized_name="服务器管理器",
            description="多服务器档案管理与持久化",
        ),
    )

    @staticmethod
    def available() -> bool:
        return exists_module("src.desktop.core.remote.server_manager")

    @staticmethod
    def create(create_type):
        return create_type()


add_creator(ServerManagerCreator)

# -*- coding: utf-8 -*-
"""NapCat Daemon 连接配置管理。

自动管理 Daemon 连接配置，包括：
- 配置保存和加载
- Token 安全存储
- 自动配置迁移
"""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass, field, asdict
from pathlib import Path
from typing import Any

try:
    import keyring
    KEYRING_AVAILABLE = True
except ImportError:
    KEYRING_AVAILABLE = False

from .models import SSHCredentials

logger = logging.getLogger(__name__)


@dataclass(slots=True)
class DaemonConnection:
    """Daemon 连接配置。"""

    # 连接信息
    name: str
    host: str
    port: int = 8443
    use_ssl: bool = True

    # 认证信息（Token 由 keyring 管理）
    token_reference: str = ""  # keyring 中的标识

    # 可选的 SSH 信息（用于自动部署）
    ssh_credentials: SSHCredentials | None = None
    ssh_config_id: str = ""  # 关联的 SSH 配置 ID

    # 高级选项
    auto_reconnect: bool = True
    reconnect_interval: float = 5.0
    request_timeout: float = 30.0
    heartbeat_interval: float = 30.0

    # 元数据
    created_at: str = ""
    last_connected: str = ""
    connection_count: int = 0
    is_favorite: bool = False

    def to_dict(self) -> dict[str, Any]:
        """转换为字典（Token 不保存）。"""
        data = {
            "name": self.name,
            "host": self.host,
            "port": self.port,
            "use_ssl": self.use_ssl,
            "token_reference": self.token_reference,
            "ssh_config_id": self.ssh_config_id,
            "auto_reconnect": self.auto_reconnect,
            "reconnect_interval": self.reconnect_interval,
            "request_timeout": self.request_timeout,
            "heartbeat_interval": self.heartbeat_interval,
            "created_at": self.created_at,
            "last_connected": self.last_connected,
            "connection_count": self.connection_count,
            "is_favorite": self.is_favorite,
        }
        return data

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> DaemonConnection:
        """从字典创建。"""
        return cls(
            name=data.get("name", ""),
            host=data.get("host", ""),
            port=data.get("port", 8443),
            use_ssl=data.get("use_ssl", True),
            token_reference=data.get("token_reference", ""),
            ssh_config_id=data.get("ssh_config_id", ""),
            auto_reconnect=data.get("auto_reconnect", True),
            reconnect_interval=data.get("reconnect_interval", 5.0),
            request_timeout=data.get("request_timeout", 30.0),
            heartbeat_interval=data.get("heartbeat_interval", 30.0),
            created_at=data.get("created_at", ""),
            last_connected=data.get("last_connected", ""),
            connection_count=data.get("connection_count", 0),
            is_favorite=data.get("is_favorite", False),
        )

    @property
    def display_name(self) -> str:
        """显示名称。"""
        if self.name:
            return f"{self.name} ({self.host}:{self.port})"
        return f"{self.host}:{self.port}"

    @property
    def ws_url(self) -> str:
        """WebSocket URL。"""
        protocol = "wss" if self.use_ssl else "ws"
        return f"{protocol}://{self.host}:{self.port}/ws"

    @property
    def http_url(self) -> str:
        """HTTP URL。"""
        protocol = "https" if self.use_ssl else "http"
        return f"{protocol}://{self.host}:{self.port}"


class DaemonConfigManager:
    """Daemon 配置管理器。"""

    CONFIG_FILE = "daemon_connections.json"
    KEYRING_SERVICE = "napcat-desktop-daemon"

    def __init__(self, config_dir: Path | None = None):
        """初始化。

        Args:
            config_dir: 配置目录，None 使用默认
        """
        if config_dir is None:
            from src.desktop.core.platform.app_paths import resolve_app_data_path
            self._config_dir = resolve_app_data_path() / "config"
        else:
            self._config_dir = Path(config_dir)

        self._config_file = self._config_dir / self.CONFIG_FILE
        self._connections: dict[str, DaemonConnection] = {}
        self._load()

    def _load(self) -> None:
        """加载配置。"""
        if not self._config_file.exists():
            logger.info("Daemon 配置文件不存在，创建新配置")
            return

        try:
            data = json.loads(self._config_file.read_text(encoding="utf-8"))
            for conn_id, conn_data in data.get("connections", {}).items():
                self._connections[conn_id] = DaemonConnection.from_dict(conn_data)
            logger.info(f"加载了 {len(self._connections)} 个 Daemon 配置")
        except Exception as e:
            logger.error(f"加载 Daemon 配置失败: {e}")

    def _save(self) -> None:
        """保存配置。"""
        try:
            self._config_dir.mkdir(parents=True, exist_ok=True)

            data = {
                "version": 1,
                "connections": {
                    conn_id: conn.to_dict()
                    for conn_id, conn in self._connections.items()
                },
            }

            self._config_file.write_text(
                json.dumps(data, indent=2, ensure_ascii=False),
                encoding="utf-8",
            )
        except Exception as e:
            logger.error(f"保存 Daemon 配置失败: {e}")
            raise

    def add_connection(
        self,
        connection: DaemonConnection,
        token: str | None = None,
    ) -> str:
        """添加连接配置。

        Args:
            connection: 连接配置
            token: Token，会存储到 keyring

        Returns:
            str: 配置 ID
        """
        import uuid
        from datetime import datetime

        conn_id = str(uuid.uuid4())[:8]

        if not connection.created_at:
            connection.created_at = datetime.now().isoformat()

        # 保存 Token 到 keyring
        if token:
            if not self._save_token(conn_id, token):
                logger.warning("无法保存 Token 到 keyring，将只保存在内存中")
            connection.token_reference = conn_id

        self._connections[conn_id] = connection
        self._save()

        logger.info(f"添加 Daemon 配置: {connection.display_name}")
        return conn_id

    def update_connection(
        self,
        conn_id: str,
        connection: DaemonConnection,
        token: str | None = None,
    ) -> bool:
        """更新连接配置。

        Args:
            conn_id: 配置 ID
            connection: 新的连接配置
            token: 新的 Token

        Returns:
            bool: 是否成功
        """
        if conn_id not in self._connections:
            return False

        if token:
            self._save_token(conn_id, token)
            connection.token_reference = conn_id

        self._connections[conn_id] = connection
        self._save()
        return True

    def remove_connection(self, conn_id: str) -> bool:
        """删除连接配置。

        Args:
            conn_id: 配置 ID

        Returns:
            bool: 是否成功
        """
        if conn_id not in self._connections:
            return False

        # 删除 Token
        self._delete_token(conn_id)

        del self._connections[conn_id]
        self._save()
        return True

    def get_connection(self, conn_id: str) -> DaemonConnection | None:
        """获取连接配置。

        Args:
            conn_id: 配置 ID

        Returns:
            DaemonConnection | None: 连接配置
        """
        return self._connections.get(conn_id)

    def get_token(self, conn_id: str) -> str | None:
        """获取 Token。

        Args:
            conn_id: 配置 ID

        Returns:
            str | None: Token
        """
        if not KEYRING_AVAILABLE:
            logger.warning("keyring 模块不可用，无法获取 Token")
            return None

        try:
            return keyring.get_password(self.KEYRING_SERVICE, conn_id)
        except Exception as e:
            logger.error(f"获取 Token 失败: {e}")
            return None

    def _save_token(self, conn_id: str, token: str) -> bool:
        """保存 Token 到 keyring。"""
        if not KEYRING_AVAILABLE:
            return False

        try:
            keyring.set_password(self.KEYRING_SERVICE, conn_id, token)
            return True
        except Exception as e:
            logger.error(f"保存 Token 失败: {e}")
            return False

    def _delete_token(self, conn_id: str) -> bool:
        """删除 Token。"""
        if not KEYRING_AVAILABLE:
            return False

        try:
            keyring.delete_password(self.KEYRING_SERVICE, conn_id)
            return True
        except Exception:
            return False

    def list_connections(
        self,
        favorites_only: bool = False,
    ) -> list[tuple[str, DaemonConnection]]:
        """列出所有连接配置。

        Args:
            favorites_only: 仅显示收藏的

        Returns:
            list[tuple[str, DaemonConnection]]: 配置列表
        """
        result = []
        for conn_id, conn in self._connections.items():
            if favorites_only and not conn.is_favorite:
                continue
            result.append((conn_id, conn))

        # 按收藏和名称排序
        result.sort(key=lambda x: (not x[1].is_favorite, x[1].name))
        return result

    def record_connection(self, conn_id: str) -> None:
        """记录连接成功。

        Args:
            conn_id: 配置 ID
        """
        from datetime import datetime

        if conn := self._connections.get(conn_id):
            conn.last_connected = datetime.now().isoformat()
            conn.connection_count += 1
            self._save()

    def get_default_connection(self) -> tuple[str, DaemonConnection] | None:
        """获取默认连接。

        Returns:
            tuple[str, DaemonConnection] | None: 默认配置
        """
        # 优先返回收藏的、最近连接的
        connections = self.list_connections()
        if not connections:
            return None

        # 查找最近连接的收藏配置
        favorites = [c for c in connections if c[1].is_favorite]
        if favorites:
            favorites.sort(key=lambda x: x[1].last_connected or "", reverse=True)
            return favorites[0]

        # 返回最近连接的
        connections.sort(key=lambda x: x[1].last_connected or "", reverse=True)
        return connections[0]

    def import_from_deploy(
        self,
        host: str,
        port: int,
        token: str,
        name: str | None = None,
        ssh_config_id: str = "",
    ) -> str:
        """从部署结果导入配置。

        Args:
            host: 主机地址
            port: 端口
            token: Token
            name: 显示名称
            ssh_config_id: 关联的 SSH 配置 ID

        Returns:
            str: 配置 ID
        """
        if not name:
            name = f"Daemon@{host}"

        connection = DaemonConnection(
            name=name,
            host=host,
            port=port,
            use_ssl=True,  # 默认使用 SSL
            ssh_config_id=ssh_config_id,
        )

        return self.add_connection(connection, token)

    def export_config(self, conn_id: str, include_token: bool = False) -> dict[str, Any] | None:
        """导出配置（用于分享/备份）。

        Args:
            conn_id: 配置 ID
            include_token: 是否包含 Token（不安全）

        Returns:
            dict | None: 配置字典
        """
        if conn := self._connections.get(conn_id):
            data = conn.to_dict()
            if include_token:
                data["token"] = self.get_token(conn_id)
            return data
        return None

    def test_connection(self, conn_id: str) -> tuple[bool, str]:
        """测试连接配置。

        Args:
            conn_id: 配置 ID

        Returns:
            tuple[bool, str]: (是否成功, 消息)
        """
        from .agent_client import AgentClient, AgentConnectionConfig

        conn = self._connections.get(conn_id)
        if not conn:
            return False, "配置不存在"

        token = self.get_token(conn_id)
        if not token:
            return False, "无法获取 Token"

        config = AgentConnectionConfig(
            host=conn.host,
            port=conn.port,
            token=token,
            use_ssl=conn.use_ssl,
        )

        client = AgentClient()
        result = [False, "连接超时"]

        def on_connected():
            result[0] = True
            result[1] = "连接成功"
            client.disconnect_from_agent()

        def on_error(msg: str):
            result[0] = False
            result[1] = f"连接错误: {msg}"

        client.connected.connect(on_connected)
        client.error.connect(on_error)

        client.connect_to_agent(config)

        # 等待结果
        import time
        for _ in range(30):  # 最多等待3秒
            if result[0] or result[1] != "连接超时":
                break
            time.sleep(0.1)

        client.disconnect_from_agent()
        return result[0], result[1]

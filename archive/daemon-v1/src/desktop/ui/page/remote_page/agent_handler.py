# -*- coding: utf-8 -*-
"""Agent/Daemon 连接处理器。

实现 BaseConnectionHandler 接口，通过 WebSocket + JSON-RPC 管理远程 NapCat。
"""
from __future__ import annotations

from typing import TYPE_CHECKING, Callable

from PySide6.QtCore import QObject

from .connection_base import BaseConnectionHandler, ConnectionInfo, ConnectionState, ServerStatus
from src.desktop.core.remote import (
    AgentClient,
    AgentConnectionConfig,
    DaemonConfigManager,
    NapCatStatus,
)

if TYPE_CHECKING:
    pass


class AgentHandler(BaseConnectionHandler):
    """Agent/Daemon 连接处理器。"""

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._client: AgentClient | None = None
        self._config: AgentConnectionConfig | None = None
        self._config_id: str = ""
        self._config_manager = DaemonConfigManager()

    def connect(self, info: ConnectionInfo, **kwargs) -> None:
        """建立 Agent 连接。

        Args:
            info: 连接信息
            **kwargs: 必须包含 token: str
        """
        self._set_state(ConnectionState.CONNECTING)

        try:
            token = kwargs.get("token")
            if not token:
                raise ValueError("需要提供 Token")

            self._info = info
            self._config = AgentConnectionConfig(
                host=info.host,
                port=info.port,
                token=token,
                use_ssl=True,
                auto_reconnect=True,
            )

            # 创建客户端
            self._client = AgentClient(self)
            self.setup_client_signals()

            # 连接
            self._client.connect_to_agent(self._config)

        except Exception as e:
            self._set_state(ConnectionState.ERROR)
            self.error_occurred.emit(f"连接失败: {e}")

    def disconnect(self) -> None:
        """断开 Agent 连接。"""
        if self._client:
            self._client.disconnect_from_agent()
            self._client = None

        self._set_state(ConnectionState.DISCONNECTED)
        self._update_status(connected=False)

    def test_connection(self) -> tuple[bool, str]:
        """测试 Agent 连接。"""
        if not self._config:
            return False, "未配置连接信息"

        try:
            import asyncio

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
            client.authenticated.connect(lambda: on_connected())

            client.connect_to_agent(self._config)

            # 等待结果
            import time
            for _ in range(30):
                if result[0] or "错误" in result[1]:
                    break
                time.sleep(0.1)

            client.disconnect_from_agent()
            return result[0], result[1]

        except Exception as e:
            return False, f"测试失败: {e}"

    def start_napcat(self) -> tuple[bool, str]:
        """启动 NapCat。"""
        if not self._client or not self.is_connected:
            return False, "未连接"

        try:
            import asyncio
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            result = loop.run_until_complete(self._client.start_napcat())
            loop.close()
            self._update_server_status()
            return True, f"NapCat 已启动 (PID: {result.get('pid', 'N/A')})"
        except Exception as e:
            return False, f"启动失败: {e}"

    def stop_napcat(self) -> tuple[bool, str]:
        """停止 NapCat。"""
        if not self._client or not self.is_connected:
            return False, "未连接"

        try:
            import asyncio
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            loop.run_until_complete(self._client.stop_napcat())
            loop.close()
            self._update_server_status()
            return True, "NapCat 已停止"
        except Exception as e:
            return False, f"停止失败: {e}"

    def restart_napcat(self) -> tuple[bool, str]:
        """重启 NapCat。"""
        if not self._client or not self.is_connected:
            return False, "未连接"

        try:
            import asyncio
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            result = loop.run_until_complete(self._client.restart_napcat())
            loop.close()
            self._update_server_status()
            return True, f"NapCat 已重启 (PID: {result.get('pid', 'N/A')})"
        except Exception as e:
            return False, f"重启失败: {e}"

    def get_logs(self, lines: int = 100) -> list[str]:
        """获取日志。"""
        if not self._client or not self.is_connected:
            return []

        try:
            import asyncio
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            logs = loop.run_until_complete(self._client.get_logs())
            loop.close()
            return logs[-lines:] if logs else []
        except Exception:
            return []

    def deploy(
        self, progress_callback: Callable[[str, int], None] | None = None
    ) -> tuple[bool, str]:
        """部署 NapCat (Agent 模式不需要部署，返回提示)。"""
        return True, "Agent 模式无需部署，Daemon 已运行"

    def save_config(self) -> None:
        """保存 Agent 配置。"""
        if self._info and self._config:
            # Token 通过 config_manager 保存
            pass

    def setup_client_signals(self) -> None:
        """设置客户端信号连接。"""
        if not self._client:
            return

        self._client.connected.connect(self._on_client_connected)
        self._client.disconnected.connect(self._on_client_disconnected)
        self._client.authenticated.connect(self._on_client_authenticated)
        self._client.error.connect(self._on_client_error)
        self._client.status_changed.connect(self._on_status_changed)
        self._client.log_received.connect(self._on_log_received)

    def _on_client_connected(self) -> None:
        """客户端连接成功。"""
        self._set_state(ConnectionState.AUTHENTICATING)

    def _on_client_disconnected(self) -> None:
        """客户端断开连接。"""
        if self._state != ConnectionState.DISCONNECTED:
            self._set_state(ConnectionState.DISCONNECTED)
            self._update_status(connected=False)

    def _on_client_authenticated(self) -> None:
        """客户端认证成功。"""
        self._set_state(ConnectionState.CONNECTED)
        self._update_status(connected=True)
        self._update_server_status()

        # 订阅日志
        if self._client:
            import asyncio

            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            try:
                loop.run_until_complete(self._client.subscribe_logs())
            except Exception:
                pass
            finally:
                loop.close()

    def _on_client_error(self, msg: str) -> None:
        """客户端错误。"""
        if self._state != ConnectionState.DISCONNECTED:
            self._set_state(ConnectionState.ERROR)
            self.error_occurred.emit(msg)

    def _on_status_changed(self, status: NapCatStatus) -> None:
        """状态变化。"""
        self._update_status(
            napcat_running=status.running,
            napcat_pid=status.pid,
            napcat_version=status.version,
            qq_number=status.qq,
        )

    def _on_log_received(self, log_entry) -> None:
        """收到日志。"""
        self.log_received.emit(f"[{log_entry.level}] {log_entry.message}")

    def _update_server_status(self) -> None:
        """更新服务器状态。"""
        if not self._client or not self.is_connected:
            return

        try:
            import asyncio

            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)

            # 获取状态
            status = loop.run_until_complete(self._client.get_status())
            self._update_status(
                napcat_running=status.running,
                napcat_pid=status.pid,
                napcat_version=status.version,
                qq_number=status.qq,
            )

            # 获取系统信息
            try:
                sys_info = loop.run_until_complete(self._client.get_system_info())
                info_data = sys_info.get("info", {})
                self._update_status(
                    os_name=info_data.get("os", "Linux"),
                    architecture=info_data.get("arch", "unknown"),
                    daemon_version=info_data.get("version", "unknown"),
                )
            except Exception:
                pass

            loop.close()

        except Exception:
            pass

    def get_config(self) -> AgentConnectionConfig | None:
        """获取当前配置。"""
        return self._config

    def save_connection_config(
        self, name: str, ssh_config_id: str = ""
    ) -> str | None:
        """保存连接配置。

        Returns:
            配置 ID
        """
        if not self._info or not self._config:
            return None

        # 获取 token
        token = self._config_manager.get_token(ssh_config_id)
        if not token:
            return None

        from src.desktop.core.remote import DaemonConnection

        conn = DaemonConnection(
            name=name,
            host=self._info.host,
            port=self._info.port,
            use_ssl=self._config.use_ssl,
            ssh_config_id=ssh_config_id,
        )

        return self._config_manager.add_connection(conn, token)

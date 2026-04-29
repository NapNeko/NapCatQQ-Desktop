# -*- coding: utf-8 -*-
"""SSH 连接处理器。

实现 BaseConnectionHandler 接口，通过 SSH 管理远程 NapCat。
"""
from __future__ import annotations

from typing import TYPE_CHECKING, Callable

from PySide6.QtCore import QRunnable, QThreadPool

from .connection_base import BaseConnectionHandler, ConnectionInfo, ConnectionState
from src.desktop.core.config import cfg
from src.desktop.core.remote import RemoteManager, SSHCredentials

if TYPE_CHECKING:
    from PySide6.QtCore import QObject


class SSHHandler(BaseConnectionHandler):
    """SSH 连接处理器。"""

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._credentials: SSHCredentials | None = None
        self._manager: RemoteManager | None = None
        self._config_id: str = ""

    def connect(self, info: ConnectionInfo, **kwargs) -> None:
        """建立 SSH 连接。

        Args:
            info: 连接信息
            **kwargs: 必须包含 credentials: SSHCredentials
        """
        self._set_state(ConnectionState.CONNECTING)

        try:
            credentials = kwargs.get("credentials")
            if not credentials:
                raise ValueError("需要提供 SSHCredentials")

            self._credentials = credentials
            self._info = info

            # 验证连接
            self._manager = RemoteManager(credentials)
            if not self._manager.connect():
                raise ConnectionError("SSH 连接失败")

            # 获取状态
            self._update_server_status()

            self._set_state(ConnectionState.CONNECTED)

        except Exception as e:
            self._set_state(ConnectionState.ERROR)
            self.error_occurred.emit(f"连接失败: {e}")
            if self._manager:
                self._manager.close()
                self._manager = None

    def disconnect(self) -> None:
        """断开 SSH 连接。"""
        if self._manager:
            self._manager.close()
            self._manager = None

        self._set_state(ConnectionState.DISCONNECTED)
        self._update_status(connected=False)

    def test_connection(self) -> tuple[bool, str]:
        """测试 SSH 连接。"""
        if not self._credentials:
            return False, "未配置连接信息"

        try:
            with RemoteManager(self._credentials) as m:
                probe = m.probe_environment()
                self._update_status(
                    os_name=probe.os_name,
                    architecture=probe.architecture,
                )
                return True, f"连接成功: {probe.os_name} {probe.architecture}"
        except Exception as e:
            return False, f"连接失败: {e}"

    def start_napcat(self) -> tuple[bool, str]:
        """启动 NapCat。"""
        if not self._manager or not self.is_connected:
            return False, "未连接"

        try:
            self._manager.start('cd "$HOME/Napcat" && ./napcat.sh start')
            self._update_server_status()
            return True, "NapCat 已启动"
        except Exception as e:
            return False, f"启动失败: {e}"

    def stop_napcat(self) -> tuple[bool, str]:
        """停止 NapCat。"""
        if not self._manager or not self.is_connected:
            return False, "未连接"

        try:
            self._manager.stop()
            self._update_server_status()
            return True, "NapCat 已停止"
        except Exception as e:
            return False, f"停止失败: {e}"

    def restart_napcat(self) -> tuple[bool, str]:
        """重启 NapCat。"""
        if not self._manager or not self.is_connected:
            return False, "未连接"

        try:
            self._manager.stop()
            self._manager.start('cd "$HOME/Napcat" && ./napcat.sh restart')
            self._update_server_status()
            return True, "NapCat 已重启"
        except Exception as e:
            return False, f"重启失败: {e}"

    def get_logs(self, lines: int = 100) -> list[str]:
        """获取日志。"""
        if not self._manager or not self.is_connected:
            return []

        try:
            tail = self._manager.tail_log(lines=lines)
            return tail.content.splitlines() if tail.content else []
        except Exception:
            return []

    def deploy(self, progress_callback: Callable[[str, int], None] | None = None) -> tuple[bool, str]:
        """部署 NapCat。"""
        if not self._credentials:
            return False, "未配置连接信息"

        def report(msg: str, pct: int):
            self.progress_updated.emit(msg, pct)
            if progress_callback:
                progress_callback(msg, pct)

        try:
            report("开始部署...", 10)

            with RemoteManager(self._credentials) as m:
                # 同步配置
                report("同步配置...", 30)
                sync_result = m.export_and_upload_current_config()

                # 上传部署脚本
                report("上传部署脚本...", 50)
                script_path = m.deployment.upload_deploy_script()

                # 执行部署
                report("执行部署...", 70)
                deploy_result = m.deployment.run_deploy_script(script_path)

                if deploy_result.script_result.ok:
                    report("部署完成", 100)
                    self._update_server_status()
                    return True, f"部署完成，已同步 {sync_result.exported_bot_count} 个配置"
                else:
                    return False, f"部署失败: {deploy_result.script_result.stderr}"

        except Exception as e:
            return False, f"部署异常: {e}"

    def save_config(self) -> None:
        """保存 SSH 配置。"""
        if self._credentials:
            cfg.set(cfg.remote_host, self._credentials.host)
            cfg.set(cfg.remote_port, self._credentials.port)
            cfg.set(cfg.remote_username, self._credentials.username)
            cfg.set(cfg.remote_auth_method, self._credentials.auth_method)
            cfg.set(cfg.remote_private_key_path, self._credentials.private_key_path)
            cfg.set(cfg.remote_connect_timeout, self._credentials.connect_timeout)
            cfg.set(cfg.remote_command_timeout, self._credentials.command_timeout)
            cfg.set(cfg.remote_host_key_policy, self._credentials.host_key_policy)

    def _update_server_status(self) -> None:
        """更新服务器状态。"""
        if not self._manager:
            return

        try:
            # 获取环境信息
            probe = self._manager.probe_environment()
            self._update_status(
                os_name=probe.os_name,
                os_version="",
                architecture=probe.architecture,
            )

            # 获取 NapCat 状态
            status = self._manager.get_status()
            self._update_status(
                napcat_running=status.is_running,
                napcat_pid=status.pid,
            )

        except Exception:
            pass

    def initialize_workspace(self) -> tuple[bool, str]:
        """初始化工作区。"""
        if not self._manager or not self.is_connected:
            return False, "未连接"

        try:
            self._manager.initialize_workspace()
            return True, "工作区初始化完成"
        except Exception as e:
            return False, str(e)

    def clean_environment(self) -> tuple[bool, str]:
        """清空环境。"""
        if not self._manager or not self.is_connected:
            return False, "未连接"

        try:
            result = self._manager.deployment.clean_environment(include_qq=True)
            if result.ok:
                self._update_status(connected=False, napcat_running=False)
                return True, "环境已清空"
            return False, result.stderr
        except Exception as e:
            return False, str(e)

    def get_credentials(self) -> SSHCredentials | None:
        """获取当前凭据。"""
        return self._credentials

# -*- coding: utf-8 -*-
"""Agent/Daemon 连接配置面板。"""
from __future__ import annotations

from PySide6.QtCore import QObject, QRunnable, QThreadPool, Signal
from PySide6.QtWidgets import QDialog, QFrame, QHBoxLayout, QProgressBar, QTextEdit, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    LineEdit,
    PrimaryPushButton,
    PushButton,
    StrongBodyLabel,
    SubtitleLabel,
    FluentIcon as FI,
)

from src.desktop.core.remote import DaemonConfigManager, DaemonConnection, DaemonDeployer, SSHCredentials
from src.desktop.ui.components.info_bar import error_bar, success_bar


class DaemonDeployTask(QObject, QRunnable):
    """一次性 Daemon 部署任务。"""

    progress_signal = Signal(str, int)
    success_signal = Signal(str, int, str)
    error_signal = Signal(str, object)

    def __init__(self, credentials: SSHCredentials, daemon_port: int) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self._credentials = credentials
        self._daemon_port = daemon_port

    def run(self) -> None:
        try:
            with DaemonDeployer() as deployer:
                result = deployer.deploy(
                    credentials=self._credentials,
                    port=self._daemon_port,
                    progress_callback=lambda msg, pct: self.progress_signal.emit(msg, pct),
                )

            if result.success and result.token:
                self.success_signal.emit(result.host, result.port, result.token)
            else:
                self.error_signal.emit(result.error or "部署失败", result.logs)
        except Exception as error:  # pragma: no cover - UI 异步异常统一反馈
            self.error_signal.emit(f"部署异常: {error}", [])


class AgentConfigPanel(QWidget):
    """Agent/Daemon 配置面板。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._config_manager = DaemonConfigManager()
        self._current_connection_id = ""
        self._setup_ui()
        self._connect_signals()
        self._load_default_connection()

    def _setup_ui(self) -> None:
        """设置界面。"""
        layout = QVBoxLayout(self)
        layout.setSpacing(16)
        layout.setContentsMargins(0, 0, 0, 0)

        title_layout = QVBoxLayout()
        title_layout.setSpacing(4)
        self.title_label = SubtitleLabel("Agent/Daemon 连接", self)
        self.caption_label = CaptionLabel("通过 WebSocket 连接远程 Daemon", self)
        title_layout.addWidget(self.title_label)
        title_layout.addWidget(self.caption_label)
        layout.addLayout(title_layout)
        layout.addSpacing(8)

        self.conn_card = QFrame(self)
        self.conn_card.setObjectName("configCard")
        conn_layout = QVBoxLayout(self.conn_card)
        conn_layout.setSpacing(12)
        conn_layout.setContentsMargins(16, 16, 16, 16)

        conn_title = StrongBodyLabel("连接信息", self.conn_card)
        conn_layout.addWidget(conn_title)

        host_row = QHBoxLayout()
        host_row.setSpacing(12)
        host_label = BodyLabel("主机:", self.conn_card)
        host_label.setFixedWidth(60)
        self.host_edit = LineEdit(self.conn_card)
        self.host_edit.setPlaceholderText("例如: 192.168.1.100")

        port_label = BodyLabel("端口:", self.conn_card)
        port_label.setFixedWidth(40)
        self.port_edit = LineEdit(self.conn_card)
        self.port_edit.setText("8443")
        self.port_edit.setFixedWidth(80)

        host_row.addWidget(host_label)
        host_row.addWidget(self.host_edit, 1)
        host_row.addWidget(port_label)
        host_row.addWidget(self.port_edit)
        conn_layout.addLayout(host_row)

        token_row = QHBoxLayout()
        token_row.setSpacing(12)
        token_label = BodyLabel("Token:", self.conn_card)
        token_label.setFixedWidth(60)
        self.token_edit = LineEdit(self.conn_card)
        self.token_edit.setEchoMode(LineEdit.EchoMode.Password)
        self.token_edit.setPlaceholderText("输入连接 Token")
        token_row.addWidget(token_label)
        token_row.addWidget(self.token_edit, 1)
        conn_layout.addLayout(token_row)

        layout.addWidget(self.conn_card)

        self.deploy_card = QFrame(self)
        self.deploy_card.setObjectName("configCard")
        deploy_layout = QVBoxLayout(self.deploy_card)
        deploy_layout.setSpacing(12)
        deploy_layout.setContentsMargins(16, 16, 16, 16)

        deploy_title = StrongBodyLabel("首次部署", self.deploy_card)
        deploy_layout.addWidget(deploy_title)

        deploy_desc = BodyLabel(
            "如果服务器上还没有安装 Daemon，可通过一次性 SSH 部署向导完成安装。\nSSH 凭据只在部署期间使用，不会被长期保存。",
            self.deploy_card,
        )
        deploy_desc.setWordWrap(True)
        deploy_layout.addWidget(deploy_desc)

        self.deploy_btn = PrimaryPushButton(FI.DOWNLOAD, "打开部署向导", self.deploy_card)
        deploy_layout.addWidget(self.deploy_btn)

        layout.addWidget(self.deploy_card)

        info_card = QFrame(self)
        info_card.setObjectName("infoCard")
        info_layout = QVBoxLayout(info_card)
        info_layout.setSpacing(8)
        info_layout.setContentsMargins(16, 16, 16, 16)

        info_title = StrongBodyLabel("关于 Agent/Daemon 模式", info_card)
        info_layout.addWidget(info_title)

        info_text = BodyLabel(
            "• 需要在服务器上安装 NapCat Daemon\n"
            "• 支持实时监控和日志推送\n"
            "• 使用 WebSocket + TLS 加密通信\n"
            "• Token 会自动保存到系统密钥库",
            info_card,
        )
        info_text.setWordWrap(True)
        info_layout.addWidget(info_text)

        layout.addWidget(info_card)
        layout.addStretch()

        btn_layout = QHBoxLayout()
        btn_layout.setSpacing(12)
        self.test_btn = PushButton(FI.GLOBE, "测试连接", self)
        self.save_btn = PrimaryPushButton(FI.SAVE, "保存配置", self)
        btn_layout.addWidget(self.test_btn)
        btn_layout.addStretch()
        btn_layout.addWidget(self.save_btn)
        layout.addLayout(btn_layout)

    def _connect_signals(self) -> None:
        """连接信号。"""
        self.deploy_btn.clicked.connect(self._on_deploy)

    def _load_default_connection(self) -> None:
        """加载默认连接。"""
        default_connection = self._config_manager.get_default_connection()
        if not default_connection:
            return

        conn_id, connection = default_connection
        token = self._config_manager.get_token(conn_id) or ""
        self._current_connection_id = conn_id
        self.set_connection_info(connection.host, connection.port, token)

    def _on_deploy(self) -> None:
        """打开一次性部署向导。"""
        dialog = QDialog(self.window())
        dialog.setWindowTitle("一次性部署 Daemon")
        dialog.setFixedSize(500, 400)

        layout = QVBoxLayout(dialog)
        layout.setSpacing(12)

        intro = BodyLabel(
            "填写一次性 SSH 凭据后，程序会安装远程 Daemon 并自动回填连接信息。\n关闭向导后 SSH 信息即失效，不写入本地配置。",
            dialog,
        )
        intro.setWordWrap(True)
        layout.addWidget(intro)

        host_edit = LineEdit(dialog)
        host_edit.setPlaceholderText("服务器主机")
        host_edit.setText(self.host_edit.text().strip())
        layout.addWidget(host_edit)

        daemon_port_edit = LineEdit(dialog)
        daemon_port_edit.setPlaceholderText("Daemon 端口")
        daemon_port_edit.setText(self.port_edit.text() or "8443")
        layout.addWidget(daemon_port_edit)

        ssh_user_edit = LineEdit(dialog)
        ssh_user_edit.setPlaceholderText("SSH 用户名")
        ssh_user_edit.setText("root")
        layout.addWidget(ssh_user_edit)

        ssh_pwd_edit = LineEdit(dialog)
        ssh_pwd_edit.setEchoMode(LineEdit.EchoMode.Password)
        ssh_pwd_edit.setPlaceholderText("SSH 密码")
        layout.addWidget(ssh_pwd_edit)

        start_btn = PrimaryPushButton(FI.PLAY, "开始部署", dialog)
        layout.addWidget(start_btn)

        progress = QProgressBar(dialog)
        progress.setRange(0, 100)
        layout.addWidget(progress)

        log_edit = QTextEdit(dialog)
        log_edit.setReadOnly(True)
        layout.addWidget(log_edit, 1)

        def update_progress(msg: str, pct: int) -> None:
            progress.setValue(pct)
            log_edit.append(f"[{pct}%] {msg}")

        def on_success(host: str, port: int, token: str) -> None:
            self.set_connection_info(host, port, token)
            success_bar("部署成功，已自动回填 Daemon 连接信息", parent=self.window())
            dialog.accept()

        def on_error(message: str, logs: object) -> None:
            error_bar(message, parent=self.window())
            if isinstance(logs, list):
                for item in logs:
                    log_edit.append(item)
            start_btn.setEnabled(True)

        def start_deploy() -> None:
            host = host_edit.text().strip()
            if not host:
                error_bar("请输入服务器主机", parent=dialog)
                return

            password = ssh_pwd_edit.text()
            if not password:
                error_bar("请输入 SSH 密码", parent=dialog)
                return

            credentials = SSHCredentials(
                host=host,
                port=22,
                username=ssh_user_edit.text().strip() or "root",
                auth_method="password",
                password=password,
            )

            log_edit.clear()
            progress.setValue(0)
            start_btn.setEnabled(False)

            task = DaemonDeployTask(credentials, int(daemon_port_edit.text() or 8443))
            task.progress_signal.connect(update_progress)
            task.success_signal.connect(on_success)
            task.error_signal.connect(on_error)
            dialog._deploy_task = task
            QThreadPool.globalInstance().start(task)

        start_btn.clicked.connect(start_deploy)
        dialog.exec()

    def get_connection_info(self) -> tuple[str, int, str]:
        """获取连接信息。"""
        return (
            self.host_edit.text().strip(),
            int(self.port_edit.text() or 8443),
            self.token_edit.text().strip(),
        )

    def set_connection_info(self, host: str, port: int, token: str) -> None:
        """设置连接信息。"""
        self.host_edit.setText(host)
        self.port_edit.setText(str(port))
        self.token_edit.setText(token)

    def save_config(self) -> None:
        """保存配置。"""
        try:
            host, port, token = self.get_connection_info()
            if not host or not token:
                error_bar("主机和 Token 不能为空", parent=self.window())
                return

            conn = DaemonConnection(
                name=f"Daemon@{host}",
                host=host,
                port=port,
                use_ssl=True,
            )

            if self._current_connection_id and self._config_manager.get_connection(self._current_connection_id):
                self._config_manager.update_connection(self._current_connection_id, conn, token)
                conn_id = self._current_connection_id
            else:
                conn_id = self._config_manager.add_connection(conn, token)
                self._current_connection_id = conn_id

            success_bar(f"配置已保存 (ID: {conn_id})", parent=self.window())
        except Exception as e:
            error_bar(f"保存失败: {e}", parent=self.window())

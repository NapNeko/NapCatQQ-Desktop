# -*- coding: utf-8 -*-
"""远程服务器状态面板 - Fluent Design 优化版"""
from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    PrimaryPushButton,
    PushButton,
    StrongBodyLabel,
    SubtitleLabel,
    TextEdit,
    FluentIcon as FI,
)

from .connection_base import ConnectionState, ServerStatus


class StatusPanel(QWidget):
    """服务器状态面板 - 紧凑 Fluent Design 布局"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self) -> None:
        """设置界面 - 紧凑布局"""
        layout = QVBoxLayout(self)
        layout.setSpacing(12)  # 减小间距
        layout.setContentsMargins(0, 0, 0, 0)

        # 标题区域
        self.title_label = SubtitleLabel("服务器状态", self)
        self.title_label.setContentsMargins(0, 0, 0, 4)
        layout.addWidget(self.title_label)

        # 状态卡片 - 紧凑设计
        self.status_card = QFrame(self)
        self.status_card.setObjectName("statusCard")
        self.status_card.setStyleSheet("""
            #statusCard {
                background: rgba(128, 128, 128, 0.05);
                border: 1px solid rgba(128, 128, 128, 0.1);
                border-radius: 8px;
            }
        """)
        status_layout = QVBoxLayout(self.status_card)
        status_layout.setSpacing(8)
        status_layout.setContentsMargins(12, 12, 12, 12)

        # 状态指示器 - 更紧凑
        status_row = QHBoxLayout()
        status_row.setSpacing(6)
        self.status_dot = BodyLabel("●", self.status_card)
        self.status_dot.setStyleSheet("color: #999; font-size: 14px;")
        self.status_text = StrongBodyLabel("未连接", self.status_card)
        self.status_text.setStyleSheet("color: #999;")
        status_row.addWidget(self.status_dot)
        status_row.addWidget(self.status_text)
        status_row.addStretch()
        status_layout.addLayout(status_row)

        # 状态信息 - 两列紧凑网格
        info_grid = QGridLayout()
        info_grid.setSpacing(4)
        info_grid.setHorizontalSpacing(16)
        info_grid.setColumnStretch(1, 1)
        info_grid.setColumnStretch(3, 1)

        self.info_labels = {}
        info_items = [
            ("系统", "os_name"),
            ("架构", "architecture"),
            ("主机名", "hostname"),
            ("Daemon", "daemon_version"),
            ("NapCat", "napcat_status"),
            ("版本", "napcat_version"),
            ("QQ", "qq_version"),
            ("账号", "qq_number"),
        ]

        for i, (label, key) in enumerate(info_items):
            row = i // 2
            col = (i % 2) * 2

            label_widget = BodyLabel(f"{label}:", self.status_card)
            label_widget.setStyleSheet("color: #666; font-size: 12px;")
            value_widget = BodyLabel("--", self.status_card)
            value_widget.setStyleSheet("font-size: 12px;")

            info_grid.addWidget(label_widget, row, col)
            info_grid.addWidget(value_widget, row, col + 1)
            self.info_labels[key] = value_widget

        status_layout.addLayout(info_grid)
        layout.addWidget(self.status_card)

        # 操作卡片
        self.action_card = QFrame(self)
        self.action_card.setObjectName("actionCard")
        self.action_card.setStyleSheet("""
            #actionCard {
                background: rgba(128, 128, 128, 0.05);
                border: 1px solid rgba(128, 128, 128, 0.1);
                border-radius: 8px;
            }
        """)
        action_layout = QVBoxLayout(self.action_card)
        action_layout.setSpacing(8)
        action_layout.setContentsMargins(12, 12, 12, 12)

        # 连接按钮行
        conn_row = QHBoxLayout()
        conn_row.setSpacing(8)
        self.connect_btn = PrimaryPushButton(FI.LINK, "连接", self.action_card)
        self.disconnect_btn = PushButton(FI.CANCEL, "断开", self.action_card)
        self.connect_btn.setMinimumWidth(80)
        self.disconnect_btn.setMinimumWidth(80)
        conn_row.addWidget(self.connect_btn)
        conn_row.addWidget(self.disconnect_btn)
        conn_row.addStretch()
        action_layout.addLayout(conn_row)

        # 控制按钮行
        ctrl_row = QHBoxLayout()
        ctrl_row.setSpacing(8)
        self.start_btn = PushButton(FI.PLAY, "启动", self.action_card)
        self.stop_btn = PushButton(FI.PAUSE, "停止", self.action_card)
        self.restart_btn = PushButton(FI.ROTATE, "重启", self.action_card)

        for btn in [self.start_btn, self.stop_btn, self.restart_btn]:
            btn.setMinimumWidth(70)
            btn.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)

        ctrl_row.addWidget(self.start_btn)
        ctrl_row.addWidget(self.stop_btn)
        ctrl_row.addWidget(self.restart_btn)
        action_layout.addLayout(ctrl_row)

        # 部署按钮（仅 SSH 模式显示）
        self.deploy_btn = PrimaryPushButton(FI.DOWNLOAD, "部署 NapCat", self.action_card)
        self.deploy_btn.setVisible(False)
        self.deploy_btn.setMinimumHeight(36)
        action_layout.addWidget(self.deploy_btn)

        layout.addWidget(self.action_card)

        # 日志卡片
        self.log_card = QFrame(self)
        self.log_card.setObjectName("logCard")
        self.log_card.setStyleSheet("""
            #logCard {
                background: rgba(0, 0, 0, 0.02);
                border: 1px solid rgba(128, 128, 128, 0.1);
                border-radius: 8px;
            }
        """)
        log_layout = QVBoxLayout(self.log_card)
        log_layout.setSpacing(6)
        log_layout.setContentsMargins(10, 10, 10, 10)

        log_header = QHBoxLayout()
        log_title = StrongBodyLabel("实时日志", self.log_card)
        self.clear_log_btn = PushButton(FI.DELETE, "清空", self.log_card)
        log_header.addWidget(log_title)
        log_header.addStretch()
        log_header.addWidget(self.clear_log_btn)
        log_layout.addLayout(log_header)

        self.log_edit = TextEdit(self.log_card)
        self.log_edit.setReadOnly(True)
        self.log_edit.setPlaceholderText("日志将显示在这里...")
        self.log_edit.document().setMaximumBlockCount(500)
        self.log_edit.setStyleSheet("""
            QTextEdit {
                background: rgba(0, 0, 0, 0.03);
                border: 1px solid rgba(128, 128, 128, 0.1);
                border-radius: 4px;
                padding: 8px;
                font-family: 'Consolas', 'Monaco', monospace;
                font-size: 12px;
            }
        """)
        self.log_edit.setMinimumHeight(120)
        self.log_edit.setMaximumHeight(200)
        log_layout.addWidget(self.log_edit)

        layout.addWidget(self.log_card, 1)

        # 初始状态
        self._update_button_state(ConnectionState.DISCONNECTED)

    def set_connection_state(self, state: ConnectionState) -> None:
        """设置连接状态显示"""
        state_map = {
            ConnectionState.DISCONNECTED: ("未连接", "#999"),
            ConnectionState.CONNECTING: ("连接中...", "#faad14"),
            ConnectionState.AUTHENTICATING: ("认证中...", "#faad14"),
            ConnectionState.CONNECTED: ("已连接", "#52c41a"),
            ConnectionState.ERROR: ("连接错误", "#f5222d"),
            ConnectionState.RECONNECTING: ("重连中...", "#faad14"),
        }

        text, color = state_map.get(state, ("未知", "#999"))
        self.status_dot.setStyleSheet(f"color: {color}; font-size: 14px;")
        self.status_text.setStyleSheet(f"color: {color};")
        self.status_text.setText(text)

        self._update_button_state(state)

    def update_server_status(self, status: ServerStatus) -> None:
        """更新服务器状态"""
        self.info_labels["os_name"].setText(status.os_name or "--")
        self.info_labels["architecture"].setText(status.architecture or "--")
        self.info_labels["hostname"].setText(status.hostname or "--")
        self.info_labels["daemon_version"].setText(status.daemon_version or "--")

        napcat_status = "运行中" if status.napcat_running else "未运行"
        if status.napcat_pid:
            napcat_status += f" (PID: {status.napcat_pid})"
        self.info_labels["napcat_status"].setText(napcat_status)

        self.info_labels["napcat_version"].setText(status.napcat_version or "--")
        self.info_labels["qq_version"].setText(status.qq_version or "--")
        self.info_labels["qq_number"].setText(status.qq_number or "--")

    def append_log(self, message: str) -> None:
        """添加日志"""
        self.log_edit.append(message)

    def clear_log(self) -> None:
        """清空日志"""
        self.log_edit.clear()

    def set_deploy_visible(self, visible: bool) -> None:
        """设置部署按钮可见性"""
        self.deploy_btn.setVisible(visible)

    def _update_button_state(self, state: ConnectionState) -> None:
        """更新按钮状态"""
        connected = state == ConnectionState.CONNECTED
        connecting = state in [ConnectionState.CONNECTING, ConnectionState.AUTHENTICATING]

        self.connect_btn.setEnabled(not connected and not connecting)
        self.disconnect_btn.setEnabled(connected or connecting)
        self.start_btn.setEnabled(connected)
        self.stop_btn.setEnabled(connected)
        self.restart_btn.setEnabled(connected)

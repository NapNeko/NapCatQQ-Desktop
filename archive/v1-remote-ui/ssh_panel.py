# -*- coding: utf-8 -*-
"""SSH 连接配置面板 - Fluent Design 紧凑版"""
from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QFrame, QHBoxLayout, QLineEdit, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    ComboBox,
    LineEdit,
    PrimaryPushButton,
    PushButton,
    StrongBodyLabel,
    SubtitleLabel,
    ToolButton,
    FluentIcon as FI,
)

from src.desktop.core.config import cfg
from src.desktop.core.remote import SSHCredentials
from src.desktop.ui.components.info_bar import error_bar, success_bar


class SSHConfigPanel(QWidget):
    """SSH 配置面板 - 紧凑 Fluent Design"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._setup_ui()
        self._connect_signals()
        self._load_config()

    def _setup_ui(self) -> None:
        """设置界面 - 紧凑布局"""
        layout = QVBoxLayout(self)
        layout.setSpacing(12)
        layout.setContentsMargins(0, 0, 0, 0)

        # 标题
        self.title_label = SubtitleLabel("SSH 连接配置", self)
        self.title_label.setContentsMargins(0, 0, 0, 2)
        layout.addWidget(self.title_label)

        self.caption_label = CaptionLabel("通过 SSH 直接管理远程服务器", self)
        self.caption_label.setContentsMargins(0, 0, 0, 8)
        layout.addWidget(self.caption_label)

        # 连接信息卡片
        self.conn_card = self._create_card()
        conn_layout = QVBoxLayout(self.conn_card)
        conn_layout.setSpacing(10)
        conn_layout.setContentsMargins(12, 12, 12, 12)

        # 主机和端口 - 单行布局
        host_row = QHBoxLayout()
        host_row.setSpacing(10)

        host_container = QHBoxLayout()
        host_container.setSpacing(6)
        host_label = BodyLabel("主机:", self.conn_card)
        host_label.setFixedWidth(40)
        self.host_edit = LineEdit(self.conn_card)
        self.host_edit.setPlaceholderText("192.168.1.100")
        host_container.addWidget(host_label)
        host_container.addWidget(self.host_edit)
        host_row.addLayout(host_container, 2)

        port_container = QHBoxLayout()
        port_container.setSpacing(6)
        port_label = BodyLabel("端口:", self.conn_card)
        port_label.setFixedWidth(35)
        self.port_edit = LineEdit(self.conn_card)
        self.port_edit.setText("22")
        self.port_edit.setFixedWidth(60)
        port_container.addWidget(port_label)
        port_container.addWidget(self.port_edit)
        host_row.addLayout(port_container, 1)

        conn_layout.addLayout(host_row)

        # 用户名
        user_row = QHBoxLayout()
        user_row.setSpacing(6)
        user_label = BodyLabel("用户名:", self.conn_card)
        user_label.setFixedWidth(50)
        self.username_edit = LineEdit(self.conn_card)
        self.username_edit.setPlaceholderText("root")
        user_row.addWidget(user_label)
        user_row.addWidget(self.username_edit)
        conn_layout.addLayout(user_row)

        layout.addWidget(self.conn_card)

        # 认证卡片
        self.auth_card = self._create_card()
        auth_layout = QVBoxLayout(self.auth_card)
        auth_layout.setSpacing(10)
        auth_layout.setContentsMargins(12, 12, 12, 12)

        auth_title = StrongBodyLabel("认证方式", self.auth_card)
        auth_layout.addWidget(auth_title)

        # 认证方式选择
        method_row = QHBoxLayout()
        method_row.setSpacing(6)
        method_label = BodyLabel("方式:", self.auth_card)
        method_label.setFixedWidth(40)
        self.method_combo = ComboBox(self.auth_card)
        self.method_combo.addItem("私钥", "key")
        self.method_combo.addItem("密码", "password")
        self.method_combo.setFixedWidth(100)
        method_row.addWidget(method_label)
        method_row.addWidget(self.method_combo)
        method_row.addStretch()
        auth_layout.addLayout(method_row)

        # 私钥路径
        self.key_widget = QWidget(self.auth_card)
        key_layout = QHBoxLayout(self.key_widget)
        key_layout.setSpacing(6)
        key_layout.setContentsMargins(0, 0, 0, 0)
        key_label = BodyLabel("私钥:", self.key_widget)
        key_label.setFixedWidth(40)
        self.key_edit = LineEdit(self.key_widget)
        self.key_edit.setPlaceholderText("~/.ssh/id_rsa")
        self.key_browse_btn = ToolButton(FI.FOLDER, self.key_widget)
        self.key_browse_btn.setFixedSize(32, 32)
        key_layout.addWidget(key_label)
        key_layout.addWidget(self.key_edit, 1)
        key_layout.addWidget(self.key_browse_btn)
        auth_layout.addWidget(self.key_widget)

        # 密码
        self.pwd_widget = QWidget(self.auth_card)
        pwd_layout = QHBoxLayout(self.pwd_widget)
        pwd_layout.setSpacing(6)
        pwd_layout.setContentsMargins(0, 0, 0, 0)
        pwd_label = BodyLabel("密码:", self.pwd_widget)
        pwd_label.setFixedWidth(40)
        self.pwd_edit = LineEdit(self.pwd_widget)
        self.pwd_edit.setEchoMode(LineEdit.EchoMode.Password)
        self.pwd_edit.setPlaceholderText("输入密码")
        pwd_layout.addWidget(pwd_label)
        pwd_layout.addWidget(self.pwd_edit, 1)
        auth_layout.addWidget(self.pwd_widget)
        self.pwd_widget.hide()

        layout.addWidget(self.auth_card)

        # 高级选项卡片
        self.advanced_card = self._create_card()
        adv_layout = QVBoxLayout(self.advanced_card)
        adv_layout.setSpacing(8)
        adv_layout.setContentsMargins(12, 12, 12, 12)

        # 高级选项标题（可点击展开）
        adv_header = QHBoxLayout()
        self.advanced_toggle = ToolButton(FI.CHEVRON_RIGHT_MED, self.advanced_card)
        self.advanced_toggle.setFixedSize(28, 28)
        adv_title = StrongBodyLabel("高级选项", self.advanced_card)
        adv_header.addWidget(self.advanced_toggle)
        adv_header.addWidget(adv_title)
        adv_header.addStretch()
        adv_layout.addLayout(adv_header)

        # 高级选项内容
        self.advanced_content = QWidget(self.advanced_card)
        adv_content_layout = QVBoxLayout(self.advanced_content)
        adv_content_layout.setSpacing(8)
        adv_content_layout.setContentsMargins(20, 0, 0, 0)

        # 超时设置
        timeout_row = QHBoxLayout()
        timeout_row.setSpacing(6)
        timeout_label = BodyLabel("连接超时:", self.advanced_content)
        timeout_label.setFixedWidth(65)
        self.timeout_edit = LineEdit(self.advanced_content)
        self.timeout_edit.setText("10")
        self.timeout_edit.setFixedWidth(50)
        timeout_unit = BodyLabel("秒", self.advanced_content)
        timeout_row.addWidget(timeout_label)
        timeout_row.addWidget(self.timeout_edit)
        timeout_row.addWidget(timeout_unit)
        timeout_row.addStretch()
        adv_content_layout.addLayout(timeout_row)

        # 命令超时
        cmd_timeout_row = QHBoxLayout()
        cmd_timeout_row.setSpacing(6)
        cmd_timeout_label = BodyLabel("命令超时:", self.advanced_content)
        cmd_timeout_label.setFixedWidth(65)
        self.cmd_timeout_edit = LineEdit(self.advanced_content)
        self.cmd_timeout_edit.setText("60")
        self.cmd_timeout_edit.setFixedWidth(50)
        cmd_timeout_unit = BodyLabel("秒", self.advanced_content)
        cmd_timeout_row.addWidget(cmd_timeout_label)
        cmd_timeout_row.addWidget(self.cmd_timeout_edit)
        cmd_timeout_row.addWidget(cmd_timeout_unit)
        cmd_timeout_row.addStretch()
        adv_content_layout.addLayout(cmd_timeout_row)

        # 指纹策略
        policy_row = QHBoxLayout()
        policy_row.setSpacing(6)
        policy_label = BodyLabel("指纹策略:", self.advanced_content)
        policy_label.setFixedWidth(65)
        self.policy_combo = ComboBox(self.advanced_content)
        self.policy_combo.addItem("严格检查", "reject")
        self.policy_combo.addItem("警告", "warning")
        self.policy_combo.addItem("自动添加", "auto_add")
        self.policy_combo.setFixedWidth(120)
        policy_row.addWidget(policy_label)
        policy_row.addWidget(self.policy_combo)
        policy_row.addStretch()
        adv_content_layout.addLayout(policy_row)

        adv_layout.addWidget(self.advanced_content)
        self.advanced_content.hide()

        layout.addWidget(self.advanced_card)
        layout.addStretch()

        # 按钮行
        btn_layout = QHBoxLayout()
        btn_layout.setSpacing(10)
        self.test_btn = PushButton(FI.GLOBE, "测试连接", self)
        self.save_btn = PrimaryPushButton(FI.SAVE, "保存配置", self)
        btn_layout.addWidget(self.test_btn)
        btn_layout.addStretch()
        btn_layout.addWidget(self.save_btn)
        layout.addLayout(btn_layout)

    def _create_card(self) -> QFrame:
        """创建标准卡片"""
        card = QFrame(self)
        card.setObjectName("configCard")
        card.setStyleSheet("""
            #configCard {
                background: rgba(128, 128, 128, 0.05);
                border: 1px solid rgba(128, 128, 128, 0.1);
                border-radius: 8px;
            }
        """)
        return card

    def _connect_signals(self) -> None:
        """连接信号"""
        self.method_combo.currentIndexChanged.connect(self._on_method_index_changed)
        self.advanced_toggle.clicked.connect(self._toggle_advanced)

    def _load_config(self) -> None:
        """加载配置"""
        self.host_edit.setText(cfg.get(cfg.remote_host) or "")
        self.port_edit.setText(str(cfg.get(cfg.remote_port) or 22))
        self.username_edit.setText(cfg.get(cfg.remote_username) or "")

        auth_method = cfg.get(cfg.remote_auth_method) or "key"
        index = self.method_combo.findData(auth_method)
        if index >= 0:
            self.method_combo.setCurrentIndex(index)

        self.key_edit.setText(cfg.get(cfg.remote_private_key_path) or "")
        self.pwd_edit.clear()
        self.timeout_edit.setText(str(cfg.get(cfg.remote_connect_timeout) or 10))
        self.cmd_timeout_edit.setText(str(cfg.get(cfg.remote_command_timeout) or 60))

        policy = cfg.get(cfg.remote_host_key_policy) or "reject"
        index = self.policy_combo.findData(policy)
        if index >= 0:
            self.policy_combo.setCurrentIndex(index)

        self._on_method_changed(auth_method)

    def _on_method_index_changed(self, index: int) -> None:
        """认证方式切换（按索引）"""
        method = self.method_combo.itemData(index)
        self._on_method_changed(method)

    def _on_method_changed(self, method: str) -> None:
        """认证方式切换"""
        is_key = method == "key"
        self.key_widget.setVisible(is_key)
        self.pwd_widget.setVisible(not is_key)

    def _toggle_advanced(self) -> None:
        """切换高级选项"""
        visible = not self.advanced_content.isVisible()
        self.advanced_content.setVisible(visible)
        self.advanced_toggle.setIcon(FI.CHEVRON_DOWN_MED if visible else FI.CHEVRON_RIGHT_MED)

    def get_credentials(self) -> SSHCredentials:
        """获取 SSH 凭据"""
        return SSHCredentials(
            host=self.host_edit.text().strip(),
            port=int(self.port_edit.text() or 22),
            username=self.username_edit.text().strip(),
            auth_method=self.method_combo.currentData(),
            password=self.pwd_edit.text() if self.method_combo.currentData() == "password" else None,
            private_key_path=self.key_edit.text() if self.method_combo.currentData() == "key" else None,
            connect_timeout=float(self.timeout_edit.text() or 10),
            command_timeout=float(self.cmd_timeout_edit.text() or 60),
            host_key_policy=self.policy_combo.currentData(),
        )

    def save_config(self) -> None:
        """保存配置"""
        try:
            cfg.set(cfg.remote_host, self.host_edit.text().strip())
            cfg.set(cfg.remote_port, int(self.port_edit.text() or 22))
            cfg.set(cfg.remote_username, self.username_edit.text().strip())
            cfg.set(cfg.remote_auth_method, self.method_combo.currentData())
            cfg.set(cfg.remote_private_key_path, self.key_edit.text())
            cfg.set(cfg.remote_connect_timeout, int(self.timeout_edit.text() or 10))
            cfg.set(cfg.remote_command_timeout, int(self.cmd_timeout_edit.text() or 60))
            cfg.set(cfg.remote_host_key_policy, self.policy_combo.currentData())
            success_bar("配置已保存", parent=self.window())
        except Exception as e:
            error_bar(f"保存失败: {e}", parent=self.window())

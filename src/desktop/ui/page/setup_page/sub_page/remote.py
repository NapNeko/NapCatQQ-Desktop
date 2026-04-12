# -*- coding: utf-8 -*-
"""远程管理设置页 - 一页式布局设计

布局规划 (基于 1100x560 可用区域):
+-------------------------------------------------------------+
|  [远程管理]                                [连接状态: ●]    |  <- 标题栏 + 状态
+------------------+------------------------------------------+
|  连接配置        |  服务器状态                              |
|  +--------------+|  +------------------------------------+  |
|  | 主机: [   ]  ||  |  已连接 / 未连接                     |  |
|  | 端口: [   ]  ||  |  系统: Ubuntu 24.04                 |  |
|  | 用户: [   ]  ||  |  架构: amd64                        |  |
|  | 认证: [▼]    ||  |  QQ版本: 3.2.25                     |  |
|  |   [密钥路径] ||  |  NapCat: v2.6.0                     |  |
|  +--------------+|  +------------------------------------+  |
|                  |                                          |
|  [测试连接]      |  快捷操作                                |
|                  |  +------------------------------------+  |
|  ----------------|  | [启动] [停止] [重启] [查看日志]      |  |
|                  |  +------------------------------------+  |
|  工作区          |                                          |
|  +--------------+|  部署流程                                |
|  | 路径: [   ]  ||  +------------------------------------+  |
|  +--------------+|  | [探测] [初始化] [同步配置] [部署]    |  |
|                  |  +------------------------------------+  |
|  [保存配置]      |                                          |
+------------------+------------------------------------------+

设计原则:
1. 左右分栏: 左栏配置(45%), 右栏状态与操作(55%)
2. 垂直紧凑: 所有控件在一页内展示，无需滚动
3. 认知分块: 每个区域5-7个控件，符合Miller定律
4. 渐进披露: 高级选项折叠，密码/密钥条件显示
"""
from __future__ import annotations

# 标准库导入
import re

# 第三方库导入
from PySide6.QtCore import QObject, QRunnable, Qt, QThreadPool, Signal, Slot
from PySide6.QtWidgets import QGridLayout, QHBoxLayout, QLineEdit, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    ExpandLayout,
    FluentIcon as FI,
    PrimaryPushButton,
    PushButton,
    ScrollArea,
    SettingCard,
    SettingCardGroup,
    StrongBodyLabel,
    SwitchButton,
    TitleLabel,
    ToolButton,
)

# 项目内模块导入
from src.desktop.core.config import cfg
from src.desktop.core.logging import LogSource, LogType, logger
from src.desktop.core.remote import RemoteManager, SSHCredentials
from src.desktop.ui.components.info_bar import error_bar, info_bar, success_bar, warning_bar
from src.desktop.ui.components.input_card.generic_card import ComboBoxConfigCard, LineEditConfigCard


class ActionButtonCard(SettingCard):
    """带操作按钮的设置卡片。"""

    def __init__(
        self,
        icon,
        title: str,
        button_text: str,
        callback,
        content: str | None = None,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(icon, title, content, parent)
        self._callback = callback
        self.button = PushButton(text=button_text, parent=self)
        self.button.clicked.connect(self._callback)
        self.hBoxLayout.addWidget(self.button, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)


class RemoteActionTask(QObject, QRunnable):
    """远程操作任务。"""

    success_signal = Signal(str, str)
    error_signal = Signal(str, str)

    def __init__(self, action_name: str, handler) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self._action_name = action_name
        self._handler = handler

    def run(self) -> None:
        try:
            message = self._handler()
            self.success_signal.emit(self._action_name, message)
        except Exception as exc:  # pragma: no cover - UI 异步异常统一反馈
            logger.exception(
                f"远程操作执行异常: action={self._action_name}",
                exc,
                log_type=LogType.NETWORK,
                log_source=LogSource.UI,
            )
            self.error_signal.emit(self._action_name, f"{type(exc).__name__}: {exc}")


class Remote(QWidget):
    """远程管理设置页 - 一页式无滚动布局。

    基于 1100x560 可用区域设计，左右分栏布局：
    - 左栏 (45%): 连接配置 + 工作区设置
    - 右栏 (55%): 状态显示 + 快捷操作 + 部署流程
    """

    # 窗口尺寸常量
    AVAILABLE_WIDTH = 1100
    AVAILABLE_HEIGHT = 560
    LEFT_PANEL_WIDTH = 480  # 左栏固定宽度
    RIGHT_PANEL_WIDTH = 580  # 右栏固定宽度
    PANEL_SPACING = 24  # 左右栏间距

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        self.setObjectName("SetupRemoteWidget")
        self._create_widgets()
        self._setup_layout()
        self._connect_signals()
        self.fill_config()

    def _create_widgets(self) -> None:
        """创建所有控件。"""
        # ========== 标题栏 ==========
        self.title_label = TitleLabel(self.tr("🌐 远程管理"), self)
        self.status_indicator = CaptionLabel(self.tr("● 未连接"), self)
        self.status_indicator.setStyleSheet("color: #999;")

        # ========== 左栏: 连接配置 ==========
        # SSH连接卡片
        self.connection_card = QWidget(self)
        self.connection_card.setObjectName("connectionCard")

        self.host_card = LineEditConfigCard(
            FI.GLOBE, self.tr("主机"), "example.com", parent=self.connection_card
        )
        self.port_card = LineEditConfigCard(
            FI.INFO, self.tr("端口"), "22", parent=self.connection_card
        )
        self.username_card = LineEditConfigCard(
            FI.CONNECT, self.tr("用户名"), "root", parent=self.connection_card
        )
        self.auth_method_card = ComboBoxConfigCard(
            FI.DOCUMENT, self.tr("认证方式"), ["key", "password"], parent=self.connection_card
        )
        self.private_key_path_card = LineEditConfigCard(
            FI.DOCUMENT, self.tr("私钥路径"), "~/.ssh/id_ed25519", parent=self.connection_card
        )
        self.password_card = LineEditConfigCard(
            FI.VPN, self.tr("密码"), self.tr("认证密码"), parent=self.connection_card
        )
        self.password_card.lineEdit.setEchoMode(QLineEdit.EchoMode.Password)

        # 高级选项折叠区域 (默认隐藏详细配置)
        self.advanced_toggle = ToolButton(FI.CHEVRON_RIGHT, self)
        self.advanced_label = StrongBodyLabel(self.tr("高级选项"), self)
        self.advanced_widget = QWidget(self)
        self.timeout_card = LineEditConfigCard(
            FI.INFO, self.tr("连接超时"), "10s", parent=self.advanced_widget
        )
        self.command_timeout_card = LineEditConfigCard(
            FI.INFO, self.tr("命令超时"), "20s", parent=self.advanced_widget
        )
        self.host_key_policy_card = ComboBoxConfigCard(
            FI.VPN, self.tr("指纹策略"), ["reject", "warning", "auto_add"], parent=self.advanced_widget
        )

        # 工作区卡片
        self.workspace_card = QWidget(self)
        self.workspace_card.setObjectName("workspaceCard")
        self.workspace_dir_card = LineEditConfigCard(
            FI.FOLDER, self.tr("工作目录"), "$HOME/Napcat", parent=self.workspace_card
        )

        # 保存按钮
        self.save_button = PrimaryPushButton(FI.SAVE, self.tr("保存配置"), self)
        self.test_connection_btn = PushButton(FI.GLOBE, self.tr("测试连接"), self)

        # ========== 右栏: 状态与操作 ==========
        # 状态卡片
        self.status_card = QWidget(self)
        self.status_card.setObjectName("statusCard")
        self.status_title = StrongBodyLabel(self.tr("📊 服务器状态"), self.status_card)
        self.status_detail = BodyLabel(
            self.tr("连接状态: 未连接\n系统: --\n架构: --\nQQ版本: --\nNapCat: --"),
            self.status_card,
        )
        self.status_detail.setStyleSheet("line-height: 1.6; color: #666;")

        # 快捷操作卡片
        self.quick_action_card = QWidget(self)
        self.quick_action_card.setObjectName("quickActionCard")
        self.quick_action_title = StrongBodyLabel(self.tr("🚀 快捷操作"), self.quick_action_card)
        self.start_btn = PushButton(FI.PLAY, self.tr("启动"), self.quick_action_card)
        self.stop_btn = PushButton(FI.PAUSE, self.tr("停止"), self.quick_action_card)
        self.restart_btn = PushButton(FI.SYNC, self.tr("重启"), self.quick_action_card)
        self.log_btn = PushButton(FI.DOCUMENT, self.tr("日志"), self.quick_action_card)

        # 部署流程卡片
        self.deploy_card = QWidget(self)
        self.deploy_card.setObjectName("deployCard")
        self.deploy_title = StrongBodyLabel(self.tr("📋 部署流程"), self.deploy_card)
        self.probe_btn = PushButton(FI.SEARCH, self.tr("探测"), self.deploy_card)
        self.init_btn = PushButton(FI.FOLDER_ADD, self.tr("初始化"), self.deploy_card)
        self.sync_btn = PushButton(FI.SYNC, self.tr("同步配置"), self.deploy_card)
        self.deploy_btn = PrimaryPushButton(FI.DOWNLOAD, self.tr("部署"), self.deploy_card)

    def _setup_layout(self) -> None:
        """设置布局 - 左右分栏，一页内展示。"""
        main_layout = QVBoxLayout(self)
        main_layout.setContentsMargins(20, 16, 20, 16)
        main_layout.setSpacing(16)

        # ========== 标题栏 ==========
        title_layout = QHBoxLayout()
        title_layout.addWidget(self.title_label)
        title_layout.addStretch()
        title_layout.addWidget(self.status_indicator)
        main_layout.addLayout(title_layout)

        # ========== 主内容区: 左右分栏 ==========
        content_layout = QHBoxLayout()
        content_layout.setSpacing(self.PANEL_SPACING)

        # ===== 左栏: 连接配置 =====
        left_panel = QVBoxLayout()
        left_panel.setSpacing(12)
        left_panel.setAlignment(Qt.AlignmentFlag.AlignTop)

        # 连接配置区域
        conn_layout = QVBoxLayout(self.connection_card)
        conn_layout.setSpacing(8)
        conn_layout.setContentsMargins(12, 12, 12, 12)

        conn_header = QHBoxLayout()
        conn_header.addWidget(StrongBodyLabel(self.tr("📡 SSH 连接配置")))
        conn_layout.addLayout(conn_header)

        # 紧凑的输入卡片布局 (2列)
        conn_grid = QGridLayout()
        conn_grid.setSpacing(8)
        conn_grid.addWidget(self.host_card, 0, 0)
        conn_grid.addWidget(self.port_card, 0, 1)
        conn_grid.addWidget(self.username_card, 1, 0)
        conn_grid.addWidget(self.auth_method_card, 1, 1)
        conn_layout.addLayout(conn_grid)

        # 认证详情 (条件显示)
        self.auth_detail_layout = QVBoxLayout()
        self.auth_detail_layout.addWidget(self.private_key_path_card)
        self.auth_detail_layout.addWidget(self.password_card)
        conn_layout.addLayout(self.auth_detail_layout)

        # 高级选项折叠
        advanced_header = QHBoxLayout()
        advanced_header.addWidget(self.advanced_toggle)
        advanced_header.addWidget(self.advanced_label)
        advanced_header.addStretch()
        conn_layout.addLayout(advanced_header)

        # 高级选项内容 (默认折叠)
        advanced_layout = QVBoxLayout(self.advanced_widget)
        advanced_layout.setSpacing(8)
        advanced_layout.setContentsMargins(24, 0, 0, 0)
        advanced_layout.addWidget(self.timeout_card)
        advanced_layout.addWidget(self.command_timeout_card)
        advanced_layout.addWidget(self.host_key_policy_card)
        self.advanced_widget.setVisible(False)
        conn_layout.addWidget(self.advanced_widget)

        left_panel.addWidget(self.connection_card)

        # 工作区配置区域
        ws_layout = QVBoxLayout(self.workspace_card)
        ws_layout.setSpacing(8)
        ws_layout.setContentsMargins(12, 12, 12, 12)
        ws_layout.addWidget(StrongBodyLabel(self.tr("📁 工作区设置")))
        ws_layout.addWidget(self.workspace_dir_card)
        left_panel.addWidget(self.workspace_card)

        # 左栏底部按钮
        btn_layout = QHBoxLayout()
        btn_layout.addWidget(self.test_connection_btn)
        btn_layout.addStretch()
        btn_layout.addWidget(self.save_button)
        left_panel.addLayout(btn_layout)

        left_panel.addStretch()
        content_layout.addLayout(left_panel, 45)

        # ===== 右栏: 状态与操作 =====
        right_panel = QVBoxLayout()
        right_panel.setSpacing(12)
        right_panel.setAlignment(Qt.AlignmentFlag.AlignTop)

        # 状态卡片
        status_layout = QVBoxLayout(self.status_card)
        status_layout.setSpacing(8)
        status_layout.setContentsMargins(12, 12, 12, 12)
        status_layout.addWidget(self.status_title)
        status_layout.addWidget(self.status_detail)
        right_panel.addWidget(self.status_card)

        # 快捷操作卡片
        quick_layout = QVBoxLayout(self.quick_action_card)
        quick_layout.setSpacing(8)
        quick_layout.setContentsMargins(12, 12, 12, 12)
        quick_layout.addWidget(self.quick_action_title)

        quick_btn_layout = QHBoxLayout()
        quick_btn_layout.setSpacing(8)
        quick_btn_layout.addWidget(self.start_btn)
        quick_btn_layout.addWidget(self.stop_btn)
        quick_btn_layout.addWidget(self.restart_btn)
        quick_btn_layout.addWidget(self.log_btn)
        quick_layout.addLayout(quick_btn_layout)
        right_panel.addWidget(self.quick_action_card)

        # 部署流程卡片
        deploy_layout = QVBoxLayout(self.deploy_card)
        deploy_layout.setSpacing(8)
        deploy_layout.setContentsMargins(12, 12, 12, 12)
        deploy_layout.addWidget(self.deploy_title)

        deploy_btn_layout = QHBoxLayout()
        deploy_btn_layout.setSpacing(8)
        deploy_btn_layout.addWidget(self.probe_btn)
        deploy_btn_layout.addWidget(self.init_btn)
        deploy_btn_layout.addWidget(self.sync_btn)
        deploy_btn_layout.addStretch()
        deploy_btn_layout.addWidget(self.deploy_btn)
        deploy_layout.addLayout(deploy_btn_layout)
        right_panel.addWidget(self.deploy_card)

        right_panel.addStretch()
        content_layout.addLayout(right_panel, 55)

        main_layout.addLayout(content_layout, 1)
        self.setLayout(main_layout)

        # 设置固定大小确保不超出
        self.setMinimumSize(self.AVAILABLE_WIDTH, self.AVAILABLE_HEIGHT)

    def _connect_signals(self) -> None:
        """连接信号槽。"""
        # 认证方式切换
        self.auth_method_card.comboBox.currentTextChanged.connect(self._on_auth_method_changed)

        # 高级选项折叠
        self.advanced_toggle.clicked.connect(self._toggle_advanced)

        # 操作按钮
        self.save_button.clicked.connect(self._on_save)
        self.test_connection_btn.clicked.connect(self._on_test_connection)
        self.start_btn.clicked.connect(self._on_start)
        self.stop_btn.clicked.connect(self._on_stop)
        self.restart_btn.clicked.connect(self._on_restart)
        self.log_btn.clicked.connect(self._on_view_log)
        self.probe_btn.clicked.connect(self._on_probe)
        self.init_btn.clicked.connect(self._on_init)
        self.sync_btn.clicked.connect(self._on_sync)
        self.deploy_btn.clicked.connect(self._on_deploy)

    def _toggle_advanced(self) -> None:
        """切换高级选项显示。"""
        visible = not self.advanced_widget.isVisible()
        self.advanced_widget.setVisible(visible)
        self.advanced_toggle.setIcon(FI.CHEVRON_DOWN if visible else FI.CHEVRON_RIGHT)

    def _on_auth_method_changed(self, method: str) -> None:
        """认证方式切换时更新UI。"""
        is_key = method == "key"
        self.private_key_path_card.setVisible(is_key)
        self.password_card.setVisible(not is_key)

    def fill_config(self) -> None:
        """填充配置值。"""
        self.host_card.fill_value(cfg.get(cfg.remote_host) or "")
        self.port_card.fill_value(str(cfg.get(cfg.remote_port)))
        self.username_card.fill_value(cfg.get(cfg.remote_username) or "")
        self.auth_method_card.fill_value(cfg.get(cfg.remote_auth_method) or "key")
        self.private_key_path_card.fill_value(cfg.get(cfg.remote_private_key_path) or "")
        self.password_card.clear()
        self.workspace_dir_card.fill_value(cfg.get(cfg.remote_workspace_dir) or "$HOME/Napcat")
        self.timeout_card.fill_value(str(cfg.get(cfg.remote_connect_timeout)))
        self.command_timeout_card.fill_value(str(cfg.get(cfg.remote_command_timeout)))
        self.host_key_policy_card.fill_value(cfg.get(cfg.remote_host_key_policy) or "reject")

        # 初始化认证方式UI
        self._on_auth_method_changed(self.auth_method_card.get_value())

    def _collect_credentials(self) -> SSHCredentials:
        """收集凭据配置。"""
        return SSHCredentials(
            host=self.host_card.get_value().strip(),
            port=int(self.port_card.get_value() or 22),
            username=self.username_card.get_value().strip(),
            auth_method=self.auth_method_card.get_value(),
            password=self.password_card.get_value() if self.auth_method_card.get_value() == "password" else None,
            private_key_path=self.private_key_path_card.get_value() if self.auth_method_card.get_value() == "key" else None,
            connect_timeout=float(self.timeout_card.get_value() or 10),
            command_timeout=float(self.command_timeout_card.get_value() or 20),
            host_key_policy=self.host_key_policy_card.get_value(),
        )

    def _run_async_task(self, action_name: str, handler) -> None:
        """运行异步任务。"""
        task = RemoteActionTask(action_name, handler)
        task.success_signal.connect(self._on_task_success)
        task.error_signal.connect(self._on_task_error)
        QThreadPool.globalInstance().start(task)

    @Slot(str, str)
    def _on_task_success(self, action_name: str, message: str) -> None:
        """任务成功回调。"""
        success_bar(message, title=action_name, parent=self)
        logger.info(f"远程操作成功: {action_name}", log_source=LogSource.UI)

    @Slot(str, str)
    def _on_task_error(self, action_name: str, message: str) -> None:
        """任务失败回调。"""
        error_bar(message, title=f"{action_name}失败", parent=self)
        logger.error(f"远程操作失败: {action_name}", log_source=LogSource.UI)

    def _update_status_display(self, connected: bool, details: dict = None) -> None:
        """更新状态显示。"""
        if connected:
            self.status_indicator.setText(self.tr("● 已连接"))
            self.status_indicator.setStyleSheet("color: #52c41a;")
        else:
            self.status_indicator.setText(self.tr("● 未连接"))
            self.status_indicator.setStyleSheet("color: #999;")

        if details:
            text = self.tr(
                "连接状态: {status}\n"
                "系统: {os}\n"
                "架构: {arch}\n"
                "QQ版本: {qq}\n"
                "NapCat: {napcat}"
            ).format(
                status=details.get("status", "--"),
                os=details.get("os", "--"),
                arch=details.get("arch", "--"),
                qq=details.get("qq", "--"),
                napcat=details.get("napcat", "--"),
            )
            self.status_detail.setText(text)

    # ========== 事件处理 ==========
    def _on_save(self) -> None:
        """保存配置。"""
        try:
            cfg.set(cfg.remote_host, self.host_card.get_value().strip())
            cfg.set(cfg.remote_port, int(self.port_card.get_value() or 22))
            cfg.set(cfg.remote_username, self.username_card.get_value().strip())
            cfg.set(cfg.remote_auth_method, self.auth_method_card.get_value())
            cfg.set(cfg.remote_private_key_path, self.private_key_path_card.get_value())
            cfg.set(cfg.remote_workspace_dir, self.workspace_dir_card.get_value().strip())
            cfg.set(cfg.remote_connect_timeout, int(self.timeout_card.get_value() or 10))
            cfg.set(cfg.remote_command_timeout, int(self.command_timeout_card.get_value() or 20))
            cfg.set(cfg.remote_host_key_policy, self.host_key_policy_card.get_value())
            success_bar(self.tr("配置已保存"), parent=self)
            if self.auth_method_card.get_value() == "password":
                info_bar(self.tr("密码不会写入配置文件，仅用于当前会话"), parent=self)
        except Exception as e:
            error_bar(str(e), parent=self)

    def _on_test_connection(self) -> None:
        """测试连接。"""
        def handler():
            credentials = self._collect_credentials()
            credentials.validate()
            with RemoteManager(credentials) as manager:
                probe = manager.probe_environment()
                self._update_status_display(True, {
                    "status": "已连接",
                    "os": probe.os_name,
                    "arch": probe.architecture,
                    "qq": "--",
                    "napcat": "--",
                })
                return self.tr(f"连接成功: {probe.os_name} {probe.architecture}")
        self._run_async_task(self.tr("测试连接"), handler)

    def _on_start(self) -> None:
        """启动服务。"""
        def handler():
            credentials = self._collect_credentials()
            with RemoteManager(credentials) as manager:
                manager.start(f'cd "$HOME/Napcat" && ./napcat.sh start')
                return self.tr("NapCat 已启动")
        self._run_async_task(self.tr("启动服务"), handler)

    def _on_stop(self) -> None:
        """停止服务。"""
        def handler():
            credentials = self._collect_credentials()
            with RemoteManager(credentials) as manager:
                manager.stop()
                return self.tr("NapCat 已停止")
        self._run_async_task(self.tr("停止服务"), handler)

    def _on_restart(self) -> None:
        """重启服务。"""
        def handler():
            credentials = self._collect_credentials()
            with RemoteManager(credentials) as manager:
                manager.restart(f'cd "$HOME/Napcat" && ./napcat.sh restart')
                return self.tr("NapCat 已重启")
        self._run_async_task(self.tr("重启服务"), handler)

    def _on_view_log(self) -> None:
        """查看日志。"""
        def handler():
            credentials = self._collect_credentials()
            with RemoteManager(credentials) as manager:
                log_tail = manager.tail_log(lines=50)
                logger.info(f"远端日志:\n{log_tail.content}", log_source=LogSource.CORE)
                return self.tr(f"已读取 {log_tail.lines} 行日志")
        self._run_async_task(self.tr("查看日志"), handler)

    def _on_probe(self) -> None:
        """环境探测。"""
        def handler():
            credentials = self._collect_credentials()
            with RemoteManager(credentials) as manager:
                probe = manager.probe_environment()
                return self.tr(f"环境: {probe.os_name} {probe.architecture}, bash={probe.has_bash}")
        self._run_async_task(self.tr("环境探测"), handler)

    def _on_init(self) -> None:
        """初始化工作区。"""
        def handler():
            credentials = self._collect_credentials()
            with RemoteManager(credentials) as manager:
                manager.initialize_workspace()
                return self.tr("工作区初始化完成")
        self._run_async_task(self.tr("初始化"), handler)

    def _on_sync(self) -> None:
        """同步配置。"""
        def handler():
            credentials = self._collect_credentials()
            with RemoteManager(credentials) as manager:
                result = manager.export_and_upload_current_config()
                return self.tr(f"已同步 {result.exported_bot_count} 个 Bot 配置")
        self._run_async_task(self.tr("同步配置"), handler)

    def _on_deploy(self) -> None:
        """部署 NapCat。"""
        def handler():
            credentials = self._collect_credentials()
            with RemoteManager(credentials) as manager:
                sync_result = manager.export_and_upload_current_config()
                script_path = manager.deployment.upload_deploy_script()
                deploy_result = manager.deployment.run_deploy_script(script_path)
                if deploy_result.script_result.ok:
                    return self.tr(f"部署完成，已同步 {sync_result.exported_bot_count} 个 Bot 配置")
                else:
                    raise RuntimeError(deploy_result.script_result.stderr)
        self._run_async_task(self.tr("部署 NapCat"), handler)

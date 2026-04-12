# -*- coding: utf-8 -*-
# 标准库导入
from __future__ import annotations

# 第三方库导入
from PySide6.QtCore import QObject, QRunnable, Qt, QThreadPool, Signal
from PySide6.QtWidgets import QGridLayout, QLineEdit, QWidget
from qfluentwidgets import ExpandLayout, FluentIcon as FI, PushButton, ScrollArea, SettingCard, SettingCardGroup

# 项目内模块导入
from src.core.config import cfg
from src.core.logging import LogSource, logger
from src.core.remote import RemoteManager, SSHCredentials
from src.ui.components.info_bar import error_bar, info_bar, success_bar, warning_bar
from src.ui.components.input_card.generic_card import ComboBoxConfigCard, LineEditConfigCard, SwitchConfigCard


class ActionButtonCard(SettingCard):
    """带单个操作按钮的设置卡片。"""

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

    success_signal = Signal(str)
    error_signal = Signal(str)

    def __init__(self, handler) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self._handler = handler

    def run(self) -> None:
        try:
            message = self._handler()
            self.success_signal.emit(message)
        except Exception as exc:  # pragma: no cover - UI 异步异常统一反馈
            self.error_signal.emit(f"{type(exc).__name__}: {exc}")


class Remote(ScrollArea):
    """远程管理设置页。"""

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        self.view = QWidget()
        self.expand_layout = ExpandLayout(self.view)

        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.view.setObjectName("SetupView")
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setObjectName("SetupRemoteWidget")

        self._create_config_cards()
        self._set_layout()

    def _create_config_cards(self) -> None:
        self.connection_group = SettingCardGroup(title=self.tr("连接与认证"), parent=self.view)
        self.host_group = SettingCardGroup(title=self.tr("远端工作区"), parent=self.view)
        self.action_group = SettingCardGroup(title=self.tr("远程操作"), parent=self.view)

        self.enable_card = SwitchConfigCard(FI.GLOBE, self.tr("启用远程模式"), parent=self.connection_group)
        self.host_card = LineEditConfigCard(FI.GLOBE, self.tr("SSH 主机"), "example.com", parent=self.connection_group)
        self.port_card = LineEditConfigCard(FI.INFO, self.tr("SSH 端口"), "22", parent=self.connection_group)
        self.username_card = LineEditConfigCard(FI.CONNECT, self.tr("用户名"), "root", parent=self.connection_group)
        self.auth_method_card = ComboBoxConfigCard(
            FI.DOCUMENT,
            self.tr("认证方式"),
            ["key", "password"],
            parent=self.connection_group,
        )
        self.private_key_path_card = LineEditConfigCard(
            FI.DOCUMENT,
            self.tr("私钥路径"),
            "C:/Users/NAME/.ssh/id_ed25519",
            parent=self.connection_group,
        )
        self.password_card = LineEditConfigCard(
            FI.VPN,
            self.tr("临时密码"),
            self.tr("仅用于本次连接，不会保存"),
            parent=self.connection_group,
        )
        self.password_card.lineEdit.setEchoMode(QLineEdit.EchoMode.Password)
        self.allow_agent_card = SwitchConfigCard(FI.ROBOT, self.tr("允许使用 SSH Agent"), parent=self.connection_group)
        self.look_for_keys_card = SwitchConfigCard(FI.SEARCH, self.tr("允许自动查找本地密钥"), parent=self.connection_group)
        self.host_key_policy_card = ComboBoxConfigCard(
            FI.VPN,
            self.tr("主机指纹策略"),
            ["reject", "warning", "auto_add"],
            parent=self.connection_group,
        )
        self.connect_timeout_card = LineEditConfigCard(
            FI.INFO,
            self.tr("连接超时(秒)"),
            "10",
            parent=self.connection_group,
        )
        self.command_timeout_card = LineEditConfigCard(
            FI.INFO,
            self.tr("命令超时(秒)"),
            "20",
            parent=self.connection_group,
        )

        self.workspace_dir_card = LineEditConfigCard(
            FI.FOLDER,
            self.tr("远端工作目录"),
            "$HOME/NapCatCore",
            parent=self.host_group,
        )

        self.test_connection_card = ActionButtonCard(
            icon=FI.GLOBE,
            title=self.tr("测试连接"),
            content=self.tr("验证 SSH 连接、认证信息与主机指纹策略是否可用"),
            button_text=self.tr("测试"),
            callback=self._test_connection,
            parent=self.action_group,
        )
        self.probe_environment_card = ActionButtonCard(
            icon=FI.SEARCH,
            title=self.tr("环境探测"),
            content=self.tr("检测远端系统类型、架构以及 bash/tar/unzip 是否可用"),
            button_text=self.tr("探测"),
            callback=self._probe_environment,
            parent=self.action_group,
        )
        self.initialize_workspace_card = ActionButtonCard(
            icon=FI.FOLDER_ADD,
            title=self.tr("初始化工作区"),
            content=self.tr("创建远端运行目录、配置目录、日志目录与临时目录"),
            button_text=self.tr("初始化"),
            callback=self._initialize_workspace,
            parent=self.action_group,
        )

        self.fill_config()
        self._sync_auth_mode_ui()
        self.auth_method_card.comboBox.currentTextChanged.connect(lambda _text: self._sync_auth_mode_ui())

    def _set_layout(self) -> None:
        for card in (
            self.enable_card,
            self.host_card,
            self.port_card,
            self.username_card,
            self.auth_method_card,
            self.private_key_path_card,
            self.password_card,
            self.allow_agent_card,
            self.look_for_keys_card,
            self.host_key_policy_card,
            self.connect_timeout_card,
            self.command_timeout_card,
        ):
            self.connection_group.addSettingCard(card)

        self.host_group.addSettingCard(self.workspace_dir_card)

        for card in (
            self.test_connection_card,
            self.probe_environment_card,
            self.initialize_workspace_card,
        ):
            self.action_group.addSettingCard(card)

        self.expand_layout.addWidget(self.connection_group)
        self.expand_layout.addWidget(self.host_group)
        self.expand_layout.addWidget(self.action_group)
        self.expand_layout.setContentsMargins(0, 0, 0, 0)
        self.view.setLayout(self.expand_layout)

    def fill_config(self) -> None:
        self.enable_card.fill_value(cfg.get(cfg.remote_enabled))
        self.host_card.fill_value(cfg.get(cfg.remote_host))
        self.port_card.fill_value(str(cfg.get(cfg.remote_port)))
        self.username_card.fill_value(cfg.get(cfg.remote_username))
        self.auth_method_card.fill_value(cfg.get(cfg.remote_auth_method))
        self.private_key_path_card.fill_value(cfg.get(cfg.remote_private_key_path))
        self.password_card.clear()
        self.allow_agent_card.fill_value(cfg.get(cfg.remote_allow_agent))
        self.look_for_keys_card.fill_value(cfg.get(cfg.remote_look_for_keys))
        self.host_key_policy_card.fill_value(cfg.get(cfg.remote_host_key_policy))
        self.connect_timeout_card.fill_value(str(cfg.get(cfg.remote_connect_timeout)))
        self.command_timeout_card.fill_value(str(cfg.get(cfg.remote_command_timeout)))
        self.workspace_dir_card.fill_value(cfg.get(cfg.remote_workspace_dir))

    def _sync_auth_mode_ui(self) -> None:
        is_key_mode = self.auth_method_card.get_value() == "key"
        self.private_key_path_card.lineEdit.setEnabled(is_key_mode)
        self.password_card.lineEdit.setEnabled(not is_key_mode)

    def _collect_config_values(self) -> list[tuple[object, object]]:
        port = int(self.port_card.get_value())
        connect_timeout = int(self.connect_timeout_card.get_value())
        command_timeout = int(self.command_timeout_card.get_value())
        values = [
            (cfg.remote_enabled, self.enable_card.get_value()),
            (cfg.remote_host, self.host_card.get_value().strip()),
            (cfg.remote_port, port),
            (cfg.remote_username, self.username_card.get_value().strip()),
            (cfg.remote_auth_method, self.auth_method_card.get_value()),
            (cfg.remote_private_key_path, self.private_key_path_card.get_value().strip()),
            (cfg.remote_allow_agent, self.allow_agent_card.get_value()),
            (cfg.remote_look_for_keys, self.look_for_keys_card.get_value()),
            (cfg.remote_host_key_policy, self.host_key_policy_card.get_value()),
            (cfg.remote_connect_timeout, connect_timeout),
            (cfg.remote_command_timeout, command_timeout),
            (cfg.remote_workspace_dir, self.workspace_dir_card.get_value().strip()),
        ]
        for item, value in values:
            if not item.validator.validate(value):
                raise ValueError(f"配置项 {item.key} 的值无效")
        return values

    def _build_runtime_credentials(self) -> SSHCredentials:
        password = self.password_card.get_value()
        credentials = SSHCredentials(
            host=self.host_card.get_value().strip(),
            port=int(self.port_card.get_value()),
            username=self.username_card.get_value().strip(),
            auth_method=self.auth_method_card.get_value(),
            password=password.strip() or None,
            private_key_path=self.private_key_path_card.get_value().strip() or None,
            connect_timeout=float(self.connect_timeout_card.get_value()),
            command_timeout=float(self.command_timeout_card.get_value()),
            host_key_policy=self.host_key_policy_card.get_value(),
            allow_agent=self.allow_agent_card.get_value(),
            look_for_keys=self.look_for_keys_card.get_value(),
        )
        credentials.validate()
        return credentials

    def _build_remote_manager(self) -> RemoteManager:
        return RemoteManager(self._build_runtime_credentials(), cfg.build_linux_core_paths())

    def save_config(self) -> bool:
        try:
            values = self._collect_config_values()
            for item, value in values:
                cfg.set(item, value)
            logger.info(
                (
                    "远程页配置已保存: "
                    f"enabled={self.enable_card.get_value()}, host={self.host_card.get_value().strip()}, "
                    f"username={self.username_card.get_value().strip()}, auth_method={self.auth_method_card.get_value()}, "
                    f"host_key_policy={self.host_key_policy_card.get_value()}"
                ),
                log_source=LogSource.UI,
            )
            success_bar(self.tr("远程配置已保存"), parent=self)
            if self.auth_method_card.get_value() == "password":
                info_bar(self.tr("密码不会写入配置文件，仅用于当前会话操作"), parent=self)
            return True
        except (TypeError, ValueError) as exc:
            logger.warning(f"远程配置保存失败: {exc}", log_source=LogSource.UI)
            warning_bar(self.tr("远程配置保存失败，请检查输入"), parent=self)
            return False

    def _run_remote_task(self, handler) -> None:
        if not self.save_config():
            return
        try:
            self._build_runtime_credentials()
        except ValueError as exc:
            warning_bar(str(exc), parent=self)
            return

        task = RemoteActionTask(handler)
        task.success_signal.connect(lambda msg: success_bar(self.tr(msg), parent=self))
        task.error_signal.connect(lambda msg: error_bar(msg, parent=self))
        QThreadPool.globalInstance().start(task)

    def _test_connection(self) -> None:
        logger.info("准备测试远程 SSH 连接", log_source=LogSource.UI)

        def handler() -> str:
            with self._build_remote_manager() as manager:
                probe = manager.probe_environment()
            return (
                f"连接成功: os={probe.os_name or 'unknown'}, arch={probe.architecture or 'unknown'}, "
                f"bash={probe.has_bash}, tar={probe.has_tar}, unzip={probe.has_unzip}"
            )

        self._run_remote_task(handler)

    def _probe_environment(self) -> None:
        logger.info("准备执行远端环境探测", log_source=LogSource.UI)

        def handler() -> str:
            with self._build_remote_manager() as manager:
                probe = manager.probe_environment()
            return (
                f"环境探测完成: os={probe.os_name or 'unknown'}, arch={probe.architecture or 'unknown'}, "
                f"bash={probe.has_bash}, tar={probe.has_tar}, unzip={probe.has_unzip}"
            )

        self._run_remote_task(handler)

    def _initialize_workspace(self) -> None:
        logger.info("准备初始化远端工作区", log_source=LogSource.UI)

        def handler() -> str:
            with self._build_remote_manager() as manager:
                manager.initialize_workspace()
            return self.tr("远端工作区初始化完成")

        self._run_remote_task(handler)

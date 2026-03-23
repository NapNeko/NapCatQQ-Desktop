# -*- coding: utf-8 -*-
"""
Bot 配置页面
"""
# 第三方库导入
from creart import it
from qfluentwidgets import ExpandLayout, FlowLayout, FluentIcon, PushButton, ScrollArea, SettingCard, SettingCardGroup
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config.config_model import (
    AdvancedConfig,
    BotConfig,
    ConnectConfig,
    HttpClientsConfig,
    HttpServersConfig,
    HttpSseServersConfig,
    NetworkBaseConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
)
from src.ui.common.icon import NapCatDesktopIcon
from src.ui.components.input_card import ComboBoxConfigCard, LineEditConfigCard, SwitchConfigCard, ShowDialogCard
from src.ui.page.bot_page.widget import (
    HttpClientConfigCard,
    HttpServerConfigCard,
    HttpSSEConfigCard,
    WebsocketClientConfigCard,
    WebsocketServersConfigCard,
    AdvancedBackendDialog,
    AutoRestartDialog,
)


class BotConfigWidget(ScrollArea):
    """Bot 设置页面"""

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        # 创建控件
        self.view = QWidget()

        self.bot_name_card = LineEditConfigCard(
            icon=FluentIcon.ROBOT,
            title=self.tr("Bot 名称"),
            content=self.tr("设置机器人的名称"),
            placeholder_text=self.tr("QIAO Bot"),
            parent=self.view,
        )
        self.bot_qq_id_card = LineEditConfigCard(
            icon=NapCatDesktopIcon.QQ,
            title=self.tr("Bot QQ"),
            content=self.tr("设置机器人 QQ 号, 不能为空"),
            placeholder_text=self.tr("114514"),
            parent=self.view,
        )
        self.music_sign_url_card = LineEditConfigCard(
            icon=FluentIcon.MUSIC,
            title=self.tr("音乐签名URL"),
            content=self.tr("用于处理音乐相关请求, 为空则使用默认签名服务器"),
            placeholder_text=self.tr("https://example.com/music"),
            parent=self.view,
        )
        self.auto_restart_dialog_card = ShowDialogCard(
            dialog=AutoRestartDialog,
            icon=FluentIcon.IOT,
            title=self.tr("自动重启"),
            content=self.tr("设置自动重启 Bot 的相关选项"),
            parent=self.view,
        )
        self.offline_auto_restart_card = SwitchConfigCard(
            icon=FluentIcon.HISTORY,
            title=self.tr("掉线重启"),
            content=self.tr("当 Bot 掉线时自动重启, 与掉线通知可以配合使用"),
            parent=self.view,
        )

        # 设置属性
        self._config = None
        self.cards = [
            self.bot_name_card,
            self.bot_qq_id_card,
            self.music_sign_url_card,
            self.auto_restart_dialog_card,
            self.offline_auto_restart_card,
        ]

        # 设置控件
        self.setWidget(self.view)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setWidgetResizable(True)

        # 创建布局
        self.card_layout = ExpandLayout(self.view)
        self.card_layout.setContentsMargins(0, 0, 0, 0)
        self.card_layout.setSpacing(2)
        for card in self.cards:
            self.card_layout.addWidget(card)
        self.adjustSize()

    # ==================== 公共方法 ====================
    def get_config(self) -> BotConfig:
        """获取配置"""
        return BotConfig(
            **{
                "name": self.bot_name_card.get_value(),
                "QQID": self.bot_qq_id_card.get_value(),
                "musicSignUrl": self.music_sign_url_card.get_value(),
                "autoRestartSchedule": self.auto_restart_dialog_card.get_value(),
                "offlineAutoRestart": self.offline_auto_restart_card.get_value(),
            }
        )

    def fill_config(self, config: BotConfig | None = None) -> None:
        """填充配置"""
        if config is None:
            return

        self._config = config
        self.bot_name_card.fill_value(self._config.name)
        self.bot_qq_id_card.fill_value(self._config.QQID)
        self.bot_qq_id_card.setEnabled(False)
        self.music_sign_url_card.fill_value(self._config.musicSignUrl)
        self.auto_restart_dialog_card.fill_value(self._config.autoRestartSchedule)
        self.offline_auto_restart_card.fill_value(self._config.offlineAutoRestart)

    def clear_config(self) -> None:
        """清空配置"""
        for card in self.cards:
            card.clear()
        self.bot_qq_id_card.setEnabled(True)

    # ==================== 重写方法 ====================
    def adjustSize(self) -> None:
        """重写方法以调整控件大小适应内容高度"""
        self.resize(self.width(), self.card_layout.heightForWidth(self.width()) + 46)


class ConnectConfigWidget(ScrollArea):
    """Bot 连接设置页面"""

    CONFIG_KEY_AND_CARD_DICT = {
        "httpServers": HttpServerConfigCard,
        "httpSseServers": HttpSSEConfigCard,
        "httpClients": HttpClientConfigCard,
        "websocketServers": WebsocketServersConfigCard,
        "websocketClients": WebsocketClientConfigCard,
    }

    CINFIG_AND_CARD_DICT = {
        HttpServersConfig: HttpServerConfigCard,
        HttpSseServersConfig: HttpSSEConfigCard,
        HttpClientsConfig: HttpClientConfigCard,
        WebsocketServersConfig: WebsocketServersConfigCard,
        WebsocketClientsConfig: WebsocketClientConfigCard,
    }

    CONFIG_KEY_NAME = [
        "httpServers",
        "httpSseServers",
        "httpClients",
        "websocketServers",
        "websocketClients",
    ]

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        # 设置属性
        self.cards = []

        # 创建控件
        self.view = QWidget()

        # 设置属性
        self._config = None

        # 设置控件
        self.setWidget(self.view)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setWidgetResizable(True)

        # 创建布局
        self.card_layout = FlowLayout(self.view)
        self.card_layout.setContentsMargins(0, 0, 0, 0)
        self.card_layout.setSpacing(8)

    # ==================== 公共方法 ====================
    def add_card(self, config: NetworkBaseConfig) -> None:
        """添加卡片到列表"""
        if card_class := self.CINFIG_AND_CARD_DICT.get(type(config)):
            card = card_class(config, self.view)
            card.remove_signal.connect(self.remove_card)
            self.cards.append(card)
            self.card_layout.addWidget(card)
            self.card_layout.update()
            self.updateGeometry()

    def has_config_name(self, name: str) -> bool:
        """检查是否已存在同名网络配置。"""
        normalized_name = name.strip().casefold()
        if not normalized_name:
            return False

        for card in self.cards:
            if card.config.name.strip().casefold() == normalized_name:
                return True
        return False

    def remove_card(self, config: NetworkBaseConfig) -> None:
        """从列表删除卡片"""
        for card in self.cards:
            if card.config != config:
                continue
            self.card_layout.removeWidget(card)
            card.setParent(None)
            card.deleteLater()
            self.cards.remove(card)

    def get_config(self) -> ConnectConfig:
        """获取配置"""
        config_data = {
            key: [card.get_config() for card in self.cards if isinstance(card, card_type)]
            for key, card_type in self.CONFIG_KEY_AND_CARD_DICT.items()
        }
        config_data["plugins"] = []

        return ConnectConfig(**config_data)

    def fill_config(self, config: ConnectConfig | None = None) -> None:
        """填充配置"""
        if config is None:
            return

        self.clear_config()

        for attr in self.CONFIG_KEY_NAME:
            for _config in getattr(config, attr, []):
                self.add_card(_config)

    def clear_config(self) -> None:
        """清空配置"""
        self.cards.clear()
        self.card_layout.takeAllWidgets()


class AdvancedConfigWidget(ScrollArea):
    """Bot 高级设置页面"""

    class BackendConfigCard(SettingCard):
        """底层配置入口卡片。"""

        def __init__(self, parent: QWidget | None = None) -> None:
            super().__init__(
                FluentIcon.DEVELOPER_TOOLS,
                self.tr("底层与反检测"),
                self.tr("Packet、O3 Hook 与 bypass 等低频配置，默认情况下无需调整"),
                parent,
            )
            self._dialog: AdvancedBackendDialog | None = None
            self._config = AdvancedConfig()
            self.button = PushButton(self.tr("展开配置"), self)
            self.button.clicked.connect(self.slot_show_dialog)
            self.hBoxLayout.addWidget(self.button, 0, Qt.AlignmentFlag.AlignRight)
            self.hBoxLayout.addSpacing(16)
            self._refresh_summary()

        def _ensure_dialog(self) -> AdvancedBackendDialog:
            """惰性创建对话框，避免把对话框控件挂进当前页面。"""
            if self._dialog is None:
                from src.ui.window.main_window import MainWindow

                self._dialog = AdvancedBackendDialog(it(MainWindow))
            return self._dialog

        def _refresh_summary(self) -> None:
            config = self._config
            enabled_bypass_count = sum(
                [
                    config.bypass.hook,
                    config.bypass.window,
                    config.bypass.module,
                    config.bypass.process,
                    config.bypass.container,
                    config.bypass.js,
                ]
            )
            packet_server_text = self.tr("已配置") if config.packetServer else self.tr("默认")
            self.setContent(
                self.tr(
                    "Packet={0} · Server={1} · O3 Hook={2} · 反检测启用 {3}/6"
                ).format(config.packetBackend, packet_server_text, config.o3HookMode, enabled_bypass_count)
            )

        def slot_show_dialog(self) -> None:
            dialog = self._ensure_dialog()
            dialog.fill_config(self._config)
            if dialog.exec():
                self._config = dialog.get_config()
                self._refresh_summary()
                return

        def get_value(self) -> AdvancedConfig:
            return self._config

        def fill_value(self, config: AdvancedConfig | None = None) -> None:
            if config is None:
                return

            self._config = config.model_copy(deep=True)
            if self._dialog is not None:
                self._dialog.fill_config(self._config)
            self._refresh_summary()

        def clear(self) -> None:
            self._config = AdvancedConfig()
            if self._dialog is not None:
                self._dialog.clear_config()
            self._refresh_summary()

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        self.view = QWidget()
        self.expand_layout = ExpandLayout(self.view)
        self.runtime_group = SettingCardGroup(self.tr("运行与消息"), self.view)
        self.log_group = SettingCardGroup(self.tr("日志与诊断"), self.view)
        self.engine_group = SettingCardGroup(self.tr("底层配置"), self.view)

        self.auto_start_card = SwitchConfigCard(
            icon=FluentIcon.PLAY,
            title=self.tr("自动启动"),
            content=self.tr("是否在启动时自动启动 bot"),
            parent=self.runtime_group,
        )
        self.offline_notice_card = SwitchConfigCard(
            icon=FluentIcon.MEGAPHONE,
            title=self.tr("掉线通知"),
            content=self.tr("当Bot状态为 离线 时, 发送通知"),
            parent=self.runtime_group,
        )
        self.parse_mult_message_card = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("解析合并转发消息"),
            content=self.tr("是否解析合并转发消息"),
            parent=self.runtime_group,
        )
        self.local_file_to_url_card = SwitchConfigCard(
            icon=FluentIcon.SHARE,
            title=self.tr("LocalFile2Url"),
            content=self.tr("是否将本地文件转换为URL, 如果获取不到url则使用base64字段返回文件内容"),
            value=True,
            parent=self.runtime_group,
        )
        self.file_log_card = SwitchConfigCard(
            icon=FluentIcon.SAVE_AS,
            title=self.tr("文件日志"),
            content=self.tr("是否要将日志记录到文件"),
            parent=self.log_group,
        )
        self.console_log_card = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("控制台日志"),
            content=self.tr("是否启用控制台日志"),
            value=True,
            parent=self.log_group,
        )
        self.file_log_level_card = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("文件日志等级"),
            content=self.tr("设置文件日志输出等级"),
            texts=["debug", "info", "error"],
            parent=self.log_group,
        )
        self.console_level_card = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("控制台日志等级"),
            content=self.tr("设置控制台日志输出等级"),
            texts=["info", "debug", "error"],
            parent=self.log_group,
        )
        self.backend_config_card = self.BackendConfigCard(self.engine_group)

        self._config = None
        self.runtime_cards = [
            self.auto_start_card,
            self.offline_notice_card,
            self.parse_mult_message_card,
            self.local_file_to_url_card,
        ]
        self.log_cards = [
            self.file_log_card,
            self.console_log_card,
            self.file_log_level_card,
            self.console_level_card,
        ]
        self.cards = self.runtime_cards + self.log_cards + [self.backend_config_card]

        self.setWidget(self.view)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setWidgetResizable(True)
        self.view.setObjectName("BotAdvancedConfigView")

        for card in self.runtime_cards:
            self.runtime_group.addSettingCard(card)
        for card in self.log_cards:
            self.log_group.addSettingCard(card)
        self.engine_group.addSettingCard(self.backend_config_card)

        self.expand_layout.addWidget(self.runtime_group)
        self.expand_layout.addWidget(self.log_group)
        self.expand_layout.addWidget(self.engine_group)
        self.expand_layout.setContentsMargins(0, 0, 0, 0)
        self.view.setLayout(self.expand_layout)

        self.file_log_card.switchButton.checkedChanged.connect(self._sync_log_level_card_state)
        self.console_log_card.switchButton.checkedChanged.connect(self._sync_log_level_card_state)
        self._sync_log_level_card_state()
        self.adjustSize()

    # ==================== 公共方法 ====================
    def get_config(self) -> AdvancedConfig:
        """获取配置"""
        backend_config = self.backend_config_card.get_value()
        return AdvancedConfig(
            **{
                "autoStart": self.auto_start_card.get_value(),
                "offlineNotice": self.offline_notice_card.get_value(),
                "parseMultMsg": self.parse_mult_message_card.get_value(),
                "packetServer": backend_config.packetServer,
                "packetBackend": backend_config.packetBackend,
                "enableLocalFile2Url": self.local_file_to_url_card.get_value(),
                "fileLog": self.file_log_card.get_value(),
                "consoleLog": self.console_log_card.get_value(),
                "fileLogLevel": self.file_log_level_card.get_value(),
                "consoleLogLevel": self.console_level_card.get_value(),
                "o3HookMode": backend_config.o3HookMode,
                "bypass": backend_config.bypass,
            }
        )

    def fill_config(self, config: AdvancedConfig | None = None) -> None:
        """填充配置"""
        if config is None:
            return

        self._config = config
        self.auto_start_card.fill_value(self._config.autoStart)
        self.offline_notice_card.fill_value(self._config.offlineNotice)
        self.parse_mult_message_card.fill_value(self._config.parseMultMsg)
        self.local_file_to_url_card.fill_value(self._config.enableLocalFile2Url)
        self.file_log_card.fill_value(self._config.fileLog)
        self.console_log_card.fill_value(self._config.consoleLog)
        self.file_log_level_card.fill_value(self._config.fileLogLevel)
        self.console_level_card.fill_value(self._config.consoleLogLevel)
        self.backend_config_card.fill_value(self._config)
        self._sync_log_level_card_state()

    def clear_config(self) -> None:
        """清空配置"""
        for card in self.cards:
            card.clear()
        self._sync_log_level_card_state()

    def _sync_log_level_card_state(self, *_args) -> None:
        """根据日志开关同步日志等级输入的可编辑状态。"""
        self.file_log_level_card.setEnabled(self.file_log_card.get_value())
        self.console_level_card.setEnabled(self.console_log_card.get_value())

    # ==================== 重写方法 ====================
    def adjustSize(self) -> None:
        """重写方法以调整控件大小适应内容高度"""
        self.resize(self.width(), self.expand_layout.heightForWidth(self.width()) + 46)

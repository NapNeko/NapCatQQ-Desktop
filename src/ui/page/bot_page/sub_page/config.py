# -*- coding: utf-8 -*-
"""
Bot 配置页面
"""
# 标准库导入
from ast import Lambda
from enum import Enum
from turtle import TPen

# 第三方库导入
from qfluentwidgets import ExpandLayout, FlowLayout, FluentIcon, ScrollArea, SegmentedWidget, TransparentPushButton
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config.config_model import (
    BotConfig,
    Config,
    ConnectConfig,
    HttpClientsConfig,
    HttpServersConfig,
    HttpSseServersConfig,
    NetworkBaseConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
)
from src.ui.common.icon import NapCatDesktopIcon
from src.ui.components.input_card import ComboBoxConfigCard, LineEditConfigCard, SwitchConfigCard
from src.ui.components.stacked_widget import TransparentStackedWidget
from src.ui.page.add_page import card
from src.ui.page.bot_page.widget import (
    ConfigCardBase,
    HttpClientConfigCard,
    HttpServerConfigCard,
    HttpSSEConfigCard,
    WebsocketClientConfigCard,
    WebsocketServersConfigCard,
    ChooseConfigTypeDialog,
    HttpClientConfigDialog,
    HttpServerConfigDialog,
    HttpSSEServerConfigDialog,
    WebsocketClientConfigDialog,
    WebsocketServerConfigDialog,
)

from src.ui.page.bot_page.bot_page_enum import ConnectType


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
        # self.auto_restart_dialog = ShowDialogCard(
        #     dialog=AutoRestartDialog,
        #     icon=FluentIcon.IOT,
        #     title=self.tr("自动重启"),
        #     content=self.tr("设置自动重启 Bot 的相关选项"),
        #     parent=self.view,
        # )

        # 设置属性
        self._config = None
        self.cards = [getattr(self, attr) for attr in dir(self) if attr.endswith("_card")]

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
                "musicSignUrl": self.music_sign_url.get_value(),
                "autoRestartSchedule": self.auto_restart_dialog.get_value(),
            }
        )

    def fill_config(self, config: BotConfig | None = None) -> None:
        """填充配置"""
        if config is None:
            return

        self._config = config
        self.bot_name_card.fill_value(self._config.name)
        self.bot_qq_id_card.fill_value(self._config.QQID)
        self.music_sign_url_card.fill_value(self._config.musicSignUrl)

    def clear_config(self) -> None:
        """清空配置"""
        for card in self.cards:
            card.clear()

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

    def remove_card(self, config: NetworkBaseConfig) -> None:
        """从列表删除卡片"""
        for card in self.cards:
            if card.config != config:
                continue
            self.card_layout.removeWidget(card)
            card.setParent(None)
            card.deleteLater()
            self.cards.remove(card)

    def get_config(self) -> BotConfig:
        """获取配置"""
        config_data = {
            key: [card.get_value() for card in self.cards if isinstance(card, card_type)]
            for key, card_type in self.CONFIG_KEY_AND_CARD_DICT.items()
        }
        config_data["plugins"] = []

        return ConnectConfig(**config_data)

    def fill_config(self, config: BotConfig | None = None) -> None:
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

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        # 创建控件
        self.view = QWidget()

        self.auto_start_card = SwitchConfigCard(
            icon=FluentIcon.PLAY,
            title=self.tr("自动启动"),
            content=self.tr("是否在启动时自动启动 bot"),
            parent=self.view,
        )
        self.offline_notice_card = SwitchConfigCard(
            icon=FluentIcon.MEGAPHONE,
            title=self.tr("掉线通知"),
            content=self.tr("当Bot状态为 离线 时, 发送通知"),
            parent=self.view,
        )
        self.parse_mult_message_card = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("解析合并转发消息"),
            content=self.tr("是否解析合并转发消息"),
            parent=self.view,
        )
        self.packet_server_card = LineEditConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("Packet Server"),
            content=self.tr("设置 Packet Server 地址, 为空则使用默认值"),
            parent=self.view,
        )
        self.packet_backend_card = LineEditConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("Packet Backend"),
            content=self.tr("设置 Packet Backend, 为空则使用默认值"),
            parent=self.view,
        )
        self.local_file_to_url_card = SwitchConfigCard(
            icon=FluentIcon.SHARE,
            title=self.tr("LocalFile2Url"),
            content=self.tr("是否将本地文件转换为URL, 如果获取不到url则使用base64字段返回文件内容"),
            value=True,
            parent=self.view,
        )
        self.file_log_card = SwitchConfigCard(
            icon=FluentIcon.SAVE_AS,
            title=self.tr("文件日志"),
            content=self.tr("是否要将日志记录到文件"),
            parent=self.view,
        )
        self.console_log_card = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("控制台日志"),
            content=self.tr("是否启用控制台日志"),
            value=True,
            parent=self.view,
        )
        self.file_log_level_card = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("文件日志等级"),
            content=self.tr("设置文件日志输出等级"),
            texts=["debug", "info", "error"],
            parent=self.view,
        )
        self.console_level_card = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("控制台日志等级"),
            content=self.tr("设置控制台日志输出等级"),
            texts=["info", "debug", "error"],
            parent=self.view,
        )
        self.o3_hook_mode_card = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("O3 Hook 模式"),
            content=self.tr("设置 O3 Hook 模式"),
            texts=["0", "1"],
            parent=self.view,
        )

        # 设置属性
        self._config = None
        self.cards = [getattr(self, attr) for attr in dir(self) if attr.endswith("_card")]

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
                "autoStart": self.auto_start_card.get_value(),
                "offlineNotice": self.offline_notice_card.get_value(),
                "parseMultMsg": self.parse_mult_message_card.get_value(),
                "packetServer": self.packet_server_card.get_value(),
                "packetBackend": self.packet_backend_card.get_value(),
                "enableLocalFile2Url": self.local_file_to_url_card.get_value(),
                "fileLog": self.file_log_card.get_value(),
                "consoleLog": self.console_log_card.get_value(),
                "fileLogLevel": self.file_log_level_card.get_value(),
                "consoleLogLevel": self.console_level_card.get_value(),
                "o3HookMode": int(self.o3_hook_mode_card.get_value()),
            }
        )

    def fill_config(self, config: BotConfig | None = None) -> None:
        """填充配置"""
        if config is None:
            return

        self._config = config
        self.auto_start_card.fill_value(self._config.autoStart)
        self.offline_notice_card.fill_value(self._config.offlineNotice)
        self.parse_mult_message_card.fill_value(self._config.parseMultMsg)
        self.packet_server_card.fill_value(self._config.packetServer)
        self.packet_backend_card.fill_value(self._config.packetBackend)
        self.local_file_to_url_card.fill_value(self._config.enableLocalFile2Url)
        self.file_log_card.fill_value(self._config.fileLog)
        self.console_log_card.fill_value(self._config.consoleLog)
        self.file_log_level_card.fill_value(self._config.fileLogLevel)
        self.console_level_card.fill_value(self._config.consoleLogLevel)
        self.o3_hook_mode_card.fill_value(str(self._config.o3HookMode))

    def clear_config(self) -> None:
        """清空配置"""
        for card in self.cards:
            card.clear()

    # ==================== 重写方法 ====================
    def adjustSize(self) -> None:
        """重写方法以调整控件大小适应内容高度"""
        self.resize(self.width(), self.card_layout.heightForWidth(self.width()) + 46)


class ConfigPage(QWidget):
    """配置机器人页面"""

    CONNECT_TYPE_AND_DIALOG = {
        ConnectType.HTTP_SERVER: HttpServerConfigDialog,
        ConnectType.HTTP_SSE_SERVER: HttpSSEServerConfigDialog,
        ConnectType.HTTP_CLIENT: HttpClientConfigDialog,
        ConnectType.WEBSOCKET_SERVER: WebsocketServerConfigDialog,
        ConnectType.WEBSOCKET_CLIENT: WebsocketClientConfigDialog,
    }

    class PageEnum(Enum):
        """页面枚举"""

        BOT_WIDGET = 0
        CONNECT_WIDGET = 1
        ADVANCED_WIDGET = 2

    def __init__(self, parent: QWidget | None = None):
        """初始化页面"""
        super().__init__(parent)
        # 设置属性
        self._config = None

        # 创建控件
        self.piovt = SegmentedWidget(self)
        self.view = TransparentStackedWidget()
        self.bot_widget = BotConfigWidget(self)
        self.connect_widget = ConnectConfigWidget(self)
        self.advanced_widget = AdvancedConfigWidget(self)
        self.return_button = TransparentPushButton(FluentIcon.LEFT_ARROW, self.tr("返回"), self)
        self.add_connect_button = TransparentPushButton(FluentIcon.ADD, self.tr("添加"), self)

        # 设置控件
        self.view.addWidget(self.bot_widget)
        self.view.addWidget(self.connect_widget)
        self.view.addWidget(self.advanced_widget)
        self.view.setCurrentWidget(self.bot_widget)

        self.piovt.addItem(
            routeKey=f"bot_widget",
            text=self.tr("基本配置"),
            onClick=lambda: self.view.setCurrentWidget(self.bot_widget),
        )
        self.piovt.addItem(
            routeKey="connect_widget",
            text=self.tr("连接配置"),
            onClick=lambda: self.view.setCurrentWidget(self.connect_widget),
        )
        self.piovt.addItem(
            routeKey=f"advanced_widget",
            text=self.tr("高级配置"),
            onClick=lambda: self.view.setCurrentWidget(self.advanced_widget),
        )
        self.piovt.setCurrentItem("bot_widget")

        self.add_connect_button.hide()

        # 设置布局
        self.top_layout = QHBoxLayout()
        self.top_layout.setContentsMargins(0, 0, 0, 0)
        self.top_layout.addWidget(self.piovt)
        self.top_layout.addStretch(1)
        self.top_layout.addWidget(self.add_connect_button)
        self.top_layout.addWidget(self.return_button)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.addLayout(self.top_layout)
        self.v_box_layout.addWidget(self.view, 1)

        # 链接信号
        self.view.currentChanged.connect(self.slot_view_current_index_changed)
        self.add_connect_button.clicked.connect(self.slot_add_connect_button)
        self.return_button.clicked.connect(self.slot_return_button)

    # ==================== 公共函数===================
    def fill_config(self, config: Config | None = None):
        """填充配置"""
        if config is None:
            return

        self._config = config
        self.bot_widget.fill_config(self._config.bot)
        self.connect_widget.fill_config(self._config.connect)
        self.advanced_widget.fill_config(self._config.advanced)

    # ==================== 槽函数 ====================
    def slot_view_current_index_changed(self, index: int) -> None:
        """当 view 切换时更新 piovt 的选中状态

        Args:
            index (int): 当前索引
        """
        match self.PageEnum(index):
            case self.PageEnum.BOT_WIDGET:
                self.piovt.setCurrentItem("bot_widget")
                self.add_connect_button.hide()
            case self.PageEnum.CONNECT_WIDGET:
                self.piovt.setCurrentItem("connect_widget")
                self.add_connect_button.show()
            case self.PageEnum.ADVANCED_WIDGET:
                self.piovt.setCurrentItem("advanced_widget")
                self.add_connect_button.hide()

    def slot_return_button(self) -> None:
        """返回按钮槽函数"""
        # 项目内模块导入
        from src.ui.page.bot_page import BotPage

        BotPage().view.setCurrentWidget(BotPage().bot_list_page)

    def slot_add_connect_button(self) -> None:
        """添加连接配置按钮槽函数"""
        from src.ui.window.main_window import MainWindow

        if not (_choose_connect_type_box := ChooseConfigTypeDialog(MainWindow())).exec():
            # 获取用户选择的结果并判断是否取消
            return

        if (_connect_type := _choose_connect_type_box.get_value()) == ConnectType.NO_TYPE:
            # 判断用户选择的类型, 如果没有选择则直接退出
            return

        if not (_connect_config_box := self.CONNECT_TYPE_AND_DIALOG.get(_connect_type)(MainWindow())).exec():
            # 判断用户在配置的时候是否选择了取消
            return

        # 拿到配置项添加卡片
        self.connect_widget.add_card(_connect_config_box.get_config())

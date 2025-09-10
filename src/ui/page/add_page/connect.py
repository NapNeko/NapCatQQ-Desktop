# -*- coding: utf-8 -*-
# 标准库导入
import random
from typing import Dict, List

# 第三方库导入
from qfluentwidgets import BodyLabel, FlowLayout, ImageLabel, ScrollArea
from PySide6.QtCore import Qt
from PySide6.QtGui import QMovie
from PySide6.QtWidgets import QStackedWidget, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config.config_model import (
    ConnectConfig,
    HttpClientsConfig,
    HttpServersConfig,
    HttpSseServersConfig,
    NetworkBaseConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
)
from src.ui.page.add_page.card import (
    ConfigCardBase,
    HttpClientConfigCard,
    HttpServerConfigCard,
    HttpSSEConfigCard,
    WebsocketClientConfigCard,
    WebsocketServersConfigCard,
)
from src.ui.page.add_page.signal_bus import add_page_singal_bus


class ConnectWidget(QStackedWidget):
    """连接配置页面，包含默认空状态和配置卡片列表两种视图"""

    def __init__(self, parent: QWidget | None = None, config: ConnectConfig | None = None) -> None:
        """初始化连接配置控件

        Args:
            parent: 父控件
            config: 初始连接配置，如果为None则显示默认空页面
        """
        super().__init__(parent=parent)
        self.setObjectName("ConnectWidget")
        self.config = config

        self._setup_ui()
        self._setup_initial_state()

    def _setup_ui(self) -> None:
        """初始化界面控件"""
        self.defult_page = DefaultPage(self)
        self.card_list_page = CardListPage(self)

        self.addWidget(self.defult_page)
        self.addWidget(self.card_list_page)

    def _setup_initial_state(self) -> None:
        """根据配置设置初始显示页面"""
        if self.config is None:
            self.setCurrentWidget(self.defult_page)
            return

        # 检查是否有配置数据，有则显示卡片列表页面
        if (
            self.config.httpServers
            or self.config.httpSseServers
            or self.config.httpClients
            or self.config.websocketServers
            or self.config.websocketClients
        ):
            self.fill_value()
            self.setCurrentWidget(self.card_list_page)
        else:
            self.setCurrentWidget(self.defult_page)

    def add_card(self, config: NetworkBaseConfig) -> None:
        """添加配置卡片到列表页面

        Args:
            config: 网络基础配置对象
        """
        card_type_map = {
            HttpServersConfig: HttpServerConfigCard,
            HttpSseServersConfig: HttpSSEConfigCard,
            HttpClientsConfig: HttpClientConfigCard,
            WebsocketServersConfig: WebsocketServersConfigCard,
            WebsocketClientsConfig: WebsocketClientConfigCard,
        }

        card_class = card_type_map.get(type(config))
        if card_class:
            card = card_class(config, self.card_list_page)
            self.card_list_page.add_card(card)
            self.setCurrentWidget(self.card_list_page)

    def fill_value(self) -> None:
        """填充配置值到卡片列表"""
        if self.config is None:
            return

        [self.add_card(config) for config in self.config.httpServers]
        [self.add_card(config) for config in self.config.httpSseServers]
        [self.add_card(config) for config in self.config.httpClients]
        [self.add_card(config) for config in self.config.websocketServers]
        [self.add_card(config) for config in self.config.websocketClients]

    def get_value(self) -> ConnectConfig:
        """获取所有卡片的配置值

        Returns:
            ConnectConfig: 连接配置对象
        """
        card_values = self.card_list_page.get_value()
        return ConnectConfig(
            **{
                "plugins": [],
                **card_values,
            }
        )

    def clear_values(self) -> None:
        """清空所有配置卡片"""
        for item in self.card_list_page.view_layout._items:
            item.close()
        self.card_list_page.view_layout.removeAllWidgets()


class DefaultPage(QWidget):
    """默认空状态页面，显示提示信息和动画"""

    def __init__(self, parent: QWidget | None = None) -> None:
        """初始化默认页面

        Args:
            parent: 父控件
        """
        super().__init__(parent)

        # 可用表情包图片列表
        self.images = [
            ":/emoticons/image/emoticons/fu_fu/g_fufu_01.gif",
            ":/emoticons/image/emoticons/fu_fu/g_fufu_02.gif",
            ":/emoticons/image/emoticons/fu_fu/g_fufu_03.gif",
            ":/emoticons/image/emoticons/fu_fu/g_fufu_04.gif",
            ":/emoticons/image/emoticons/fu_fu/g_fufu_05.gif",
            ":/emoticons/image/emoticons/fu_fu/g_fufu_06.gif",
            ":/emoticons/image/emoticons/fu_fu/g_fufu_07.gif",
            ":/emoticons/image/emoticons/fu_fu/g_fufu_08.gif",
            ":/emoticons/image/emoticons/fu_fu/g_fufu_09.gif",
            ":/emoticons/image/emoticons/fu_fu/g_fufu_10.gif",
        ]

        self._setup_ui()

    def _setup_ui(self) -> None:
        """设置界面控件"""
        self.image_label = ImageLabel(self)
        self.movie = QMovie(random.choice(self.images))
        self.label = BodyLabel(self.tr("还没有添加配置项喔, 快点击左上角按钮添加一个吧"), self)
        self.h_box_layout = QVBoxLayout(self)

        self.image_label.setMovie(self.movie)

        self.h_box_layout.addStretch(3)
        self.h_box_layout.addWidget(self.image_label, alignment=Qt.AlignmentFlag.AlignCenter)
        self.h_box_layout.addStretch(1)
        self.h_box_layout.addWidget(self.label, alignment=Qt.AlignmentFlag.AlignCenter)
        self.h_box_layout.addStretch(3)

    def resizeEvent(self, event) -> None:
        """调整大小时重新缩放图片

        Args:
            event: 调整大小事件
        """
        super().resizeEvent(event)
        self.image_label.scaledToWidth(self.width() // 6)


class CardListPage(ScrollArea):
    """配置卡片列表页面"""

    def __init__(self, parent: ConnectWidget) -> None:
        """初始化卡片列表页面

        Args:
            parent: 父控件
        """
        super().__init__(parent)
        self.cards: List[ConfigCardBase] = []

        self._setup_ui()
        self._connect_signals()

    def _setup_ui(self) -> None:
        """设置界面控件"""
        self.view = QWidget(self)
        self.view_layout = FlowLayout(self.view, needAni=True)

        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.view.setObjectName("ConnectListView")

        self.view_layout.setSpacing(8)
        self.view_layout.setContentsMargins(0, 0, 0, 0)

    def _connect_signals(self) -> None:
        """连接信号"""
        add_page_singal_bus.remove_card_signal.connect(self.remove_card)

    def add_card(self, card: ConfigCardBase) -> None:
        """添加配置卡片

        Args:
            card: 配置卡片对象
        """
        self.cards.append(card)
        self.view_layout.addWidget(card)
        self.view_layout.update()
        self.updateGeometry()

    def remove_card(self, card: ConfigCardBase) -> None:
        """移除配置卡片

        Args:
            card: 要移除的配置卡片对象
        """
        self.cards.remove(card)
        self.view_layout.removeWidget(card)
        card.close()

    def get_value(self) -> Dict[str, list]:
        """获取所有卡片的配置值

        Returns:
            Dict[str, list]: 按类型分类的配置字典
        """
        return {
            "httpServers": [card.get_value() for card in self.cards if isinstance(card, HttpServerConfigCard)],
            "httpSseServers": [card.get_value() for card in self.cards if isinstance(card, HttpSSEConfigCard)],
            "httpClients": [card.get_value() for card in self.cards if isinstance(card, HttpClientConfigCard)],
            "websocketServers": [
                card.get_value() for card in self.cards if isinstance(card, WebsocketServersConfigCard)
            ],
            "websocketClients": [
                card.get_value() for card in self.cards if isinstance(card, WebsocketClientConfigCard)
            ],
        }

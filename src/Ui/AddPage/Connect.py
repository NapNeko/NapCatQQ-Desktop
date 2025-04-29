# -*- coding: utf-8 -*-
# 标准库导入
import random

# 第三方库导入
from qfluentwidgets import BodyLabel, FlowLayout, ImageLabel, ScrollArea
from PySide6.QtGui import QMovie
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QVBoxLayout, QStackedWidget

# 项目内模块导入
from src.Ui.AddPage.card import (
    ConfigCardBase,
    HttpSSEConfigCard,
    HttpClientConfigCard,
    HttpServerConfigCard,
    WebsocketClientConfigCard,
    WebsocketServersConfigCard,
)
from src.Ui.AddPage.signal_bus import addPageSingalBus
from src.Core.Config.ConfigModel import (
    ConnectConfig,
    HttpClientsConfig,
    HttpServersConfig,
    NetworkBaseConfig,
    HttpSseServersConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
)


class ConnectWidget(QStackedWidget):
    """连接配置项对应的 QStackedWidget"""

    def __init__(self, parent=None, config: ConnectConfig | None = None) -> None:
        super().__init__(parent=parent)
        self.setObjectName("ConnectWidget")

        self._postInit()

        if not config is None:
            # 如果传入了 config 则进行解析并填充内部卡片
            self.config = config
            self.fillValue()
            self.setCurrentWidget(self.cardListPage)
        else:
            self.setCurrentWidget(self.defultPage)

    def _postInit(self) -> None:
        """初始化"""
        self.defultPage = DefaultPage(self)
        self.cardListPage = CardListPage(self)

        self.addWidget(self.defultPage)
        self.addWidget(self.cardListPage)

    def addCard(self, config: NetworkBaseConfig) -> None:
        """添加到卡片"""
        self.cardListPage.addCard(
            {
                HttpServersConfig: HttpServerConfigCard,
                HttpSseServersConfig: HttpSSEConfigCard,
                HttpClientsConfig: HttpClientConfigCard,
                WebsocketServersConfig: WebsocketServersConfigCard,
                WebsocketClientsConfig: WebsocketClientConfigCard,
            }.get(type(config))(config, self.cardListPage)
        )

    def fillValue(self) -> None:
        """如果传入了 config 则对其内部卡片的值进行填充"""
        [self.addCard(_) for _ in self.config.httpServers]
        [self.addCard(_) for _ in self.config.httpSseServers]
        [self.addCard(_) for _ in self.config.httpClients]
        [self.addCard(_) for _ in self.config.websocketServers]
        [self.addCard(_) for _ in self.config.websocketClients]

    def getValue(self) -> ConnectConfig:
        """返回内部卡片的配置结果"""
        return ConnectConfig(
            **{
                "plugins": [],
                **self.cardListPage.getValue(),
            }
        )

    def clearValues(self) -> None:
        """清空(还原)内部卡片的配置"""
        for item in self.cardListPage.viewLayout._items:
            item.close()
        self.cardListPage.viewLayout.removeAllWidgets()


class DefaultPage(QWidget):
    """无配置时的默认页面"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.images = [
            ":/FuFuFace/image/FuFuFace/g_fufu_01.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_02.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_03.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_04.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_05.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_06.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_07.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_08.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_09.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_10.gif",
        ]

        self.imageLabel = ImageLabel(self)
        self.movie = QMovie(random.choice(self.images))
        self.label = BodyLabel(self.tr("还没有添加配置项喔, 快点击左上角按钮添加一个吧"), self)
        self.hBoxLayout = QVBoxLayout(self)

        self.imageLabel.setMovie(self.movie)

        self.hBoxLayout.addStretch(3)
        self.hBoxLayout.addWidget(self.imageLabel, alignment=Qt.AlignmentFlag.AlignCenter)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addWidget(self.label, alignment=Qt.AlignmentFlag.AlignCenter)
        self.hBoxLayout.addStretch(3)

    def resizeEvent(self, event):
        super().resizeEvent(event)
        self.imageLabel.scaledToWidth(self.width() // 6)


class CardListPage(ScrollArea):

    def __init__(self, parent: ConnectWidget):
        super().__init__(parent)
        # 属性
        self.cards = []

        self.view = QWidget(self)
        self.viewLayout = FlowLayout(self.view, needAni=True)

        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.view.setObjectName("ConnectListView")

        self.viewLayout.setSpacing(8)
        self.viewLayout.setContentsMargins(0, 0, 0, 0)

        addPageSingalBus.removeCard.connect(self.removeCard)

    def addCard(self, card: ConfigCardBase) -> None:
        self.cards.append(card)
        self.viewLayout.addWidget(card)
        self.viewLayout.update()
        self.updateGeometry()

    def removeCard(self, card: ConfigCardBase) -> None:
        """删除卡片"""
        self.cards.remove(card)
        self.viewLayout.removeWidget(card)
        card.close()

    def getValue(self) -> dict:
        print([_.getValue() for _ in self.cards if isinstance(_, HttpServerConfigCard)])
        return {
            "httpServers": [_.getValue() for _ in self.cards if isinstance(_, HttpServerConfigCard)],
            "httpSseServers": [_.getValue() for _ in self.cards if isinstance(_, HttpSSEConfigCard)],
            "httpClients": [_.getValue() for _ in self.cards if isinstance(_, HttpClientConfigCard)],
            "websocketServers": [_.getValue() for _ in self.cards if isinstance(_, WebsocketServersConfigCard)],
            "websocketClients": [_.getValue() for _ in self.cards if isinstance(_, WebsocketClientConfigCard)],
        }

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
    HttpSSEConfigCard,
    HttpClientConfigCard,
    HttpServerConfigCard,
    WebsocketClientConfigCard,
    WebsocketServersConfigCard,
)
from src.Core.Config.ConfigModel import ConnectConfig


class ConnectWidget(QStackedWidget):
    """连接配置项对应的 QStackedWidget"""

    def __init__(self, parent=None, config: ConnectConfig | None = None) -> None:
        super().__init__(parent=parent)
        self.setObjectName("ConnectWidget")

        if config is not None:
            # 如果传入了 config 则进行解析并填充内部卡片
            self.config = config
            self.fillValue()

        self._postInit()

    def _postInit(self) -> None:
        """初始化"""
        self.defultPage = DefaultPage(self)
        self.cardListPage = CardListPage(self)

        self.addWidget(self.defultPage)
        self.addWidget(self.cardListPage)

        self.setCurrentWidget(self.cardListPage)

    def fillValue(self) -> None:
        """如果传入了 config 则对其内部卡片的值进行填充"""
        ...

    def getValue(self) -> dict:
        """返回内部卡片的配置结果"""
        return {}

    def clearValues(self) -> None:
        """清空(还原)内部卡片的配置"""
        ...


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

        self.view = QWidget(self)
        self.viewLayout = FlowLayout(self.view, needAni=True)

        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.view.setObjectName("ConnectListView")

        # 项目内模块导入
        from src.Core.Config.ConfigModel import (
            HttpClientsConfig,
            HttpServersConfig,
            HttpSseServersConfig,
            WebsocketClientsConfig,
            WebsocketServersConfig,
        )

        test_http_config = HttpServersConfig(
            enable=True,
            name="Http Server",
            messagePostFormat="array",
            token="",
            debug=False,
            host="127.0.0.1",
            port="8080",
            enableCors=True,
            enableWebsocket=False,
        )
        test_http_sse_config = HttpSseServersConfig(
            enable=True,
            name="Http SSE Server",
            messagePostFormat="array",
            token="",
            debug=False,
            host="127.0.0.1",
            port="8080",
            enableCors=True,
            enableWebsocket=False,
            reportSelfMessage=True,
        )
        test_http_client_config = HttpClientsConfig(
            enable=True,
            name="Http Client",
            messagePostFormat="array",
            url="http://localhost:8080",
            token="",
            debug=False,
            reportSelfMessage=True,
        )
        test_wss_config = WebsocketServersConfig(
            enable=True,
            name="Websocket Server",
            messagePostFormat="array",
            token="",
            debug=False,
            host="127.0.0.1",
            port="8080",
            reportSelfMessage=True,
            enableForcePushEvent=True,
            heartInterval=30000,
        )
        test_wsc_config = WebsocketClientsConfig(
            enable=True,
            name="Websocket Client",
            messagePostFormat="array",
            token="",
            debug=False,
            url="ws://localhost:8080",
            reportSelfMessage=True,
            reconnectInterval=30000,
            heartInterval=30000,
        )

        self.card1 = HttpServerConfigCard(test_http_config, self.view)
        self.card2 = HttpSSEConfigCard(test_http_sse_config, self.view)
        self.card3 = HttpClientConfigCard(test_http_client_config, self.view)
        self.card4 = WebsocketServersConfigCard(test_wss_config, self.view)
        self.card5 = WebsocketClientConfigCard(test_wsc_config, self.view)

        self.viewLayout.addWidget(self.card1)
        self.viewLayout.addWidget(self.card2)
        self.viewLayout.addWidget(self.card3)
        self.viewLayout.addWidget(self.card4)
        self.viewLayout.addWidget(self.card5)

        self.viewLayout.setSpacing(8)
        self.viewLayout.setContentsMargins(0, 0, 0, 0)

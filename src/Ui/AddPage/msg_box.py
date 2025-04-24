# -*- coding: utf-8 -*-

# 第三方库导入
from qfluentwidgets import BodyLabel
from qfluentwidgets import FluentIcon as FI
from qfluentwidgets import TitleLabel, RadioButton, MessageBoxBase, SimpleCardWidget
from PySide6.QtCore import Qt, Slot
from PySide6.QtWidgets import QGridLayout, QVBoxLayout, QButtonGroup

# 项目内模块导入
from src.Ui.AddPage.enum import ConnectType
from src.Ui.AddPage.signal_bus import addPageSingalBus
from src.Ui.common.InputCard.GenericCard import SwitchConfigCard, ComboBoxConfigCard, LineEditConfigCard


class ChooseConfigCard(SimpleCardWidget):
    """选择配置类型的卡片"""

    def __init__(self, button: RadioButton, content: str, parent: "ChooseConfigTypeDialog") -> None:
        super().__init__(parent)

        # 创建控件
        self.button = button
        self.contentLabel = BodyLabel(content, self)
        self.vBoxLayout = QVBoxLayout(self)

        # 设置属性
        self.contentLabel.setWordWrap(True)
        self.setMinimumSize(175, 128)

        # 将组件添加到布局
        self.vBoxLayout.setContentsMargins(16, 16, 16, 16)
        self.vBoxLayout.setSpacing(4)
        self.vBoxLayout.addWidget(self.button, alignment=Qt.AlignmentFlag.AlignTop)
        self.vBoxLayout.addWidget(self.contentLabel, alignment=Qt.AlignmentFlag.AlignTop)

        # 设置点击事件
        self.clicked.connect(lambda: self.button.setChecked(True))


class ChooseConfigTypeDialog(MessageBoxBase):
    """选择配置类型的对话框"""

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)

        # 介绍
        contents = [
            self.tr("「由NapCat建立」的HTTP服务器, 可「用框架连接」或「手动发送请求」。NapCat会提供配置的地址供连接。"),
            self.tr("「由NapCat建立」的HTTP SSE服务器, 可「用框架连接」或「手动发送请求」。NapCat会提供配置的地址。"),
            self.tr("「由框架或自建」的HTTP客户端, 用于接收NapCat的请求。通常是框架提供的地址, NapCat会主动连接。"),
            self.tr("「由NapCat建立」的WebSocket服务器, 你的框架需要连接其提供的地址。"),
            self.tr("「由框架提供」的WebSocket地址, NapCat会主动连接。"),
        ]

        # 创建控件
        self.titleLabel = TitleLabel(self.tr("请选择配置类型"), self)
        self.buttonGroup = QButtonGroup(self)
        self.httpServerConfigButton = RadioButton(self.tr("HTTP 服务器"))
        self.httpSSEServerConfigButton = RadioButton(self.tr("HTTP SSE 服务器"))
        self.httpClientConfigButton = RadioButton(self.tr("HTTP 客户端"))
        self.WebSocketServerConfigButton = RadioButton(self.tr("WebSocket 服务器"))
        self.WebSocketClientConfigButton = RadioButton(self.tr("WebSocket 客户端"))

        self.httpServerCard = ChooseConfigCard(self.httpServerConfigButton, contents[0], self)
        self.httpSSEServerCard = ChooseConfigCard(self.httpSSEServerConfigButton, contents[1], self)
        self.httpClientCard = ChooseConfigCard(self.httpClientConfigButton, contents[2], self)
        self.WebSocketServerCard = ChooseConfigCard(self.WebSocketServerConfigButton, contents[3], self)
        self.WebSocketClientCard = ChooseConfigCard(self.WebSocketClientConfigButton, contents[4], self)

        # 设置属性
        self.buttonGroup.setExclusive(True)
        self.buttonGroup.addButton(self.httpServerConfigButton, 0)
        self.buttonGroup.addButton(self.httpSSEServerConfigButton, 1)
        self.buttonGroup.addButton(self.httpClientConfigButton, 2)
        self.buttonGroup.addButton(self.WebSocketServerConfigButton, 3)
        self.buttonGroup.addButton(self.WebSocketClientConfigButton, 4)
        self.widget.setMinimumSize(850, 400)

        # 设置布局
        self.cardLayout = QGridLayout()
        self.cardLayout.setSpacing(10)
        self.cardLayout.setContentsMargins(0, 0, 0, 0)
        self.cardLayout.addWidget(self.httpServerCard, 0, 0)
        self.cardLayout.addWidget(self.httpSSEServerCard, 0, 1)
        self.cardLayout.addWidget(self.httpClientCard, 0, 2)
        self.cardLayout.addWidget(self.WebSocketServerCard, 1, 0)
        self.cardLayout.addWidget(self.WebSocketClientCard, 1, 1)

        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addLayout(self.cardLayout, stretch=1)

        # 链接信号
        self.yesButton.clicked.connect(self._onYesButtonClicked)

    @Slot()
    def _onYesButtonClicked(self) -> None:
        """Yes 按钮槽函数"""

        if (id := self.buttonGroup.checkedId()) == -1:
            return
        else:
            addPageSingalBus.chooseConnectType.emit(list(ConnectType)[id])


class HttpServerConfigDialog(MessageBoxBase):

    def __init__(self, parent) -> None:
        super().__init__(parent)

        # 创建控件
        self.titleLabel = TitleLabel(self.tr("HTTP Server"), self)
        self.enableCard = SwitchConfigCard(FI.IOT, self.tr("启用"))
        self.debugCard = SwitchConfigCard(FI.DEVELOPER_TOOLS, self.tr("调试"))
        self.nameCard = LineEditConfigCard(FI.TAG, self.tr("名称*"), "HTTP Server", self.tr("设置配置名称"))
        self.hostCard = LineEditConfigCard(FI.HOME, self.tr("Host*"), "0.0.0.0", self.tr("设置主机地址"))
        self.portCard = LineEditConfigCard(FI.LINK, self.tr("Port*"), "3000", self.tr("设置端口号"))
        self.CORSCard = SwitchConfigCard(FI.GLOBE, self.tr("CORS"))
        self.websocketCard = SwitchConfigCard(FI.SCROLL, self.tr("WebSocket"))
        self.msgFormatCard = ComboBoxConfigCard(
            FI.MESSAGE, self.tr("消息格式"), ["Array", "String"], self.tr("设置消息格式")
        )
        self.tokenCard = LineEditConfigCard(FI.VPN, self.tr("Token"), content=self.tr("设置连接Token"))

        # 设置属性
        self.widget.setMinimumWidth(600)

        # 创建布局
        self.gridLayout = QGridLayout()
        self.gridLayout.setSpacing(8)
        self.gridLayout.setContentsMargins(0, 0, 0, 0)
        self.gridLayout.addWidget(self.enableCard, 0, 0, 1, 1)
        self.gridLayout.addWidget(self.debugCard, 0, 1, 1, 1)
        self.gridLayout.addWidget(self.nameCard, 2, 0, 1, 2)
        self.gridLayout.addWidget(self.hostCard, 3, 0, 1, 2)
        self.gridLayout.addWidget(self.portCard, 4, 0, 1, 2)
        self.gridLayout.addWidget(self.CORSCard, 5, 0, 1, 1)
        self.gridLayout.addWidget(self.websocketCard, 5, 1, 1, 1)
        self.gridLayout.addWidget(self.msgFormatCard, 6, 0, 1, 2)
        self.gridLayout.addWidget(self.tokenCard, 7, 0, 1, 2)

        # 设置布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addLayout(self.gridLayout)


class HttpSSEServerConfigDialog(MessageBoxBase):

    def __init__(self, parent):
        super().__init__(parent)

        # 创建控件
        self.titleLabel = TitleLabel(self.tr("HTTP SSE Server"), self)
        self.enableCard = SwitchConfigCard(FI.IOT, self.tr("启用"))
        self.debugCard = SwitchConfigCard(FI.DEVELOPER_TOOLS, self.tr("调试"))
        self.nameCard = LineEditConfigCard(FI.TAG, self.tr("名称*"), "HTTP SSE Server", self.tr("设置配置名称"))
        self.hostCard = LineEditConfigCard(FI.HOME, self.tr("Host*"), "0.0.0.0", self.tr("设置主机地址"))
        self.portCard = LineEditConfigCard(FI.LINK, self.tr("Port*"), "3000", self.tr("设置端口号"))
        self.CORSCard = SwitchConfigCard(FI.GLOBE, self.tr("CORS"))
        self.websocketCard = SwitchConfigCard(FI.SCROLL, self.tr("WebSocket"))
        self.reportSelfMsgCard = SwitchConfigCard(FI.MESSAGE, self.tr("上报自身消息"))
        self.msgFormatCard = ComboBoxConfigCard(
            FI.MESSAGE, self.tr("消息格式"), ["Array", "String"], self.tr("设置消息格式")
        )
        self.tokenCard = LineEditConfigCard(FI.VPN, self.tr("Token"), content=self.tr("设置连接Token"))

        # 设置属性
        self.widget.setMinimumWidth(600)

        # 创建布局
        self.gridLayout = QGridLayout()
        self.gridLayout.setSpacing(8)
        self.gridLayout.setContentsMargins(0, 0, 0, 0)
        self.gridLayout.addWidget(self.enableCard, 0, 0, 1, 3)
        self.gridLayout.addWidget(self.debugCard, 0, 3, 1, 3)
        self.gridLayout.addWidget(self.nameCard, 2, 0, 1, 6)
        self.gridLayout.addWidget(self.hostCard, 3, 0, 1, 6)
        self.gridLayout.addWidget(self.portCard, 4, 0, 1, 6)
        self.gridLayout.addWidget(self.CORSCard, 5, 0, 1, 2)
        self.gridLayout.addWidget(self.websocketCard, 5, 2, 1, 2)
        self.gridLayout.addWidget(self.reportSelfMsgCard, 5, 4, 1, 2)
        self.gridLayout.addWidget(self.msgFormatCard, 6, 0, 1, 6)
        self.gridLayout.addWidget(self.tokenCard, 7, 0, 1, 6)

        # 设置布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addLayout(self.gridLayout)


class HttpClientConfigDialog(MessageBoxBase):

    def __init__(self, parent):
        super().__init__(parent)

        # 创建控件
        self.titleLabel = TitleLabel(self.tr("HTTP Client"), self)
        self.enableCard = SwitchConfigCard(FI.IOT, self.tr("启用"))
        self.debugCard = SwitchConfigCard(FI.DEVELOPER_TOOLS, self.tr("调试"))
        self.reportSelfMsgCard = SwitchConfigCard(FI.MESSAGE, self.tr("上报自身消息"))
        self.nameCard = LineEditConfigCard(FI.TAG, self.tr("名称*"), "HTTP Client", self.tr("设置配置名称"))
        self.urlCard = LineEditConfigCard(FI.LINK, "URL*", "http://localhost:8080", self.tr("设置请求地址"))
        self.msgFormatCard = ComboBoxConfigCard(
            FI.MESSAGE, self.tr("消息格式"), ["Array", "String"], self.tr("设置消息格式")
        )
        self.tokenCard = LineEditConfigCard(FI.VPN, self.tr("Token"), content=self.tr("设置连接Token"))

        # 设置属性
        self.widget.setMinimumWidth(600)

        # 创建布局
        self.gridLayout = QGridLayout()
        self.gridLayout.setSpacing(8)
        self.gridLayout.setContentsMargins(0, 0, 0, 0)
        self.gridLayout.addWidget(self.enableCard, 0, 0, 1, 2)
        self.gridLayout.addWidget(self.debugCard, 0, 2, 1, 2)
        self.gridLayout.addWidget(self.reportSelfMsgCard, 0, 4, 1, 2)
        self.gridLayout.addWidget(self.nameCard, 1, 0, 1, 6)
        self.gridLayout.addWidget(self.urlCard, 2, 0, 1, 6)
        self.gridLayout.addWidget(self.msgFormatCard, 3, 0, 1, 6)
        self.gridLayout.addWidget(self.tokenCard, 4, 0, 1, 6)

        # 设置布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addLayout(self.gridLayout)


class WebsocketServerConfigDialog(MessageBoxBase):

    def __init__(self, parent):
        super().__init__(parent)

        # 创建控件
        self.titleLabel = TitleLabel(self.tr("Websocket Server"), self)
        self.enableCard = SwitchConfigCard(FI.IOT, self.tr("启用"))
        self.debugCard = SwitchConfigCard(FI.DEVELOPER_TOOLS, self.tr("调试"))
        self.forcePushEventCard = SwitchConfigCard(FI.MESSAGE, self.tr("强制推送事件"), value=True)
        self.reportSelfMsgCard = SwitchConfigCard(FI.MESSAGE, self.tr("上报自身消息"))
        self.nameCard = LineEditConfigCard(FI.TAG, self.tr("名称*"), "Websocket Server", self.tr("设置配置名称"))
        self.hostCard = LineEditConfigCard(FI.HOME, self.tr("Host*"), "0.0.0.0", self.tr("设置主机地址"))
        self.portCard = LineEditConfigCard(FI.LINK, self.tr("Port*"), "3000", self.tr("设置端口号"))
        self.msgFormatCard = ComboBoxConfigCard(
            FI.MESSAGE, self.tr("消息格式"), ["Array", "String"], self.tr("设置消息格式")
        )
        self.tokenCard = LineEditConfigCard(FI.VPN, self.tr("Token"), content=self.tr("设置连接Token"))
        self.heartIntervalCard = LineEditConfigCard(FI.HEART, self.tr("心跳间隔"), "30000", self.tr("设置心跳间隔"))

        # 设置属性
        self.widget.setMinimumWidth(600)

        # 创建布局
        self.gridLayout = QGridLayout()
        self.gridLayout.setSpacing(8)
        self.gridLayout.setContentsMargins(0, 0, 0, 0)
        self.gridLayout.addWidget(self.enableCard, 0, 0, 1, 1)
        self.gridLayout.addWidget(self.debugCard, 0, 1, 1, 1)
        self.gridLayout.addWidget(self.forcePushEventCard, 0, 2, 1, 1)
        self.gridLayout.addWidget(self.reportSelfMsgCard, 0, 3, 1, 1)
        self.gridLayout.addWidget(self.nameCard, 2, 0, 1, 4)
        self.gridLayout.addWidget(self.hostCard, 3, 0, 1, 2)
        self.gridLayout.addWidget(self.portCard, 3, 2, 1, 2)
        self.gridLayout.addWidget(self.msgFormatCard, 4, 0, 1, 4)
        self.gridLayout.addWidget(self.tokenCard, 5, 0, 1, 2)
        self.gridLayout.addWidget(self.heartIntervalCard, 5, 2, 1, 2)

        # 设置布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addLayout(self.gridLayout)


class WebsocketClientConfigDialog(MessageBoxBase):

    def __init__(self, parent):
        super().__init__(parent)

        # 创建控件
        self.titleLabel = TitleLabel(self.tr("Websocket Client"), self)
        self.enableCard = SwitchConfigCard(FI.IOT, self.tr("启用"))
        self.debugCard = SwitchConfigCard(FI.DEVELOPER_TOOLS, self.tr("调试"))
        self.reportSelfMsgCard = SwitchConfigCard(FI.MESSAGE, self.tr("上报自身消息"))
        self.nameCard = LineEditConfigCard(FI.TAG, self.tr("名称*"), "Websocket Client", self.tr("设置配置名称"))
        self.urlCard = LineEditConfigCard(FI.LINK, "URL*", "ws://localhost:8080", self.tr("设置请求地址"))
        self.msgFormatCard = ComboBoxConfigCard(
            FI.MESSAGE, self.tr("消息格式"), ["Array", "String"], self.tr("设置消息格式")
        )
        self.tokenCard = LineEditConfigCard(FI.VPN, self.tr("Token"), content=self.tr("设置连接Token"))
        self.heartIntervalCard = LineEditConfigCard(FI.HEART, self.tr("心跳间隔"), "30000", self.tr("设置心跳间隔"))
        self.reconnectIntervalCard = LineEditConfigCard(
            FI.UPDATE, self.tr("重连间隔"), "30000", self.tr("设置重连间隔")
        )

        # 设置属性
        self.widget.setMinimumWidth(600)

        # 创建布局
        self.gridLayout = QGridLayout()
        self.gridLayout.setSpacing(8)
        self.gridLayout.setContentsMargins(0, 0, 0, 0)
        self.gridLayout.addWidget(self.enableCard, 0, 0, 1, 2)
        self.gridLayout.addWidget(self.debugCard, 0, 2, 1, 2)
        self.gridLayout.addWidget(self.reportSelfMsgCard, 0, 4, 1, 2)
        self.gridLayout.addWidget(self.nameCard, 1, 0, 1, 6)
        self.gridLayout.addWidget(self.urlCard, 2, 0, 1, 6)
        self.gridLayout.addWidget(self.msgFormatCard, 3, 0, 1, 3)
        self.gridLayout.addWidget(self.tokenCard, 3, 3, 1, 3)
        self.gridLayout.addWidget(self.heartIntervalCard, 4, 0, 1, 3)
        self.gridLayout.addWidget(self.reconnectIntervalCard, 4, 3, 1, 3)

        # 设置布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addLayout(self.gridLayout)

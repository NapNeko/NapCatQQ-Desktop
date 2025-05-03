# -*- coding: utf-8 -*-

# 第三方库导入
from pydantic import ValidationError
from qfluentwidgets import BodyLabel
from qfluentwidgets import FluentIcon as FI
from qfluentwidgets import TitleLabel, RadioButton, MessageBoxBase, SimpleCardWidget
from PySide6.QtCore import Qt, Slot, QObject
from PySide6.QtWidgets import QGridLayout, QVBoxLayout, QButtonGroup

# 项目内模块导入
from src.Core.Utils import my_int
from src.Ui.AddPage.enum import ConnectType
from src.Ui.AddPage.signal_bus import addPageSingalBus
from src.Core.Config.ConfigModel import (
    HttpClientsConfig,
    HttpServersConfig,
    NetworkBaseConfig,
    HttpSseServersConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
)
from src.Ui.common.input_card.generic_card import SwitchConfigCard, ComboBoxConfigCard, LineEditConfigCard


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

    def __init__(self, parent: QObject) -> None:
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

        if (id := self.buttonGroup.checkedId()) != -1:
            addPageSingalBus.chooseConnectType.emit(list(ConnectType)[id])


class ConfigDialogBase(MessageBoxBase):
    """配置对话框基类"""

    def __init__(self, parent: QObject, config: NetworkBaseConfig) -> None:
        super().__init__(parent)
        # 属性
        self.config = config

        # 创建控件
        self.titleLabel = TitleLabel(self)
        self.enableCard = SwitchConfigCard(FI.IOT, self.tr("启用"))
        self.nameCard = LineEditConfigCard(FI.TAG, self.tr("名称*"), "输入配置名称", self.tr("设置配置名称"))
        self.msgFormatCard = ComboBoxConfigCard(
            FI.MESSAGE, self.tr("消息格式"), ["array", "string"], self.tr("设置消息格式")
        )
        self.tokenCard = LineEditConfigCard(FI.VPN, self.tr("Token"), content=self.tr("设置连接Token"))
        self.debugCard = SwitchConfigCard(FI.DEVELOPER_TOOLS, self.tr("调试"))

        # 布局
        self.gridLayout = QGridLayout()
        self.gridLayout.setSpacing(8)
        self.gridLayout.setContentsMargins(0, 0, 0, 0)

        # 设置布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addLayout(self.gridLayout)

    def fillConfig(self) -> None:
        """如果传入了配置,则该对话框作为编辑对话框"""
        if self.config is None:
            return

        self.enableCard.fillValue(self.config.enable)
        self.debugCard.fillValue(self.config.debug)
        self.nameCard.fillValue(self.config.name)
        self.msgFormatCard.fillValue(self.config.messagePostFormat)
        self.tokenCard.fillValue(self.config.token)

        # 禁用名字卡片
        self.nameCard.setEnabled(False)

    def accept(self):
        """重写accept方法, 以便在点击确定按钮时验证配置"""
        try:
            # 验证配置
            self.getConfig()
            # 关闭对话框
            super().accept()
        except ValidationError as e:
            # 显示错误信息
            if "配置错误请重试" in self.titleLabel.text():
                return
            self.titleLabel.setText(self.titleLabel.text() + f" - 配置错误请重试")

    def getConfig(self) -> NetworkBaseConfig:
        """获取配置"""
        return NetworkBaseConfig(
            **{
                "enable": self.enableCard.getValue(),
                "name": self.nameCard.getValue(),
                "messagePostFormat": self.msgFormatCard.getValue().lower(),
                "token": self.tokenCard.getValue(),
                "debug": self.debugCard.getValue(),
            }
        )


class HttpServerConfigDialog(ConfigDialogBase):
    config: HttpServersConfig | None

    def __init__(self, parent: QObject, config: HttpServersConfig | None = None) -> None:
        super().__init__(parent, config)
        # 创建控件
        self.hostCard = LineEditConfigCard(FI.HOME, self.tr("Host*"), "0.0.0.0", self.tr("设置主机地址"))
        self.portCard = LineEditConfigCard(FI.LINK, self.tr("Port*"), "3000", self.tr("设置端口号"))
        self.CORSCard = SwitchConfigCard(FI.GLOBE, self.tr("CORS"))
        self.websocketCard = SwitchConfigCard(FI.SCROLL, self.tr("WebSocket"))

        # 设置属性
        self.titleLabel.setText("HTTP Server")
        self.widget.setMinimumWidth(600)

        # 设置布局
        self.gridLayout.addWidget(self.enableCard, 0, 0, 1, 1)
        self.gridLayout.addWidget(self.debugCard, 0, 1, 1, 1)
        self.gridLayout.addWidget(self.nameCard, 2, 0, 1, 2)
        self.gridLayout.addWidget(self.hostCard, 3, 0, 1, 2)
        self.gridLayout.addWidget(self.portCard, 4, 0, 1, 2)
        self.gridLayout.addWidget(self.CORSCard, 5, 0, 1, 1)
        self.gridLayout.addWidget(self.websocketCard, 5, 1, 1, 1)
        self.gridLayout.addWidget(self.msgFormatCard, 6, 0, 1, 2)
        self.gridLayout.addWidget(self.tokenCard, 7, 0, 1, 2)

        # 调用方法
        self.fillConfig()

    def fillConfig(self) -> None:
        """如果传入了配置,则该对话框作为编辑对话框"""
        if self.config is None:
            return

        super().fillConfig()
        self.hostCard.fillValue(self.config.host)
        self.portCard.fillValue(self.config.port)
        self.CORSCard.fillValue(self.config.enableCors)
        self.websocketCard.fillValue(self.config.enableWebsocket)

    def getConfig(self) -> HttpServersConfig:
        return HttpServersConfig(
            **{
                "host": self.hostCard.getValue(),
                "port": self.portCard.getValue(),
                "enableCors": self.CORSCard.getValue(),
                "enableWebsocket": self.websocketCard.getValue(),
                **super().getConfig().model_dump(),
            }
        )


class HttpSSEServerConfigDialog(ConfigDialogBase):
    config: HttpSseServersConfig | None

    def __init__(self, parent: QObject, config: HttpSseServersConfig | None = None) -> None:
        super().__init__(parent, config)
        # 创建控件
        self.hostCard = LineEditConfigCard(FI.HOME, self.tr("Host*"), "0.0.0.0", self.tr("设置主机地址"))
        self.portCard = LineEditConfigCard(FI.LINK, self.tr("Port*"), "3000", self.tr("设置端口号"))
        self.CORSCard = SwitchConfigCard(FI.GLOBE, self.tr("CORS"))
        self.websocketCard = SwitchConfigCard(FI.SCROLL, self.tr("WebSocket"))
        self.reportSelfMsgCard = SwitchConfigCard(FI.MESSAGE, self.tr("上报自身消息"))

        # 设置属性
        self.titleLabel.setText("HTTP SSE Server")
        self.widget.setMinimumWidth(600)

        # 创建布局
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

        # 调用方法
        self.fillConfig()

    def fillConfig(self) -> None:
        """如果传入了配置,则该对话框作为编辑对话框"""
        if self.config is None:
            return

        super().fillConfig()
        self.hostCard.fillValue(self.config.host)
        self.portCard.fillValue(self.config.port)
        self.CORSCard.fillValue(self.config.enableCors)
        self.websocketCard.fillValue(self.config.enableWebsocket)
        self.reportSelfMsgCard.fillValue(self.config.reportSelfMessage)

    def getConfig(self) -> HttpSseServersConfig:
        return HttpSseServersConfig(
            **{
                "host": self.hostCard.getValue(),
                "port": self.portCard.getValue(),
                "enableCors": self.CORSCard.getValue(),
                "enableWebsocket": self.websocketCard.getValue(),
                "reportSelfMessage": self.reportSelfMsgCard.getValue(),
                **super().getConfig().model_dump(),
            }
        )


class HttpClientConfigDialog(ConfigDialogBase):
    config: HttpClientsConfig | None

    def __init__(self, parent: QObject, config: HttpClientsConfig | None = None) -> None:
        super().__init__(parent, config)

        # 创建控件
        self.reportSelfMsgCard = SwitchConfigCard(FI.MESSAGE, self.tr("上报自身消息"))
        self.urlCard = LineEditConfigCard(FI.LINK, "URL*", "http://localhost:8080", self.tr("设置请求地址"))

        # 设置属性
        self.titleLabel.setText("HTTP Client")
        self.widget.setMinimumWidth(600)

        # 设置布局
        self.gridLayout.addWidget(self.enableCard, 0, 0, 1, 2)
        self.gridLayout.addWidget(self.debugCard, 0, 2, 1, 2)
        self.gridLayout.addWidget(self.reportSelfMsgCard, 0, 4, 1, 2)
        self.gridLayout.addWidget(self.nameCard, 1, 0, 1, 6)
        self.gridLayout.addWidget(self.urlCard, 2, 0, 1, 6)
        self.gridLayout.addWidget(self.msgFormatCard, 3, 0, 1, 6)
        self.gridLayout.addWidget(self.tokenCard, 4, 0, 1, 6)

        # 调用方法
        self.fillConfig()

    def fillConfig(self) -> None:
        """如果传入了配置,则该对话框作为编辑对话框"""
        if self.config is None:
            return

        super().fillConfig()
        self.reportSelfMsgCard.fillValue(self.config.reportSelfMessage)
        self.urlCard.fillValue(str(self.config.url))

    def getConfig(self) -> HttpClientsConfig:
        return HttpClientsConfig(
            **{
                "url": self.urlCard.getValue(),
                "reportSelfMessage": self.reportSelfMsgCard.getValue(),
                **super().getConfig().model_dump(),
            }
        )


class WebsocketServerConfigDialog(ConfigDialogBase):
    config: WebsocketServersConfig | None

    def __init__(self, parent: QObject, config: WebsocketServersConfig | None = None) -> None:
        super().__init__(parent, config)

        # 创建控件
        self.forcePushEventCard = SwitchConfigCard(FI.MESSAGE, self.tr("强制推送事件"), value=True)
        self.reportSelfMsgCard = SwitchConfigCard(FI.MESSAGE, self.tr("上报自身消息"))
        self.hostCard = LineEditConfigCard(FI.HOME, self.tr("Host*"), "0.0.0.0", self.tr("设置主机地址"))
        self.portCard = LineEditConfigCard(FI.LINK, self.tr("Port*"), "3000", self.tr("设置端口号"))
        self.heartIntervalCard = LineEditConfigCard(FI.HEART, self.tr("心跳间隔"), "30000", self.tr("设置心跳间隔"))

        # 设置属性
        self.titleLabel.setText("Websocket Server")
        self.widget.setMinimumWidth(600)

        # 设置布局
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

        # 调用方法
        self.fillConfig()

    def fillConfig(self) -> None:
        """如果传入了配置,则该对话框作为编辑对话框"""
        if self.config is None:
            return

        super().fillConfig()
        self.forcePushEventCard.fillValue(self.config.enableForcePushEvent)
        self.reportSelfMsgCard.fillValue(self.config.reportSelfMessage)
        self.hostCard.fillValue(self.config.host)
        self.portCard.fillValue(self.config.port)
        self.heartIntervalCard.fillValue(self.config.heartInterval)

    def getConfig(self) -> WebsocketServersConfig:
        return WebsocketServersConfig(
            **{
                "host": self.hostCard.getValue(),
                "port": self.portCard.getValue(),
                "reportSelfMessage": self.reportSelfMsgCard.getValue(),
                "enableForcePushEvent": self.forcePushEventCard.getValue(),
                "heartInterval": my_int(self.heartIntervalCard.getValue(), 300000),
                **super().getConfig().model_dump(),
            }
        )


class WebsocketClientConfigDialog(ConfigDialogBase):
    config: WebsocketClientsConfig | None

    def __init__(self, parent: QObject, config: WebsocketClientsConfig | None = None) -> None:
        super().__init__(parent, config)

        # 创建控件
        self.reportSelfMsgCard = SwitchConfigCard(FI.MESSAGE, self.tr("上报自身消息"))
        self.urlCard = LineEditConfigCard(FI.LINK, "URL*", "ws://localhost:8080", self.tr("设置请求地址"))
        self.heartIntervalCard = LineEditConfigCard(FI.HEART, self.tr("心跳间隔"), "30000", self.tr("设置心跳间隔"))
        self.reconnectIntervalCard = LineEditConfigCard(
            FI.UPDATE, self.tr("重连间隔"), "30000", self.tr("设置重连间隔")
        )

        # 设置属性
        self.titleLabel.setText("Websocket Client")
        self.widget.setMinimumWidth(600)

        # 设置布局
        self.gridLayout.addWidget(self.enableCard, 0, 0, 1, 2)
        self.gridLayout.addWidget(self.debugCard, 0, 2, 1, 2)
        self.gridLayout.addWidget(self.reportSelfMsgCard, 0, 4, 1, 2)
        self.gridLayout.addWidget(self.nameCard, 1, 0, 1, 6)
        self.gridLayout.addWidget(self.urlCard, 2, 0, 1, 6)
        self.gridLayout.addWidget(self.msgFormatCard, 3, 0, 1, 3)
        self.gridLayout.addWidget(self.tokenCard, 3, 3, 1, 3)
        self.gridLayout.addWidget(self.heartIntervalCard, 4, 0, 1, 3)
        self.gridLayout.addWidget(self.reconnectIntervalCard, 4, 3, 1, 3)

        # 调用方法
        self.fillConfig()

    def fillConfig(self) -> None:
        """如果传入了配置,则该对话框作为编辑对话框"""
        if self.config is None:
            return

        super().fillConfig()
        self.reportSelfMsgCard.fillValue(self.config.reportSelfMessage)
        self.urlCard.fillValue(str(self.config.url))
        self.heartIntervalCard.fillValue(self.config.heartInterval)
        self.reconnectIntervalCard.fillValue(self.config.reconnectInterval)

    def getConfig(self) -> WebsocketClientsConfig:
        return WebsocketClientsConfig(
            **{
                "url": self.urlCard.getValue(),
                "reportSelfMessage": self.reportSelfMsgCard.getValue(),
                "heartInterval": my_int(self.heartIntervalCard.getValue(), 300000),
                "reconnectInterval": my_int(self.reconnectIntervalCard.getValue(), 300000),
                **super().getConfig().model_dump(),
            }
        )

# -*- coding: utf-8 -*-

# 第三方库导入
from qfluentwidgets import BodyLabel, TitleLabel, RadioButton, MessageBoxBase, SimpleCardWidget
from PySide6.QtCore import Qt, Slot
from PySide6.QtWidgets import QGridLayout, QVBoxLayout, QButtonGroup

# 项目内模块导入
from src.Ui.AddPage.enum import ConnectType
from src.Ui.AddPage.signal_bus import addPageSingalBus


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
    def _onYesButtonClicked(self):
        """Yes 按钮槽函数"""

        if id := self.buttonGroup.checkedId() == -1:
            return
        else:
            addPageSingalBus.ChooseConnectType.emit(list(ConnectType)[id])


class HttpServerConfigDialog(MessageBoxBase):

    def __init__(self, parent):
        super().__init__(parent)

# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import ExpandLayout, FluentIcon, ScrollArea
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config.config_model import AdvancedConfig
from src.ui.components.input_card import ComboBoxConfigCard, LineEditConfigCard, SwitchConfigCard


class AdvancedWidget(ScrollArea):
    """
    ## Advance Item 项对应的 QWidget
    """

    def __init__(self, parent=None, config: AdvancedConfig = None) -> None:
        super().__init__(parent=parent)

        self.view = QWidget()
        self.cardLayout = ExpandLayout(self.view)

        # 设置 ScrollArea
        self.setObjectName("AdvanceWidget")
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.view.setObjectName("AdvanceWidgetView")

        # 调用方法
        self._initWidget()
        self._setLayout()

        if config is not None:
            # 如果传入了 config 则进行解析并填充内部卡片
            self.config = config
            self.fillValue()

    def _initWidget(self) -> None:
        """
        ## 初始化 QWidget 所需要的控件并配置
        """
        self.autoStartCard = SwitchConfigCard(
            icon=FluentIcon.PLAY,
            title=self.tr("自动启动"),
            content=self.tr("是否在启动时自动启动 bot"),
            parent=self.view,
        )
        self.offlineNotice = SwitchConfigCard(
            icon=FluentIcon.MEGAPHONE,
            title=self.tr("掉线通知"),
            content=self.tr("当Bot状态为 离线 时, 发送通知"),
            parent=self.view,
        )
        self.parseMultMsg = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("解析合并转发消息"),
            content=self.tr("是否解析合并转发消息"),
            parent=self.view,
        )
        self.packetServerCard = LineEditConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("Packet Server"),
            content=self.tr("设置 Packet Server 地址, 为空则使用默认值"),
            parent=self.view,
        )
        self.packetBackendCard = LineEditConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("Packet Backend"),
            content=self.tr("设置 Packet Backend, 为空则使用默认值"),
            parent=self.view,
        )
        self.localFile2UrlCard = SwitchConfigCard(
            icon=FluentIcon.SHARE,
            title=self.tr("LocalFile2Url"),
            content=self.tr("是否将本地文件转换为URL, 如果获取不到url则使用base64字段返回文件内容"),
            value=True,
            parent=self.view,
        )
        self.fileLogCard = SwitchConfigCard(
            icon=FluentIcon.SAVE_AS,
            title=self.tr("文件日志"),
            content=self.tr("是否要将日志记录到文件"),
            parent=self.view,
        )
        self.consoleLogCard = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("控制台日志"),
            content=self.tr("是否启用控制台日志"),
            value=True,
            parent=self.view,
        )
        self.fileLogLevelCard = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("文件日志等级"),
            content=self.tr("设置文件日志输出等级"),
            texts=["debug", "info", "error"],
            parent=self.view,
        )
        self.consoleLevelCard = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("控制台日志等级"),
            content=self.tr("设置控制台日志输出等级"),
            texts=["info", "debug", "error"],
            parent=self.view,
        )
        self.o3HookModeCard = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("O3 Hook 模式"),
            content=self.tr("设置 O3 Hook 模式"),
            texts=["0", "1"],
            parent=self.view,
        )

        self.cards = [
            self.autoStartCard,
            self.offlineNotice,
            self.parseMultMsg,
            self.packetServerCard,
            self.packetBackendCard,
            self.localFile2UrlCard,
            self.fileLogCard,
            self.consoleLogCard,
            self.fileLogLevelCard,
            self.consoleLevelCard,
            self.o3HookModeCard,
        ]

    def fillValue(self) -> None:
        """
        ## 如果传入了 config 则对其内部卡片的值进行填充
        """
        self.autoStartCard.fill_value(self.config.autoStart)
        self.offlineNotice.fill_value(self.config.offlineNotice)
        self.parseMultMsg.fill_value(self.config.parseMultMsg)
        self.packetServerCard.fill_value(self.config.packetServer)
        self.packetBackendCard.fill_value(self.config.packetBackend)
        self.localFile2UrlCard.fill_value(self.config.enableLocalFile2Url)
        self.fileLogCard.fill_value(self.config.fileLog)
        self.consoleLogCard.fill_value(self.config.consoleLog)
        self.fileLogLevelCard.fill_value(self.config.fileLogLevel)
        self.consoleLevelCard.fill_value(self.config.consoleLogLevel)
        self.o3HookModeCard.fill_value(str(self.config.o3HookMode))

    def _setLayout(self) -> None:
        """
        ## 将 QWidget 内部的 InputCard 添加到布局中
        """
        self.cardLayout.setContentsMargins(0, 0, 0, 0)
        self.cardLayout.setSpacing(2)
        for card in self.cards:
            self.cardLayout.addWidget(card)
            self.adjustSize()

        self.view.setLayout(self.cardLayout)

    def getValue(self) -> dict:
        """
        ## 返回内部卡片的配置结果
        """
        return {
            "autoStart": self.autoStartCard.get_value(),
            "offlineNotice": self.offlineNotice.get_value(),
            "parseMultMsg": self.parseMultMsg.get_value(),
            "packetServer": self.packetServerCard.get_value(),
            "packetBackend": self.packetBackendCard.get_value(),
            "enableLocalFile2Url": self.localFile2UrlCard.get_value(),
            "fileLog": self.fileLogCard.get_value(),
            "consoleLog": self.consoleLogCard.get_value(),
            "fileLogLevel": self.fileLogLevelCard.get_value(),
            "consoleLogLevel": self.consoleLevelCard.get_value(),
            "o3HookMode": int(self.o3HookModeCard.get_value()),
        }

    def clearValues(self) -> None:
        """
        ## 清空(还原)内部卡片的配置
        """
        for card in self.cards:
            card.clear()

    def adjustSize(self) -> None:
        h = self.cardLayout.heightForWidth(self.width()) + 46
        return self.resize(self.width(), h)

# -*- coding: utf-8 -*-

"""
## Bot 设置界面, 设置选项卡直接调用 AddBot 中的选项卡
"""
from typing import TYPE_CHECKING

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout
from qfluentwidgets import BodyLabel, CardWidget, FluentIcon, IconWidget, CaptionLabel, FluentIconBase

from src.Core.Config.ConfigModel import Config
from src.Ui.AddPage.Advanced import AdvancedWidget
from src.Ui.AddPage.BotWidget import BotWidget
from src.Ui.AddPage.Connect import ConnectWidget

if TYPE_CHECKING:
    from src.Ui.BotListPage.BotWidget import BotWidget


class AppCard(CardWidget):
    """
    ## BotSetupPage  所需使用的卡片
    """

    def __init__(self, icon: FluentIconBase, title: str, content: str, parent=None) -> None:
        """
        ## 初始化
        """
        super().__init__(parent)
        self.iconWidget = IconWidget(icon, self)
        self.titleLabel = BodyLabel(title, self)
        self.contentLabel = CaptionLabel(content, self)
        self.openLabel = IconWidget(FluentIcon.CHEVRON_RIGHT_MED, self)

        self.hBoxLayout = QHBoxLayout()
        self.vBoxLayout = QVBoxLayout()

        self.setFixedHeight(70)
        self.iconWidget.setFixedSize(24, 24)
        self.openLabel.setFixedSize(12, 12)
        self.contentLabel.setTextColor("#606060", "#d2d2d2")

        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.addWidget(self.titleLabel, 0, Qt.AlignmentFlag.AlignVCenter)
        self.vBoxLayout.addWidget(self.contentLabel, 0, Qt.AlignmentFlag.AlignVCenter)
        self.vBoxLayout.setAlignment(Qt.AlignmentFlag.AlignVCenter)

        self.hBoxLayout.setContentsMargins(20, 10, 30, 10)
        self.hBoxLayout.setSpacing(15)
        self.hBoxLayout.addWidget(self.iconWidget, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addLayout(self.vBoxLayout)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addWidget(self.openLabel, 0, Qt.AlignmentFlag.AlignRight)

        self.setLayout(self.hBoxLayout)


class BotSetupPage(QWidget):
    """
    ## 窗体 Bot List 中, 对应 QQ 的 BotSetupPage
    """

    def __init__(self, config: Config, view: "BotWidget") -> None:
        """
        ## 初始化 BotSetupPage
        """
        super().__init__()
        self.config = config
        self.view = view

        # 调用方法
        self._createSubPages()
        self._createCard()
        self._setLayout()

        # 设置全局唯一名称
        self.setObjectName(f"{self.config.bot.QQID}_BotWidgetPivot_BotSetup")

    def getValue(self) -> dict:
        """
        ## 返回配置结果
        """
        return {
            "bot": self.botWidget.getValue(),
            "connect": self.connectWidget.getValue(),
            "advanced": self.advancedWidget.getValue(),
        }

    def _createCard(self) -> None:
        """
        ## 创建页面内的卡片
        """
        self.botSetupCard = AppCard(FluentIcon.ROBOT, self.tr("Bot settings"), self.tr("Basic settings for the bot"))
        self.connectSetupCard = AppCard(
            FluentIcon.GLOBE, self.tr("Connection settings"), self.tr("Settings related to bot connections")
        )
        self.advancedSetupCard = AppCard(
            FluentIcon.BOOK_SHELF, self.tr("Advanced settings"), self.tr("Advanced bot settings")
        )

        self.botSetupCard.clicked.connect(lambda: self.view.view.setCurrentWidget(self.botWidget))
        self.connectSetupCard.clicked.connect(lambda: self.view.view.setCurrentWidget(self.connectWidget))
        self.advancedSetupCard.clicked.connect(lambda: self.view.view.setCurrentWidget(self.advancedWidget))

        self.botSetupCard.clicked.connect(lambda: self.view.botSetupSubPageReturnButton.show())
        self.connectSetupCard.clicked.connect(lambda: self.view.botSetupSubPageReturnButton.show())
        self.advancedSetupCard.clicked.connect(lambda: self.view.botSetupSubPageReturnButton.show())

        self.botSetupCard.clicked.connect(lambda: self.view.returnListButton.hide())
        self.connectSetupCard.clicked.connect(lambda: self.view.returnListButton.hide())
        self.advancedSetupCard.clicked.connect(lambda: self.view.returnListButton.hide())

    def _createSubPages(self) -> None:
        """
        ## 创建子页面, 并添加到 view 中
            子页面直接使用 AddWidget 中的页面
        """
        self.botWidget = BotWidget(self, self.config.bot)
        self.connectWidget = ConnectWidget("Setup", self, self.config.connect)
        self.advancedWidget = AdvancedWidget("Setup", self, self.config.advanced)

        self.botWidget.view.setObjectName("BotListBotSetupView")
        self.connectWidget.view.setObjectName("BotListConnectSetupView")
        self.advancedWidget.view.setObjectName("BotListAdvancedSetupView")

        self.botWidget.botQQIdCard.lineEdit.setEnabled(False)

        self.view.view.addWidget(self.botWidget)
        self.view.view.addWidget(self.connectWidget)
        self.view.view.addWidget(self.advancedWidget)

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.vBoxLayout = QVBoxLayout()

        self.vBoxLayout.setContentsMargins(0, 5, 0, 5)
        self.vBoxLayout.addWidget(self.botSetupCard)
        self.vBoxLayout.addWidget(self.connectSetupCard)
        self.vBoxLayout.addWidget(self.advancedSetupCard)
        self.vBoxLayout.addStretch(1)

        self.setLayout(self.vBoxLayout)

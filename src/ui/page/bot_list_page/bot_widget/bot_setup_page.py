# -*- coding: utf-8 -*-

"""
## Bot 设置界面, 设置选项卡直接调用 AddBot 中的选项卡
"""
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import BodyLabel, CaptionLabel, CardWidget, FluentIcon, FluentIconBase, IconWidget
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config.config_model import Config
from src.ui.page.add_page.advanced import AdvancedWidget
from src.ui.page.add_page.bot_widget import BotWidget as BotConfigWidget
from src.ui.page.add_page.connect import ConnectWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.page.bot_list_page.bot_widget import bot_widget


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

    def __init__(self, config: Config, view: "bot_widget") -> None:
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
        self.setObjectName(f"{self.config.bot.qq_id}_BotWidgetPivot_BotSetup")

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
        self.botSetupCard = AppCard(FluentIcon.ROBOT, self.tr("基本设置"), self.tr("机器人基本配置项"))
        self.connectSetupCard = AppCard(FluentIcon.GLOBE, self.tr("连接设置"), self.tr("机器人连接配置项"))
        self.advancedSetupCard = AppCard(FluentIcon.BOOK_SHELF, self.tr("高级设置"), self.tr("机器人高级配置项"))

        self.botSetupCard.clicked.connect(lambda: self.view.view.setCurrentWidget(self.botWidget))
        self.connectSetupCard.clicked.connect(lambda: self.view.view.setCurrentWidget(self.connectWidget))
        self.advancedSetupCard.clicked.connect(lambda: self.view.view.setCurrentWidget(self.advancedWidget))

    def _createSubPages(self) -> None:
        """
        ## 创建子页面, 并添加到 view 中
            子页面直接使用 AddWidget 中的页面
        """
        self.botWidget = BotConfigWidget(self, self.config.bot)
        self.connectWidget = ConnectWidget(self, self.config.connect)
        self.advancedWidget = AdvancedWidget(self, self.config.advanced)

        self.botWidget.view.setObjectName("BotListBotSetupView")
        self.connectWidget.cardListPage.setObjectName("BotListConnectSetupView")
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

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
        self.icon_widget = IconWidget(icon, self)
        self.title_label = BodyLabel(title, self)
        self.content_label = CaptionLabel(content, self)
        self.open_label = IconWidget(FluentIcon.CHEVRON_RIGHT_MED, self)

        self.h_box_layout = QHBoxLayout()
        self.v_box_layout = QVBoxLayout()

        self.setFixedHeight(70)
        self.icon_widget.setFixedSize(24, 24)
        self.open_label.setFixedSize(12, 12)
        self.content_label.setTextColor("#606060", "#d2d2d2")

        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(0)
        self.v_box_layout.addWidget(self.title_label, 0, Qt.AlignmentFlag.AlignVCenter)
        self.v_box_layout.addWidget(self.content_label, 0, Qt.AlignmentFlag.AlignVCenter)
        self.v_box_layout.setAlignment(Qt.AlignmentFlag.AlignVCenter)

        self.h_box_layout.setContentsMargins(20, 10, 30, 10)
        self.h_box_layout.setSpacing(15)
        self.h_box_layout.addWidget(self.icon_widget, 0, Qt.AlignmentFlag.AlignRight)
        self.h_box_layout.addLayout(self.v_box_layout)
        self.h_box_layout.addStretch(1)
        self.h_box_layout.addWidget(self.open_label, 0, Qt.AlignmentFlag.AlignRight)

        self.setLayout(self.h_box_layout)


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
        self._create_sub_pages()
        self._create_card()
        self._set_layout()

        # 设置全局唯一名称
        self.setObjectName(f"{self.config.bot.QQID}_BotWidgetPivot_BotSetup")

    def get_value(self) -> dict:
        """
        ## 返回配置结果
        """
        return {
            "bot": self.bot_widget.get_value(),
            "connect": self.connect_widget.get_value(),
            "advanced": self.advanced_widget.get_value(),
        }

    def _create_card(self) -> None:
        """
        ## 创建页面内的卡片
        """
        self.bot_setup_card = AppCard(FluentIcon.ROBOT, self.tr("基本设置"), self.tr("机器人基本配置项"))
        self.connect_setup_card = AppCard(FluentIcon.GLOBE, self.tr("连接设置"), self.tr("机器人连接配置项"))
        self.advanced_setup_card = AppCard(FluentIcon.BOOK_SHELF, self.tr("高级设置"), self.tr("机器人高级配置项"))

        self.bot_setup_card.clicked.connect(lambda: self.view.view.setCurrentWidget(self.bot_widget))
        self.connect_setup_card.clicked.connect(lambda: self.view.view.setCurrentWidget(self.connect_widget))
        self.advanced_setup_card.clicked.connect(lambda: self.view.view.setCurrentWidget(self.advanced_widget))

    def _create_sub_pages(self) -> None:
        """
        ## 创建子页面, 并添加到 view 中
            子页面直接使用 AddWidget 中的页面
        """
        self.bot_widget = BotConfigWidget(self, self.config.bot)
        self.connect_widget = ConnectWidget(self, self.config.connect)
        self.advanced_widget = AdvancedWidget(self, self.config.advanced)

        self.bot_widget.view.setObjectName("BotListBotSetupView")
        self.connect_widget.card_list_page.setObjectName("BotListConnectSetupView")
        self.advanced_widget.view.setObjectName("BotListAdvancedSetupView")

        self.bot_widget.bot_qq_id_card.lineEdit.setEnabled(False)

        self.view.view.addWidget(self.bot_widget)
        self.view.view.addWidget(self.connect_widget)
        self.view.view.addWidget(self.advanced_widget)

    def _set_layout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.v_box_layout = QVBoxLayout()

        self.v_box_layout.setContentsMargins(0, 5, 0, 5)
        self.v_box_layout.addWidget(self.bot_setup_card)
        self.v_box_layout.addWidget(self.connect_setup_card)
        self.v_box_layout.addWidget(self.advanced_setup_card)
        self.v_box_layout.addStretch(1)

        self.setLayout(self.v_box_layout)

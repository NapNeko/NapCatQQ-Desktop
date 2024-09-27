# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from qfluentwidgets import FluentIcon, CaptionLabel, BreadcrumbBar, ToolTipFilter, TransparentToolButton, setFont
from PySide6.QtGui import QFont
from PySide6.QtCore import Qt, Slot
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.BotListPage.BotListWidget import BotListWidget


class BotTopCard(QWidget):
    """
    ## BotListWidget 顶部展示的 InputCard

    用于展示 Breadcrumb navigation 以及一些操作按钮
    """

    def __init__(self, parent: "BotListWidget") -> None:
        super().__init__(parent=parent)

        # 创建所需控件
        self.breadcrumbBar = BreadcrumbBar(self)
        self.subtitleLabel = CaptionLabel(self.tr("您可以在此对机器人进行配置、启动以及管理"), self)
        self.updateListButton = TransparentToolButton(FluentIcon.SYNC, self)  # 刷新列表按钮

        self.hBoxLayout = QHBoxLayout()
        self.labelLayout = QVBoxLayout()
        self.buttonLayout = QHBoxLayout()

        # 设置控件
        setFont(self.breadcrumbBar, 28, QFont.Weight.DemiBold)
        self.breadcrumbBar.addItem(routeKey="BotTopCardTitle", text=self.tr("机器人列表"))
        self.breadcrumbBar.setSpacing(15)
        self.updateListButton.clicked.connect(self._updateListButtonSlot)
        self.breadcrumbBar.currentIndexChanged.connect(self._breadcrumbBarSlot)

        self._addTooltips()
        self._setLayout()

    def addItem(self, route_key: str) -> None:
        """
        ## 给 breadcrumbBar 添加 item 项接口
        """
        self.breadcrumbBar.addItem(route_key, route_key)

    def _addTooltips(self) -> None:
        """
        ## 为按钮添加悬停提示
        """
        # 添加提示
        self.updateListButton.setToolTip(self.tr("点击刷新列表"))
        self.updateListButton.installEventFilter(ToolTipFilter(self.updateListButton))

    @Slot()
    def _breadcrumbBarSlot(self, index: int) -> None:
        """
        ## 判断用户是否点击的是 Bot List
        如果是则返回 Bot List 页面
        """
        if index == 0:
            # 项目内模块导入
            from src.Ui.BotListPage.BotListWidget import BotListWidget

            it(BotListWidget).view.setCurrentIndex(index)
            self.updateListButton.show()

    @staticmethod
    @Slot()
    def _updateListButtonSlot() -> None:
        """
        ## 更新列表按钮的槽函数
        """
        # 项目内模块导入
        from src.Ui.BotListPage.BotListWidget import BotListWidget

        it(BotListWidget).botList.updateList()

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.labelLayout.setSpacing(0)
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.addWidget(self.breadcrumbBar)
        self.labelLayout.addSpacing(5)
        self.labelLayout.addWidget(self.subtitleLabel)

        self.buttonLayout.setSpacing(0)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)
        self.buttonLayout.addWidget(self.updateListButton)
        self.buttonLayout.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignBottom)

        self.hBoxLayout.addLayout(self.labelLayout)
        self.hBoxLayout.addLayout(self.buttonLayout)
        self.hBoxLayout.setContentsMargins(0, 0, 20, 0)

        self.setLayout(self.hBoxLayout)

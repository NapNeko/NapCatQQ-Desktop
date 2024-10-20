# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self, Optional

# 第三方库导入
from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtCore import Slot
from PySide6.QtWidgets import QVBoxLayout

# 项目内模块导入
from src.Core.Config import cfg
from src.Ui.StyleSheet import StyleSheet
from src.Ui.UnitPage.top import TopWidget
from src.Ui.common.widget import BackgroundWidget
from src.Ui.UnitPage.QQPage import QQPage
from src.Ui.UnitPage.DLCPage import DLCPage
from src.Ui.UnitPage.NCDPage import NCDPage
from src.Core.Utils.GetVersion import GetVersion
from src.Ui.UnitPage.NapCatPage import NapCatPage
from src.Ui.common.stacked_widget import TransparentStackedWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


class UnitWidget(BackgroundWidget):

    def __init__(self) -> None:
        super().__init__()
        # 预定义控件
        self.view: Optional[TransparentStackedWidget] = None
        self.topCard: Optional[TopWidget] = None
        self.vBoxLayout: Optional[QVBoxLayout] = None

        # 传入配置
        self.bgEnabledConfig = cfg.bgUnitPage
        self.bgPixmapLightConfig = cfg.bgUnitPageLight
        self.bgPixmapDarkConfig = cfg.bgUnitPageDark
        self.bgOpacityConfig = cfg.bgUnitPageOpacity

        # 调用方法
        super().updateBgImage()

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        # 创建布局和控件
        self.vBoxLayout = QVBoxLayout(self)
        self.topCard = TopWidget(self)
        self.getVersion = GetVersion(self)
        self._createView()

        # 调用方法
        self.setParent(parent)
        self.setObjectName("UpdatePage")
        self.view.setObjectName("UpdateView")
        self._setLayout()
        self.topCard.updateButton.clicked.connect(self.updateSlot)

        # 创建获取版本功能
        self.getVersion.remoteUpdateFinish.connect(self.napcatPage.updateRemoteVersion)
        self.getVersion.remoteUpdateFinish.connect(self.qqPage.updateRemoteVersion)
        self.getVersion.remoteUpdateFinish.connect(self.ncdPage.updateRemoteVersion)
        self.getVersion.remoteUpdateFinish.connect(lambda: self.topCard.updateButton.setEnabled(True))

        self.getVersion.localUpdateFinish.connect(self.napcatPage.updateLocalVersion)
        self.getVersion.localUpdateFinish.connect(self.qqPage.updateLocalVersion)
        self.getVersion.localUpdateFinish.connect(self.ncdPage.updateLocalVersion)

        # 启用一次更新
        self.updateSlot()

        # 应用样式表
        StyleSheet.UNIT_WIDGET.apply(self)

        return self

    def _createView(self) -> None:
        """
        ## 创建并配置 QStackedWidget
        """
        self.view = TransparentStackedWidget()
        self.napcatPage = NapCatPage(self)
        self.napcatDLCPage = DLCPage(self)
        self.qqPage = QQPage(self)
        self.ncdPage = NCDPage(self)

        self.view.addWidget(self.napcatPage)
        self.view.addWidget(self.napcatDLCPage)
        self.view.addWidget(self.qqPage)
        self.view.addWidget(self.ncdPage)

        self.topCard.pivot.addItem(
            routeKey=self.napcatPage.objectName(),
            text=self.tr("NapCat"),
            onClick=lambda: self.view.setCurrentWidget(self.napcatPage),
        )
        self.topCard.pivot.addItem(
            routeKey=self.napcatDLCPage.objectName(),
            text=self.tr("NapCat DLC"),
            onClick=lambda: self.view.setCurrentWidget(self.napcatDLCPage),
        )
        self.topCard.pivot.addItem(
            routeKey=self.qqPage.objectName(),
            text=self.tr("QQ"),
            onClick=lambda: self.view.setCurrentWidget(self.qqPage),
        )
        self.topCard.pivot.addItem(
            routeKey=self.ncdPage.objectName(),
            text=self.tr("Desktop"),
            onClick=lambda: self.view.setCurrentWidget(self.ncdPage),
        )

        # 链接信号并初始化当前标签页
        self.view.currentChanged.connect(self.onCurrentIndexChanged)
        self.view.setCurrentWidget(self.napcatPage)
        self.topCard.pivot.setCurrentItem(self.napcatPage.objectName())

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.view)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)

    def onCurrentIndexChanged(self, index) -> None:
        """
        ## 切换 Pivot 和 view 的槽函数
        """
        widget = self.view.widget(index)
        self.topCard.pivot.setCurrentItem(widget.objectName())

    @Slot()
    def updateSlot(self) -> None:
        """
        ## 刷新页面
        """
        self.getVersion.update()
        self.topCard.updateButton.setEnabled(False)


class UnitWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.UnitPage.view", "UnitWidget"),)

    # 静态方法available()，用于检查模块"BotListWidget"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.UnitWidget.view")

    # 静态方法create()，用于创建BotListWidget类的实例，返回值为BotListWidget对象。
    @staticmethod
    def create(create_type: list[UnitWidget]) -> UnitWidget:
        return UnitWidget()


add_creator(UnitWidgetClassCreator)

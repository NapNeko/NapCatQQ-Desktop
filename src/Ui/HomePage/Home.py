# -*- coding: utf-8 -*-

"""
主页
"""
from abc import ABC
from typing import TYPE_CHECKING, Self, Optional

from creart import it, add_creator, exists_module
from PySide6.QtGui import QPixmap, QPainter
from creart.creator import AbstractCreator, CreateTargetInfo
from PySide6.QtCore import Qt
from qfluentwidgets import PushButton, isDarkTheme
from PySide6.QtWidgets import QStackedWidget

from src.Core.Config import StartOpenHomePageViewEnum as SEnum
from src.Core.Config import cfg
from src.Ui.StyleSheet import StyleSheet
from src.Ui.common.info_bar import info_bar
from src.Ui.HomePage.ContentView import ContentViewWidget
from src.Ui.HomePage.DisplayView import DisplayViewWidget
from src.Ui.HomePage.DownloadView import DownloadViewWidget

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class HomeWidget(QStackedWidget):

    def __init__(self) -> None:
        super().__init__()
        self.displayView: Optional[DisplayViewWidget] = None
        self.contentView: Optional[ContentViewWidget] = None
        self.downloadView: Optional[DownloadViewWidget] = None

        # 加载背景图片
        self.bgPixmap = None
        self._bgPixmapLight = QPixmap(":Global/image/Global/page_bg_light.png")
        self._bgPixmapDark = QPixmap(":Global/image/Global/page_bg_dark.png")

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        # 创建控件
        self.displayView = DisplayViewWidget()
        self.contentView = ContentViewWidget()
        self.downloadView = DownloadViewWidget()

        # 设置控件
        self.setParent(parent)
        self.setObjectName("HomePage")
        self.addWidget(self.displayView)
        self.addWidget(self.contentView)
        self.addWidget(self.downloadView)
        self.setCurrentWidget(self.contentView)
        self.displayView.goBtnSignal.connect(self._goBtnSlot)

        # 调用方法
        self.chooseView()
        self.updateBgImage()

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

        return self

    def _goBtnSlot(self) -> None:
        """
        ## Start Using 的槽函数
        """
        self.setCurrentWidget(self.contentView)
        if cfg.get(cfg.HideUsGoBtnTips):
            # 是否隐藏提示
            return
        info_bar(self.tr("您可以选择在设置页面中选择启动时的展示页💕"))

    def chooseView(self) -> None:
        """
        判断并加载相应的 Widget。
        根据配置确定是打开首页视图还是内容视图。
        """
        match cfg.get(cfg.StartOpenHomePageView):
            case SEnum.DISPLAY_VIEW:
                self.setCurrentWidget(self.displayView)
            case SEnum.CONTENT_VIEW:
                self.setCurrentWidget(self.contentView)

    def updateBgImage(self) -> None:
        """
        用于更新图片大小
        """
        # 重新加载图片保证缩放后清晰
        if not isDarkTheme():
            self.bgPixmap = self._bgPixmapLight
        else:
            self.bgPixmap = self._bgPixmapDark

        self.bgPixmap = self.bgPixmap.scaled(
            self.size(),
            aspectMode=Qt.AspectRatioMode.KeepAspectRatioByExpanding,  # 等比缩放
            mode=Qt.TransformationMode.SmoothTransformation,  # 平滑效果
        )
        self.update()

    def paintEvent(self, event) -> None:
        """
        重写绘制事件绘制背景图片
        """
        painter = QPainter(self)
        painter.drawPixmap(self.rect(), self.bgPixmap)
        super().paintEvent(event)

    def resizeEvent(self, event) -> None:
        """
        重写缩放事件
        """
        self.updateBgImage()
        super().resizeEvent(event)


class HomeWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.HomePage.Home", "HomeWidget"),)

    # 静态方法available()，用于检查模块"HomeWidget"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.HomePage.Home")

    # 静态方法create()，用于创建HomeWidget类的实例，返回值为HomeWidget对象。
    @staticmethod
    def create(create_type: [HomeWidget]) -> HomeWidget:
        return HomeWidget()


add_creator(HomeWidgetClassCreator)

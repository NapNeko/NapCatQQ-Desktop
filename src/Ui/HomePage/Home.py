# -*- coding: utf-8 -*-

"""
主页
"""
from abc import ABC
from typing import TYPE_CHECKING, Self

from PySide6.QtCore import Qt
from PySide6.QtGui import QPixmap, QPainter
from PySide6.QtWidgets import QStackedWidget
from creart import add_creator, exists_module, it
from creart.creator import AbstractCreator, CreateTargetInfo
from qfluentwidgets import isDarkTheme, InfoBar, InfoBarIcon, InfoBarPosition, PushButton

from src.Core.Config import StartOpenHomePageViewEnum as SE
from src.Core.Config import cfg
from src.Ui.HomePage.ContentView import ContentViewWidget
from src.Ui.HomePage.DisplayView import DisplayViewWidget
from src.Ui.StyleSheet import StyleSheet

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class HomeWidget(QStackedWidget):

    def __init__(self):
        super().__init__()
        self.contentView = None
        self.displayView = None

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

        # 设置控件
        self.setParent(parent)
        self.setObjectName("HomePage")
        self.addWidget(self.displayView)
        self.addWidget(self.contentView)
        self.displayView.goBtnSignal.connect(self._goBtnSlot)

        # 调用方法
        self._chooseView()
        self.updateBgImage()

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

        return self

    def _goBtnSlot(self):
        """
        ## Start Using 的槽函数
        """
        self.setCurrentWidget(self.contentView)

        if cfg.get(cfg.HideUsGoBtnTips):
            # 是否隐藏提示
            return

        info = InfoBar(
            icon=InfoBarIcon.INFORMATION,
            title="Tips",
            content=self.tr("You can choose the page to display at \n" "startup in the settings page"),
            orient=Qt.Orientation.Vertical,
            isClosable=True,
            position=InfoBarPosition.BOTTOM_RIGHT,
            duration=10000,
            parent=self,
        )
        info_button = PushButton(self.tr("Don't show again"), self)
        info_button.clicked.connect(lambda: cfg.set(cfg.HideUsGoBtnTips, True, True))
        info_button.clicked.connect(info.close)
        info.addWidget(info_button)
        info.show()

    def _chooseView(self) -> None:
        """
        判断并加载相应的 Widget。
        根据配置确定是打开首页视图还是内容视图。
        """
        match cfg.get(cfg.StartOpenHomePageView):
            case SE.DISPLAY_VIEW:
                self.setCurrentWidget(self.displayView)
            case SE.CONTENT_VIEW:
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

    def showInfo(self, title: str, content: str):
        """
        # 配置 InfoBar 的一些配置, 简化内部使用 InfoBar 的步骤
        """
        from src.Ui.MainWindow.Window import MainWindow
        it(MainWindow).showInfo(
            title=title,
            content=content,
            showcasePage=self
        )

    def showError(self, title: str, content: str):
        """
        # 配置 InfoBar 的一些配置, 简化内部使用 InfoBar 的步骤
        """
        from src.Ui.MainWindow.Window import MainWindow
        it(MainWindow).showError(
            title=title,
            content=content,
            showcasePage=self
        )

    def showWarning(self, title: str, content: str):
        """
        # 配置 InfoBar 的一些配置, 简化内部使用 InfoBar 的步骤
        """
        from src.Ui.MainWindow.Window import MainWindow
        it(MainWindow).showWarning(
            title=title,
            content=content,
            showcasePage=self
        )

    def showSuccess(self, title: str, content: str):
        """
        # 配置 InfoBar 的一些配置, 简化内部使用 InfoBar 的步骤
        """
        from src.Ui.MainWindow.Window import MainWindow
        it(MainWindow).showSuccess(
            title=title,
            content=content,
            showcasePage=self
        )

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

# -*- coding: utf-8 -*-

"""
## 主页
"""
# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self, Optional

# 第三方库导入
from creart import add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from qfluentwidgets import isDarkTheme
from PySide6.QtGui import QPixmap, QRegion, QPainter, QPainterPath
from PySide6.QtCore import Qt, QPoint, QRectF
from PySide6.QtWidgets import QStackedWidget

# 项目内模块导入
from src.Core.Config import cfg
from src.Ui.StyleSheet import StyleSheet
from src.Ui.HomePage.ContentView import ContentViewWidget
from src.Ui.HomePage.DisplayView import DisplayViewWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


class HomeWidget(QStackedWidget):

    def __init__(self) -> None:
        super().__init__()
        self.displayView: Optional[DisplayViewWidget] = None
        # self.contentView: Optional[ContentViewWidget] = None

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        # 创建控件
        self.displayView = DisplayViewWidget()
        # self.contentView = ContentViewWidget()

        # 设置控件
        self.setParent(parent)
        self.setObjectName("HomePage")
        self.addWidget(self.displayView)
        # self.addWidget(self.contentView)
        self.setCurrentWidget(self.displayView)

        # 链接信号
        self.displayView.buttonGroup.goButton.clicked.connect(lambda: self.parent().setCurrentIndex(1))

        # 调用方法
        self.updateBaseBgImage()

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

        return self

    def updateBaseBgImage(self) -> None:
        """
        ## 更新背景图片
        """
        if cfg.get(cfg.bgHomePage):
            self._bgPixmapLight = QPixmap(cfg.get(cfg.bgHomePageLight))
            self._bgPixmapDark = QPixmap(cfg.get(cfg.bgHomePageDark))
        else:
            self._bgPixmapLight = QPixmap(":Global/image/Global/page_bg_light.png")
            self._bgPixmapDark = QPixmap(":Global/image/Global/page_bg_dark.png")
        self.updateBgImage()

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
            aspectMode=Qt.AspectRatioMode.IgnoreAspectRatio,  # 等比缩放
            mode=Qt.TransformationMode.FastTransformation,  # 平滑效果
        )
        self.update()

    def paintEvent(self, event) -> None:
        """
        重写绘制事件绘制背景图片
        """
        painter = QPainter(self)
        rect = self.rect()
        radius = 8  # 圆角半径，可以根据需要调整

        # 创建一个路径，包含全局区域，除了左上角之外
        path = QPainterPath()
        path.addRect(rect)

        # 创建左上角的圆角矩形路径
        corner_rect = QRectF(rect.left(), rect.top(), 2 * radius, 2 * radius)
        corner_path = QPainterPath()
        corner_path.moveTo(corner_rect.topLeft())
        corner_path.arcTo(corner_rect, 90, 90)
        corner_path.lineTo(rect.topLeft())
        corner_path.closeSubpath()

        # 从全局路径中减去左上角圆角路径
        path = path.subtracted(corner_path)

        # 设置剪裁区域为非左上角区域，这样左上角就有了圆角效果
        region = QRegion(path.toFillPolygon().toPolygon())
        painter.setClipRegion(region)

        # 设置图片透明度
        painter.setOpacity(cfg.get(cfg.bgHomePageOpacity) / 100 if cfg.get(cfg.bgHomePage) else 1)

        # 绘制背景图像
        painter.drawPixmap(rect, self.bgPixmap)

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

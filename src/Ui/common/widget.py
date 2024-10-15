# -*- coding: utf-8 -*-
# 标准库导入
from typing import Optional

# 第三方库导入
from qfluentwidgets import ConfigItem, isDarkTheme
from PySide6.QtGui import QPixmap, QRegion, QPainter, QPainterPath
from PySide6.QtCore import Qt, QRectF
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.Core.Config import cfg


class BackgroundWidget(QWidget):

    def __init__(self) -> None:
        """
        ## 初始化
        """
        super().__init__()
        self.bgPixmap: Optional[QPixmap] = None

        self.enabledDefaultBg = False
        self.bgDefaultLight: Optional[str] = None
        self.bgDefaultDark: Optional[str] = None

        self.bgEnabledConfig: Optional[ConfigItem] = None
        self.bgPixmapLightConfig: Optional[ConfigItem] = None
        self.bgPixmapDarkConfig: Optional[ConfigItem] = None
        self.bgOpacityConfig: Optional[ConfigItem] = None

    def updateBgImage(self) -> None:
        """
        ## 更新背景图片
        """
        if cfg.get(self.bgEnabledConfig):
            self._bgPixmapLight = QPixmap(cfg.get(self.bgPixmapLightConfig))
            self._bgPixmapDark = QPixmap(cfg.get(self.bgPixmapDarkConfig))
            self.updateBgImageSize()
            return
        elif self.enabledDefaultBg:
            self._bgPixmapLight = QPixmap(self.bgDefaultLight)
            self._bgPixmapDark = QPixmap(self.bgDefaultDark)
            self.updateBgImageSize()
            return

    def updateBgImageSize(self) -> None:
        """
        ## 用于更新图片大小
        """
        if not self.enabledDefaultBg and not cfg.get(self.bgEnabledConfig):
            return  # 如果没有启用背景图片则不进行操作

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
        if not self.enabledDefaultBg and not cfg.get(self.bgEnabledConfig):
            super().paintEvent(event)
            return
        elif self.bgPixmap is None:
            super().paintEvent(event)
            return
        else:
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
            painter.setOpacity(cfg.get(self.bgOpacityConfig) / 100 if cfg.get(self.bgEnabledConfig) else 1)

            # 绘制背景图像
            painter.drawPixmap(rect, self.bgPixmap)

            super().paintEvent(event)

    def resizeEvent(self, event) -> None:
        """
        重写缩放事件
        """
        if self.enabledDefaultBg or cfg.get(self.bgEnabledConfig):
            self.updateBgImageSize()
        super().resizeEvent(event)

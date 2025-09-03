# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import TabBar
from qfluentwidgets.common import Theme
from qfluentwidgets.window import MSFluentTitleBar
from qframelesswindow.titlebar import CloseButton, MaximizeButton, MinimizeButton
from PySide6.QtGui import QPen, QPainter, QPaintEvent, QPainterPath
from PySide6.QtSvg import QSvgRenderer
from PySide6.QtCore import Qt, QPoint, QRectF, QPointF

# 项目内模块导入
from src.core.config import cfg
from src.ui.common.icon import NapCatDesktopIcon
from src.core.config.enum import CloseActionEnum

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import main_window


class CustomTitleBar(MSFluentTitleBar):

    def __init__(self, parent: "main_window") -> None:
        """
        ## 自定义一个标题栏控件
        """
        super().__init__(parent)

        self.set_title()
        self.set_buttons()

    def set_title(self) -> None:
        self.setTitle("NapCatQQ Desktop")
        self.setIcon(NapCatDesktopIcon.LOGO.path(Theme.LIGHT))

        self.tabBar = TabBar(self)
        self.tabBar.setMovable(cfg.get(cfg.titleTabBarMovable))
        self.tabBar.setTabMaximumWidth(cfg.get(cfg.titleTabBarMaxWidth))
        self.tabBar.setTabMinimumWidth(cfg.get(cfg.titleTabBarMinWidth))
        self.tabBar.setTabShadowEnabled(cfg.get(cfg.titleTabBarShadow))
        self.tabBar.setScrollable(cfg.get(cfg.titleTabBarScrollable))
        self.tabBar.setCloseButtonDisplayMode(cfg.get(cfg.titleTabBarCloseMode))
        self.tabBar.setAddButtonVisible(False)
        self.tabBar.tabCloseRequested.connect(self.tabBar.removeTab)

        self.hBoxLayout.insertWidget(4, self.tabBar, 1)

    def set_buttons(self) -> None:
        """配置窗口控制按钮并初始化槽连接"""
        button_info = {
            "minBtn": (MinBtn, self.window().showMinimized),
            "maxBtn": (MaxBtn, self.__toggle_maximization),
            "closeBtn": (CloseBtn, self.__close),
        }

        for btn_name, (btn_class, slot) in button_info.items():
            self.__replace_button(btn_class, btn_name)
            getattr(self, btn_name).clicked.connect(slot)

        self.buttonLayout.setContentsMargins(0, 8, 10, 0)

    def __replace_button(self, btn_class, btn_name: str) -> None:
        """替换指定的按钮"""
        old_btn = getattr(self, btn_name, None)
        if old_btn:
            self.buttonLayout.removeWidget(old_btn)
            old_btn.close()

        new_btn = btn_class(self)
        new_btn.setFixedHeight(32)
        self.buttonLayout.addWidget(new_btn)
        setattr(self, btn_name, new_btn)

    def __toggle_maximization(self) -> None:
        """切换窗口的最大化和正常状态"""
        if self.window().isMaximized():
            self.window().showNormal()
        else:
            self.window().showMaximized()

    def __close(self) -> None:
        """关闭窗口"""
        if cfg.get(cfg.closeBtnAction) == CloseActionEnum.CLOSE:
            self.window().close()
        else:
            self.window().hide()

    def canDrag(self, pos: QPoint) -> bool:
        if not super().canDrag(pos):
            return False

        pos.setX(pos.x() - self.tabBar.x())
        return not self.tabBar.tabRegion().contains(pos)

    def resizeEvent(self, event) -> None:
        super().resizeEvent(event)
        self.tabBar.setMinimumWidth(self.width() - 400)


class MaxBtn(MaximizeButton):

    def __init__(self, parent) -> None:
        """
        重新绘制放大按钮
        :param parent:
        """
        super().__init__(parent=parent)

    def paintEvent(self, event: QPaintEvent) -> None:
        painter = QPainter(self)
        color, background_color = self._getColors()

        # 绘制背景
        painter.setBrush(background_color)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRoundedRect(self.rect(), 4, 4)

        # 绘制图标
        painter.setBrush(Qt.BrushStyle.NoBrush)
        pen = QPen(color, 1)
        pen.setCosmetic(True)
        painter.setPen(pen)

        r = self.devicePixelRatioF()
        painter.scale(1 / r, 1 / r)
        if not self._isMax:
            painter.drawRect(int(18 * r), int(11 * r), int(10 * r), int(10 * r))
        else:
            painter.drawRect(int(18 * r), int(13 * r), int(8 * r), int(8 * r))
            x0 = int(18 * r) + int(2 * r)
            y0 = 13 * r
            dw = int(2 * r)
            path = QPainterPath(QPointF(x0, y0))
            path.lineTo(x0, y0 - dw)
            path.lineTo(x0 + 8 * r, y0 - dw)
            path.lineTo(x0 + 8 * r, y0 - dw + 8 * r)
            path.lineTo(x0 + 8 * r - dw, y0 - dw + 8 * r)
            painter.drawPath(path)


class MinBtn(MinimizeButton):
    def __init__(self, parent) -> None:
        super().__init__(parent=parent)

    def paintEvent(self, event: QPaintEvent) -> None:
        painter = QPainter(self)
        color, background_color = self._getColors()

        # 绘制背景
        painter.setBrush(background_color)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRoundedRect(self.rect(), 4, 4)

        # 绘制图标
        painter.setBrush(Qt.BrushStyle.NoBrush)
        pen = QPen(color, 1)
        pen.setCosmetic(True)
        painter.setPen(pen)
        painter.drawLine(18, 16, 28, 16)


class CloseBtn(CloseButton):

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)

    def paintEvent(self, event: QPaintEvent) -> None:
        painter = QPainter(self)
        painter.setRenderHints(QPainter.RenderHint.Antialiasing | QPainter.RenderHint.SmoothPixmapTransform)
        color, background_color = self._getColors()

        # draw background
        painter.setBrush(background_color)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRoundedRect(self.rect(), 4, 4)

        # draw icon
        color = color.name()
        path_nodes = self._svgDom.elementsByTagName("path")
        for i in range(path_nodes.length()):
            element = path_nodes.at(i).toElement()
            element.setAttribute("stroke", color)

        renderer = QSvgRenderer(self._svgDom.toByteArray())
        renderer.render(painter, QRectF(self.rect()))

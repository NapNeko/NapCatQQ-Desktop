# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import MSFluentTitleBar, TabBar, Theme
from qframelesswindow.titlebar import CloseButton, MaximizeButton, MinimizeButton, TitleBarButton
from PySide6.QtCore import QPoint, QPointF, QRectF, Qt
from PySide6.QtGui import QPainter, QPainterPath, QPaintEvent, QPen, QResizeEvent
from PySide6.QtSvg import QSvgRenderer

# 项目内模块导入
from src.core.config import cfg
from src.core.config.enum import CloseActionEnum
from src.ui.common.icon import NapCatDesktopIcon

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import MainWindow


"""NapCatQQ Desktop 主窗口标题栏模块

该模块定义了一个自定义的标题栏类 CustomTitleBar
继承自 MSFluentTitleBar, 并包含一个标签栏 TabBar 用于管理标签页

Attributes:
    CustomTitleBar (MSFluentTitleBar): 主窗口的自定义标题栏类
    MaxBtn (MaximizeButton): 自定义的最大化按钮类
    MinBtn (MinimizeButton): 自定义的最小化按钮类
    CloseBtn (CloseButton): 自定义的关闭按钮类
"""


class CustomTitleBar(MSFluentTitleBar):
    """主窗口的标题栏, 继承自 MSFluentTitleBar

    Attributes:
        tab_bar (TabBar): 标题栏中的标签栏

    """

    tab_bar: TabBar

    def __init__(self, parent: "MainWindow") -> None:
        """初始化标题栏, 执行标题栏和按钮的配置

        Args:
            parent (MainWindow): 主窗体
        """
        super().__init__(parent)

        self.setup_title()
        self.setup_buttons()

    def setup_title(self) -> None:
        """设置标题栏标题和图标, 并配置标签栏

        该方法会在标题栏中插入一个 TabBar 组件, 用于显示和管理标签页
        """
        self.setTitle("NapCatQQ Desktop")
        self.setIcon(NapCatDesktopIcon.LOGO.path(Theme.LIGHT))

        self.tab_bar = TabBar(self)
        self.tab_bar.setMovable(cfg.get(cfg.title_tab_bar_movable))
        self.tab_bar.setTabMaximumWidth(cfg.get(cfg.title_tab_bar_max_width))
        self.tab_bar.setTabMinimumWidth(cfg.get(cfg.title_tab_bar_min_width))
        self.tab_bar.setTabShadowEnabled(cfg.get(cfg.title_tab_bar_shadow))
        self.tab_bar.setScrollable(cfg.get(cfg.title_tab_bar_scrollable))
        self.tab_bar.setCloseButtonDisplayMode(cfg.get(cfg.title_tab_bar_close_mode))
        self.tab_bar.setAddButtonVisible(False)
        self.tab_bar.tabCloseRequested.connect(self.tab_bar.removeTab)

        self.hBoxLayout.insertWidget(4, self.tab_bar, 1)

    def setup_buttons(self) -> None:
        """设置标题栏按钮

        该方法会替换默认的最小化、最大化和关闭按钮, 并连接相应的槽函数
        """
        button_info = {
            "minBtn": (MinBtn, self.window().showMinimized),
            "maxBtn": (
                MaxBtn,
                # 虽然可以简写,但是格式化后会被压缩成一行,不利于阅读
                lambda: (
                    self.window().showNormal()
                    if (self.window().isMaximized() == True)
                    else self.window().showMaximized()
                ),
            ),
            "closeBtn": (
                CloseBtn,
                lambda: (
                    self.window().close()
                    if cfg.get(cfg.close_button_action) == CloseActionEnum.CLOSE
                    else self.window().hide()
                ),
            ),
        }

        for btn_name, (btn_class, slot) in button_info.items():
            self._replace_button(btn_class, btn_name)
            getattr(self, btn_name).clicked.connect(slot)

        self.buttonLayout.setContentsMargins(0, 8, 10, 0)

    def canDrag(self, pos: QPoint) -> bool:
        """判断是否可以拖动窗口

        检查用户鼠标是否是在拖动区域内, 如果在标签栏区域内则不允许拖动窗口

        Args:
            pos (QPoint): 鼠标位置

        Returns:
            bool: 是否可以拖动窗口
        """

        if not super().canDrag(pos):
            return False

        pos.setX(pos.x() - self.tab_bar.x())
        return not self.tab_bar.tabRegion().contains(pos)

    def resizeEvent(self, event: QResizeEvent) -> None:
        """重写窗口大小调整事件

        该方法会在窗口大小调整时, 动态调整标签栏的最小宽度, 以适应窗口大小变化

        Args:
            event (QResizeEvent): 窗口大小调整事件
        """

        super().resizeEvent(event)
        self.tab_bar.setMinimumWidth(self.width() - 400)

    def _replace_button(self, btn_class: TitleBarButton, btn_name: str) -> None:
        """替换标题栏按钮

        该方法会移除旧的按钮, 并添加新的按钮到标题栏中

        Args:
            btn_class (TitleBarButton): 替换后的按钮类
            btn_name (str): 按钮属性名称
        """

        # 移除旧按钮
        if old_btn := getattr(self, btn_name, None):
            self.buttonLayout.removeWidget(old_btn)
            old_btn.close()

        # 添加新按钮
        new_btn = btn_class(self)
        new_btn.setFixedHeight(32)
        self.buttonLayout.addWidget(new_btn)
        setattr(self, btn_name, new_btn)


class MaxBtn(MaximizeButton):
    """最大化按钮

    该按钮相较于 MaximizeButton 增加了圆角矩形背景

    Attributes:
        _isMax (bool): 当前窗口是否处于最大化状态
    """

    _isMax: bool = False

    def __init__(self, parent: CustomTitleBar) -> None:
        """初始化最大化按钮

        Args:
            parent (CustomTitleBar): 按钮的父组件
        """
        super().__init__(parent=parent)

    def paintEvent(self, event: QPaintEvent) -> None:
        """重写绘制事件

        该方法会根据当前窗口状态绘制不同的图标, 并添加圆角矩形背景

        Args:
            event (QPaintEvent): 绘制事件
        """
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
    """最小化按钮

    该按钮相较于 MinimizeButton 增加了圆角矩形背景
    """

    def __init__(self, parent: CustomTitleBar) -> None:
        """初始化最小化按钮

        Args:
            parent (CustomTitleBar): 按钮的父组件
        """
        super().__init__(parent=parent)

    def paintEvent(self, event: QPaintEvent) -> None:
        """重写绘制事件

        该方法会绘制一个带有圆角矩形背景的最小化图标

        Args:
            event (QPaintEvent): 绘制事件
        """
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
    """关闭按钮

    该按钮相较于 CloseButton 增加了圆角矩形背景

    Attributes:
        _svgDom (QDomDocument): 用于存储 SVG 图标数据的 DOM 对象
    """

    def __init__(self, parent: QPaintEvent) -> None:
        """初始化关闭按钮

        Args:
            parent (QPaintEvent): 按钮的父组件
        """
        super().__init__(parent=parent)

    def paintEvent(self, event: QPaintEvent) -> None:
        """重写绘制事件

        该方法会绘制一个带有圆角矩形背景的关闭图标

        Args:
            event (QPaintEvent): 绘制事件
        """
        painter = QPainter(self)
        painter.setRenderHints(QPainter.RenderHint.Antialiasing | QPainter.RenderHint.SmoothPixmapTransform)
        color, background_color = self._getColors()

        # 绘制背景
        painter.setBrush(background_color)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRoundedRect(self.rect(), 4, 4)

        # 绘制图标
        path_nodes = self._svgDom.elementsByTagName("path")
        for i in range(path_nodes.length()):
            element = path_nodes.at(i).toElement()
            element.setAttribute("stroke", color.name())

        renderer = QSvgRenderer(self._svgDom.toByteArray())
        renderer.render(painter, QRectF(self.rect()))

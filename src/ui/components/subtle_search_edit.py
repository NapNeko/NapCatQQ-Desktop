# -*- coding: utf-8 -*-
"""轻量搜索输入框组件.

提供一个视觉上不抢眼的搜索输入框, 适用于侧边栏等需要低调搜索功能的场景.
相比 qfluentwidgets.SearchLineEdit, 本组件:
- 无明显边框, 仅有淡色底色
- 搜索图标更小更淡
- 整体视觉权重更低, 不会抢夺列表内容的注意力
"""
from __future__ import annotations

from PySide6.QtCore import QRectF, Qt, Signal
from PySide6.QtGui import QColor, QPainter, QPaintEvent
from PySide6.QtWidgets import QHBoxLayout, QLineEdit, QWidget
from qfluentwidgets import FluentIcon, isDarkTheme
from qfluentwidgets.common.icon import drawIcon

from src.ui.common.style_sheet import WidgetStyleSheet


class SubtleSearchEdit(QWidget):
    """轻量搜索输入框.

    视觉上低调的搜索框, 适合放在侧边栏列表顶部.
    淡色背景 + 小搜索图标 + 无明显边框, 不抢夺视觉焦点.

    Signals:
        textChanged: 文本变化时发射, 携带当前文本.
    """

    textChanged = Signal(str)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._setup_ui()
        self.setFixedHeight(32)

    def _setup_ui(self) -> None:
        """构建 UI."""
        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        self._line_edit = QLineEdit(self)
        self._line_edit.setObjectName("subtleSearchLineEdit")
        self._line_edit.setFrame(False)
        self._line_edit.setClearButtonEnabled(True)
        layout.addWidget(self._line_edit)

        # 为搜索图标留出左侧空间
        self._line_edit.setTextMargins(22, 0, 0, 0)

        self._line_edit.textChanged.connect(self.textChanged.emit)

        # 应用 QSS
        WidgetStyleSheet.SUBTLE_SEARCH_EDIT.apply(self)

    # ------------------------------------------------------------------
    # 公共接口
    # ------------------------------------------------------------------

    def setPlaceholderText(self, text: str) -> None:
        """设置占位文本.

        Args:
            text: 占位提示文本.
        """
        self._line_edit.setPlaceholderText(text)

    def text(self) -> str:
        """返回当前输入文本."""
        return self._line_edit.text()

    def clear(self) -> None:
        """清空输入框."""
        self._line_edit.clear()

    def setText(self, text: str) -> None:
        """设置输入文本.

        Args:
            text: 要设置的文本.
        """
        self._line_edit.setText(text)

    # ------------------------------------------------------------------
    # 绘制
    # ------------------------------------------------------------------

    def paintEvent(self, event: QPaintEvent) -> None:
        """绘制淡色圆角背景和搜索图标."""
        painter = QPainter(self)
        painter.setRenderHints(QPainter.RenderHint.Antialiasing)

        # 绘制淡色背景
        if isDarkTheme():
            bg_color = QColor(255, 255, 255, 15)
        else:
            bg_color = QColor(0, 0, 0, 10)

        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(bg_color)
        painter.drawRoundedRect(self.rect(), 6, 6)

        # 绘制搜索图标 (左侧居中)
        icon_size = 14
        icon_x = 8
        icon_y = (self.height() - icon_size) / 2.0
        icon_rect = QRectF(icon_x, icon_y, icon_size, icon_size)
        drawIcon(FluentIcon.SEARCH, painter, icon_rect)

        painter.end()

# -*- coding: utf-8 -*-
"""通用骨架屏组件。"""

# 标准库导入
import math
from dataclasses import dataclass
from typing import Callable, Sequence

# 第三方库导入
from qfluentwidgets import isDarkTheme
from PySide6.QtCore import QTimer, Qt
from PySide6.QtGui import QColor, QPainter, QPaintEvent
from PySide6.QtWidgets import QWidget


@dataclass(frozen=True)
class SkeletonShape:
    """骨架元素描述。"""

    x: int
    y: int
    width: int
    height: int
    emphasis: float = 1.0
    radius: int | None = None


class SkeletonWidget(QWidget):
    """带有低对比呼吸动画的通用骨架屏。"""

    def __init__(
        self,
        shape_builder: Callable[[QWidget], Sequence[SkeletonShape]],
        parent: QWidget | None = None,
        *,
        panel_margin: int = 14,
        panel_radius: int = 18,
    ) -> None:
        super().__init__(parent)
        self._shape_builder = shape_builder
        self._panel_margin = panel_margin
        self._panel_radius = panel_radius
        self._phase = 0.0

        self._timer = QTimer(self)
        self._timer.setInterval(24)
        self._timer.timeout.connect(self._advance_phase)

        self.setAttribute(Qt.WidgetAttribute.WA_StyledBackground, False)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)

    def showEvent(self, event) -> None:
        super().showEvent(event)
        if not self._timer.isActive():
            self._timer.start()

    def hideEvent(self, event) -> None:
        self._timer.stop()
        super().hideEvent(event)

    def _advance_phase(self) -> None:
        self._phase += 0.012
        if self._phase > 1.0:
            self._phase = 0.0
        self.update()

    def _base_color(self) -> QColor:
        return QColor(255, 255, 255, 18) if isDarkTheme() else QColor(15, 23, 42, 10)

    def _highlight_color(self) -> QColor:
        return QColor(255, 255, 255, 42) if isDarkTheme() else QColor(255, 255, 255, 84)

    def _panel_color(self) -> QColor:
        return QColor(255, 255, 255, 0)

    def _current_fill_color(self, emphasis: float = 1.0) -> QColor:
        base = self._base_color()
        highlight = self._highlight_color()
        pulse = 0.5 - 0.5 * math.cos(self._phase * math.tau)
        mix = 0.14 + 0.58 * pulse

        r = int(base.red() * (1.0 - mix) + highlight.red() * mix)
        g = int(base.green() * (1.0 - mix) + highlight.green() * mix)
        b = int(base.blue() * (1.0 - mix) + highlight.blue() * mix)
        a = int((base.alpha() * (1.0 - mix) + highlight.alpha() * mix) * emphasis)
        return QColor(r, g, b, max(0, min(255, a)))

    def _draw_shape(self, painter: QPainter, shape: SkeletonShape) -> None:
        radius = shape.radius if shape.radius is not None else int(shape.height / 2)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(self._current_fill_color(shape.emphasis))
        painter.drawRoundedRect(shape.x, shape.y, shape.width, shape.height, radius, radius)

    def paintEvent(self, event: QPaintEvent) -> None:
        super().paintEvent(event)

        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        panel_rect = self.rect().adjusted(
            self._panel_margin,
            self._panel_margin,
            -self._panel_margin,
            -self._panel_margin,
        )
        if panel_rect.width() <= 0 or panel_rect.height() <= 0:
            painter.end()
            return

        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(self._panel_color())
        painter.drawRoundedRect(panel_rect, self._panel_radius, self._panel_radius)

        for shape in self._shape_builder(self):
            self._draw_shape(painter, shape)

        painter.end()

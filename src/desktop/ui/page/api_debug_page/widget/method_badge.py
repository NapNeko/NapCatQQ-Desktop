# -*- coding: utf-8 -*-
from __future__ import annotations

"""请求方法徽标自绘组件。"""

from PySide6.QtCore import QSize, Qt
from PySide6.QtGui import QColor, QFont, QFontMetrics, QPainter
from PySide6.QtWidgets import QSizePolicy, QWidget

_SUPPORTED_METHOD_TYPES = {"get", "post", "put", "delete", "patch"}
_METHOD_BACKGROUND_MAP = {
    "get": QColor(22, 163, 74, 245),
    "post": QColor(37, 99, 235, 250),
    "put": QColor(234, 88, 12, 250),
    "delete": QColor(220, 38, 38, 250),
    "patch": QColor(147, 51, 234, 250),
    "default": QColor(71, 85, 105, 245),
}


class MethodBadge(QWidget):
    """基于 paintEvent 的请求方法徽标。"""

    def __init__(self, method_text: str = "POST", parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._text = "POST"
        self._method_type = "post"
        self.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Fixed)
        self.set_method(method_text)

    def set_method(self, method_text: str) -> None:
        normalized_text = (method_text or "POST").upper()
        normalized_type = normalized_text.lower()
        self._text = normalized_text
        self._method_type = normalized_type if normalized_type in _SUPPORTED_METHOD_TYPES else "default"
        self.updateGeometry()
        self.update()

    def sizeHint(self) -> QSize:
        metrics = QFontMetrics(self._badge_font())
        return QSize(metrics.horizontalAdvance(self._text) + 24, metrics.height() + 10)

    def minimumSizeHint(self) -> QSize:
        return self.sizeHint()

    def paintEvent(self, _event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(_METHOD_BACKGROUND_MAP.get(self._method_type, _METHOD_BACKGROUND_MAP["default"]))
        painter.drawRoundedRect(self.rect().adjusted(0, 0, -1, -1), 6, 6)

        painter.setPen(QColor("white"))
        painter.setFont(self._badge_font())
        painter.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, self._text)

    @staticmethod
    def _badge_font() -> QFont:
        font = QFont()
        font.setPointSizeF(9.5)
        font.setWeight(QFont.Weight.DemiBold)
        return font


def apply_method_badge(label: MethodBadge, method_text: str) -> None:
    """更新请求方法徽标文本。"""
    label.set_method(method_text)

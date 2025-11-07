# -*- coding: utf-8 -*-
"""
一个可复用的点阵背景控件 (DottedBackground)

放在 `src.ui.components` 下以便作为 UI 组件复用。
"""
from __future__ import annotations

from typing import Optional

from qfluentwidgets import Theme
from PySide6.QtCore import Qt
from PySide6.QtGui import QColor, QPainter
from PySide6.QtWidgets import QWidget

from src.core.config import cfg


class DottedBackground(QWidget):
    """在 widget 背景绘制点阵的 QWidget

    设计目标: 点要细小、带透明度、不抢眼, 能增加页面层次感
    默认参数已经做了较为保守的调优: 点小、间距较大、alpha 低
    """

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._dot_size = 4
        self._spacing = 32
        self._color = self._default_color_for_theme(cfg.theme)
        self._padding = 8

        # 让控件可以绘制自定义背景而不覆盖子控件
        self.setAttribute(Qt.WidgetAttribute.WA_OpaquePaintEvent, False)

        # 连接信号
        cfg.themeChanged.connect(self.refresh_theme)

    def set_dot_size(self, size: int) -> None:
        self._dot_size = max(1, int(size))
        self.update()

    def set_spacing(self, spacing: int) -> None:
        self._spacing = max(4, int(spacing))
        self.update()

    def set_padding(self, padding: int) -> None:
        """设置点阵与控件边缘的内边距（像素）。最小为 0。"""
        self._padding = max(0, int(padding))
        self.update()

    def set_color(self, color: QColor) -> None:
        self._color = color
        self.update()

    def _default_color_for_theme(self, theme: Theme) -> QColor:
        """根据主题返回默认点色

        - 暗色主题：使用接近白色的点、较低 alpha, 让点不会太突兀
        - 亮色主题：使用中灰色的点、低 alpha, 便于在浅背景上可见但不抢眼
        """
        try:
            theme_resolved = cfg.theme if theme == Theme.AUTO else theme
        except Exception:
            theme_resolved = Theme.LIGHT

        if theme_resolved == Theme.DARK:
            return QColor(144, 144, 144, 24)
        else:
            return QColor(120, 120, 120, 40)

    def refresh_theme(self, theme: Optional[Theme] = None) -> None:
        """手动刷新主题颜色 (当主题在运行时改变时可调用)

        如果不传 `theme`, 则使用 cfg.theme
        """
        t = theme if theme is not None else cfg.theme
        self._color = self._default_color_for_theme(t)
        self.update()

    def paintEvent(self, event) -> None:
        """在整个 widget 区域绘制点阵交错排列以获得更自然的效果"""
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(self._color)

        w = self.width()
        h = self.height()
        step = max(1, self._spacing)
        dot = max(1, self._dot_size)

        # 将绘制区域限制在控件内边距内，避免靠边绘制
        pad = max(0, int(self._padding))
        # 确保范围有效
        if w - pad * 2 <= 0 or h - pad * 2 <= 0:
            painter.end()
            return

        # 从 pad 开始到 h - pad，x 也类似；为了保留交错效果，x 的循环可以从 pad-step 开始
        for row, y in enumerate(range(pad, h - pad + step, step)):
            row_offset = (step // 2) + 1 if (row % 2) else 0
            for x in range(pad - step, w - pad + step * 2, step):
                cx = x + row_offset
                # 如果圆完全超出内边距区域则跳过
                if cx + dot < pad or cx > w - pad:
                    continue
                if y + dot < pad or y > h - pad:
                    continue
                painter.drawEllipse(cx, y, dot, dot)

        painter.end()

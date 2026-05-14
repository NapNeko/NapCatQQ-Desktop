# -*- coding: utf-8 -*-
"""分组卡片容器组件.

提供带边框圆角的轻量卡片容器, 用于将相关配置项进行视觉分组.
不使用 SimpleCardWidget (自带阴影), 而是通过 QSS 精确控制样式:
- 0.5px 边框 (亮色 rgba(0,0,0,0.08) / 暗色 rgba(255,255,255,0.08))
- 8px 圆角
- 16px 内边距
"""
from __future__ import annotations

from PySide6.QtWidgets import QFrame, QVBoxLayout, QWidget
from qfluentwidgets import isDarkTheme

from src.core.config import cfg


class SettingGroup(QFrame):
    """带边框圆角的卡片容器组件.

    轻量级分组卡片, 通过 QSS 实现主题自适应的边框和圆角样式.
    监听主题变更信号自动更新样式.
    """

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._layout = QVBoxLayout(self)
        self._layout.setContentsMargins(16, 16, 16, 16)
        self._layout.setSpacing(8)

        self._apply_theme_style()
        cfg.themeChanged.connect(self._apply_theme_style)

    def _apply_theme_style(self, *_args) -> None:
        """根据当前主题应用对应的 QSS 边框样式."""
        if isDarkTheme():
            border_color = "rgba(255, 255, 255, 0.08)"
        else:
            border_color = "rgba(0, 0, 0, 0.08)"

        self.setStyleSheet(
            f"SettingGroup {{"
            f"  border: 0.5px solid {border_color};"
            f"  border-radius: 8px;"
            f"  padding: 16px;"
            f"}}"
        )

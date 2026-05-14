# -*- coding: utf-8 -*-
"""分区标题组件."""

from PySide6.QtGui import QFont
from PySide6.QtWidgets import QLabel, QWidget


class SettingSubtitle(QLabel):
    """加粗 14px 分区标题.

    用于在 SettingGroup 卡片上方标识配置区域名称,
    例如 "API 配置"、"模型列表" 等.
    """

    def __init__(self, text: str, parent: QWidget | None = None) -> None:
        """初始化分区标题.

        Args:
            text: 标题文本内容.
            parent: 父组件, 可为 None.
        """
        super().__init__(text, parent)
        self._setup_font()

    def _setup_font(self) -> None:
        """设置加粗 14px 字体样式."""
        font = self.font()
        font.setPixelSize(14)
        font.setWeight(QFont.Weight.Bold)
        self.setFont(font)

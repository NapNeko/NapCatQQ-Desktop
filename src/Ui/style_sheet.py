# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum

# 第三方库导入
from qfluentwidgets import Theme, StyleSheetBase

# 项目内模块导入
from src.core.config import cfg


class StyleSheet(StyleSheetBase, Enum):
    """样式表"""

    # common
    TRANSPARENT_SCROLL_AREA = "transparent_scroll_area"

    def path(self, theme=Theme.AUTO):
        theme = cfg.theme if theme == Theme.AUTO else theme
        return f":QSS/style/{theme.value.lower()}/{self.value}.qss"

# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum

# 第三方库导入
from qfluentwidgets import StyleSheetBase, Theme, qconfig


class PageStyleSheet(StyleSheetBase, Enum):
    """页面样式表"""

    HOME = "home"
    # BOT = "bot"
    SETUP = "setup"
    UNIT = "unit"

    def path(self, theme: Theme = Theme.AUTO) -> str:
        theme = qconfig.theme if theme == Theme.AUTO else theme
        return f":style/style/{theme.value.lower()}/page/{self.value}.qss"


class WidgetStyleSheet(StyleSheetBase, Enum):
    """控件样式表"""

    UPDATE_LOG_CARD = "update_log_card"
    CODE_EDITOR = "code_editor"

    def path(self, theme: Theme = Theme.AUTO) -> str:
        theme = qconfig.theme if theme == Theme.AUTO else theme
        return f":style/style/{theme.value.lower()}/widget/{self.value}.qss"

# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum

# 第三方库导入
from qfluentwidgets import Theme, StyleSheetBase, qconfig


class StyleSheet(StyleSheetBase, Enum):
    """样式表"""

    # widget
    HOME_WIDGET = "home_widget"
    SETUP_WIDGET = "setup_widget"
    ADD_WIDGET = "add_widget"
    BOT_LIST_WIDGET = "bot_list_widget"
    BOT_WIDGET = "bot_widget"
    UNIT_WIDGET = "unit_widget"

    # common
    UPDATE_LOG_CARD = "update_log_card"
    CODE_EDITOR = "code_editor"

    def path(self, theme=Theme.AUTO):
        theme = qconfig.theme if theme == Theme.AUTO else theme
        return f":QSS/qss/{theme.value.lower()}/{self.value}.qss"


class StaticStyleSheet(StyleSheetBase, Enum):
    """静态样式表"""

    def path(self, theme=Theme.AUTO):
        return f":QSS/qss/static/{self.value}.qss"

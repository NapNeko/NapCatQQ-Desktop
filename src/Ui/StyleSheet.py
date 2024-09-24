# -*- coding: utf-8 -*-
from enum import Enum

from qfluentwidgets import StyleSheetBase, Theme, qconfig


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

    def path(self, theme=Theme.AUTO):
        theme = qconfig.theme if theme == Theme.AUTO else theme
        return f":QSS/qss/{theme.value.lower()}/{self.value}.qss"

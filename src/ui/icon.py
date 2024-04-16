# -*- coding: utf-8 -*-
from enum import Enum

from qfluentwidgets.common import getIconColor, Theme, FluentIconBase


class MainWindowIcon(FluentIconBase, Enum):
    """主窗体所需要的图标"""
    LOGO = "Logo"

    def path(self, theme=Theme.AUTO) -> str:
        return f":MainWindow/image/MainWindow/{self.value}_{getIconColor(theme)}.svg"




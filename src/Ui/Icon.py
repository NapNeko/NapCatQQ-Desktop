# -*- coding: utf-8 -*-
from enum import Enum

from qfluentwidgets.common import getIconColor, Theme, FluentIconBase


class NapCatDesktopIcon(FluentIconBase, Enum):
    """主窗体所需要的图标"""
    LOGO = "Logo"
    QQ = "QQ"

    def path(self, theme=Theme.AUTO) -> str:
        return f":Icon/image/Icon/{getIconColor(theme)}/{self.value}.svg"




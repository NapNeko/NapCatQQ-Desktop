# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum

# 第三方库导入
from qfluentwidgets.common import Theme, FluentIconBase, getIconColor


class NapCatDesktopIcon(FluentIconBase, Enum):
    """主窗体所需要的图标"""

    LOGO = "Logo"
    QQ = "QQ"

    def path(self, theme=Theme.AUTO) -> str:
        return f":Icon/image/Icon/{getIconColor(theme)}/{self.value}.svg"


class StaticIcon(FluentIconBase, Enum):
    """静态图标"""

    LOGO = "logo"

    def path(self, theme=Theme.AUTO) -> str:
        return f":Icon/image/Icon/static/{self.value}.png"

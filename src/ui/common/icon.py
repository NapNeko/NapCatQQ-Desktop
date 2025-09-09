# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum

# 第三方库导入
from qfluentwidgets.common import FluentIconBase, Theme, getIconColor


class NapCatDesktopIcon(FluentIconBase, Enum):
    """主窗体所需要的图标"""

    LOGO = "logo"
    QQ = "qq"

    def path(self, theme=Theme.AUTO) -> str:
        return f":mono_icon/icon/mono_icon/{getIconColor(theme)}/{self.value}.svg"


class StaticIcon(FluentIconBase, Enum):
    """静态图标"""

    LOGO = "logo"
    NAPCAT = "napcat"

    def path(self, theme=Theme.AUTO) -> str:
        return f":color_icon/icon/color_icon/{self.value}.png"

# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum

# 第三方库导入
from qfluentwidgets.common import Theme, FluentIconBase

# 项目内模块导入
from src.core.config import cfg


class NCDFluentIcon(FluentIconBase, Enum):
    """主窗体所需要的图标(适配主题切换)"""

    def path(self, theme=Theme.AUTO) -> str:
        theme = cfg.theme if theme == Theme.AUTO else theme
        return f":Icon/icon/{theme.value.lower}/{self.value}.svg"


class NCDIcon(Enum):
    """主窗体所需要的图标(不适配主题切换)"""

    LOGO = ":Global/logo.png"

    def __str__(self) -> str:
        return self.value

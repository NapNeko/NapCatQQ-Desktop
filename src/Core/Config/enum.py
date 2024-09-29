# -*- coding: utf-8 -*-
"""
## 配置项所需的一些枚举值
"""
# 标准库导入
from enum import Enum

from PySide6.QtCore import QLocale


class StartOpenHomePageViewEnum(Enum):
    """启动页面枚举"""

    DISPLAY_VIEW = "DisplayView"
    CONTENT_VIEW = "ContentView"

    @staticmethod
    def values():
        return [value.value for value in StartOpenHomePageViewEnum]


class Language(Enum):
    """语言枚举"""

    CHINESE_SIMPLIFIED = QLocale(QLocale.Language.Chinese, QLocale.Script.SimplifiedChineseScript)
    CHINESE_TRADITIONAL = QLocale(QLocale.Language.Chinese, QLocale.Script.TraditionalChineseScript)
    ENGLISH = QLocale(QLocale.Language.English)
    AUTO = QLocale()

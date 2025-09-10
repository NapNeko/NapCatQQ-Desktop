# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum

from PySide6.QtCore import QLocale

"""
这个模块包含的是 NapCatQQ Desktop 的枚举类型

每个枚举类型都对应一个类, 类名即为枚举类型的名称
每个枚举类型都可以包含多个枚举值, 每个枚举值都有一个名称和值
每个枚举类型都可以定义一个静态方法 values(), 用于返回所有枚举值的列表

注意:
- 枚举值的名称应该是大写字母和下划线组成的, 以便与其他变量区分
- 枚举值的值可以是任意类型, 但通常是字符串或整数
- 如果需要在配置项中使用枚举类型, 可以使用 EnumSerializer 来进行序列化和反序列化
- 具体使用方法可以参考 qfluentwidgets.common.EnumSerializer 类的文档
"""


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
    ENGLISH = QLocale(QLocale.Language.English)
    AUTO = QLocale()


class CloseActionEnum(Enum):
    """关闭动作枚举"""

    CLOSE = 0
    HIDE = 1

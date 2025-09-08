# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum

# 第三方库导入
from qfluentwidgets.common import FluentIconBase

"""表情包模块

该模块定义了各种表情包以及其引用关系
"""


class FuFuEmoticons(FluentIconBase, Enum):
    """敷敷表情包"""

    # 想过自动生成, 但是 vsCode 等编辑器就没有智能提示了, 所以还是手动写吧
    FU_01 = "g_fufu_01"
    FU_02 = "g_fufu_02"
    FU_03 = "g_fufu_03"
    FU_04 = "g_fufu_04"
    FU_05 = "g_fufu_05"
    FU_06 = "g_fufu_06"
    FU_07 = "g_fufu_07"
    FU_08 = "g_fufu_08"
    FU_09 = "g_fufu_09"
    FU_10 = "g_fufu_10"
    FU_11 = "g_fufu_11"
    FU_12 = "g_fufu_12"
    FU_13 = "g_fufu_13"

    def path(self) -> str:
        print(f":emoticons/image/emoticons/fu_fu/{self.value}.gif")
        return f":emoticons/image/emoticons/fu_fu/{self.value}.gif"

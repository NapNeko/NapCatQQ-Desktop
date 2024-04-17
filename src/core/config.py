# -*- coding: utf-8 -*-
from pathlib import Path
from enum import Enum

from qfluentwidgets.common import qconfig, QConfig, ConfigItem, BoolValidator


class Config(QConfig):
    """程序配置"""

    # 新手引导配置项
    beginner_guidance = ConfigItem(
        group="others",
        name="Beginner Guidance State",
        default=False,
        validator=BoolValidator(),
        restart=True
    )


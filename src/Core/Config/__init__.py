# -*- coding: utf-8 -*-
# 标准库导入
import json
import time
import platform
from pathlib import Path

# 第三方库导入
from qfluentwidgets import RangeValidator, RangeConfigItem, TabCloseButtonDisplayMode, qconfig
from qfluentwidgets.common import (
    Theme,
    QConfig,
    ConfigItem,
    BoolValidator,
    EnumSerializer,
    ColorConfigItem,
    ConfigSerializer,
    OptionsValidator,
    OptionsConfigItem,
)
from qfluentwidgets.common.exception_handler import exceptionHandler
from PySide6.QtCore import Signal

# 项目内模块导入
from src.core.utils.path import PathFunc


class Config(QConfig):
    """程序配置"""

    # 信号
    appRestartSig = Signal()

    # 信息项
    NCDVersion = ConfigItem(group="Info", name="NCDVersion", default="")

    # 个性化项目

    themeMode = OptionsConfigItem(
        group="Personalized",
        name="ThemeMode",
        default=Theme.AUTO,
        validator=OptionsValidator(Theme),
        serializer=EnumSerializer(Theme),
    )
    themeColor = ColorConfigItem(
        group="Personalized",
        name="ThemeColor",
        default="#009faa",
    )
    dpiScale = OptionsConfigItem(
        group="Personalized",
        name="DpiScale",
        default="Auto",
        validator=OptionsValidator([1, 1.25, 1.5, 1.75, 2, "Auto"]),
        restart=True,
    )

    def __init__(self):
        super().__init__()

    @exceptionHandler()
    def load(self, file=None, config=None) -> None:
        """
        ## 加载配置

        ## 参数
            file: str 或者 Path
                json 配置文件的路径

            config: Config
                被初始化的配置对象
        """
        if isinstance(config, QConfig):
            self._cfg = config
            self._cfg.themeChanged.connect(self.themeChanged)

        if isinstance(file, (str, Path)):
            self._cfg.file = Path(file)

        # 加载现有配置
        try:
            with open(self._cfg.file, "r", encoding="utf-8") as f:
                cfg = json.load(f)
        except FileNotFoundError:
            cfg = {}

        # 将配置项的键映射到配置项
        items = {item.key: item for name, item in self._cfg.__class__.__dict__.items() if isinstance(item, ConfigItem)}

        # 更新配置项的值
        for k, v in cfg.items():
            if not isinstance(v, dict) and k in items:
                items[k].deserializeFrom(v)
            elif isinstance(v, dict):
                for key, value in v.items():
                    compound_key = f"{k}.{key}"
                    if compound_key in items:
                        items[compound_key].deserializeFrom(value)

        # 确保所有配置项都有一个值，为新添加的配置项填充默认值
        for key, item in items.items():
            if not hasattr(item, "value"):
                item.value = item.default

        self.theme = self.get(self._cfg.themeMode)


cfg = Config()
qconfig.load(PathFunc().config_path, cfg)
cfg.set(cfg.NCDVersion, "v2.0.0", True)

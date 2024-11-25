# -*- coding: utf-8 -*-
# 标准库导入
import json
import time
import platform

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
from PySide6.QtCore import Signal, QLocale

# 项目内模块导入
from src.Core.Utils.PathFunc import PathFunc


class Config(QConfig):
    """程序配置"""

    # 信号
    appRestartSig = Signal()

    # 信息项
    NCDVersion = ConfigItem(group="Info", name="NCDVersion", default="")
    StartTime = ConfigItem(group="Info", name="StartTime", default="")
    SystemType = ConfigItem(group="Info", name="SystemType", default="")
    PlatformType = ConfigItem(group="Info", name="PlatformType", default="")

    # 路径项
    # 注意: default 为空字符串则默认以程序根目录为路径

    # 启动项

    # 个性化项目
    themeMode = OptionsConfigItem(
        group="Personalize",
        name="ThemeMode",
        default=Theme.AUTO,
        validator=OptionsValidator(Theme),
        serializer=EnumSerializer(Theme),
    )
    themeColor = ColorConfigItem(group="Personalize", name="ThemeColor", default="#009faa")

    windowOpacity = RangeConfigItem(
        group="Personalize", name="WindowOpacity", default=100, validator=RangeValidator(10, 100)
    )

    bgHomePage = ConfigItem(group="Personalize", name="BgHomePage", default=False, validator=BoolValidator())
    bgHomePageOpacity = RangeConfigItem(
        group="Personalize", name="BgHomePageOpacity", default=100, validator=RangeValidator(1, 100)
    )
    bgHomePageLight = ConfigItem(group="Personalize", name="BgHomePageLight", default="")
    bgHomePageDark = ConfigItem(group="Personalize", name="BgHomePageDark", default="")
    bgAddPage = ConfigItem(group="Personalize", name="BgAddPage", default=False, validator=BoolValidator())
    bgAddPageOpacity = RangeConfigItem(
        group="Personalize", name="BgAddPageOpacity", default=100, validator=RangeValidator(1, 100)
    )
    bgAddPageLight = ConfigItem(group="Personalize", name="BgAddPageLight", default="")
    bgAddPageDark = ConfigItem(group="Personalize", name="BgAddPageDark", default="")
    bgListPage = ConfigItem(group="Personalize", name="BgListPage", default=False, validator=BoolValidator())
    bgListPageOpacity = RangeConfigItem(
        group="Personalize", name="BgListPageOpacity", default=100, validator=RangeValidator(1, 100)
    )
    bgListPageLight = ConfigItem(group="Personalize", name="BgListPageLight", default="")
    bgListPageDark = ConfigItem(group="Personalize", name="BgListPageDark", default="")
    bgUnitPage = ConfigItem(group="Personalize", name="BgUnitPage", default=False, validator=BoolValidator())
    bgUnitPageOpacity = RangeConfigItem(
        group="Personalize", name="BgUnitPageOpacity", default=100, validator=RangeValidator(1, 100)
    )
    bgUnitPageLight = ConfigItem(group="Personalize", name="BgUnitPageLight", default="")
    bgUnitPageDark = ConfigItem(group="Personalize", name="BgUnitPageDark", default="")
    bgSettingPage = ConfigItem(group="Personalize", name="BgSettingPage", default=False, validator=BoolValidator())
    bgSettingPageOpacity = RangeConfigItem(
        group="Personalize", name="BgSettingPageOpacity", default=100, validator=RangeValidator(1, 100)
    )
    bgSettingPageLight = ConfigItem(group="Personalize", name="BgSettingPageLight", default="")
    bgSettingPageDark = ConfigItem(group="Personalize", name="BgSettingPageDark", default="")

    titleTabBar = ConfigItem(group="Personalize", name="TitleTabBar", default=False, validator=BoolValidator())
    titleTabBarMovable = ConfigItem(
        group="Personalize", name="TitleTabBarIsMovable", default=False, validator=BoolValidator()
    )
    titleTabBarScrollable = ConfigItem(
        group="Personalize", name="TitleTabBarIsScrollable", default=False, validator=BoolValidator()
    )
    titleTabBarShadow = ConfigItem(
        group="Personalize", name="TitleTabBarIsShadow", default=False, validator=BoolValidator()
    )
    titleTabBarCloseMode = OptionsConfigItem(
        group="Personalize",
        name="TitleTabBarCloseButton",
        default=TabCloseButtonDisplayMode.ON_HOVER,
        validator=OptionsValidator(TabCloseButtonDisplayMode),
        serializer=EnumSerializer(TabCloseButtonDisplayMode),
        restart=True,
    )
    titleTabBarMinWidth = RangeConfigItem(
        group="Personalize", name="TitleTabBarMinWidth", default=64, validator=RangeValidator(32, 64)
    )
    titleTabBarMaxWidth = RangeConfigItem(
        group="Personalize", name="TitleTabBarMaxWidth", default=135, validator=RangeValidator(64, 200)
    )

    # 事件项
    botOfflineEmailNotice = ConfigItem(
        group="Event", name="BotOfflineEmailNotice", default=False, validator=BoolValidator()
    )

    # 邮件项
    emailReceiver = ConfigItem(group="Email", name="EmailReceiver", default="")
    emailSender = ConfigItem(group="Email", name="EmailSender", default="")
    emailToken = ConfigItem(group="Email", name="EmailToken", default="")
    emailStmpServer = ConfigItem(group="Email", name="EmailStmpServer", default="")

    # 隐藏提示项
    HideUsGoBtnTips = ConfigItem(group="HideTips", name="HideUsingGoBtnTips", default=False, validator=BoolValidator())

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
cfg.set(cfg.StartTime, time.time(), True)
cfg.set(cfg.NCDVersion, "v1.4.0", True)
cfg.set(cfg.SystemType, platform.system(), True)
cfg.set(cfg.PlatformType, platform.machine(), True)

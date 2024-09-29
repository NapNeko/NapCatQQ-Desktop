# -*- coding: utf-8 -*-
# 标准库导入
import time
import platform

# 第三方库导入
from creart import it
from qfluentwidgets.common import (
    QConfig,
    ConfigItem,
    BoolValidator,
    EnumSerializer,
    ConfigSerializer,
    OptionsValidator,
    OptionsConfigItem,
    qconfig,
)
from PySide6.QtCore import QLocale

# 项目内模块导入
from src.Core.Config import Language
from src.Core.Config.enum import Language, StartOpenHomePageViewEnum
from src.Core.Utils.PathFunc import PathFunc


class LanguageSerializer(ConfigSerializer):
    """语言序列化"""

    def serialize(self, language):
        return language.value.name() if language != Language.AUTO else "Auto"

    def deserialize(self, value: str):
        return Language(QLocale(value)) if value != "Auto" else Language.AUTO


class Config(QConfig):
    """程序配置"""

    # 信息项
    NCDVersion = ConfigItem(group="Info", name="NCDVersion", default="")
    StartTime = ConfigItem(group="Info", name="StartTime", default="")
    SystemType = ConfigItem(group="Info", name="SystemType", default="")
    PlatformType = ConfigItem(group="Info", name="PlatformType", default="")

    # 路径项
    # 注意: default 为空字符串则默认以程序根目录为路径

    # 启动项
    StartOpenHomePageView = OptionsConfigItem(
        group="StartupItem",
        name="StartOpenHomePageView",
        default=StartOpenHomePageViewEnum.DISPLAY_VIEW,
        validator=OptionsValidator(StartOpenHomePageViewEnum),
        serializer=EnumSerializer(StartOpenHomePageViewEnum),
        restart=True,
    )

    # 新手引导配置项
    BeginnerGuidance = ConfigItem(
        group="others", name="BeginnerGuidanceState", default=False, validator=BoolValidator()
    )

    # 个性化项目
    language = OptionsConfigItem(
        group="Personalize",
        name="Language",
        default=Language.AUTO,
        validator=OptionsValidator(Language),
        serializer=LanguageSerializer(),
        restart=True,
    )

    # 隐藏提示项
    HideUsGoBtnTips = ConfigItem(group="HideTips", name="HideUsingGoBtnTips", default=False, validator=BoolValidator())


cfg = Config()
qconfig.load(it(PathFunc).config_path, cfg)
cfg.set(cfg.StartTime, time.time(), True)
cfg.set(cfg.NCDVersion, "beta1.1.0", True)
cfg.set(cfg.SystemType, platform.system(), True)
cfg.set(cfg.PlatformType, platform.machine(), True)

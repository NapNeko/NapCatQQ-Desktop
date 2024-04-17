# -*- coding: utf-8 -*-
import sys
from enum import Enum

from PySide6.QtCore import QLocale
from creart import it
from qfluentwidgets.common import (
    qconfig, QConfig, ConfigItem, BoolValidator, FolderValidator,
    OptionsConfigItem, OptionsValidator, EnumSerializer, Theme,
    ConfigSerializer,
)

from src.core.path_func import PathFunc


def is_win11():
    return sys.platform == 'win32' and sys.getwindowsversion().build >= 22000


class Language(Enum):
    """语言枚举"""

    CHINESE_SIMPLIFIED = QLocale(
        QLocale.Language.Chinese, QLocale.Script.SimplifiedChineseScript
    )
    CHINESE_TRADITIONAL = QLocale(
        QLocale.Language.Chinese, QLocale.Script.TraditionalChineseScript
    )
    ENGLISH = QLocale(QLocale.Language.English)
    AUTO = QLocale()


class LanguageSerializer(ConfigSerializer):
    """语言序列化"""

    def serialize(self, language):
        return language.value.name() if language != Language.AUTO else "Auto"

    def deserialize(self, value: str):
        return Language(QLocale(value)) if value != "Auto" else Language.AUTO


class Config(QConfig):
    """程序配置"""

    # 路径项
    # 注意: default 为空字符串则默认以程序根目录为路径
    qq_path = ConfigItem(
        group="Path",
        name="QQPath",
        default="",
        validator=FolderValidator()
    )
    nap_cat_path = ConfigItem(
        group="Path",
        name="NapCatPath",
        default="",
        validator=FolderValidator()
    )

    # 启动项
    start_open_display_view = ConfigItem(
        group="StartupItem",
        name="StartOpenDisplayView",
        default=True,
        validator=BoolValidator()
    )

    # 新手引导配置项
    beginner_guidance = ConfigItem(
        group="others",
        name="BeginnerGuidanceState",
        default=False,
        validator=BoolValidator()
    )

    # 个性化项目
    micaEnabled = ConfigItem(
        group="Personalize",
        name="MicaEnabled",
        default=is_win11(),
        validator=BoolValidator()
    )
    themeMode = OptionsConfigItem(
        group="Personalize",
        name="ThemeMode",
        default=Theme.LIGHT,
        validator=OptionsValidator(Theme),
        serializer=EnumSerializer(Theme)
    )

    themeColor = OptionsConfigItem(
        group="Personalize",
        name="ThemeColor",
        default="#009faa"
    )

    language = OptionsConfigItem(
        group="Personalize",
        name="Language",
        default=Language.AUTO,
        validator=OptionsValidator(Language),
        serializer=LanguageSerializer(),
        restart=True
    )


cfg = Config()
qconfig.load(it(PathFunc).config_path, cfg)

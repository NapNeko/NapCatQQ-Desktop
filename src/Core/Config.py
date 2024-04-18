# -*- coding: utf-8 -*-
from enum import Enum

from PySide6.QtCore import QLocale
from creart import it
from qfluentwidgets.common import (
    qconfig, QConfig, ConfigItem, BoolValidator, FolderValidator,
    OptionsConfigItem, OptionsValidator, EnumSerializer, Theme,
    ConfigSerializer, ColorConfigItem
)

from src.Core.PathFunc import PathFunc


class StartOpenHomePageViewEnum(Enum):
    """启动页面枚举"""
    DISPLAY_VIEW = "DisplayView"
    CONTENT_VIEW = "ContentView"

    @staticmethod
    def values():
        return [value.value for value in StartOpenHomePageViewEnum]


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
    QQPath = ConfigItem(
        group="Path",
        name="QQPath",
        default="",
        validator=FolderValidator()
    )
    NapCatPath = ConfigItem(
        group="Path",
        name="NapCatPath",
        default="",
        validator=FolderValidator()
    )

    # 启动项
    StartOpenHomePageView = OptionsConfigItem(
        group="StartupItem",
        name="StartOpenHomePageView",
        default=StartOpenHomePageViewEnum.DISPLAY_VIEW,
        validator=OptionsValidator(StartOpenHomePageViewEnum),
        serializer=EnumSerializer(StartOpenHomePageViewEnum),
        restart=True
    )

    # 新手引导配置项
    BeginnerGuidance = ConfigItem(
        group="others",
        name="BeginnerGuidanceState",
        default=False,
        validator=BoolValidator()
    )

    # 个性化项目
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

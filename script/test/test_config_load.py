# -*- coding: utf-8 -*-

# 标准库导入
import json

# 第三方库导入
from qfluentwidgets import Theme, qconfig, setTheme, setThemeColor
from qfluentwidgets.common import BoolValidator, ConfigItem
from PySide6.QtGui import QColor

# 项目内模块导入
from src.core.config import Config as AppConfig
from src.core.config import bind_qfluent_qconfig


class SampleConfig(AppConfig):
    """用于验证自定义 load() 的测试配置。"""

    custom_flag = ConfigItem(group="Test", name="CustomFlag", default=True, validator=BoolValidator())
    custom_text = ConfigItem(group="Test", name="CustomText", default="fallback")


def test_config_load_keeps_falsey_values_and_restores_missing_defaults(tmp_path) -> None:
    """自定义 load() 应保留合法假值，并仅对缺失项回落默认值。"""
    config_path = tmp_path / "config.json"
    config_path.write_text(
        json.dumps(
            {
                "Test": {
                    "CustomFlag": False,
                    "CustomText": "",
                }
            },
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )

    config = SampleConfig()
    config.load(config_path)

    assert config.get(config.custom_flag) is False
    assert config.get(config.custom_text) == ""
    assert config.get(config.email_stmp_port) == 465


def test_config_load_syncs_legacy_personalize_theme_values(tmp_path) -> None:
    """旧版 Personalize 主题字段应自动同步到 QFluentWidgets 配置项。"""
    config_path = tmp_path / "config.json"
    config_path.write_text(
        json.dumps(
            {
                "Personalize": {
                    "ThemeMode": "Dark",
                    "ThemeColor": "#ff123456",
                }
            },
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )

    config = SampleConfig()
    config.load(config_path)

    assert config.get(config.theme_mode) == Theme.DARK
    assert config.get(config.themeMode) == Theme.DARK
    assert config.get(config.theme_color).name(QColor.NameFormat.HexArgb) == "#ff123456"
    assert config.get(config.themeColor).name(QColor.NameFormat.HexArgb) == "#ff123456"


def test_bind_qfluent_qconfig_persists_theme_changes_to_runtime_config(tmp_path) -> None:
    """QFluentWidgets 的 setTheme/setThemeColor 应写入程序真实配置文件。"""
    config_path = tmp_path / "config.json"
    config = SampleConfig()
    config.load(config_path)

    old_cfg = qconfig._cfg
    old_file = qconfig.file
    old_theme = qconfig.theme

    try:
        bind_qfluent_qconfig(config)
        setTheme(Theme.DARK, save=True)
        setThemeColor(QColor("#123456"), save=True)

        saved = json.loads(config_path.read_text(encoding="utf-8"))
        assert saved["QFluentWidgets"]["ThemeMode"] == "Dark"
        assert saved["QFluentWidgets"]["ThemeColor"] == "#ff123456"
    finally:
        qconfig._cfg = old_cfg
        qconfig.file = old_file
        qconfig.theme = old_theme


def test_bind_qfluent_qconfig_relays_theme_signal_to_qfluent_widgets(tmp_path) -> None:
    """绑定后，QFluentWidgets 监听的 qconfig.themeChanged 也应收到通知。"""
    config_path = tmp_path / "config.json"
    config = SampleConfig()
    config.load(config_path)

    old_cfg = qconfig._cfg
    old_file = qconfig.file
    old_theme = qconfig.theme
    received: list[Theme] = []

    def on_theme_changed(theme: Theme) -> None:
        received.append(theme)

    try:
        bind_qfluent_qconfig(config)
        qconfig.themeChanged.connect(on_theme_changed)
        setTheme(Theme.DARK, save=False)
        assert received[-1] == Theme.DARK
    finally:
        try:
            qconfig.themeChanged.disconnect(on_theme_changed)
        except (RuntimeError, TypeError):
            pass
        qconfig._cfg = old_cfg
        qconfig.file = old_file
        qconfig.theme = old_theme

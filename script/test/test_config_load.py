# -*- coding: utf-8 -*-

# 标准库导入
import json

# 第三方库导入
from qfluentwidgets import Theme, qconfig, setTheme, setThemeColor
from qfluentwidgets.common import BoolValidator, ConfigItem
from PySide6.QtGui import QColor

# 项目内模块导入
import src.desktop.core.config as app_config_module
from src.desktop.core.config.config_enum import CloseActionEnum
from src.desktop.core.config import Config as AppConfig
from src.desktop.core.config import bind_qfluent_qconfig


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
    legacy_payload = {
        "Personalize": {
            "ThemeMode": "Dark",
            "ThemeColor": "#ff123456",
        }
    }
    config_path.write_text(
        json.dumps(legacy_payload, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )

    config = SampleConfig()
    config.load(config_path)

    assert config.get(config.theme_mode) == Theme.DARK
    assert config.get(config.themeMode) == Theme.DARK
    assert config.get(config.theme_color).name(QColor.NameFormat.HexArgb) == "#ff123456"
    assert config.get(config.themeColor).name(QColor.NameFormat.HexArgb) == "#ff123456"
    assert config.get(config.config_version) == app_config_module._CURRENT_CONFIG_COMPAT_VERSION
    migrated = json.loads(config_path.read_text(encoding="utf-8"))
    backup = config_path.with_name(f"{config_path.name}.bak")

    assert migrated["Info"]["ConfigVersion"] == app_config_module._CURRENT_CONFIG_COMPAT_VERSION
    assert migrated["Info"]["EulaAccepted"] is False
    assert migrated["Personalize"] == legacy_payload["Personalize"]
    assert migrated["QFluentWidgets"]["ThemeMode"] == "Dark"
    assert migrated["QFluentWidgets"]["ThemeColor"] == "#ff123456"
    assert migrated["Home"]["IgnoredNoticeKeys"] == "[]"
    assert migrated["Home"]["SnoozedNoticeItems"] == "{}"
    assert json.loads(backup.read_text(encoding="utf-8")) == legacy_payload


def test_config_load_migrates_main_window_and_cleans_removed_keys(tmp_path) -> None:
    """旧版 Info.main_window 和已废弃个性化键应迁移并清理。"""
    config_path = tmp_path / "config.json"
    config_path.write_text(
        json.dumps(
            {
                "Info": {
                    "main_window": True,
                },
                "Personalized": {
                    "CloseBtnAction": 1,
                },
                "Personalize": {
                    "BgHomePage": True,
                    "TitleTabBar": True,
                },
            },
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )

    config = SampleConfig()
    config.load(config_path)

    migrated = json.loads(config_path.read_text(encoding="utf-8"))

    assert config.get(config.main_window) is True
    assert config.get(config.close_button_action) == CloseActionEnum.HIDE
    assert config.get(config.config_version) == app_config_module._CURRENT_CONFIG_COMPAT_VERSION
    assert migrated["Info"]["MainWindow"] is True
    assert migrated["General"]["CloseBtnAction"] == 1
    assert "main_window" not in migrated["Info"]
    assert "Personalize" not in migrated or "BgHomePage" not in migrated["Personalize"]
    assert "Personalize" not in migrated or "TitleTabBar" not in migrated["Personalize"]


def test_config_load_removes_transient_remote_password_field(tmp_path) -> None:
    """远程密码属于临时凭据，不应继续保留在配置文件中。"""
    config_path = tmp_path / "config.json"
    config_path.write_text(
        json.dumps(
            {
                "Info": {
                    "ConfigVersion": "v2.0",
                },
                "Remote": {
                    "Host": "example.com",
                    "Password": "secret",
                },
            },
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )

    config = SampleConfig()
    config.load(config_path)

    migrated = json.loads(config_path.read_text(encoding="utf-8"))

    assert migrated["Info"]["ConfigVersion"] == app_config_module._CURRENT_CONFIG_COMPAT_VERSION
    assert migrated["Remote"]["Host"] == "example.com"
    assert "Password" not in migrated["Remote"]


def test_read_config_version_inferrs_pre_v160_shape() -> None:
    """包含旧背景键时应识别为 v1.5.4 结构。"""
    payload = {
        "Personalize": {
            "BgHomePage": True,
        }
    }

    assert app_config_module._read_config_version(payload) == "v1.5.4"


def test_read_config_version_inferrs_v160_shape() -> None:
    """存在 Info.main_window 时应识别为 v1.6.0 结构。"""
    payload = {
        "Info": {
            "main_window": True,
        }
    }

    assert app_config_module._read_config_version(payload) == "v1.6.0"


def test_read_config_version_inferrs_v170_shape() -> None:
    """缺失 EulaAccepted 的 MainWindow 结构应识别为 v1.7.0。"""
    payload = {
        "Info": {
            "MainWindow": True,
        }
    }

    assert app_config_module._read_config_version(payload) == "v1.7.0"


def test_read_config_version_prefers_explicit_config_version() -> None:
    """存在 ConfigVersion 时应优先使用该兼容版本。"""
    payload = {
        "Info": {
            "ConfigVersion": "v2.0",
        }
    }

    assert app_config_module._read_config_version(payload) == "v2.0"


def test_read_config_version_accepts_legacy_schema_marker_as_current() -> None:
    """若用户本地残留旧实验字段 ConfigSchemaVersion，当前按已迁移处理。"""
    payload = {
        "Info": {
            "ConfigSchemaVersion": 3,
        }
    }

    assert app_config_module._read_config_version(payload) == app_config_module._CURRENT_CONFIG_COMPAT_VERSION


def test_config_load_reads_explicit_config_version(tmp_path) -> None:
    """显式配置版本号应能被当前配置对象读取。"""
    config_path = tmp_path / "config.json"
    payload = {
        "Info": {
            "ConfigVersion": "v2.0",
        },
    }
    config_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")

    config = SampleConfig()
    config.load(config_path)

    assert config.get(config.config_version) == "v2.0"
    assert json.loads(config_path.read_text(encoding="utf-8")) == payload


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

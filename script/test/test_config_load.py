# -*- coding: utf-8 -*-

# 标准库导入
import json

# 第三方库导入
from qfluentwidgets.common import BoolValidator, ConfigItem

# 项目内模块导入
from src.core.config import Config as AppConfig


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

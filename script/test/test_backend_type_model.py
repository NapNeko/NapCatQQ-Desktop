# -*- coding: utf-8 -*-
"""SnowLuma 适配 P7.1: BackendType 枚举 + BotConfig.backend_type 字段单测.

参见: ``docs/requirements/2026-05-10-snowluma-backend-adapter.md`` §4.2
"""

from __future__ import annotations

import pytest

from src.core.config.config_model import (
    BotConfig,
    Config,
    _migrate_legacy_bot_fields,
)
from src.core.runtime.backend_type import BackendType


# ==================== BackendType 枚举 ====================
class TestBackendTypeEnum:
    def test_value_napcat(self) -> None:
        assert BackendType.NAPCAT.value == "napcat"

    def test_value_snowluma(self) -> None:
        assert BackendType.SNOWLUMA.value == "snowluma"

    def test_display_name(self) -> None:
        assert BackendType.NAPCAT.display_name == "NapCat"
        assert BackendType.SNOWLUMA.display_name == "SnowLuma"

    @pytest.mark.parametrize(
        "input_value, expected",
        [
            (None, BackendType.NAPCAT),
            ("", BackendType.NAPCAT),
            ("napcat", BackendType.NAPCAT),
            ("snowluma", BackendType.SNOWLUMA),
            ("NapCat", BackendType.NAPCAT),  # 严格小写匹配, 大小写形式视为未知 → NAPCAT
            ("garbage", BackendType.NAPCAT),
            ("unknown_xxx", BackendType.NAPCAT),
        ],
    )
    def test_from_str_fallback(self, input_value, expected) -> None:
        assert BackendType.from_str(input_value) is expected


# ==================== BotConfig.backend_type 字段 ====================
class TestBotConfigBackendType:
    def test_default_is_napcat(self) -> None:
        bot = BotConfig.model_validate({"name": "test", "QQID": 10000})
        assert bot.backend_type is BackendType.NAPCAT

    def test_explicit_snowluma_str(self) -> None:
        bot = BotConfig.model_validate(
            {"name": "test", "QQID": 10001, "backend_type": "snowluma"}
        )
        assert bot.backend_type is BackendType.SNOWLUMA

    def test_explicit_napcat_str(self) -> None:
        bot = BotConfig.model_validate(
            {"name": "test", "QQID": 10002, "backend_type": "napcat"}
        )
        assert bot.backend_type is BackendType.NAPCAT

    def test_garbage_str_falls_back_to_napcat(self) -> None:
        bot = BotConfig.model_validate(
            {"name": "test", "QQID": 10003, "backend_type": "unknown_xxx"}
        )
        assert bot.backend_type is BackendType.NAPCAT

    def test_none_backend_type_falls_back_to_napcat(self) -> None:
        bot = BotConfig.model_validate(
            {"name": "test", "QQID": 10004, "backend_type": None}
        )
        assert bot.backend_type is BackendType.NAPCAT

    def test_enum_instance_passes_through(self) -> None:
        bot = BotConfig.model_validate(
            {"name": "test", "QQID": 10005, "backend_type": BackendType.SNOWLUMA}
        )
        assert bot.backend_type is BackendType.SNOWLUMA


# ==================== 旧配置兼容 (P1 SnowLuma 适配 §2.1 推断假设) ====================
class TestLegacyBotMigration:
    def test_missing_backend_type_default_supplied(self) -> None:
        """旧 bot.json 没有 backend_type 字段时, _migrate_legacy_bot_fields 补默认值."""
        legacy_bot = {"name": "legacy-bot", "QQID": 99999}
        normalized, rules = _migrate_legacy_bot_fields(legacy_bot)
        assert normalized["backend_type"] == BackendType.NAPCAT.value
        assert "bot.backend_type default" in rules

    def test_existing_backend_type_kept(self) -> None:
        """已显式声明 snowluma 的 bot 不会被覆盖回 napcat."""
        bot = {"name": "snowluma-bot", "QQID": 99998, "backend_type": "snowluma"}
        normalized, _rules = _migrate_legacy_bot_fields(bot)
        assert normalized["backend_type"] == "snowluma"

    def test_full_config_legacy_payload_normalized(self) -> None:
        """完整 Config 反序列化路径上, 旧 payload 不报错且 bot.backend_type 默认 NAPCAT."""
        legacy_payload = {
            "bot": {"name": "legacy", "QQID": 12345},
            "connect": {},
            "advanced": {},
        }
        config = Config.model_validate(legacy_payload)
        assert config.bot.backend_type is BackendType.NAPCAT

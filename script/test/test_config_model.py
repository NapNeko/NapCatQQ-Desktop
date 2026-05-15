# -*- coding: utf-8 -*-

# 标准库导入
import random

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.config.config_enum import TimeUnitEnum
from src.core.config.config_model import (
    AdvancedConfig,
    AutoRestartScheduleConfig,
    BOT_CONFIG_COMPAT_VERSION,
    BotConfig,
    Config,
    ConnectConfig,
    HttpClientsConfig,
    HttpServersConfig,
    RUNTIME_TARGET_LOCAL,
    WebsocketClientsConfig,
    WebsocketServersConfig,
    _coerce_interval_default,
    consume_deferred_snowluma_overrides,
    migrate_bot_config_payload,
    reset_deferred_snowluma_overrides,
)


@pytest.fixture(autouse=True)
def _reset_deferred_overrides():
    """W4: 每个测试前后清空迁移 deferred 队列, 防止 cross-test 污染."""
    reset_deferred_snowluma_overrides()
    yield
    reset_deferred_snowluma_overrides()


def test_coerce_interval_default_handles_none_blank_and_invalid_values() -> None:
    """间隔值规范化应为 None, 空白和非法输入回退默认值. """
    assert _coerce_interval_default(None, 123) == 123
    assert _coerce_interval_default("   ", 456) == 456
    assert _coerce_interval_default("abc", 789) == 789
    assert _coerce_interval_default("42", 100) == 42
    assert _coerce_interval_default(18, 100) == 18


def test_auto_restart_schedule_normalizes_legacy_interval_payload() -> None:
    """旧版 interval 调度配置应在模型层被转换为当前结构. """
    schedule = AutoRestartScheduleConfig.model_validate({"taskType": "interval", "interval": "15m", "jitter": 0})

    assert schedule.enable is True
    assert schedule.time_unit == TimeUnitEnum.MINUTE
    assert schedule.duration == 15


def test_auto_restart_schedule_disables_legacy_crontab_payload() -> None:
    """旧版 crontab 调度配置应安全降级为禁用. """
    schedule = AutoRestartScheduleConfig.model_validate(
        {"taskType": "crontab", "interval": "6h", "crontab": "0 4 * * *", "jitter": 0}
    )

    assert schedule.enable is False
    assert schedule.time_unit == TimeUnitEnum.HOUR
    assert schedule.duration == 6


def test_bot_config_generates_name_when_value_is_empty(monkeypatch: pytest.MonkeyPatch) -> None:
    """空名称应退回为随机生成的 8 位字母串. """
    monkeypatch.setattr(random, "choices", lambda population, k: list("AbCdEfGh"))

    bot = BotConfig(name="", QQID="123456", autoRestartSchedule=AutoRestartScheduleConfig())

    assert bot.name == "AbCdEfGh"
    assert bot.QQID == 123456


def test_bot_config_rejects_invalid_qqid() -> None:
    """非法 QQID 应在模型层直接报错. """
    with pytest.raises(ValueError, match="无法转换为整数"):
        BotConfig(name="demo", QQID="not-a-number", autoRestartSchedule=AutoRestartScheduleConfig())


def test_websocket_configs_coerce_blank_intervals_to_defaults() -> None:
    """WebSocket 相关配置中的空白间隔应回退为默认值. """
    server = WebsocketServersConfig(name="server", host="127.0.0.1", port=8080, heartInterval=" ")
    client = WebsocketClientsConfig(
        name="client",
        url="ws://127.0.0.1:3000/ws",
        heartInterval="",
        reconnectInterval="invalid",
    )

    assert server.heartInterval == 30000
    assert client.heartInterval == 30000
    assert client.reconnectInterval == 30000


def test_advanced_config_normalizes_bypass_payload_and_fills_missing_keys() -> None:
    """新版 bypass 配置应兼容布尔字符串并补齐缺失字段. """
    advanced = AdvancedConfig.model_validate(
        {
            "bypass": {
                "hook": "true",
                "module": 1,
                "process": 0,
            }
        }
    )

    assert advanced.bypass.hook is True
    assert advanced.bypass.window is False
    assert advanced.bypass.module is True
    assert advanced.bypass.process is False
    assert advanced.bypass.container is False
    assert advanced.bypass.js is False


def test_connect_config_rejects_duplicate_names_across_protocol_types() -> None:
    """单个 Bot 内不同协议的连接名称也必须唯一. """
    with pytest.raises(ValueError, match="连接配置名称不能重复"):
        ConnectConfig(
            httpServers=[HttpServersConfig(name="shared", host="127.0.0.1", port=3000)],
            httpClients=[HttpClientsConfig(name=" shared ", url="https://127.0.0.1:3001")],
        )


def test_config_accepts_pydantic_submodels_without_json_serialization_error() -> None:
    """Config 构造时应能安全处理已实例化的子模型. """
    config = Config(
        bot=BotConfig(name="TestBot", QQID=123456),
        connect=ConnectConfig(),
        advanced=AdvancedConfig(),
    )

    assert config.bot.name == "TestBot"
    assert config.bot.QQID == 123456


# ==================== P2: runtime_target ====================
class TestRuntimeTargetField:
    """[`BotConfig.runtime_target`](src/core/config/config_model.py) 字段契约. """

    def test_default_runtime_target_is_local(self) -> None:
        bot = BotConfig(name="x", QQID=1)
        assert bot.runtime_target == RUNTIME_TARGET_LOCAL
        assert bot.is_remote is False

    def test_runtime_target_accepts_server_uuid(self) -> None:
        server_id = "0d4b3a8f-2c2d-4f1c-9a52-9f1c0fdfb4ad"
        bot = BotConfig(name="x", QQID=1, runtime_target=server_id)
        assert bot.runtime_target == server_id
        assert bot.is_remote is True

    def test_runtime_target_normalizes_blank_to_local(self) -> None:
        bot = BotConfig(name="x", QQID=1, runtime_target="   ")
        assert bot.runtime_target == RUNTIME_TARGET_LOCAL

    def test_runtime_target_normalizes_none_to_local(self) -> None:
        bot = BotConfig(name="x", QQID=1, runtime_target=None)
        assert bot.runtime_target == RUNTIME_TARGET_LOCAL

    def test_runtime_target_normalizes_non_string_to_local(self) -> None:
        # 兼容历史损坏配置: 数字 / bool 等异常输入应静默回退而非抛错
        bot = BotConfig(name="x", QQID=1, runtime_target=0)  # type: ignore[arg-type]
        assert bot.runtime_target == RUNTIME_TARGET_LOCAL

    def test_runtime_target_strips_whitespace(self) -> None:
        bot = BotConfig(name="x", QQID=1, runtime_target="  abc  ")
        assert bot.runtime_target == "abc"

    def test_legacy_payload_without_runtime_target_migrates_to_local(self) -> None:
        legacy_payload = {
            "bot": {"name": "Legacy", "QQID": 114514},
            "connect": {},
            "advanced": {},
        }
        migrated, _, rules_applied = migrate_bot_config_payload(legacy_payload)
        bots = migrated["bots"]
        assert isinstance(bots, list) and len(bots) == 1
        assert bots[0]["bot"]["runtime_target"] == RUNTIME_TARGET_LOCAL
        assert any("bot.runtime_target default" in rule for rule in rules_applied)


# ==================== W4: SnowLuma WebUI 密码 override 迁移 ====================
class TestSnowLumaPasswordOverrideMigration:
    """W4 (2026-05-11): ``bot.snowluma_webui_password_override`` 迁移到 App 级 cfg.

    迁移路径: 旧 bot.json 字段被 ``_migrate_legacy_bot_fields`` pop 掉, 非空值 push 到
    ``_DEFERRED_APP_SNOWLUMA_OVERRIDES`` 模块队列. ``operate_config._read_config_file``
    在迁移完成后调 :func:`consume_deferred_snowluma_overrides` 取出值, 写到
    ``cfg.snowluma_webui_password_override``.
    """

    def test_botconfig_no_longer_has_password_override_field(self) -> None:
        """W4: ``BotConfig.snowluma_webui_password_override`` 字段应被删除."""
        bot = BotConfig(name="x", QQID=1)
        assert not hasattr(bot, "snowluma_webui_password_override"), (
            "W4 应删除 BotConfig.snowluma_webui_password_override 字段"
        )

    def test_legacy_payload_with_nonempty_override_pushed_to_deferred(self) -> None:
        """旧 bot.json 含非空 override → 字段被 pop + push 到 deferred 队列."""
        legacy_payload = {
            "bot": {
                "name": "Legacy",
                "QQID": 12345,
                "snowluma_webui_password_override": "MyP@ssw0rd!",
            },
            "connect": {},
            "advanced": {},
        }
        migrated, _src_v, rules_applied = migrate_bot_config_payload(legacy_payload)

        # bot.json 中字段消失
        migrated_bot = migrated["bots"][0]["bot"]
        assert "snowluma_webui_password_override" not in migrated_bot

        # 迁移 rule 含 "migrated to app.snowluma.webui_password_override"
        assert any(
            "snowluma_webui_password_override migrated" in rule for rule in rules_applied
        )

        # deferred 队列已收到值
        drained = consume_deferred_snowluma_overrides()
        assert drained == ["MyP@ssw0rd!"]

    def test_legacy_payload_with_empty_override_silently_dropped(self) -> None:
        """旧 bot.json 含**空**字符串 override → 字段被 pop 但**不** push 到 deferred."""
        legacy_payload = {
            "bot": {
                "name": "Legacy",
                "QQID": 22222,
                "snowluma_webui_password_override": "",
            },
            "connect": {},
            "advanced": {},
        }
        migrated, _src_v, rules_applied = migrate_bot_config_payload(legacy_payload)

        # 字段被 pop
        assert "snowluma_webui_password_override" not in migrated["bots"][0]["bot"]
        # 迁移 rule 应含 "removed (empty legacy default)"
        assert any(
            "removed (empty legacy default)" in rule for rule in rules_applied
        )
        # deferred 队列保持空
        assert consume_deferred_snowluma_overrides() == []

    def test_legacy_payload_without_override_field_no_op(self) -> None:
        """旧 bot.json **不**含 override 字段 (新 bot.json 或干净 legacy) → 无迁移."""
        legacy_payload = {
            "bot": {"name": "Clean", "QQID": 33333},
            "connect": {},
            "advanced": {},
        }
        migrated, _src_v, rules_applied = migrate_bot_config_payload(legacy_payload)

        assert "snowluma_webui_password_override" not in migrated["bots"][0]["bot"]
        # 没有迁移 rule 涉及 password override
        assert not any("snowluma_webui_password_override" in rule for rule in rules_applied)
        assert consume_deferred_snowluma_overrides() == []

    def test_multiple_bots_with_different_overrides_all_queued(self) -> None:
        """多 Bot 各自有 override → deferred 队列按 push 顺序收集, 由消费者决策保留哪个."""
        legacy_payload_a = {
            "bot": {
                "name": "BotA",
                "QQID": 40001,
                "snowluma_webui_password_override": "FirstPass",
            },
            "connect": {},
            "advanced": {},
        }
        legacy_payload_b = {
            "bot": {
                "name": "BotB",
                "QQID": 40002,
                "snowluma_webui_password_override": "SecondPass",
            },
            "connect": {},
            "advanced": {},
        }
        # 模拟一次性迁移两个 Bot (实际场景下 read_config 会循环)
        migrate_bot_config_payload(legacy_payload_a)
        migrate_bot_config_payload(legacy_payload_b)

        drained = consume_deferred_snowluma_overrides()
        # 顺序保留, 消费者负责取首项
        assert drained == ["FirstPass", "SecondPass"]

    def test_compat_version_bumped_to_v21(self) -> None:
        """W4: ``BOT_CONFIG_COMPAT_VERSION`` 应从 v2.0 升到 v2.1 (删字段是 breaking)."""
        assert BOT_CONFIG_COMPAT_VERSION == "v2.1"

    def test_consume_drains_and_clears_queue(self) -> None:
        """``consume_deferred_snowluma_overrides`` 调一次后队列应清空, 第二次返回空 list."""
        legacy_payload = {
            "bot": {
                "name": "x",
                "QQID": 50001,
                "snowluma_webui_password_override": "Drained!",
            },
            "connect": {},
            "advanced": {},
        }
        migrate_bot_config_payload(legacy_payload)

        first = consume_deferred_snowluma_overrides()
        second = consume_deferred_snowluma_overrides()
        assert first == ["Drained!"]
        assert second == []

    def test_reset_helper_clears_queue(self) -> None:
        """``reset_deferred_snowluma_overrides`` 用于 pytest fixture, 强制清空."""
        legacy_payload = {
            "bot": {
                "name": "x",
                "QQID": 50002,
                "snowluma_webui_password_override": "WillBeReset!",
            },
            "connect": {},
            "advanced": {},
        }
        migrate_bot_config_payload(legacy_payload)
        reset_deferred_snowluma_overrides()
        assert consume_deferred_snowluma_overrides() == []

# -*- coding: utf-8 -*-

# 标准库导入
import json
from pathlib import Path

# 第三方库导入
import pytest

# 项目内模块导入
import src.core.config.operate_config as operate_config
from src.core.config.config_enum import TimeUnitEnum
from src.core.config.config_model import (
    BOT_CONFIG_COMPAT_VERSION,
    AdvancedConfig,
    AutoRestartScheduleConfig,
    BotConfig,
    Config,
    ConnectConfig,
    NapCatConfig,
    OneBotConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
)


class DummyPathFunc:
    """用于测试的路径对象。"""

    def __init__(self, root: Path) -> None:
        self.runtime_path = root / "runtime"
        self.config_dir_path = self.runtime_path / "config"
        self.bot_config_path = self.config_dir_path / "bot.json"
        self.napcat_config_path = self.runtime_path / "NapCatQQ" / "config"


def patch_path_func(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> DummyPathFunc:
    """将 operate_config 的路径解析定向到临时目录。"""
    fake_path_func = DummyPathFunc(tmp_path)
    monkeypatch.setattr(operate_config, "it", lambda _cls: fake_path_func)
    return fake_path_func


def make_config(qqid: int, name: str = "TestBot") -> Config:
    """构造一份可写入的完整 Bot 配置。"""
    return Config(
        bot=BotConfig(
            name=name,
            QQID=qqid,
            musicSignUrl=f"https://example.com/music/{qqid}",
            autoRestartSchedule=AutoRestartScheduleConfig(enable=True, time_unit="h", duration=2),
            offlineAutoRestart=False,
        ),
        connect=ConnectConfig(
            httpServers=[],
            httpSseServers=[],
            httpClients=[],
            websocketServers=[],
            websocketClients=[],
            plugins=[],
        ),
        advanced=AdvancedConfig(
            autoStart=False,
            offlineNotice=True,
            parseMultMsg=False,
            packetServer="ws://127.0.0.1:3001",
            enableLocalFile2Url=False,
            fileLog=True,
            consoleLog=False,
            fileLogLevel="debug",
            consoleLogLevel="info",
            o3HookMode=1,
        ),
    )


def write_json(path: Path, payload) -> None:
    """写入 JSON 文件。"""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=4), encoding="utf-8")


def read_json(path: Path):
    """读取 JSON 文件。"""
    return json.loads(path.read_text(encoding="utf-8"))


def payload(model) -> dict | list:
    """将模型序列化为 JSON 结构。"""
    return json.loads(model.model_dump_json())


def expected_bot_root(configs: list[Config]) -> dict:
    """构造当前 bot.json 根结构。"""
    return {
        "info": {
            "configVersion": BOT_CONFIG_COMPAT_VERSION,
        },
        "bots": [payload(config) for config in configs],
    }


def expected_onebot_config(config: Config) -> dict:
    """构造预期的 onebot JSON。"""
    return payload(
        OneBotConfig(
            network=config.connect,
            musicSignUrl=config.bot.musicSignUrl,
            enableLocalFile2Url=config.advanced.enableLocalFile2Url,
            parseMultMsg=config.advanced.parseMultMsg,
        )
    )


def expected_napcat_config(config: Config) -> dict:
    """构造预期的 napcat JSON。"""
    return payload(
        NapCatConfig(
            fileLog=config.advanced.fileLog,
            consoleLog=config.advanced.consoleLog,
            fileLogLevel=config.advanced.fileLogLevel,
            consoleLogLevel=config.advanced.consoleLogLevel,
            packetBackend=config.advanced.packetBackend,
            packetServer=config.advanced.packetServer,
            o3HookMode=config.advanced.o3HookMode,
            bypass=config.advanced.bypass,
        )
    )


def make_v15_legacy_payload() -> list[dict]:
    """构造 v1.4/v1.5 旧版 bot.json 结构。"""
    return [
        {
            "bot": {
                "name": "LegacyBot",
                "QQID": "114514",
                "messagePostFormat": "array",
                "reportSelfMessage": True,
                "musicSignUrl": "https://example.com/music/114514",
                "heartInterval": "45000",
                "token": "legacy-token",
            },
            "connect": {
                "http": {
                    "enable": True,
                    "host": "127.0.0.1",
                    "port": 3000,
                    "secret": "",
                    "enableHeart": False,
                    "enablePost": True,
                    "postUrls": ["https://example.com/post"],
                },
                "ws": {
                    "enable": True,
                    "host": "127.0.0.1",
                    "port": 3001,
                },
                "reverseWs": {
                    "enable": True,
                    "urls": ["ws://example.com/ws"],
                },
            },
            "advanced": {
                "debug": True,
                "localFile2url": True,
                "fileLog": True,
                "consoleLog": False,
                "enableLocalFile2Url": "debug",
                "consoleLogLevel": "info",
                "autoStart": True,
                "offline_notice": True,
                "packetServer": "http://packet.example.com",
            },
        }
    ]


def test_read_config_returns_empty_when_file_missing(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """缺少 bot.json 时不应创建新文件。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)

    assert operate_config.read_config() == []
    assert not fake_path_func.bot_config_path.exists()


def test_read_config_keeps_invalid_file_untouched(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """bot.json 损坏时只返回空列表，不覆盖原文件。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    fake_path_func.bot_config_path.parent.mkdir(parents=True, exist_ok=True)
    fake_path_func.bot_config_path.write_text("{invalid json", encoding="utf-8")

    assert operate_config.read_config() == []
    assert fake_path_func.bot_config_path.read_text(encoding="utf-8") == "{invalid json"


def test_read_config_keeps_invalid_root_object_untouched(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """非法对象根节点不应被静默迁移为空配置。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    invalid_payload = {
        "info": {
            "configVersion": BOT_CONFIG_COMPAT_VERSION,
        },
        "items": [],
    }
    write_json(fake_path_func.bot_config_path, invalid_payload)

    assert operate_config.read_config() == []
    assert read_json(fake_path_func.bot_config_path) == invalid_payload


def test_read_config_migrates_legacy_root_list_to_versioned_object(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """legacy 纯列表根节点应迁移到带版本号的对象根结构。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    legacy_payload = [payload(make_config(114514, "LegacyRoot"))]
    write_json(fake_path_func.bot_config_path, legacy_payload)

    configs = operate_config.read_config()

    assert len(configs) == 1
    assert configs[0].bot.QQID == 114514
    assert read_json(fake_path_func.bot_config_path) == expected_bot_root(configs)
    assert read_json(fake_path_func.bot_config_path.with_name("bot.json.bak")) == legacy_payload


def test_read_config_migrates_v15_network_shape(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """v1.4/v1.5 的旧网络结构应迁移为当前 connect 列表结构。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    legacy_payload = make_v15_legacy_payload()
    write_json(fake_path_func.bot_config_path, legacy_payload)

    configs = operate_config.read_config()

    assert len(configs) == 1
    config = configs[0]

    assert config.advanced.parseMultMsg is False
    assert config.advanced.offlineNotice is True
    assert config.advanced.enableLocalFile2Url is True
    assert config.advanced.fileLogLevel == "debug"
    assert config.advanced.bypass.hook is False
    assert config.advanced.bypass.window is False
    assert config.advanced.bypass.module is False
    assert config.advanced.bypass.process is False
    assert config.advanced.bypass.container is False
    assert config.advanced.bypass.js is False
    assert len(config.connect.httpServers) == 1
    assert len(config.connect.httpClients) == 1
    assert len(config.connect.websocketServers) == 1
    assert len(config.connect.websocketClients) == 1
    assert config.connect.httpServers[0].host == "127.0.0.1"
    assert config.connect.httpClients[0].url.unicode_string() == "https://example.com/post"
    assert config.connect.websocketClients[0].url.unicode_string() == "ws://example.com/ws"
    assert config.connect.websocketClients[0].enable is True

    migrated_root = read_json(fake_path_func.bot_config_path)
    assert migrated_root["info"]["configVersion"] == BOT_CONFIG_COMPAT_VERSION
    assert "debug" not in migrated_root["bots"][0]["advanced"]
    assert "localFile2url" not in migrated_root["bots"][0]["advanced"]
    assert "offline_notice" not in migrated_root["bots"][0]["advanced"]
    assert "http" not in migrated_root["bots"][0]["connect"]
    assert "ws" not in migrated_root["bots"][0]["connect"]
    assert "reverseWs" not in migrated_root["bots"][0]["connect"]
    assert migrated_root["bots"][0]["advanced"]["bypass"] == {
        "hook": False,
        "window": False,
        "module": False,
        "process": False,
        "container": False,
        "js": False,
    }
    assert read_json(fake_path_func.bot_config_path.with_name("bot.json.bak")) == legacy_payload


def test_read_config_preserves_legacy_reverse_ws_enable_flag(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """旧版 reverseWs.enable 为 false 时，迁移后不应被强制打开。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    legacy_payload = make_v15_legacy_payload()
    legacy_payload[0]["connect"]["reverseWs"]["enable"] = False
    write_json(fake_path_func.bot_config_path, legacy_payload)

    configs = operate_config.read_config()

    assert len(configs) == 1
    assert len(configs[0].connect.websocketClients) == 1
    assert configs[0].connect.websocketClients[0].enable is False


def test_read_config_normalizes_legacy_interval_schedule(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """旧版 interval/jitter 自动重启配置应兼容为当前结构。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    legacy_payload = payload(make_config(114514, "LegacyInterval"))
    legacy_payload["bot"]["autoRestartSchedule"] = {
        "enable": True,
        "interval": "15m",
        "jitter": 0,
    }
    write_json(fake_path_func.bot_config_path, [legacy_payload])

    configs = operate_config.read_config()

    assert len(configs) == 1
    schedule = configs[0].bot.autoRestartSchedule
    assert schedule.enable is True
    assert schedule.time_unit == TimeUnitEnum.MINUTE
    assert schedule.duration == 15


def test_read_config_disables_unsupported_legacy_cron_schedule(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """旧版 crontab 自动重启配置应安全降级为禁用状态。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    legacy_payload = payload(make_config(114514, "LegacyCron"))
    legacy_payload["bot"]["autoRestartSchedule"] = {
        "taskType": "crontab",
        "interval": "6h",
        "crontab": "0 4 * * *",
        "jitter": 0,
    }
    write_json(fake_path_func.bot_config_path, [legacy_payload])

    configs = operate_config.read_config()

    assert len(configs) == 1
    schedule = configs[0].bot.autoRestartSchedule
    assert schedule.enable is False
    assert schedule.time_unit == TimeUnitEnum.HOUR
    assert schedule.duration == 6


def test_update_config_writes_all_files_atomically(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """成功保存时应同时写入 bot.json 和 NapCat 派生配置。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    config = make_config(114514, "AtomicBot")

    assert operate_config.update_config(config) is True

    onebot_path = fake_path_func.napcat_config_path / "onebot11_114514.json"
    napcat_path = fake_path_func.napcat_config_path / "napcat_114514.json"

    assert read_json(fake_path_func.bot_config_path) == expected_bot_root([config])
    assert read_json(onebot_path) == expected_onebot_config(config)
    assert read_json(napcat_path) == expected_napcat_config(config)


def test_update_config_preserves_websocket_entries_in_bot_and_onebot_files(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """保存带 WebSocket 配置的 Bot 时，不应在 bot.json 或 onebot 配置里丢失。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    config = make_config(114514, "WebsocketBot")
    config.connect.websocketServers.append(
        WebsocketServersConfig(
            name="wss-main",
            host="127.0.0.1",
            port=3001,
            enable=True,
            reportSelfMessage=True,
            enableForcePushEvent=True,
            heartInterval=45000,
        )
    )
    config.connect.websocketClients.append(
        WebsocketClientsConfig(
            name="wsc-main",
            url="ws://127.0.0.1:3002/ws",
            enable=True,
            reportSelfMessage=True,
            heartInterval=46000,
            reconnectInterval=47000,
        )
    )

    assert operate_config.update_config(config) is True

    saved_configs = operate_config.read_config()
    assert len(saved_configs) == 1
    assert len(saved_configs[0].connect.websocketServers) == 1
    assert len(saved_configs[0].connect.websocketClients) == 1
    assert saved_configs[0].connect.websocketServers[0].name == "wss-main"
    assert saved_configs[0].connect.websocketServers[0].port == 3001
    assert saved_configs[0].connect.websocketClients[0].name == "wsc-main"
    assert saved_configs[0].connect.websocketClients[0].url.unicode_string() == "ws://127.0.0.1:3002/ws"

    onebot_path = fake_path_func.napcat_config_path / "onebot11_114514.json"
    onebot_payload = read_json(onebot_path)
    assert len(onebot_payload["network"]["websocketServers"]) == 1
    assert len(onebot_payload["network"]["websocketClients"]) == 1
    assert onebot_payload["network"]["websocketServers"][0]["name"] == "wss-main"
    assert onebot_payload["network"]["websocketClients"][0]["name"] == "wsc-main"


def test_merge_config_for_update_preserves_webui_changes_when_desktop_edits_other_fields(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """WebUI 修改派生配置、Desktop 修改其他字段时，应无感合并两边改动。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    base_config = make_config(114514, "BaseBot")
    write_json(fake_path_func.bot_config_path, expected_bot_root([base_config]))
    write_json(fake_path_func.napcat_config_path / "onebot11_114514.json", expected_onebot_config(base_config))
    write_json(fake_path_func.napcat_config_path / "napcat_114514.json", expected_napcat_config(base_config))

    webui_onebot_payload = expected_onebot_config(base_config)
    webui_onebot_payload["network"]["websocketClients"] = [
        payload(
            WebsocketClientsConfig(
                name="webui-wsc",
                url="ws://127.0.0.1:3002/ws",
                reportSelfMessage=True,
                heartInterval=46000,
                reconnectInterval=47000,
            )
        )
    ]
    write_json(fake_path_func.napcat_config_path / "onebot11_114514.json", webui_onebot_payload)

    local_draft = base_config.model_copy(deep=True)
    local_draft.bot.name = "DesktopRenamed"

    merged = operate_config.merge_config_for_update(local_draft, base_config=base_config)

    assert merged.bot.name == "DesktopRenamed"
    assert len(merged.connect.websocketClients) == 1
    assert merged.connect.websocketClients[0].name == "webui-wsc"
    assert merged.connect.websocketClients[0].url.unicode_string() == "ws://127.0.0.1:3002/ws"


def test_merge_config_for_update_prefers_desktop_changes_on_conflict(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """同一字段两边都改时，应以 Desktop 当前保存值为准。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    base_config = make_config(114514, "BaseBot")
    base_config.connect.websocketClients.append(
        WebsocketClientsConfig(name="shared", url="ws://127.0.0.1:3001/ws")
    )
    write_json(fake_path_func.bot_config_path, expected_bot_root([base_config]))
    write_json(fake_path_func.napcat_config_path / "onebot11_114514.json", expected_onebot_config(base_config))
    write_json(fake_path_func.napcat_config_path / "napcat_114514.json", expected_napcat_config(base_config))

    webui_onebot_payload = expected_onebot_config(base_config)
    webui_onebot_payload["network"]["websocketClients"][0]["url"] = "ws://127.0.0.1:4000/ws"
    write_json(fake_path_func.napcat_config_path / "onebot11_114514.json", webui_onebot_payload)

    local_draft = base_config.model_copy(deep=True)
    local_draft.connect.websocketClients[0] = WebsocketClientsConfig(name="shared", url="ws://127.0.0.1:5000/ws")

    merged = operate_config.merge_config_for_update(local_draft, base_config=base_config)

    assert merged.connect.websocketClients[0].url.unicode_string() == "ws://127.0.0.1:5000/ws"


def test_update_config_fails_when_existing_bot_file_is_invalid(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """现有 bot.json 非法时应直接失败且不写新文件。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    fake_path_func.bot_config_path.parent.mkdir(parents=True, exist_ok=True)
    fake_path_func.bot_config_path.write_text("{broken", encoding="utf-8")

    onebot_path = fake_path_func.napcat_config_path / "onebot11_114514.json"
    napcat_path = fake_path_func.napcat_config_path / "napcat_114514.json"
    write_json(onebot_path, {"old": "onebot"})
    write_json(napcat_path, {"old": "napcat"})

    assert operate_config.update_config(make_config(114514)) is False

    assert fake_path_func.bot_config_path.read_text(encoding="utf-8") == "{broken"
    assert read_json(onebot_path) == {"old": "onebot"}
    assert read_json(napcat_path) == {"old": "napcat"}


def test_update_config_rolls_back_when_replace_fails(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """提交阶段失败时，所有目标文件都应保持原状。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    old_config = make_config(114514, "OldBot")
    new_config = make_config(114514, "NewBot")

    onebot_path = fake_path_func.napcat_config_path / "onebot11_114514.json"
    napcat_path = fake_path_func.napcat_config_path / "napcat_114514.json"

    original_bot_payload = expected_bot_root([old_config])
    original_onebot_payload = expected_onebot_config(old_config)
    original_napcat_payload = expected_napcat_config(old_config)

    write_json(fake_path_func.bot_config_path, original_bot_payload)
    write_json(onebot_path, original_onebot_payload)
    write_json(napcat_path, original_napcat_payload)

    original_replace = operate_config._replace_path

    def flaky_replace(src: Path, dst: Path) -> None:
        if dst == napcat_path and ".tmp." in src.name:
            raise OSError("simulated replace failure")
        original_replace(src, dst)

    monkeypatch.setattr(operate_config, "_replace_path", flaky_replace)

    assert operate_config.update_config(new_config) is False

    assert read_json(fake_path_func.bot_config_path) == original_bot_payload
    assert read_json(onebot_path) == original_onebot_payload
    assert read_json(napcat_path) == original_napcat_payload


def test_delete_config_removes_saved_files(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """删除成功时应同步移除 bot.json 条目和派生配置。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    target_config = make_config(114514, "DeleteMe")
    remain_config = make_config(1919810, "KeepMe")

    onebot_path = fake_path_func.napcat_config_path / "onebot11_114514.json"
    napcat_path = fake_path_func.napcat_config_path / "napcat_114514.json"

    write_json(fake_path_func.bot_config_path, expected_bot_root([target_config, remain_config]))
    write_json(onebot_path, expected_onebot_config(target_config))
    write_json(napcat_path, expected_napcat_config(target_config))

    assert operate_config.delete_config(target_config) is True

    assert read_json(fake_path_func.bot_config_path) == expected_bot_root([remain_config])
    assert not onebot_path.exists()
    assert not napcat_path.exists()


def test_delete_config_succeeds_when_derived_files_are_missing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """派生配置已缺失时，删除主配置仍应成功。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    target_config = make_config(114514, "DeleteMissing")

    write_json(fake_path_func.bot_config_path, expected_bot_root([target_config]))

    assert operate_config.delete_config(target_config) is True
    assert read_json(fake_path_func.bot_config_path) == expected_bot_root([])


def test_delete_config_rolls_back_when_commit_fails(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """删除流程提交失败时应恢复 bot.json 与派生配置。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    target_config = make_config(114514, "DeleteRollback")

    onebot_path = fake_path_func.napcat_config_path / "onebot11_114514.json"
    napcat_path = fake_path_func.napcat_config_path / "napcat_114514.json"

    original_bot_payload = expected_bot_root([target_config])
    original_onebot_payload = expected_onebot_config(target_config)
    original_napcat_payload = expected_napcat_config(target_config)

    write_json(fake_path_func.bot_config_path, original_bot_payload)
    write_json(onebot_path, original_onebot_payload)
    write_json(napcat_path, original_napcat_payload)

    original_replace = operate_config._replace_path

    def flaky_replace(src: Path, dst: Path) -> None:
        if dst == fake_path_func.bot_config_path and ".tmp." in src.name:
            raise OSError("simulated delete failure")
        original_replace(src, dst)

    monkeypatch.setattr(operate_config, "_replace_path", flaky_replace)

    assert operate_config.delete_config(target_config) is False

    assert read_json(fake_path_func.bot_config_path) == original_bot_payload
    assert read_json(onebot_path) == original_onebot_payload
    assert read_json(napcat_path) == original_napcat_payload

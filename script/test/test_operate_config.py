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
    AdvancedConfig,
    AutoRestartScheduleConfig,
    BotConfig,
    Config,
    ConnectConfig,
    NapCatConfig,
    OneBotConfig,
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
        )
    )


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

    assert read_json(fake_path_func.bot_config_path) == [payload(config)]
    assert read_json(onebot_path) == expected_onebot_config(config)
    assert read_json(napcat_path) == expected_napcat_config(config)


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

    original_bot_payload = [payload(old_config)]
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

    write_json(fake_path_func.bot_config_path, [payload(target_config), payload(remain_config)])
    write_json(onebot_path, expected_onebot_config(target_config))
    write_json(napcat_path, expected_napcat_config(target_config))

    assert operate_config.delete_config(target_config) is True

    assert read_json(fake_path_func.bot_config_path) == [payload(remain_config)]
    assert not onebot_path.exists()
    assert not napcat_path.exists()


def test_delete_config_succeeds_when_derived_files_are_missing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """派生配置已缺失时，删除主配置仍应成功。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    target_config = make_config(114514, "DeleteMissing")

    write_json(fake_path_func.bot_config_path, [payload(target_config)])

    assert operate_config.delete_config(target_config) is True
    assert read_json(fake_path_func.bot_config_path) == []


def test_delete_config_rolls_back_when_commit_fails(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """删除流程提交失败时应恢复 bot.json 与派生配置。"""
    fake_path_func = patch_path_func(monkeypatch, tmp_path)
    target_config = make_config(114514, "DeleteRollback")

    onebot_path = fake_path_func.napcat_config_path / "onebot11_114514.json"
    napcat_path = fake_path_func.napcat_config_path / "napcat_114514.json"

    original_bot_payload = [payload(target_config)]
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

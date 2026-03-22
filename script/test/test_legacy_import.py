# -*- coding: utf-8 -*-

# 标准库导入
import json
from pathlib import Path

# 第三方库导入
import pytest

# 项目内模块导入
import src.core.config as app_config_module
import src.core.config.legacy_import as legacy_import
from src.core.config.config_model import BOT_CONFIG_COMPAT_VERSION, serialize_bot_config_collection


class DummyPathFunc:
    """用于测试导入服务的路径对象。"""

    def __init__(self, root: Path) -> None:
        self.runtime_path = root / "runtime"
        self.config_dir_path = self.runtime_path / "config"
        self.bot_config_path = self.config_dir_path / "bot.json"
        self.config_path = self.config_dir_path / "config.json"
        self.tmp_path = self.runtime_path / "tmp"
        self.napcat_config_path = self.runtime_path / "NapCatQQ" / "config"


def write_json(path: Path, payload) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=4), encoding="utf-8")


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def payload(model) -> dict | list:
    return json.loads(model.model_dump_json())


def patch_runtime(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> DummyPathFunc:
    fake_path_func = DummyPathFunc(tmp_path)
    monkeypatch.setattr(legacy_import, "_get_path_func", lambda: fake_path_func)
    return fake_path_func


def test_scan_legacy_config_folder_prefers_runtime_layout_and_ignores_qq_version_config(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    fake_path_func = patch_runtime(monkeypatch, tmp_path)
    current_config = config_factory(114514, "CurrentBot")
    monkeypatch.setattr(legacy_import, "_read_config_file", lambda strict: [current_config])

    source_root = tmp_path / "source"
    app_payload = {
        "Info": {"main_window": True},
        "Personalized": {"CloseBtnAction": 1},
        "Personalize": {"ThemeMode": "Dark"},
    }
    imported_bot = config_factory(114514, "ImportedBot")

    write_json(source_root / "runtime" / "config" / "config.json", app_payload)
    write_json(source_root / "runtime" / "config" / "bot.json", [payload(imported_bot)])
    write_json(source_root / "QQ" / "versions" / "config.json", {"curVersion": "9.9.99"})
    write_json(source_root / "random" / "config.json", {"foo": "bar"})
    write_json(source_root / "runtime" / "NapCatQQ" / "config" / "onebot11_114514.json", {"legacy": True})

    result = legacy_import.scan_legacy_config_folder(source_root)

    assert result.root_path == source_root.resolve()
    assert result.app_config_path == source_root / "runtime" / "config" / "config.json"
    assert result.bot_config_path == source_root / "runtime" / "config" / "bot.json"
    assert result.imported_bot_count == 1
    assert len(result.conflicts) == 1
    assert result.conflicts[0].qqid == 114514
    assert result.conflicts[0].current_name == "CurrentBot"
    assert result.conflicts[0].imported_name == "ImportedBot"
    assert result.auxiliary_paths == (source_root / "runtime" / "NapCatQQ" / "config" / "onebot11_114514.json",)


def test_apply_legacy_config_import_replace_overwrites_targets(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    fake_path_func = patch_runtime(monkeypatch, tmp_path)
    fake_path_func.tmp_path.mkdir(parents=True, exist_ok=True)

    current_bot = config_factory(114514, "CurrentBot")
    removed_bot = config_factory(223344, "RemovedBot")
    monkeypatch.setattr(legacy_import, "_read_config_file", lambda strict: [current_bot, removed_bot])

    write_json(fake_path_func.config_path, {"Info": {"ConfigVersion": "v2.0"}})
    write_json(
        fake_path_func.bot_config_path,
        serialize_bot_config_collection([current_bot, removed_bot]),
    )
    write_json(fake_path_func.napcat_config_path / "onebot11_114514.json", {"old": "onebot"})
    write_json(fake_path_func.napcat_config_path / "napcat_114514.json", {"old": "napcat"})
    write_json(fake_path_func.napcat_config_path / "onebot11_223344.json", {"old": "onebot"})
    write_json(fake_path_func.napcat_config_path / "napcat_223344.json", {"old": "napcat"})

    source_root = tmp_path / "source"
    imported_bot = config_factory(114514, "ImportedBot")
    imported_payload = {
        "Info": {"main_window": True},
        "Personalize": {"ThemeMode": "Dark", "ThemeColor": "#ff123456"},
    }
    write_json(source_root / "config" / "config.json", imported_payload)
    write_json(source_root / "config" / "bot.json", [payload(imported_bot)])

    scan_result = legacy_import.scan_legacy_config_folder(source_root)
    result = legacy_import.apply_legacy_config_import(
        legacy_import.ImportExecutionPlan(
            scan_result=scan_result,
            import_app_config=True,
            bot_import_mode="replace",
        )
    )

    migrated_app = read_json(fake_path_func.config_path)
    migrated_bot_root = read_json(fake_path_func.bot_config_path)

    assert result.app_imported is True
    assert result.bot_imported is True
    assert result.imported_bot_count == 1
    assert result.replaced_bot_count == 1
    assert result.appended_bot_count == 0
    assert result.skipped_bot_count == 0
    assert result.backup_dir.exists()
    assert migrated_app["Info"]["ConfigVersion"] == app_config_module._CURRENT_CONFIG_COMPAT_VERSION
    assert migrated_app["Info"]["MainWindow"] is True
    assert migrated_app["QFluentWidgets"]["ThemeMode"] == "Dark"
    assert migrated_bot_root["info"]["configVersion"] == BOT_CONFIG_COMPAT_VERSION
    assert [bot["bot"]["name"] for bot in migrated_bot_root["bots"]] == ["ImportedBot"]
    assert (fake_path_func.napcat_config_path / "onebot11_114514.json").exists()
    assert (fake_path_func.napcat_config_path / "napcat_114514.json").exists()
    assert not (fake_path_func.napcat_config_path / "onebot11_223344.json").exists()
    assert not (fake_path_func.napcat_config_path / "napcat_223344.json").exists()


def test_apply_legacy_config_import_append_overwrites_selected_conflicts(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    fake_path_func = patch_runtime(monkeypatch, tmp_path)
    fake_path_func.tmp_path.mkdir(parents=True, exist_ok=True)

    current_a = config_factory(114514, "CurrentA")
    current_b = config_factory(223344, "CurrentB")
    monkeypatch.setattr(legacy_import, "_read_config_file", lambda strict: [current_a, current_b])

    source_root = tmp_path / "source"
    imported_a = config_factory(114514, "ImportedA")
    imported_c = config_factory(556677, "ImportedC")
    write_json(source_root / "runtime" / "config" / "bot.json", [payload(imported_a), payload(imported_c)])

    scan_result = legacy_import.scan_legacy_config_folder(source_root)
    result = legacy_import.apply_legacy_config_import(
        legacy_import.ImportExecutionPlan(
            scan_result=scan_result,
            import_app_config=False,
            bot_import_mode="append",
            overwrite_conflict_qqids=frozenset({114514}),
        )
    )

    migrated_root = read_json(fake_path_func.bot_config_path)
    bot_names_by_qqid = {int(item["bot"]["QQID"]): item["bot"]["name"] for item in migrated_root["bots"]}

    assert result.app_imported is False
    assert result.bot_imported is True
    assert result.imported_bot_count == 2
    assert result.replaced_bot_count == 1
    assert result.appended_bot_count == 1
    assert result.skipped_bot_count == 0
    assert bot_names_by_qqid[114514] == "ImportedA"
    assert bot_names_by_qqid[223344] == "CurrentB"
    assert bot_names_by_qqid[556677] == "ImportedC"
    assert (fake_path_func.napcat_config_path / "onebot11_556677.json").exists()
    assert (fake_path_func.napcat_config_path / "napcat_556677.json").exists()

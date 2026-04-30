# -*- coding: utf-8 -*-

# 标准库导入
import json
import zipfile
from pathlib import Path

# 第三方库导入
import pytest

# 项目内模块导入
import src.core.config as app_config_module
import src.core.config.config_export as config_export


class DummyPathFunc:
    """用于测试导出服务的路径对象。"""

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


def read_zip_json(zip_path: Path, member_name: str):
    with zipfile.ZipFile(zip_path, "r") as zip_file:
        return json.loads(zip_file.read(member_name).decode("utf-8"))


def list_zip_members(zip_path: Path) -> list[str]:
    with zipfile.ZipFile(zip_path, "r") as zip_file:
        return zip_file.namelist()


def patch_runtime(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> DummyPathFunc:
    fake_path_func = DummyPathFunc(tmp_path)
    monkeypatch.setattr(config_export, "_get_path_func", lambda: fake_path_func)
    return fake_path_func


def test_apply_config_export_exports_only_bot_config_when_app_config_missing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    patch_runtime(monkeypatch, tmp_path)
    bot_config = config_factory(114514, "ExportBot")
    monkeypatch.setattr(config_export, "read_config", lambda: [bot_config])

    target_dir = tmp_path / "exports" / "bot-only"
    scan_result = config_export.scan_current_config_export(target_dir)
    result = config_export.apply_config_export(
        config_export.ExportExecutionPlan(
            scan_result=scan_result,
            export_app_config=False,
            export_bot_config=True,
        )
    )

    assert result.output_dir == target_dir.resolve()
    assert result.app_exported is False
    assert result.bot_exported is True
    assert result.exported_bot_count == 1
    assert result.archive_path.parent == target_dir.resolve()
    assert result.archive_path.name.startswith("napcat-config-export-")
    assert result.archive_path.suffix == ".zip"
    assert result.archive_path.exists()
    assert list_zip_members(result.archive_path) == ["bot.json", "export_meta.json"]
    meta = read_zip_json(result.archive_path, "export_meta.json")
    assert meta["containsAppConfig"] is False
    assert meta["containsBotConfig"] is True
    assert meta["botCount"] == 1


def test_apply_config_export_exports_only_app_config_when_no_bot_config(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    fake_path_func = patch_runtime(monkeypatch, tmp_path)
    monkeypatch.setattr(config_export, "read_config", lambda: [])
    write_json(
        fake_path_func.config_path,
        {
            "Info": {"main_window": True},
            "Personalize": {"ThemeMode": "Dark"},
        },
    )

    target_dir = tmp_path / "exports" / "app-only"
    scan_result = config_export.scan_current_config_export(target_dir)
    result = config_export.apply_config_export(
        config_export.ExportExecutionPlan(
            scan_result=scan_result,
            export_app_config=True,
            export_bot_config=False,
        )
    )

    exported_app = read_zip_json(result.archive_path, "config.json")
    meta = read_zip_json(result.archive_path, "export_meta.json")

    assert result.app_exported is True
    assert result.bot_exported is False
    assert result.archive_path.parent == target_dir.resolve()
    assert result.archive_path.name.startswith("napcat-config-export-")
    assert exported_app["Info"]["ConfigVersion"] == app_config_module._CURRENT_CONFIG_COMPAT_VERSION
    assert exported_app["Info"]["MainWindow"] is True
    assert meta["containsAppConfig"] is True
    assert meta["containsBotConfig"] is False
    assert meta["appConfigVersion"] == app_config_module._CURRENT_CONFIG_COMPAT_VERSION


def test_apply_config_export_creates_target_dir_and_writes_standard_package(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    fake_path_func = patch_runtime(monkeypatch, tmp_path)
    bot_config = config_factory(223344, "ExportBot")
    monkeypatch.setattr(config_export, "read_config", lambda: [bot_config])
    write_json(
        fake_path_func.config_path,
        {
            "Info": {"MainWindow": True, "ConfigVersion": app_config_module._CURRENT_CONFIG_COMPAT_VERSION},
            "Personalize": {"ThemeMode": "Dark"},
        },
    )

    target_dir = tmp_path / "nested" / "exports" / "full"
    scan_result = config_export.scan_current_config_export(target_dir)
    result = config_export.apply_config_export(
        config_export.ExportExecutionPlan(
            scan_result=scan_result,
            export_app_config=True,
            export_bot_config=True,
        )
    )

    exported_bot = read_zip_json(result.archive_path, "bot.json")

    assert result.output_dir == target_dir.resolve()
    assert result.archive_path.exists()
    assert list(result.exported_files) == ["config.json", "bot.json", "export_meta.json"]
    assert exported_bot["info"]["configVersion"] == config_export.BOT_CONFIG_COMPAT_VERSION
    assert exported_bot["bots"][0]["bot"]["name"] == "ExportBot"


def test_scan_current_config_export_auto_renames_zip_when_same_name_exists(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    fake_path_func = patch_runtime(monkeypatch, tmp_path)
    bot_config = config_factory(998877, "ExportBot")
    monkeypatch.setattr(config_export, "read_config", lambda: [bot_config])
    monkeypatch.setattr(config_export, "_default_archive_name", lambda now=None: "napcat-config-export-test.zip")
    write_json(fake_path_func.config_path, {"Info": {"ConfigVersion": app_config_module._CURRENT_CONFIG_COMPAT_VERSION}})

    target_dir = tmp_path / "exports" / "zip-conflict"
    target_dir.mkdir(parents=True, exist_ok=True)
    (target_dir / "napcat-config-export-test.zip").write_text("occupied", encoding="utf-8")

    scan_result = config_export.scan_current_config_export(target_dir)
    result = config_export.apply_config_export(
        config_export.ExportExecutionPlan(
            scan_result=scan_result,
            export_app_config=True,
            export_bot_config=True,
        )
    )

    assert scan_result.archive_path.name == "napcat-config-export-test-1.zip"
    assert result.archive_path.name.endswith("-1.zip")
    assert any("自动写出为" in warning for warning in result.warnings)

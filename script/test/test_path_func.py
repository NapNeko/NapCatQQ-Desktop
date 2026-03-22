# -*- coding: utf-8 -*-

# 标准库导入
from pathlib import Path

# 项目内模块导入
import src.core.runtime.paths as path_func_module


def mute_path_logger(monkeypatch) -> None:
    """屏蔽路径模块的日志副作用。"""
    monkeypatch.setattr(path_func_module.logger, "info", lambda *args, **kwargs: None)
    monkeypatch.setattr(path_func_module.logger, "debug", lambda *args, **kwargs: None)
    monkeypatch.setattr(path_func_module.logger, "warning", lambda *args, **kwargs: None)


def test_path_validator_creates_runtime_directories(monkeypatch, tmp_path: Path) -> None:
    """路径校验应补齐 tmp、config 和 NapCat 目录。"""
    mute_path_logger(monkeypatch)
    monkeypatch.setattr(path_func_module, "resolve_app_base_path", lambda: tmp_path)
    monkeypatch.setattr(path_func_module, "resolve_app_data_path", lambda: tmp_path / "ProgramData")

    path_func = path_func_module.PathFunc()
    path_func.path_validator()

    assert path_func.tmp_path.exists()
    assert path_func.config_dir_path.exists()
    assert path_func.napcat_path.exists()
    assert path_func.runtime_path == tmp_path / "ProgramData" / "runtime"


def test_get_qq_path_reads_registry_install_path(monkeypatch, tmp_path: Path) -> None:
    """QQ 路径解析应从注册表读取 Install 值。"""
    monkeypatch.setattr(path_func_module.winreg, "OpenKey", lambda **kwargs: object())
    monkeypatch.setattr(path_func_module.winreg, "QueryValueEx", lambda key, name: (str(tmp_path / "QQ"), 1))

    assert path_func_module.PathFunc.get_qq_path() == tmp_path / "QQ"


def test_path_migration_moves_old_layout_into_runtime(monkeypatch, tmp_path: Path) -> None:
    """旧版目录布局应被迁移到 runtime 结构。"""
    mute_path_logger(monkeypatch)
    monkeypatch.setattr(path_func_module, "resolve_app_base_path", lambda: tmp_path)
    monkeypatch.setattr(path_func_module, "resolve_app_data_path", lambda: tmp_path / "ProgramData")

    old_napcat = tmp_path / "NapCat"
    old_config = tmp_path / "config"
    old_tmp = tmp_path / "tmp"
    old_napcat.mkdir()
    old_config.mkdir()
    old_tmp.mkdir()
    (old_napcat / "core.txt").write_text("napcat", encoding="utf-8")
    (old_config / "config.json").write_text("{}", encoding="utf-8")
    (old_tmp / "cache.tmp").write_text("tmp", encoding="utf-8")

    path_func = path_func_module.PathFunc()

    assert (path_func.napcat_path / "core.txt").read_text(encoding="utf-8") == "napcat"
    assert (path_func.config_dir_path / "config.json").read_text(encoding="utf-8") == "{}"
    assert (path_func.tmp_path / "cache.tmp").read_text(encoding="utf-8") == "tmp"
    assert not old_napcat.exists()
    assert not old_config.exists()
    assert not old_tmp.exists()


def test_path_migration_moves_install_runtime_into_programdata(monkeypatch, tmp_path: Path) -> None:
    """安装目录下的 runtime 布局应迁移到 ProgramData。"""
    mute_path_logger(monkeypatch)
    monkeypatch.setattr(path_func_module, "resolve_app_base_path", lambda: tmp_path / "install")
    monkeypatch.setattr(path_func_module, "resolve_app_data_path", lambda: tmp_path / "ProgramData")

    install_root = tmp_path / "install"
    old_runtime = install_root / "runtime"
    (old_runtime / "NapCatQQ").mkdir(parents=True)
    (old_runtime / "config").mkdir(parents=True)
    (old_runtime / "tmp").mkdir(parents=True)
    (old_runtime / "NapCatQQ" / "core.txt").write_text("napcat", encoding="utf-8")
    (old_runtime / "config" / "config.json").write_text("{}", encoding="utf-8")
    (old_runtime / "tmp" / "cache.tmp").write_text("tmp", encoding="utf-8")

    path_func = path_func_module.PathFunc()

    assert (path_func.napcat_path / "core.txt").read_text(encoding="utf-8") == "napcat"
    assert (path_func.config_dir_path / "config.json").read_text(encoding="utf-8") == "{}"
    assert (path_func.tmp_path / "cache.tmp").read_text(encoding="utf-8") == "tmp"
    assert not (install_root / "runtime" / "NapCatQQ").exists()
    assert not (install_root / "runtime" / "config").exists()
    assert not (install_root / "runtime" / "tmp").exists()


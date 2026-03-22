# -*- coding: utf-8 -*-

# 标准库导入
import sys
from pathlib import Path

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.platform.app_paths import APP_DATA_DIR_NAME, resolve_app_base_path, resolve_app_data_path


def test_resolve_app_base_path_uses_repo_root_in_source_mode(monkeypatch: pytest.MonkeyPatch) -> None:
    """源码模式下应固定到仓库根目录。"""

    monkeypatch.delattr(sys, "frozen", raising=False)
    assert resolve_app_base_path() == Path(__file__).resolve().parents[2]


def test_resolve_app_data_path_uses_repo_root_in_source_mode(monkeypatch: pytest.MonkeyPatch) -> None:
    """源码模式下数据目录应继续指向仓库根目录。"""

    monkeypatch.delattr(sys, "frozen", raising=False)
    assert resolve_app_data_path() == Path(__file__).resolve().parents[2]


def test_resolve_app_data_path_uses_programdata_when_frozen(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """冻结模式下数据目录应切换到 ProgramData。"""

    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "executable", str(tmp_path / "NapCatQQ-Desktop.exe"), raising=False)
    monkeypatch.setenv("ProgramData", str(tmp_path / "ProgramData"))

    assert resolve_app_data_path() == (tmp_path / "ProgramData" / APP_DATA_DIR_NAME)


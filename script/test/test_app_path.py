# -*- coding: utf-8 -*-

# 标准库导入
import sys
from pathlib import Path

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.platform.app_paths import APP_DATA_DIR_NAME, resolve_app_base_path, resolve_app_data_path


def test_resolve_app_base_path_uses_repo_root_in_source_mode(monkeypatch: pytest.MonkeyPatch) -> None:
    """源码模式下基准目录应固定到仓库根目录.

    flatten 后 ``parents[3]`` 从 ``src/core/platform/app_paths.py`` 解析为仓库根,
    让 ``runtime/`` / ``log/`` / ``config/`` 落在 Python 包外, 便于 git 忽略与备份.
    """

    monkeypatch.delattr(sys, "frozen", raising=False)
    repo_root = Path(__file__).resolve().parents[2]
    assert resolve_app_base_path() == repo_root


def test_resolve_app_data_path_uses_repo_root_in_source_mode(monkeypatch: pytest.MonkeyPatch) -> None:
    """源码模式下数据目录应与基准目录保持一致 (均为仓库根)."""

    monkeypatch.delattr(sys, "frozen", raising=False)
    repo_root = Path(__file__).resolve().parents[2]
    assert resolve_app_data_path() == repo_root


def test_resolve_app_data_path_uses_programdata_when_frozen(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """冻结模式下数据目录应切换到 ProgramData。"""

    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "executable", str(tmp_path / "NapCatQQ-Desktop.exe"), raising=False)
    monkeypatch.setenv("ProgramData", str(tmp_path / "ProgramData"))

    assert resolve_app_data_path() == (tmp_path / "ProgramData" / APP_DATA_DIR_NAME)


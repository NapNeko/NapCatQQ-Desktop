# -*- coding: utf-8 -*-

# 标准库导入
import sys
from pathlib import Path

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.utils.app_path import resolve_app_base_path


def test_resolve_app_base_path_uses_repo_root_in_source_mode(monkeypatch: pytest.MonkeyPatch) -> None:
    """源码模式下应固定到仓库根目录。"""

    monkeypatch.delattr(sys, "frozen", raising=False)
    assert resolve_app_base_path() == Path(__file__).resolve().parents[2]

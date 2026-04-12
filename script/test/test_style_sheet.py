# -*- coding: utf-8 -*-

# 第三方库导入
import pytest

# 项目内模块导入
import src.desktop.ui.common.style_sheet as style_sheet_module
from src.desktop.ui.common.style_sheet import PageStyleSheet, WidgetStyleSheet
from qfluentwidgets import Theme


def test_page_style_sheet_falls_back_to_shared_when_themed_resource_missing(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """页面样式在缺少主题资源时应回退到 shared 资源。"""

    def fake_exists(path: str) -> bool:
        return path == ":style/style/shared/page/home.qss"

    monkeypatch.setattr(style_sheet_module.QFile, "exists", fake_exists)

    assert PageStyleSheet.HOME.path(Theme.DARK) == ":style/style/shared/page/home.qss"


def test_widget_style_sheet_prefers_themed_resource(monkeypatch: pytest.MonkeyPatch) -> None:
    """控件样式存在主题资源时应优先使用主题资源。"""

    def fake_exists(path: str) -> bool:
        return path == ":style/style/dark/widget/code_editor.qss"

    monkeypatch.setattr(style_sheet_module.QFile, "exists", fake_exists)

    assert WidgetStyleSheet.CODE_EDITOR.path(Theme.DARK) == ":style/style/dark/widget/code_editor.qss"


def test_widget_style_sheet_falls_back_to_shared_when_themed_resource_missing(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """控件样式在缺少主题资源时应回退到 shared 资源。"""

    def fake_exists(path: str) -> bool:
        return path == ":style/style/shared/widget/code_editor.qss"

    monkeypatch.setattr(style_sheet_module.QFile, "exists", fake_exists)

    assert WidgetStyleSheet.CODE_EDITOR.path(Theme.DARK) == ":style/style/shared/widget/code_editor.qss"

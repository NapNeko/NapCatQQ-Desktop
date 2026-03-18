# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum

# 第三方库导入
from qfluentwidgets import StyleSheetBase, Theme, qconfig
from PySide6.QtCore import QFile
from src.resource import resource as _resource  # noqa: F401


def _resolve_theme(theme: Theme) -> Theme:
    """解析样式表实际使用的主题。"""
    return qconfig.theme if theme == Theme.AUTO else theme


class _ResourceStyleSheet(StyleSheetBase):
    """Qt 资源样式表基类。"""

    def _build_path(self, resource_group: str, style_name: str, theme: Theme = Theme.AUTO) -> str:
        resolved_theme = _resolve_theme(theme)
        return f":style/style/{resolved_theme.value.lower()}/{resource_group}/{style_name}.qss"

    def _build_shared_path(self, resource_group: str, style_name: str) -> str:
        return f":style/style/shared/{resource_group}/{style_name}.qss"

    def _resolve_path(self, resource_group: str, style_name: str, theme: Theme = Theme.AUTO) -> str:
        themed_path = self._build_path(resource_group, style_name, theme)
        shared_path = self._build_shared_path(resource_group, style_name)
        return themed_path if QFile.exists(themed_path) else shared_path


class PageStyleSheet(_ResourceStyleSheet, Enum):
    """页面样式表"""

    HOME = "home"
    # BOT = "bot"
    SETUP = "setup"
    UNIT = "unit"

    def path(self, theme: Theme = Theme.AUTO) -> str:
        return self._resolve_path("page", self.value, theme)


class WidgetStyleSheet(_ResourceStyleSheet, Enum):
    """控件样式表"""

    UPDATE_LOG_CARD = "update_log_card"
    CODE_EDITOR = "code_editor"

    def path(self, theme: Theme = Theme.AUTO) -> str:
        return self._resolve_path("widget", self.value, theme)

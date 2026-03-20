# -*- coding: utf-8 -*-
"""项目内统一使用的不透明卡片基类。"""

from __future__ import annotations

from types import MethodType

from PySide6.QtCore import QRectF, Qt
from PySide6.QtGui import QColor, QPainter, QPainterPath
from qfluentwidgets import CustomColorSettingCard, OptionsSettingCard, RangeSettingCard
from qfluentwidgets.common.style_sheet import isDarkTheme
from qfluentwidgets.components.settings import ExpandGroupSettingCard, ExpandSettingCard, SettingCard
from qfluentwidgets.components.widgets.card_widget import CardWidget, HeaderCardWidget, SimpleCardWidget


def _card_background_color() -> QColor:
    return QColor(36, 39, 46) if isDarkTheme() else QColor(255, 255, 255)


def _card_hover_background_color() -> QColor:
    return QColor(44, 48, 56) if isDarkTheme() else QColor(248, 250, 252)


def _card_pressed_background_color() -> QColor:
    return QColor(30, 34, 40) if isDarkTheme() else QColor(241, 245, 249)


def _card_border_color() -> QColor:
    return QColor(255, 255, 255, 18) if isDarkTheme() else QColor(15, 23, 42, 18)


def _opaque_setting_card_paint_event(self, event) -> None:
    painter = QPainter(self)
    painter.setRenderHints(QPainter.RenderHint.Antialiasing)
    painter.setBrush(_card_background_color())
    painter.setPen(_card_border_color())
    painter.drawRoundedRect(self.rect().adjusted(1, 1, -1, -1), 6, 6)


def _opaque_header_setting_card_paint_event(self, event) -> None:
    painter = QPainter(self)
    painter.setRenderHints(QPainter.RenderHint.Antialiasing)
    painter.setPen(_card_border_color())
    painter.setBrush(_card_background_color())

    parent = self.parent()
    path = QPainterPath()
    path.setFillRule(Qt.FillRule.WindingFill)
    path.addRoundedRect(QRectF(self.rect().adjusted(1, 1, -1, -1)), 6, 6)

    if hasattr(parent, "isExpand") and parent.isExpand:
        path.addRect(1, self.height() - 8, self.width() - 2, 8)

    painter.drawPath(path.simplified())


def _opaque_expand_border_paint_event(self, event) -> None:
    painter = QPainter(self)
    painter.setRenderHints(QPainter.RenderHint.Antialiasing)
    painter.setBrush(Qt.BrushStyle.NoBrush)
    painter.setPen(_card_border_color())
    painter.drawRoundedRect(self.rect().adjusted(1, 1, -1, -1), 6, 6)

    parent = self.parent()
    card_height = parent.card.height()
    height = self.height()
    width = self.width()

    if card_height < height:
        painter.drawLine(1, card_height, width - 1, card_height)


def _attach_opaque_expand_surface(card: ExpandSettingCard) -> None:
    card.card.paintEvent = MethodType(_opaque_header_setting_card_paint_event, card.card)
    card.borderWidget.paintEvent = MethodType(_opaque_expand_border_paint_event, card.borderWidget)
    card.card.update()
    card.borderWidget.update()


class OpaqueCardWidget(CardWidget):
    """不透明的 CardWidget。"""

    def _normalBackgroundColor(self):
        return _card_background_color()

    def _hoverBackgroundColor(self):
        return _card_hover_background_color()

    def _pressedBackgroundColor(self):
        return _card_pressed_background_color()


class OpaqueSimpleCardWidget(SimpleCardWidget):
    """不透明的 SimpleCardWidget。"""

    def _normalBackgroundColor(self):
        return _card_background_color()

    def _hoverBackgroundColor(self):
        return _card_background_color()

    def _pressedBackgroundColor(self):
        return _card_background_color()


class OpaqueHeaderCardWidget(HeaderCardWidget):
    """不透明的 HeaderCardWidget。"""

    def _normalBackgroundColor(self):
        return _card_background_color()

    def _hoverBackgroundColor(self):
        return _card_background_color()

    def _pressedBackgroundColor(self):
        return _card_background_color()


class OpaqueSettingCard(SettingCard):
    """不透明的 SettingCard。"""

    def paintEvent(self, event) -> None:
        _opaque_setting_card_paint_event(self, event)


class OpaqueOptionsSettingCard(OptionsSettingCard):
    """不透明的 OptionsSettingCard。"""

    def paintEvent(self, event) -> None:
        _opaque_setting_card_paint_event(self, event)


class OpaqueRangeSettingCard(RangeSettingCard):
    """不透明的 RangeSettingCard。"""

    def paintEvent(self, event) -> None:
        _opaque_setting_card_paint_event(self, event)


class OpaqueExpandSettingCard(ExpandSettingCard):
    """不透明的 ExpandSettingCard。"""

    def __init__(self, icon, title: str, content: str | None = None, parent=None):
        super().__init__(icon, title, content, parent)
        _attach_opaque_expand_surface(self)


class OpaqueExpandGroupSettingCard(ExpandGroupSettingCard):
    """不透明的 ExpandGroupSettingCard。"""

    def __init__(self, icon, title: str, content: str | None = None, parent=None):
        super().__init__(icon, title, content, parent)
        _attach_opaque_expand_surface(self)


class OpaqueCustomColorSettingCard(CustomColorSettingCard):
    """不透明的 CustomColorSettingCard。"""

    def __init__(self, configItem, icon, title: str, content=None, parent=None, enableAlpha=False):
        super().__init__(configItem, icon, title, content, parent, enableAlpha)
        _attach_opaque_expand_surface(self)

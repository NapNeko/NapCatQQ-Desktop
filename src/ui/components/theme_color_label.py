# -*- coding: utf-8 -*-
from __future__ import annotations

from enum import Enum
from typing import Protocol, cast

from PySide6.QtGui import QColor
from PySide6.QtWidgets import QWidget
from qfluentwidgets.common.overload import singledispatchmethod
from qfluentwidgets.components.widgets.label import (
    BodyLabel,
    CaptionLabel,
    StrongBodyLabel,
    SubtitleLabel,
    TitleLabel,
)

from src.core.config import cfg


def _mix_color(source: QColor, target: QColor | str, ratio: float) -> QColor:
    """按比例混合两种颜色。"""
    source = QColor(source)
    target = QColor(target)
    ratio = max(0.0, min(1.0, ratio))
    return QColor(
        round(source.red() * (1 - ratio) + target.red() * ratio),
        round(source.green() * (1 - ratio) + target.green() * ratio),
        round(source.blue() * (1 - ratio) + target.blue() * ratio),
        round(source.alpha() * (1 - ratio) + target.alpha() * ratio),
    )


class ThemeColorTone(str, Enum):
    """文本色调强度。"""

    STRONG = "strong"
    PRIMARY = "primary"
    SOFT = "soft"


class _ThemeColorLabelProtocol(Protocol):
    def setTextColor(self, light=QColor(0, 0, 0), dark=QColor(255, 255, 255)) -> None: ...


class _ThemeColorLabelMixin:
    """让标签文本颜色跟随主题色变化。"""

    _tone_light_mix = {
        ThemeColorTone.STRONG: 0.30,
        ThemeColorTone.PRIMARY: 0.18,
        ThemeColorTone.SOFT: 0.08,
    }
    _tone_dark_mix = {
        ThemeColorTone.STRONG: 0.42,
        ThemeColorTone.PRIMARY: 0.32,
        ThemeColorTone.SOFT: 0.18,
    }

    def _init_theme_color_label(self, tone: ThemeColorTone = ThemeColorTone.PRIMARY) -> None:
        self._theme_color_tone = ThemeColorTone(tone)
        self.refresh_theme_color()
        cfg.themeChanged.connect(self.refresh_theme_color)
        cfg.themeColorChanged.connect(self.refresh_theme_color)

    def set_theme_color_tone(self, tone: ThemeColorTone) -> None:
        self._theme_color_tone = ThemeColorTone(tone)
        self.refresh_theme_color()

    def theme_color_tone(self) -> ThemeColorTone:
        return self._theme_color_tone

    def refresh_theme_color(self, *_args) -> None:
        base = QColor(cfg.get(cfg.themeColor))
        tone = self._theme_color_tone

        light_color = _mix_color(base, "#000000", self._tone_light_mix[tone])
        dark_color = _mix_color(base, "#ffffff", self._tone_dark_mix[tone])
        cast(_ThemeColorLabelProtocol, self).setTextColor(light_color, dark_color)


class ThemeColorTitleLabel(TitleLabel, _ThemeColorLabelMixin):
    """跟随主题色变化的 TitleLabel。"""

    @singledispatchmethod
    def __init__(self, parent: QWidget | None = None, tone: ThemeColorTone = ThemeColorTone.STRONG):
        super().__init__(parent)
        self._init_theme_color_label(tone)

    @__init__.register
    def _(self, text: str, parent: QWidget | None = None, tone: ThemeColorTone = ThemeColorTone.STRONG):
        self.__init__(parent, tone)
        self.setText(text)


class ThemeColorSubtitleLabel(SubtitleLabel, _ThemeColorLabelMixin):
    """跟随主题色变化的 SubtitleLabel。"""

    @singledispatchmethod
    def __init__(self, parent: QWidget | None = None, tone: ThemeColorTone = ThemeColorTone.PRIMARY):
        super().__init__(parent)
        self._init_theme_color_label(tone)

    @__init__.register
    def _(self, text: str, parent: QWidget | None = None, tone: ThemeColorTone = ThemeColorTone.PRIMARY):
        self.__init__(parent, tone)
        self.setText(text)


class ThemeColorBodyLabel(BodyLabel, _ThemeColorLabelMixin):
    """跟随主题色变化的 BodyLabel。"""

    @singledispatchmethod
    def __init__(self, parent: QWidget | None = None, tone: ThemeColorTone = ThemeColorTone.PRIMARY):
        super().__init__(parent)
        self._init_theme_color_label(tone)

    @__init__.register
    def _(self, text: str, parent: QWidget | None = None, tone: ThemeColorTone = ThemeColorTone.PRIMARY):
        self.__init__(parent, tone)
        self.setText(text)


class ThemeColorStrongBodyLabel(StrongBodyLabel, _ThemeColorLabelMixin):
    """跟随主题色变化的 StrongBodyLabel。"""

    @singledispatchmethod
    def __init__(self, parent: QWidget | None = None, tone: ThemeColorTone = ThemeColorTone.STRONG):
        super().__init__(parent)
        self._init_theme_color_label(tone)

    @__init__.register
    def _(self, text: str, parent: QWidget | None = None, tone: ThemeColorTone = ThemeColorTone.STRONG):
        self.__init__(parent, tone)
        self.setText(text)


class ThemeColorCaptionLabel(CaptionLabel, _ThemeColorLabelMixin):
    """跟随主题色变化的 CaptionLabel。"""

    @singledispatchmethod
    def __init__(self, parent: QWidget | None = None, tone: ThemeColorTone = ThemeColorTone.SOFT):
        super().__init__(parent)
        self._init_theme_color_label(tone)

    @__init__.register
    def _(self, text: str, parent: QWidget | None = None, tone: ThemeColorTone = ThemeColorTone.SOFT):
        self.__init__(parent, tone)
        self.setText(text)


ThemeColorLabel = ThemeColorBodyLabel

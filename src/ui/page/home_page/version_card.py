# -*- coding: utf-8 -*-
from __future__ import annotations

from dataclasses import dataclass

from PySide6.QtCore import QRectF, Qt
from PySide6.QtGui import QColor, QPainter, QPen
from PySide6.QtWidgets import QBoxLayout, QHBoxLayout, QSizePolicy, QVBoxLayout, QWidget
from qfluentwidgets import BodyLabel, CaptionLabel, SimpleCardWidget, isDarkTheme
from qfluentwidgets.common.icon import drawIcon

from src.core.home import HomeVersionService
from src.ui.common.icon import NapCatDesktopIcon, StaticIcon


@dataclass(slots=True)
class VersionCardData:
    version: str
    icon: object


class _VersionIconBadge(QWidget):
    def __init__(self, icon, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._icon = icon
        self.setFixedSize(48, 48)

    def paintEvent(self, event) -> None:
        painter = QPainter(self)
        painter.setRenderHints(QPainter.RenderHint.Antialiasing | QPainter.RenderHint.SmoothPixmapTransform)

        if isDarkTheme():
            background = QColor(255, 255, 255, 18)
            border = QColor(255, 255, 255, 30)
        else:
            background = QColor(15, 23, 42, 10)
            border = QColor(15, 23, 42, 18)

        painter.setPen(QPen(border, 1))
        painter.setBrush(background)
        painter.drawRoundedRect(self.rect().adjusted(1, 1, -1, -1), 4, 4)

        icon_rect = QRectF(0, 0, 32, 32)
        icon_rect.moveCenter(QRectF(self.rect()).center())
        drawIcon(self._icon, painter, icon_rect)


class VersionShowcaseCard(SimpleCardWidget):
    def __init__(self, data: VersionCardData, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.icon_widget = _VersionIconBadge(data.icon, self)
        self.meta_label = CaptionLabel("Version", self)
        self.version_label = BodyLabel(data.version, self)
        self.version_label.setWordWrap(True)

        self._set_layout()

    def _set_layout(self) -> None:
        self.text_layout = QVBoxLayout()
        self.text_layout.setContentsMargins(0, 2, 0, 2)
        self.text_layout.setSpacing(2)
        self.text_layout.addWidget(self.meta_label)
        self.text_layout.addWidget(self.version_label)

        self.h_box_layout = QHBoxLayout(self)
        self.h_box_layout.setContentsMargins(18, 16, 18, 16)
        self.h_box_layout.setSpacing(14)
        self.h_box_layout.addWidget(self.icon_widget, 0, Qt.AlignmentFlag.AlignVCenter)
        self.h_box_layout.addLayout(self.text_layout, 1)

        self.setMinimumHeight(64)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)

    def setVersion(self, version: str) -> None:
        self.version_label.setText(version)


class VersionCardsPanel(QWidget):
    """首页版本信息卡片组。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._version_service = HomeVersionService()
        self._create_cards()
        self._set_layout()

    def _create_cards(self) -> None:
        self.napcat_card = VersionShowcaseCard(self._create_napcat_data(), self)
        self.qq_card = VersionShowcaseCard(self._create_qq_data(), self)

    def _set_layout(self) -> None:
        self.box_layout = QBoxLayout(QBoxLayout.Direction.LeftToRight, self)
        self.box_layout.setContentsMargins(0, 0, 0, 0)
        self.box_layout.setSpacing(12)
        self.box_layout.addWidget(self.napcat_card)
        self.box_layout.addWidget(self.qq_card)

        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)

    def resizeEvent(self, event) -> None:
        super().resizeEvent(event)
        direction = QBoxLayout.Direction.TopToBottom if self.width() < 420 else QBoxLayout.Direction.LeftToRight
        if self.box_layout.direction() != direction:
            self.box_layout.setDirection(direction)

    def refresh_versions(self) -> None:
        summary = self._version_service.get_summary()
        self.napcat_card.setVersion(self._format_version("NapCat", summary.napcat_version))
        self.qq_card.setVersion(self._format_version("QQ", summary.qq_version))

    def _create_napcat_data(self) -> VersionCardData:
        summary = self._version_service.get_summary()
        return VersionCardData(
            version=self._format_version("NapCat", summary.napcat_version),
            icon=StaticIcon.LOGO,
        )

    def _create_qq_data(self) -> VersionCardData:
        summary = self._version_service.get_summary()
        return VersionCardData(
            version=self._format_version("QQ", summary.qq_version),
            icon=NapCatDesktopIcon.QQ,
        )

    @staticmethod
    def _format_version(name: str, version: str | None) -> str:
        if not version:
            return f"{name} 未安装"
        return version

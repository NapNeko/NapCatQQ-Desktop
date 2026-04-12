# -*- coding: utf-8 -*-
from __future__ import annotations

from PySide6.QtCore import QSize, Qt, Signal
from PySide6.QtGui import QColor, QFont, QGuiApplication, QPainter, QPen
from PySide6.QtWidgets import QFrame, QHBoxLayout, QSizePolicy, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    FluentIcon,
    ScrollArea,
    StrongBodyLabel,
    ToolTipFilter,
    TransparentToolButton,
    isDarkTheme,
)
from qfluentwidgets.components.widgets.tool_tip import ToolTipPosition
from qfluentwidgets.common.smooth_scroll import SmoothMode

from src.desktop.core.config import cfg
from src.desktop.core.home.notice_model import (
    NoticeDismissMode,
    NoticeTimelineItemData,
    NoticeTimelineSectionData,
    NoticeTimelineStatus,
)


def _status_color(status: NoticeTimelineStatus) -> QColor:
    colors = {
        NoticeTimelineStatus.SUCCESS: QColor("#1f8f3a"),
        NoticeTimelineStatus.INFO: QColor("#1f9cb5"),
        NoticeTimelineStatus.WARNING: QColor("#a86817"),
        NoticeTimelineStatus.ERROR: QColor("#c53b30"),
    }
    return colors[status]


def _status_glyph(status: NoticeTimelineStatus) -> str:
    glyphs = {
        NoticeTimelineStatus.SUCCESS: "✓",
        NoticeTimelineStatus.INFO: "i",
        NoticeTimelineStatus.WARNING: "!",
        NoticeTimelineStatus.ERROR: "×",
    }
    return glyphs[status]


def _set_font_pixel_size(font: QFont, pixel_size: int) -> None:
    app = QGuiApplication.instance()
    screen = app.primaryScreen() if app is not None else None
    dpi = screen.logicalDotsPerInchY() if screen is not None else 96.0
    font.setPointSizeF(pixel_size * 72 / (dpi or 96.0))


class _StatusBadge(QWidget):
    def __init__(self, status: NoticeTimelineStatus, diameter: int = 20, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._status = status
        self._diameter = diameter
        self.setFixedSize(diameter, diameter)

    def sizeHint(self) -> QSize:
        return QSize(self._diameter, self._diameter)

    def set_status(self, status: NoticeTimelineStatus) -> None:
        self._status = status
        self.update()

    def paintEvent(self, event) -> None:
        painter = QPainter(self)
        painter.setRenderHints(QPainter.RenderHint.Antialiasing | QPainter.RenderHint.TextAntialiasing)

        color = _status_color(self._status)
        rect = self.rect().adjusted(1, 1, -1, -1)

        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(color)
        painter.drawEllipse(rect)

        font = QFont(self.font())
        font.setBold(True)
        _set_font_pixel_size(font, max(11, self._diameter - 9))
        painter.setFont(font)
        painter.setPen(QColor("#ffffff"))
        painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, _status_glyph(self._status))


class _TimelineRail(QWidget):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setFixedWidth(20)
        self.setMinimumHeight(8)
        self.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Expanding)
        cfg.themeChanged.connect(self.update)

    def paintEvent(self, event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        color = QColor(255, 255, 255, 78) if isDarkTheme() else QColor(15, 23, 42, 88)
        painter.setPen(QPen(color, 1.2))

        center_x = self.width() // 2
        painter.drawLine(center_x, 2, center_x, max(2, self.height() - 2))


class NoticeTimelineCard(QWidget):
    dismissed = Signal(object)

    def __init__(self, item: NoticeTimelineItemData, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._item = item

        self.icon_widget = _StatusBadge(item.status, diameter=18, parent=self)
        self.text_label = BodyLabel(item.text, self)
        self.text_label.setWordWrap(True)
        self.dismiss_button = TransparentToolButton(FluentIcon.CLOSE, self)
        self.dismiss_button.setVisible(item.dismiss_mode != NoticeDismissMode.NONE)
        self.dismiss_button.setToolTip(self._dismiss_tooltip(item.dismiss_mode))
        self.dismiss_button.setToolTipDuration(1500)
        self.dismiss_button.installEventFilter(
            ToolTipFilter(self.dismiss_button, showDelay=300, position=ToolTipPosition.TOP_RIGHT)
        )
        self.dismiss_button.clicked.connect(lambda: self.dismissed.emit(self._item))

        self.h_box_layout = QHBoxLayout(self)
        self.h_box_layout.setContentsMargins(18, 14, 18, 14)
        self.h_box_layout.setSpacing(12)
        self.h_box_layout.addWidget(self.icon_widget, 0, Qt.AlignmentFlag.AlignVCenter)
        self.h_box_layout.addWidget(self.text_label, 1)
        self.h_box_layout.addWidget(self.dismiss_button, 0, Qt.AlignmentFlag.AlignTop)

        self.setMinimumHeight(56)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)

        cfg.themeChanged.connect(self.update)

    @staticmethod
    def _dismiss_tooltip(mode: NoticeDismissMode) -> str:
        if mode == NoticeDismissMode.SNOOZE:
            return "稍后提醒"
        if mode == NoticeDismissMode.PERSISTENT:
            return "忽略此通知"
        if mode == NoticeDismissMode.SESSION:
            return "关闭本次提醒"
        return ""

    def paintEvent(self, event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        if isDarkTheme():
            background = QColor(30, 34, 40, 196)
            border = QColor(255, 255, 255, 24)
        else:
            background = QColor(255, 255, 255, 228)
            border = QColor(15, 23, 42, 18)

        painter.setBrush(background)
        painter.setPen(QPen(border, 1))
        painter.drawRoundedRect(self.rect().adjusted(1, 1, -1, -1), 8, 8)


class NoticeTimelineSection(QWidget):
    itemDismissed = Signal(object)

    def __init__(self, section: NoticeTimelineSectionData, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._section = section

        self.header_icon = _StatusBadge(section.status, diameter=20, parent=self)
        self.title_label = StrongBodyLabel(section.title, self)

        self.header_layout = QHBoxLayout()
        self.header_layout.setContentsMargins(0, 0, 0, 0)
        self.header_layout.setSpacing(12)
        self.header_layout.addWidget(self.header_icon, 0, Qt.AlignmentFlag.AlignVCenter)
        self.header_layout.addWidget(self.title_label, 1, Qt.AlignmentFlag.AlignVCenter)

        self.timeline_rail = _TimelineRail(self)
        self.items_container = QWidget(self)
        self.items_layout = QVBoxLayout(self.items_container)
        self.items_layout.setContentsMargins(0, 0, 0, 0)
        self.items_layout.setSpacing(12)

        for item in section.items:
            card = NoticeTimelineCard(item, self.items_container)
            card.dismissed.connect(self.itemDismissed.emit)
            self.items_layout.addWidget(card)

        self.body_layout = QHBoxLayout()
        self.body_layout.setContentsMargins(0, 0, 0, 0)
        self.body_layout.setSpacing(12)
        self.body_layout.addWidget(self.timeline_rail)
        self.body_layout.addWidget(self.items_container, 1)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(10)
        self.v_box_layout.addLayout(self.header_layout)
        self.v_box_layout.addLayout(self.body_layout)

        if not section.items:
            self.timeline_rail.hide()
            self.items_container.hide()


class NoticeTimelineWidget(ScrollArea):
    itemDismissed = Signal(object)

    def __init__(
        self,
        sections: list[NoticeTimelineSectionData] | None = None,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._sections: list[NoticeTimelineSectionData] = []
        self.view = QWidget(self)

        self.v_box_layout = QVBoxLayout(self.view)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(18)

        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setAlignment(Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignLeft)
        self.setFrameShape(QFrame.Shape.NoFrame)
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setSmoothMode(SmoothMode.LINEAR, Qt.Orientation.Vertical)
        self.enableTransparentBackground()
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        if sections:
            self.set_sections(sections)

    def set_sections(self, sections: list[NoticeTimelineSectionData]) -> None:
        self._sections = list(sections)
        self._rebuild()

    def add_section(self, section: NoticeTimelineSectionData) -> None:
        self._sections.append(section)
        self._rebuild()

    def clear_sections(self) -> None:
        self.set_sections([])

    def _rebuild(self) -> None:
        while self.v_box_layout.count():
            item = self.v_box_layout.takeAt(0)
            if item is None:
                continue

            widget = item.widget()
            if widget is not None:
                widget.deleteLater()

        for section in self._sections:
            section_widget = NoticeTimelineSection(section, self)
            section_widget.itemDismissed.connect(self.itemDismissed.emit)
            self.v_box_layout.addWidget(section_widget)

        self.v_box_layout.addStretch(1)

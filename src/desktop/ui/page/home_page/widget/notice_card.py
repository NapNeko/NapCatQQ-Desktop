# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QSizePolicy, QVBoxLayout

from qfluentwidgets import SimpleCardWidget

from src.desktop.core.home import HomeNoticeService
from src.desktop.ui.components.notice_timeline import (
    NoticeTimelineWidget,
)


class NoticeCard(SimpleCardWidget):
    """首页通知卡片。"""

    def __init__(self) -> None:
        super().__init__()
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
        self._create_widgets()
        self._connect_signals()
        self._set_layout()
        self.notice_service.refresh()

    def _create_widgets(self) -> None:
        self.notice_timeline = NoticeTimelineWidget(parent=self)
        self.notice_service = HomeNoticeService(self)

    def _connect_signals(self) -> None:
        self.notice_service.sectionsChanged.connect(self.notice_timeline.set_sections)
        self.notice_timeline.itemDismissed.connect(self.notice_service.dismiss_notice)

    def _set_layout(self) -> None:
        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(24, 22, 24, 22)
        self.v_box_layout.addWidget(self.notice_timeline, 1)

    def showEvent(self, event) -> None:
        super().showEvent(event)
        self.notice_service.refresh()

# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QSizePolicy, QVBoxLayout

from qfluentwidgets import SimpleCardWidget

from src.ui.components.notice_timeline import (
    NoticeTimelineItemData,
    NoticeTimelineSectionData,
    NoticeTimelineStatus,
    NoticeTimelineWidget,
)


class NoticeCard(SimpleCardWidget):
    """首页通知卡片。"""

    def __init__(self) -> None:
        super().__init__()
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
        self._create_widgets()
        self._set_layout()

    def _create_widgets(self) -> None:
        self.notice_timeline = NoticeTimelineWidget(
            self._default_sections(),
            self,
        )

    def _set_layout(self) -> None:
        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(24, 22, 24, 22)
        self.v_box_layout.setAlignment(Qt.AlignmentFlag.AlignTop)
        self.v_box_layout.addWidget(self.notice_timeline)
        self.v_box_layout.addStretch(1)

    @staticmethod
    def _default_sections() -> list[NoticeTimelineSectionData]:
        return [
            NoticeTimelineSectionData(
                title="提醒",
                status=NoticeTimelineStatus.INFO,
                items=[
                    NoticeTimelineItemData(
                        text="这里可以展示更新提醒、Bot 异常和待办事项。",
                        status=NoticeTimelineStatus.INFO,
                    )
                ],
            ),
            NoticeTimelineSectionData(
                title="公告",
                status=NoticeTimelineStatus.SUCCESS,
                items=[
                    NoticeTimelineItemData(
                        text="这里可以展示一些公告，比如新版本发布了。",
                        status=NoticeTimelineStatus.SUCCESS,
                    )
                ],
            ),
        ]

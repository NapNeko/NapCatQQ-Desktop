# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QVBoxLayout

from qfluentwidgets import SimpleCardWidget

from src.ui.components.notice_timeline import (
    NoticeTimelineItemData,
    NoticeTimelineSectionData,
    NoticeTimelineStatus,
    NoticeTimelineWidget,
)
from src.ui.components.theme_color_label import ThemeColorSubtitleLabel, ThemeColorTone


class NoticeCard(SimpleCardWidget):
    """首页通知卡片。"""

    def __init__(self) -> None:
        super().__init__()
        self.setMaximumWidth(760)

        self.notice_timeline = NoticeTimelineWidget(
            [
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
            ],
            self,
        )
        self.notice_timeline.setFixedHeight(248)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(24, 22, 24, 22)
        self.v_box_layout.setAlignment(Qt.AlignmentFlag.AlignTop)
        self.v_box_layout.addWidget(self.notice_timeline)
        self.v_box_layout.addStretch(1)

# -*- coding: utf-8 -*-
from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum


class NoticeTimelineStatus(str, Enum):
    SUCCESS = "success"
    INFO = "info"
    WARNING = "warning"
    ERROR = "error"


class NoticeDismissMode(str, Enum):
    NONE = "none"
    SESSION = "session"
    PERSISTENT = "persistent"
    SNOOZE = "snooze"


@dataclass(slots=True)
class NoticeTimelineItemData:
    key: str
    text: str
    status: NoticeTimelineStatus = NoticeTimelineStatus.INFO
    dismiss_mode: NoticeDismissMode = NoticeDismissMode.NONE


@dataclass(slots=True)
class NoticeTimelineSectionData:
    title: str
    status: NoticeTimelineStatus = NoticeTimelineStatus.INFO
    items: list[NoticeTimelineItemData] = field(default_factory=list)

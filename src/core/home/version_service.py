# -*- coding: utf-8 -*-
from __future__ import annotations

from dataclasses import dataclass

from PySide6.QtCore import QObject, Signal

from src.core.versioning import LocalVersionTask


@dataclass(slots=True)
class VersionSummary:
    napcat_version: str | None
    qq_version: str | None
    desktop_version: str | None


class HomeVersionService:
    """首页版本信息服务。"""

    def __init__(self) -> None:
        self._local_version = LocalVersionTask()

    def get_summary(self) -> VersionSummary:
        return VersionSummary(
            napcat_version=self._local_version.get_napcat_version(),
            qq_version=self._local_version.get_qq_version(),
            desktop_version=self._local_version.get_ncd_version(),
        )


class HomeVersionRefreshBus(QObject):
    """主页版本卡片刷新事件总线。"""

    refresh_requested = Signal()

    def request_refresh(self) -> None:
        """通知主页版本卡片重新读取本地版本。"""
        self.refresh_requested.emit()


home_version_refresh_bus = HomeVersionRefreshBus()

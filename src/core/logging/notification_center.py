# -*- coding: utf-8 -*-
"""崩溃诊断包通知中心。"""

from dataclasses import dataclass
from pathlib import Path

from PySide6.QtCore import QObject, Signal


@dataclass(frozen=True, slots=True)
class CrashBundleNotification:
    """崩溃诊断包生成后的通知载荷。"""

    bundle_path: Path
    trigger: str
    output_source: str


class CrashBundleNotificationCenter(QObject):
    """向 UI 广播崩溃诊断包生成事件。"""

    crash_bundle_created = Signal(object)

    def __init__(self) -> None:
        super().__init__()
        self._pending_notifications: list[CrashBundleNotification] = []

    def publish(self, notification: CrashBundleNotification) -> None:
        """发布一条崩溃包生成通知。"""
        self._pending_notifications.append(notification)
        self.crash_bundle_created.emit(notification)

    def consume_pending(self) -> list[CrashBundleNotification]:
        """取出当前尚未被 UI 消费的通知。"""
        pending_notifications = list(self._pending_notifications)
        self._pending_notifications.clear()
        return pending_notifications


crash_bundle_notification_center = CrashBundleNotificationCenter()

# -*- coding: utf-8 -*-
"""文本类控件使用的轻量平滑滚动支持。"""

from PySide6.QtCore import QEasingCurve, QPropertyAnimation
from PySide6.QtGui import QWheelEvent


class SmoothTextScrollMixin:
    """为文本滚动区域提供兼容的滚轮平滑滚动。"""

    def _init_smooth_scroll(self, duration: int = 150) -> None:
        self._pending_scroll_value = self.verticalScrollBar().value()
        self._scroll_animation = QPropertyAnimation(self.verticalScrollBar(), b"value", self)
        self._scroll_animation.setDuration(duration)
        self._scroll_animation.setEasingCurve(QEasingCurve.Type.OutQuad)
        self.verticalScrollBar().rangeChanged.connect(self._sync_pending_scroll_value)
        self.verticalScrollBar().valueChanged.connect(self._sync_pending_scroll_value_from_value)

    def _sync_pending_scroll_value(self, minimum: int, maximum: int) -> None:
        self._pending_scroll_value = max(minimum, min(maximum, self._pending_scroll_value))

    def _sync_pending_scroll_value_from_value(self, value: int) -> None:
        self._pending_scroll_value = value

    def _handle_smooth_wheel_event(self, event: QWheelEvent) -> bool:
        scroll_bar = self.verticalScrollBar()
        if scroll_bar.maximum() <= scroll_bar.minimum():
            return False

        step = self._resolve_scroll_step(event)
        if step == 0:
            return False

        current_value = scroll_bar.value()
        target_value = max(scroll_bar.minimum(), min(scroll_bar.maximum(), current_value + step))
        if target_value == scroll_bar.value() and self._scroll_animation.state() != QPropertyAnimation.State.Running:
            return False

        self._pending_scroll_value = target_value
        self._scroll_animation.stop()
        self._scroll_animation.setStartValue(scroll_bar.value())
        self._scroll_animation.setEndValue(target_value)
        self._scroll_animation.start()
        event.accept()
        return True

    def _resolve_scroll_step(self, event: QWheelEvent) -> int:
        pixel_delta = event.pixelDelta().y()
        if pixel_delta:
            return -pixel_delta

        angle_delta = event.angleDelta().y()
        if angle_delta == 0:
            return 0

        base_step = max(28, self.fontMetrics().height() * 2)
        return int(-(angle_delta / 120) * base_step)

# -*- coding: utf-8 -*-
from PySide6.QtCore import Property, QEasingCurve, QParallelAnimationGroup, QPropertyAnimation, Qt, Signal
from PySide6.QtGui import QPaintEvent, QPainter, QPixmap, QResizeEvent
from PySide6.QtWidgets import QStackedWidget, QWidget


class _SnapshotOverlay(QWidget):
    """用于播放截图淡出动画的轻量遮罩层。"""

    def __init__(self, parent: QWidget):
        super().__init__(parent)
        self._pixmap = QPixmap()
        self._opacity = 1.0
        self._offset_x = 0.0
        self._offset_y = 0.0

        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.hide()

    def get_opacity(self) -> float:
        return self._opacity

    def set_opacity(self, value: float) -> None:
        self._opacity = max(0.0, min(float(value), 1.0))
        self.update()

    opacity = Property(float, get_opacity, set_opacity)

    def get_offset_x(self) -> float:
        return self._offset_x

    def set_offset_x(self, value: float) -> None:
        self._offset_x = float(value)
        self.update()

    offset_x = Property(float, get_offset_x, set_offset_x)

    def get_offset_y(self) -> float:
        return self._offset_y

    def set_offset_y(self, value: float) -> None:
        self._offset_y = float(value)
        self.update()

    offset_y = Property(float, get_offset_y, set_offset_y)

    def set_snapshot(self, pixmap: QPixmap) -> None:
        self._pixmap = pixmap
        self._offset_x = 0.0
        self._offset_y = 0.0
        self.update()

    def clear_snapshot(self) -> None:
        self._pixmap = QPixmap()
        self._offset_x = 0.0
        self._offset_y = 0.0
        self.update()

    def paintEvent(self, event: QPaintEvent) -> None:
        if self._pixmap.isNull():
            return

        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform)
        painter.setOpacity(self._opacity)
        painter.drawPixmap(self.rect().translated(int(self._offset_x), int(self._offset_y)), self._pixmap)
        painter.end()


class TransparentStackedWidget(QStackedWidget):
    """带轻量截图淡出动画的透明堆叠控件。"""

    aniStart = Signal()
    aniFinished = Signal()

    def __init__(
        self,
        parent=None,
        *,
        delta_x: int = 0,
        delta_y: int = 24,
        duration: int = 220,
        easing_curve: QEasingCurve.Type = QEasingCurve.Type.OutCubic,
    ):
        super().__init__(parent)
        self._default_duration = duration
        self._default_easing_curve = easing_curve
        self._is_animation_enabled = True
        self._animation_active = False

        # 保留参数兼容，避免现有调用点因签名变化而中断。
        self._default_delta_x = delta_x
        self._default_delta_y = delta_y

        self._snapshot_overlay = _SnapshotOverlay(self)

        self._fade_animation = QPropertyAnimation(self._snapshot_overlay, b"opacity", self)
        self._offset_x_animation = QPropertyAnimation(self._snapshot_overlay, b"offset_x", self)
        self._offset_y_animation = QPropertyAnimation(self._snapshot_overlay, b"offset_y", self)
        self._animation_group = QParallelAnimationGroup(self)
        self._animation_group.addAnimation(self._fade_animation)
        self._animation_group.addAnimation(self._offset_x_animation)
        self._animation_group.addAnimation(self._offset_y_animation)
        self._animation_group.finished.connect(self._finish_snapshot_animation)

        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setStyleSheet("background:transparent;border: none;")

    def addWidget(self, widget: QWidget) -> int:
        """添加页面，重复添加时保持与旧实现兼容。"""
        if (current_index := self.indexOf(widget)) != -1:
            return current_index

        return super().addWidget(widget)

    def isAnimationEnabled(self) -> bool:
        """返回当前是否启用切页动画。"""
        return self._is_animation_enabled

    def setAnimationEnabled(self, is_enabled: bool) -> None:
        """设置是否启用切页动画。"""
        self._is_animation_enabled = bool(is_enabled)
        if not self._is_animation_enabled:
            self._stop_snapshot_animation()

    def setCurrentIndex(
        self,
        index: int,
        needPopOut: bool = False,
        showNextWidgetDirectly: bool = True,
        duration: int | None = None,
        easingCurve: QEasingCurve.Type | None = None,
    ) -> None:
        """切换页面，用上一页截图淡出避免干扰真实控件绘制。"""
        _ = (needPopOut, showNextWidgetDirectly)

        if index < 0 or index >= self.count():
            return

        current_index = self.currentIndex()
        if index == current_index:
            return

        current_widget = self.currentWidget()
        self._stop_snapshot_animation()

        QStackedWidget.setCurrentIndex(self, index)

        if (
            not self._is_animation_enabled
            or current_index == -1
            or self.count() <= 1
            or current_widget is None
            or not self.isVisible()
        ):
            return

        snapshot = current_widget.grab()
        if snapshot.isNull():
            return

        self._snapshot_overlay.set_snapshot(snapshot)
        self._snapshot_overlay.setGeometry(self.rect())
        self._snapshot_overlay.set_opacity(1.0)
        self._snapshot_overlay.set_offset_x(0.0)
        self._snapshot_overlay.set_offset_y(0.0)
        self._snapshot_overlay.show()
        self._snapshot_overlay.raise_()

        duration_ = self._default_duration if duration is None else duration
        easing_curve_ = self._default_easing_curve if easingCurve is None else easingCurve
        drift_x = self._default_delta_x * 0.35
        drift_y = min(abs(self._default_delta_y), 12) * -0.5 if self._default_delta_y else -8.0

        self._fade_animation.setDuration(duration_)
        self._fade_animation.setEasingCurve(easing_curve_)
        self._fade_animation.setStartValue(1.0)
        self._fade_animation.setEndValue(0.0)

        self._offset_x_animation.setDuration(duration_)
        self._offset_x_animation.setEasingCurve(QEasingCurve.Type.OutCubic)
        self._offset_x_animation.setStartValue(0.0)
        self._offset_x_animation.setEndValue(drift_x)

        self._offset_y_animation.setDuration(duration_)
        self._offset_y_animation.setEasingCurve(QEasingCurve.Type.OutCubic)
        self._offset_y_animation.setStartValue(0.0)
        self._offset_y_animation.setEndValue(drift_y)

        self._animation_active = True
        self.aniStart.emit()
        self._animation_group.start()

    def setCurrentWidget(
        self,
        widget: QWidget,
        needPopOut: bool = False,
        showNextWidgetDirectly: bool = True,
        duration: int | None = None,
        easingCurve: QEasingCurve.Type | None = None,
    ) -> None:
        """切换到目标页面。"""
        self.setCurrentIndex(
            self.indexOf(widget),
            needPopOut=needPopOut,
            showNextWidgetDirectly=showNextWidgetDirectly,
            duration=duration,
            easingCurve=easingCurve,
        )

    def resizeEvent(self, event: QResizeEvent) -> None:
        """保持截图遮罩尺寸与容器同步。"""
        super().resizeEvent(event)
        if self._snapshot_overlay.isVisible():
            self._snapshot_overlay.setGeometry(self.rect())

    def _stop_snapshot_animation(self) -> None:
        if self._animation_group.state() == QPropertyAnimation.State.Running:
            self._animation_group.stop()
        self._finish_snapshot_animation()

    def _finish_snapshot_animation(self) -> None:
        if not self._animation_active and not self._snapshot_overlay.isVisible():
            return

        self._animation_active = False
        self._snapshot_overlay.set_opacity(1.0)
        self._snapshot_overlay.set_offset_x(0.0)
        self._snapshot_overlay.set_offset_y(0.0)
        self._snapshot_overlay.hide()
        self._snapshot_overlay.clear_snapshot()
        self.aniFinished.emit()

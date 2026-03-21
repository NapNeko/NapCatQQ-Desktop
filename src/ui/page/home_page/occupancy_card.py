# -*- coding: utf-8 -*-
from __future__ import annotations

from dataclasses import dataclass

import psutil
from PySide6.QtCore import QObject, QEasingCurve, QPointF, QRectF, QSize, Qt, QTimer, Signal, QVariantAnimation
from PySide6.QtGui import QColor, QFont, QLinearGradient, QPainter, QPainterPath, QPen
from PySide6.QtWidgets import QHBoxLayout, QSizePolicy, QVBoxLayout, QWidget
from qfluentwidgets import BodyLabel, FluentIcon, SimpleCardWidget, StrongBodyLabel, isDarkTheme

from src.core.config import cfg


def _clamp_percent(value: float) -> float:
    return max(0.0, min(100.0, float(value)))


def _mix_color(source: QColor | str, target: QColor | str, ratio: float) -> QColor:
    source = QColor(source)
    target = QColor(target)
    ratio = max(0.0, min(1.0, ratio))
    return QColor(
        round(source.red() * (1 - ratio) + target.red() * ratio),
        round(source.green() * (1 - ratio) + target.green() * ratio),
        round(source.blue() * (1 - ratio) + target.blue() * ratio),
        round(source.alpha() * (1 - ratio) + target.alpha() * ratio),
    )


@dataclass(slots=True)
class OccupancySnapshot:
    cpu_percent: float
    memory_percent: float


class _MetricHistory:
    def __init__(self, size: int) -> None:
        self._size = size
        self._values = [0.0] * size

    def reset(self, value: float) -> None:
        normalized = _clamp_percent(value)
        self._values = [normalized] * self._size

    def append(self, value: float) -> None:
        normalized = _clamp_percent(value)
        if not self._values:
            self._values = [normalized] * self._size
            return

        self._values = [*self._values[1:], normalized]

    def values(self) -> list[float]:
        return list(self._values)


class _SystemOccupancySampler(QObject):
    sampleChanged = Signal(float, float)

    def __init__(self, interval_ms: int = 1200, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._initialized = False
        self._timer = QTimer(self)
        self._timer.setInterval(interval_ms)
        self._timer.timeout.connect(self.poll_now)
        psutil.cpu_percent(interval=None)

    def start(self) -> None:
        self._timer.start()
        self.poll_now()

    def poll_now(self) -> None:
        cpu_percent = psutil.cpu_percent(interval=None)
        if cpu_percent == 0 and not self._initialized:
            cpu_percent = psutil.cpu_percent(interval=0.05)

        memory_percent = psutil.virtual_memory().percent
        self._initialized = True
        self.sampleChanged.emit(cpu_percent, memory_percent)


class _ColoredFluentIconWidget(QWidget):
    def __init__(self, icon: FluentIcon, color: QColor | str, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._icon = icon
        self._color = QColor(color)
        self.setFixedSize(18, 18)

    def setColor(self, color: QColor | str) -> None:
        self._color = QColor(color)
        self.update()

    def sizeHint(self) -> QSize:
        return QSize(18, 18)

    def paintEvent(self, event) -> None:
        painter = QPainter(self)
        painter.setRenderHints(QPainter.RenderHint.Antialiasing | QPainter.RenderHint.SmoothPixmapTransform)
        self._icon.render(painter, self.rect(), fill=self._color.name())


class _OccupancyCanvas(QWidget):
    hoverValueChanged = Signal(int, float)
    hoverLeft = Signal()

    def __init__(self, accent_color: QColor | str, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._accent_color = QColor(accent_color)
        self._values: list[float] = [0.0] * 24
        self._animation_source_values: list[float] | None = None
        self._animation_progress = 1.0
        self._hover_index: int | None = None

        self.setMinimumHeight(148)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.setMouseTracking(True)

        self._scroll_animation = QVariantAnimation(self)
        self._scroll_animation.setStartValue(0.0)
        self._scroll_animation.setEndValue(1.0)
        self._scroll_animation.setDuration(860)
        self._scroll_animation.setEasingCurve(QEasingCurve.Type.InOutCubic)
        self._scroll_animation.valueChanged.connect(self._on_animation_value_changed)
        self._scroll_animation.finished.connect(self._on_animation_finished)

        cfg.themeChanged.connect(self.update)

    def setAccentColor(self, color: QColor | str) -> None:
        self._accent_color = QColor(color)
        self.update()

    def setValues(self, values: list[float]) -> None:
        self._scroll_animation.stop()
        self._animation_source_values = None
        self._animation_progress = 1.0
        self._values = [_clamp_percent(value) for value in values] or [0.0]
        if self._hover_index is not None:
            self._hover_index = max(0, min(self._hover_index, len(self._values) - 1))
        self.update()

    def appendValue(self, value: float) -> None:
        normalized = _clamp_percent(value)
        if not self._values:
            self.setValues([normalized])
            return

        self._animation_source_values = self._display_values()
        self._values = [*self._values[1:], normalized] if len(self._values) > 1 else [normalized]
        if self._hover_index is not None:
            self._hover_index = max(0, min(self._hover_index, len(self._values) - 1))

        self._animation_progress = 0.0
        self._scroll_animation.stop()
        self._scroll_animation.start()

    def paintEvent(self, event) -> None:
        values = self._display_values()
        if not values:
            return

        painter = QPainter(self)
        painter.setRenderHints(QPainter.RenderHint.Antialiasing | QPainter.RenderHint.TextAntialiasing)

        chart_rect = self._chart_rect()
        if chart_rect.width() <= 0 or chart_rect.height() <= 0:
            return

        self._draw_grid(painter, chart_rect)
        points = self._build_points(chart_rect, values)
        self._draw_fill(painter, chart_rect, points)
        self._draw_curve(painter, points)
        self._draw_hover_indicator(painter, chart_rect, points, values)

    def mouseMoveEvent(self, event) -> None:
        chart_rect = self._chart_rect()
        if not chart_rect.contains(event.position()):
            self._clear_hover()
            super().mouseMoveEvent(event)
            return

        values = self._display_values()
        if not values:
            return

        if len(values) == 1:
            hover_index = 0
        else:
            step_x = chart_rect.width() / (len(values) - 1)
            hover_index = round((event.position().x() - chart_rect.left()) / step_x)
            hover_index = max(0, min(len(values) - 1, hover_index))

        self._hover_index = hover_index
        self.hoverValueChanged.emit(hover_index, values[hover_index])
        self.update()
        super().mouseMoveEvent(event)

    def leaveEvent(self, event) -> None:
        self._clear_hover()
        super().leaveEvent(event)

    def _draw_grid(self, painter: QPainter, chart_rect: QRectF) -> None:
        grid_color = QColor(255, 255, 255, 34) if isDarkTheme() else QColor(148, 163, 184, 42)
        text_color = QColor("#cbd5e1") if isDarkTheme() else QColor("#94a3b8")
        label_width = 38

        font = QFont(self.font())
        font.setPixelSize(11)
        font.setWeight(QFont.Weight.DemiBold)
        painter.setFont(font)

        for index, value in enumerate((100, 75, 50, 25, 0)):
            y = chart_rect.top() + chart_rect.height() * (index / 4)
            if value != 0:
                painter.setPen(QPen(grid_color, 1))
                painter.drawLine(QPointF(chart_rect.left(), y), QPointF(chart_rect.right(), y))

            painter.setPen(text_color)
            label_rect = QRectF(0, y - 8, label_width - 10, 16)
            painter.drawText(label_rect, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter, f"{value}%")

    def _build_points(self, chart_rect: QRectF, values: list[float]) -> list[QPointF]:
        if len(values) == 1:
            return [QPointF(chart_rect.center().x(), self._value_to_y(values[0], chart_rect))]

        step_x = chart_rect.width() / (len(values) - 1)
        return [
            QPointF(chart_rect.left() + step_x * index, self._value_to_y(value, chart_rect))
            for index, value in enumerate(values)
        ]

    def _draw_fill(self, painter: QPainter, chart_rect: QRectF, points: list[QPointF]) -> None:
        if len(points) < 2:
            return

        line_path = self._build_smooth_path(points)
        fill_path = QPainterPath(line_path)
        fill_path.lineTo(chart_rect.right(), chart_rect.bottom())
        fill_path.lineTo(chart_rect.left(), chart_rect.bottom())
        fill_path.closeSubpath()

        gradient = QLinearGradient(chart_rect.left(), chart_rect.top(), chart_rect.left(), chart_rect.bottom())
        top_color = QColor(self._accent_color)
        top_color.setAlpha(72)
        bottom_color = QColor(self._accent_color)
        bottom_color.setAlpha(6)
        gradient.setColorAt(0.0, top_color)
        gradient.setColorAt(1.0, bottom_color)

        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(gradient)
        painter.drawPath(fill_path)

    def _draw_curve(self, painter: QPainter, points: list[QPointF]) -> None:
        if len(points) < 2:
            return

        pen = QPen(self._accent_color, 2.2)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.setPen(pen)
        painter.drawPath(self._build_smooth_path(points))

    def _draw_hover_indicator(
        self,
        painter: QPainter,
        chart_rect: QRectF,
        points: list[QPointF],
        values: list[float],
    ) -> None:
        if not points or self._hover_index is None:
            return

        point = points[self._hover_index]
        stem_color = QColor(self._accent_color)
        stem_color.setAlpha(120)
        painter.setPen(QPen(stem_color, 1.6))
        painter.drawLine(QPointF(point.x(), point.y() + 10), QPointF(point.x(), chart_rect.bottom()))

        painter.setBrush(QColor("#ffffff"))
        painter.setPen(QPen(self._accent_color, 1.4))
        painter.drawEllipse(QPointF(point.x(), chart_rect.bottom()), 2.5, 2.5)

        halo_color = QColor(self._accent_color)
        halo_color.setAlpha(48)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(halo_color)
        painter.drawEllipse(point, 13, 13)

        painter.setBrush(self._accent_color)
        painter.setPen(QPen(QColor("#ffffff"), 3))
        painter.drawEllipse(point, 7, 7)

        value_text = f"{round(values[self._hover_index])}%"
        font = QFont(self.font())
        font.setPixelSize(11)
        font.setWeight(QFont.Weight.DemiBold)
        painter.setFont(font)

        metrics = painter.fontMetrics()
        bubble_rect = QRectF(0, 0, metrics.horizontalAdvance(value_text) + 18, 24)
        bubble_rect.moveLeft(max(chart_rect.left(), min(point.x() - bubble_rect.width() / 2, chart_rect.right() - bubble_rect.width())))
        bubble_rect.moveTop(max(chart_rect.top(), point.y() - bubble_rect.height() - 14))

        bubble_color = QColor(self._accent_color)
        bubble_color.setAlpha(230)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(bubble_color)
        painter.drawRoundedRect(bubble_rect, 8, 8)

        painter.setPen(QColor("#ffffff"))
        painter.drawText(bubble_rect, Qt.AlignmentFlag.AlignCenter, value_text)

    @staticmethod
    def _build_smooth_path(points: list[QPointF]) -> QPainterPath:
        path = QPainterPath(points[0])
        for index in range(1, len(points)):
            previous = points[index - 1]
            current = points[index]
            mid_x = (previous.x() + current.x()) / 2
            path.cubicTo(QPointF(mid_x, previous.y()), QPointF(mid_x, current.y()), current)
        return path

    @staticmethod
    def _value_to_y(value: float, chart_rect: QRectF) -> float:
        return chart_rect.bottom() - chart_rect.height() * (_clamp_percent(value) / 100.0)

    def _display_values(self) -> list[float]:
        if self._animation_source_values is None or len(self._animation_source_values) != len(self._values):
            return list(self._values)

        if self._animation_progress >= 1.0:
            return list(self._values)

        displayed: list[float] = []
        for index, start in enumerate(self._animation_source_values):
            end = self._values[min(index + 1, len(self._values) - 1)] if index < len(self._values) - 1 else self._values[-1]
            displayed.append(start + (end - start) * self._animation_progress)

        return displayed

    def _on_animation_value_changed(self, value) -> None:
        self._animation_progress = float(value)
        if self._hover_index is not None:
            values = self._display_values()
            self.hoverValueChanged.emit(self._hover_index, values[self._hover_index])
        self.update()

    def _on_animation_finished(self) -> None:
        self._animation_progress = 1.0
        self._animation_source_values = None
        self.update()

    def _clear_hover(self) -> None:
        if self._hover_index is None:
            return

        self._hover_index = None
        self.hoverLeft.emit()
        self.update()

    def _chart_rect(self) -> QRectF:
        return QRectF(self.rect()).adjusted(38, 12, -8, -18)


class OccupancyChartWidget(SimpleCardWidget):
    """占用图表控件，适合展示 CPU、RAM 等 0-100 的占用值。"""

    def __init__(
        self,
        title: str = "",
        accent_color: QColor | str = "#ff8ed0",
        values: list[float] | None = None,
        glyph_icon: FluentIcon = FluentIcon.SPEED_HIGH,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._accent_color = QColor(accent_color)
        self._value_text = "0%"
        self._hovering = False

        self._create_widgets(title, glyph_icon)
        self._set_layout()
        self._connect_signals()

        self.setAccentColor(self._accent_color)
        self.setValues(values or [0.0] * 24)

    def _create_widgets(self, title: str, glyph_icon: FluentIcon) -> None:
        self.icon_widget = _ColoredFluentIconWidget(glyph_icon, self._accent_color, self)
        self.title_label = BodyLabel(title, self)
        self.value_label = StrongBodyLabel(self._value_text, self)
        self.canvas = _OccupancyCanvas(self._accent_color, self)

    def _set_layout(self) -> None:
        self.header_layout = QHBoxLayout()
        self.header_layout.setContentsMargins(0, 0, 0, 0)
        self.header_layout.setSpacing(10)
        self.header_layout.addWidget(self.icon_widget, 0, Qt.AlignmentFlag.AlignVCenter)
        self.header_layout.addWidget(self.title_label, 1, Qt.AlignmentFlag.AlignVCenter)
        self.header_layout.addWidget(self.value_label, 0, Qt.AlignmentFlag.AlignVCenter)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(16, 16, 16, 12)
        self.v_box_layout.setSpacing(8)
        self.v_box_layout.addLayout(self.header_layout)
        self.v_box_layout.addWidget(self.canvas)

        self.setMinimumHeight(196)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def _connect_signals(self) -> None:
        self.canvas.hoverValueChanged.connect(self._on_hover_value_changed)
        self.canvas.hoverLeft.connect(self._on_hover_left)

    def setTitle(self, title: str) -> None:
        self.title_label.setText(title)

    def title(self) -> str:
        return self.title_label.text()

    def setAccentColor(self, color: QColor | str) -> None:
        self._accent_color = QColor(color)
        self.canvas.setAccentColor(self._accent_color)
        self.icon_widget.setColor(self._accent_color)

    def accentColor(self) -> QColor:
        return QColor(self._accent_color)

    def setValueText(self, text: str) -> None:
        self._value_text = text
        if not self._hovering:
            self.value_label.setText(text)

    def valueText(self) -> str:
        return self._value_text

    def setValue(self, value: float) -> None:
        self.setValueText(f"{round(_clamp_percent(value))}%")

    def setValues(self, values: list[float]) -> None:
        normalized = [_clamp_percent(value) for value in values] or [0.0]
        self.canvas.setValues(normalized)
        self.setValue(normalized[-1])

    def appendValue(self, value: float) -> None:
        normalized = _clamp_percent(value)
        self.canvas.appendValue(normalized)
        self.setValue(normalized)

    def _on_hover_value_changed(self, _index: int, value: float) -> None:
        self._hovering = True
        self.value_label.setText(f"{round(value)}%")

    def _on_hover_left(self) -> None:
        self._hovering = False
        self.value_label.setText(self._value_text)


class OccupancyPanel(QWidget):
    """首页右侧占用概览面板。"""

    HISTORY_SIZE = 24

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._initialized = False
        self._cpu_history = _MetricHistory(self.HISTORY_SIZE)
        self._memory_history = _MetricHistory(self.HISTORY_SIZE)
        self._sampler = _SystemOccupancySampler(parent=self)

        self._create_widgets()
        self._set_layout()
        self._connect_signals()

        self._sampler.start()

    def _create_widgets(self) -> None:
        cpu_color, memory_color = self._theme_accent_colors()
        self.cpu_chart = OccupancyChartWidget("CPU", cpu_color, self._cpu_history.values(), FluentIcon.SPEED_HIGH, self)
        self.memory_chart = OccupancyChartWidget(
            "RAM",
            memory_color,
            self._memory_history.values(),
            FluentIcon.IOT,
            self,
        )

    def _set_layout(self) -> None:
        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.addWidget(self.cpu_chart)
        self.v_box_layout.addWidget(self.memory_chart)

        self.setMinimumWidth(320)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def _connect_signals(self) -> None:
        self._sampler.sampleChanged.connect(self._on_sample_changed)
        cfg.themeChanged.connect(self._apply_theme_accent_colors)
        cfg.themeColorChanged.connect(self._apply_theme_accent_colors)

    def _on_sample_changed(self, cpu_percent: float, memory_percent: float) -> None:
        snapshot = OccupancySnapshot(cpu_percent=cpu_percent, memory_percent=memory_percent)
        if not self._initialized:
            self._cpu_history.reset(snapshot.cpu_percent)
            self._memory_history.reset(snapshot.memory_percent)
            self.cpu_chart.setValues(self._cpu_history.values())
            self.memory_chart.setValues(self._memory_history.values())
            self._initialized = True
            return

        self._cpu_history.append(snapshot.cpu_percent)
        self._memory_history.append(snapshot.memory_percent)
        self.cpu_chart.appendValue(snapshot.cpu_percent)
        self.memory_chart.appendValue(snapshot.memory_percent)

    def _apply_theme_accent_colors(self, *_args) -> None:
        cpu_color, memory_color = self._theme_accent_colors()
        self.cpu_chart.setAccentColor(cpu_color)
        self.memory_chart.setAccentColor(memory_color)

    @staticmethod
    def _theme_accent_colors() -> tuple[QColor, QColor]:
        base = QColor(cfg.get(cfg.themeColor))
        if isDarkTheme():
            return (
                _mix_color(base, "#ffffff", 0.18),
                _mix_color(base, "#ffffff", 0.36),
            )

        return (
            _mix_color(base, "#000000", 0.08),
            _mix_color(base, "#ffffff", 0.24),
        )

# -*- coding: utf-8 -*-
from __future__ import annotations

from PySide6.QtCore import QEasingCurve, QPointF, QRectF, QSize, Qt, Signal, QVariantAnimation
from PySide6.QtGui import QColor, QFont, QLinearGradient, QPainter, QPainterPath, QPen
from PySide6.QtWidgets import QHBoxLayout, QSizePolicy, QVBoxLayout, QWidget
from qfluentwidgets import BodyLabel, FluentIcon, SimpleCardWidget, StrongBodyLabel, isDarkTheme

from src.core.config import cfg
from src.core.home import OccupancySnapshot, SystemOccupancySampler


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
    hoverValueChanged = Signal(float)
    hoverLeft = Signal()

    def __init__(self, accent_color: QColor | str, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._accent_color = QColor(accent_color)
        self._values: list[float] = [0.0] * 24
        self._animation_source_values: list[float] | None = None
        self._incoming_value: float | None = None
        self._animation_progress = 1.0
        self._hover_x: float | None = None

        self.setMinimumHeight(148)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.setMouseTracking(True)

        self._scroll_animation = QVariantAnimation(self)
        self._scroll_animation.setStartValue(0.0)
        self._scroll_animation.setEndValue(1.0)
        self._scroll_animation.setDuration(1180)
        self._scroll_animation.setEasingCurve(QEasingCurve.Type.Linear)
        self._scroll_animation.valueChanged.connect(self._on_animation_value_changed)
        self._scroll_animation.finished.connect(self._on_animation_finished)

        cfg.themeChanged.connect(self.update)

    def setAccentColor(self, color: QColor | str) -> None:
        self._accent_color = QColor(color)
        self.update()

    def setScrollDuration(self, duration_ms: int) -> None:
        self._scroll_animation.setDuration(max(120, int(duration_ms)))

    def setValues(self, values: list[float]) -> None:
        self._scroll_animation.stop()
        self._animation_source_values = None
        self._incoming_value = None
        self._animation_progress = 1.0
        self._values = [_clamp_percent(value) for value in values] or [0.0]
        self.update()

    def appendValue(self, value: float) -> None:
        normalized = _clamp_percent(value)
        if not self._values:
            self.setValues([normalized])
            return

        self._animation_source_values = list(self._values)
        self._incoming_value = normalized
        self._values = [*self._values[1:], normalized] if len(self._values) > 1 else [normalized]

        self._animation_progress = 0.0
        self._scroll_animation.stop()
        self._scroll_animation.start()

    def paintEvent(self, event) -> None:
        values = self._render_values()
        if not values:
            return

        painter = QPainter(self)
        painter.setRenderHints(QPainter.RenderHint.Antialiasing | QPainter.RenderHint.TextAntialiasing)

        chart_rect = self._chart_rect()
        if chart_rect.width() <= 0 or chart_rect.height() <= 0:
            return

        self._draw_grid(painter, chart_rect)
        points = self._build_points(chart_rect, values)
        display_points = self._build_display_points(chart_rect, points)
        painter.save()
        painter.setClipRect(chart_rect.adjusted(-6, -6, 6, 6))
        self._draw_fill(painter, chart_rect, display_points)
        self._draw_curve(painter, display_points)
        painter.restore()
        self._draw_hover_indicator(painter, chart_rect, points, values)

    def mouseMoveEvent(self, event) -> None:
        chart_rect = self._chart_rect()
        if not chart_rect.contains(event.position()):
            self._clear_hover()
            super().mouseMoveEvent(event)
            return

        values = self._render_values()
        if not values:
            return

        points = self._build_points(chart_rect, values)
        self._hover_x = max(chart_rect.left(), min(event.position().x(), chart_rect.right()))
        _, hover_value = self._point_and_value_at_x(points, values, self._hover_x)
        self.hoverValueChanged.emit(hover_value)
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

        visible_count = len(self._values)
        if visible_count <= 1:
            visible_count = len(values)

        step_x = chart_rect.width() / max(1, visible_count - 1)
        shift = step_x * self._animation_progress if self._is_animating_scroll(values) else 0.0
        return [
            QPointF(chart_rect.left() + step_x * index - shift, self._value_to_y(value, chart_rect))
            for index, value in enumerate(values)
        ]

    def _build_display_points(self, chart_rect: QRectF, points: list[QPointF]) -> list[QPointF]:
        if len(points) < 2 or not self._is_animating_scroll(self._render_values()):
            return points

        display_points: list[QPointF] = []

        if points[0].x() < chart_rect.left():
            display_points.append(self._interpolate_point_at_x(points[0], points[1], chart_rect.left()))

        display_points.extend(
            point for point in points if chart_rect.left() <= point.x() <= chart_rect.right()
        )

        if points[-1].x() > chart_rect.right():
            display_points.append(self._interpolate_point_at_x(points[-2], points[-1], chart_rect.right()))

        return display_points or points

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
        if not points or self._hover_x is None:
            return

        point, hover_value = self._point_and_value_at_x(points, values, self._hover_x)
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

        value_text = f"{round(hover_value)}%"
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

    @staticmethod
    def _interpolate_point_at_x(start: QPointF, end: QPointF, x: float) -> QPointF:
        if end.x() == start.x():
            return QPointF(x, end.y())

        ratio = (x - start.x()) / (end.x() - start.x())
        return QPointF(x, start.y() + (end.y() - start.y()) * ratio)

    @staticmethod
    def _point_and_value_at_x(points: list[QPointF], values: list[float], x: float) -> tuple[QPointF, float]:
        if len(points) == 1 or len(values) == 1:
            return points[0], values[0]

        if x <= points[0].x():
            return QPointF(x, points[0].y()), values[0]

        for index in range(1, len(points)):
            start = points[index - 1]
            end = points[index]
            if x > end.x():
                continue

            if end.x() == start.x():
                return QPointF(x, end.y()), values[index]

            ratio = (x - start.x()) / (end.x() - start.x())
            point = QPointF(x, start.y() + (end.y() - start.y()) * ratio)
            value = values[index - 1] + (values[index] - values[index - 1]) * ratio
            return point, value

        return QPointF(x, points[-1].y()), values[-1]

    def _render_values(self) -> list[float]:
        if (
            self._animation_source_values is None
            or self._incoming_value is None
            or self._animation_progress >= 1.0
        ):
            return list(self._values)

        return [*self._animation_source_values, self._incoming_value]

    def _is_animating_scroll(self, values: list[float]) -> bool:
        return (
            self._animation_source_values is not None
            and self._incoming_value is not None
            and self._animation_progress < 1.0
            and len(values) == len(self._values) + 1
        )

    def _on_animation_value_changed(self, value) -> None:
        self._animation_progress = float(value)
        if self._hover_x is not None:
            values = self._render_values()
            points = self._build_points(self._chart_rect(), values)
            _, hover_value = self._point_and_value_at_x(points, values, self._hover_x)
            self.hoverValueChanged.emit(hover_value)
        self.update()

    def _on_animation_finished(self) -> None:
        self._animation_progress = 1.0
        self._animation_source_values = None
        self._incoming_value = None
        if self._hover_x is not None:
            self._hover_x = max(self._chart_rect().left(), min(self._hover_x, self._chart_rect().right()))
        self.update()

    def _clear_hover(self) -> None:
        if self._hover_x is None:
            return

        self._hover_x = None
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

    def setScrollDuration(self, duration_ms: int) -> None:
        self.canvas.setScrollDuration(duration_ms)

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

    def _on_hover_value_changed(self, value: float) -> None:
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
        self._sampler = SystemOccupancySampler(parent=self)

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
        scroll_duration = max(120, self._sampler.interval_ms() - 20)
        self.cpu_chart.setScrollDuration(scroll_duration)
        self.memory_chart.setScrollDuration(scroll_duration)

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

# -*- coding: utf-8 -*-
"""通用拖拽文件夹组件。"""

# 标准库导入
from pathlib import Path

# 第三方库导入
from qfluentwidgets import BodyLabel, CaptionLabel, isDarkTheme, setFont, TransparentPushButton
from PySide6.QtCore import Property, QEasingCurve, QDir, QPropertyAnimation, Qt, QVariantAnimation, Signal
from PySide6.QtGui import QColor, QDragEnterEvent, QDragMoveEvent, QDropEvent, QEnterEvent, QPaintEvent, QPainter, QPainterPath, QPen
from PySide6.QtWidgets import QFileDialog, QSizePolicy, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config import cfg


class DropFolderWidget(QWidget):
    """支持拖拽或浏览文件夹的通用控件。"""

    folder_selected = Signal(object)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self._folder_path: Path | None = None
        self._hover_active = False
        self._drag_active = False
        self._hover_progress = 0.0
        self._drag_progress = 0.0
        self._ripple_progress = 0.0
        self._ripple_opacity = 0.0
        self.setAcceptDrops(True)
        self.setMinimumHeight(320)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        self.title_label = BodyLabel(self.tr("拖拽文件夹到此处"), self)
        self.or_label = BodyLabel(self.tr("或者"), self)
        self.browse_button = TransparentPushButton(self.tr("浏览文件夹"), self)
        self.path_label = CaptionLabel("", self)

        setFont(self.title_label, 18)
        setFont(self.or_label, 15)
        setFont(self.browse_button, 17)
        setFont(self.path_label, 13)

        self.title_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.or_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.path_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.path_label.setWordWrap(True)
        self.path_label.hide()
        self.path_label.setMinimumWidth(200)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(56, 48, 56, 48)
        layout.setSpacing(16)
        layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(self.title_label)
        layout.addWidget(self.or_label)
        layout.addWidget(self.browse_button, 0, Qt.AlignmentFlag.AlignHCenter)
        layout.addWidget(self.path_label)

        self._init_animations()
        self.browse_button.clicked.connect(self._browse_folder)
        cfg.themeChanged.connect(self._refresh_visuals)
        cfg.themeColorChanged.connect(self._refresh_visuals)

    @property
    def folder_path(self) -> Path | None:
        return self._folder_path

    def browse_folder(self) -> Path | None:
        """打开目录选择器，并在成功选择后发出信号。"""

        folder = QFileDialog.getExistingDirectory(
            self,
            self.tr("选择旧版配置目录"),
            QDir.homePath(),
            QFileDialog.Option.ShowDirsOnly | QFileDialog.Option.DontResolveSymlinks,
        )
        if not folder:
            return None

        folder_path = Path(folder)
        self.set_folder_path(folder_path)
        return folder_path

    def set_folder_path(self, folder_path: Path) -> None:
        self._folder_path = folder_path
        self.path_label.setText(str(folder_path))
        self.path_label.show()
        self.folder_selected.emit(folder_path)
        self.update()

    def enterEvent(self, event: QEnterEvent) -> None:
        self._hover_active = True
        self._start_hover_animation(True)
        super().enterEvent(event)

    def leaveEvent(self, event) -> None:
        self._hover_active = False
        if not self._drag_active:
            self._start_hover_animation(False)
        super().leaveEvent(event)

    def dragEnterEvent(self, event: QDragEnterEvent) -> None:
        folder_path = self._extract_folder_path(event)
        if folder_path is None:
            event.ignore()
            return

        self._hover_active = True
        self._drag_active = True
        self._start_hover_animation(True)
        self._start_drag_animation()
        event.acceptProposedAction()

    def dragMoveEvent(self, event: QDragMoveEvent) -> None:
        if self._extract_folder_path(event) is None:
            event.ignore()
            return
        if not self._drag_active:
            self._drag_active = True
            self._start_drag_animation()
        event.acceptProposedAction()

    def dragLeaveEvent(self, event) -> None:
        self._drag_active = False
        self._stop_drag_animation(keep_hover=self._hover_active)
        super().dragLeaveEvent(event)

    def dropEvent(self, event: QDropEvent) -> None:
        folder_path = self._extract_folder_path(event)
        self._drag_active = False
        if folder_path is None:
            self._stop_drag_animation(keep_hover=self._hover_active)
            event.ignore()
            return

        self.set_folder_path(folder_path)
        keep_hover = self.rect().contains(event.position().toPoint())
        self._hover_active = keep_hover
        self._stop_drag_animation(keep_hover=keep_hover)
        event.acceptProposedAction()

    def paintEvent(self, event: QPaintEvent) -> None:
        super().paintEvent(event)

        rect = self.rect().adjusted(18, 18, -18, -18)
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        accent_color, border_color, fill_color = self._resolve_accent_colors()
        self._paint_fill(painter, rect, accent_color, fill_color)
        self._paint_border(painter, rect, border_color, accent_color)
        self._paint_wave_border(painter, rect, accent_color)
        painter.end()

    def _browse_folder(self) -> None:
        self.browse_folder()

    def _init_animations(self) -> None:
        self._hover_animation = QPropertyAnimation(self, b"hover_progress", self)
        self._hover_animation.setDuration(180)
        self._hover_animation.setEasingCurve(QEasingCurve.Type.OutQuad)

        self._drag_animation = QPropertyAnimation(self, b"drag_progress", self)
        self._drag_animation.setDuration(210)
        self._drag_animation.setEasingCurve(QEasingCurve.Type.OutCubic)

        self._ripple_animation = QVariantAnimation(self)
        self._ripple_animation.setStartValue(0.0)
        self._ripple_animation.setEndValue(1.0)
        self._ripple_animation.setDuration(1020)
        self._ripple_animation.setLoopCount(-1)
        self._ripple_animation.setEasingCurve(QEasingCurve.Type.OutCubic)
        self._ripple_animation.valueChanged.connect(self._on_ripple_value_changed)

    def _start_hover_animation(self, active: bool) -> None:
        self._hover_animation.stop()
        self._hover_animation.setStartValue(self._hover_progress)
        self._hover_animation.setEndValue(1.0 if active else 0.0)
        self._hover_animation.start()

    def _start_drag_animation(self) -> None:
        self._drag_animation.stop()
        self._drag_animation.setStartValue(self._drag_progress)
        self._drag_animation.setEndValue(1.0)
        self._drag_animation.start()
        if self._ripple_animation.state() != QVariantAnimation.State.Running:
            self._ripple_animation.start()

    def _stop_drag_animation(self, keep_hover: bool) -> None:
        self._drag_animation.stop()
        self._drag_animation.setStartValue(self._drag_progress)
        self._drag_animation.setEndValue(0.0)
        self._drag_animation.start()

        if self._ripple_animation.state() == QVariantAnimation.State.Running:
            self._ripple_animation.stop()
        self._ripple_progress = 0.0
        self._ripple_opacity = 0.0

        if keep_hover:
            self._start_hover_animation(True)
        else:
            self._start_hover_animation(False)
        self.update()

    def _resolve_accent_colors(self) -> tuple[QColor, QColor, QColor]:
        try:
            accent_color = QColor(cfg.get(cfg.themeColor))
        except Exception:
            accent_color = QColor("#009faa")

        if not accent_color.isValid():
            accent_color = QColor("#009faa")

        neutral_border = QColor(255, 255, 255, 92) if isDarkTheme() else QColor(154, 160, 166, 255)
        toned_accent = self._mix_color(accent_color, QColor("#ffffff") if isDarkTheme() else QColor("#000000"), 0.18)
        border_color = self._mix_color(neutral_border, toned_accent, 0.22 * self._hover_progress)
        fill_color = QColor(toned_accent)
        fill_color.setAlpha(6 + round(10 * self._hover_progress + 20 * self._drag_progress))
        return toned_accent, border_color, fill_color

    def _paint_fill(self, painter: QPainter, rect, accent_color: QColor, fill_color: QColor) -> None:
        if fill_color.alpha() <= 0:
            return

        painter.save()
        clip_path = QPainterPath()
        clip_path.addRoundedRect(rect, 18, 18)
        painter.setClipPath(clip_path)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(fill_color)
        painter.drawRoundedRect(rect, 18, 18)

        # 命中区域的亮起感主要通过矩形面高亮表达，而不是中心特效。
        top_highlight = QColor("#ffffff" if isDarkTheme() else "#f8fafc")
        top_highlight.setAlpha(round(8 + 10 * self._hover_progress + 18 * self._drag_progress))
        painter.setBrush(top_highlight)
        painter.drawRoundedRect(rect.adjusted(6, 6, -6, -rect.height() * 0.48), 14, 14)

        inner_glow = QColor(accent_color)
        inner_glow.setAlpha(round(18 + 18 * self._drag_progress + 8 * self._hover_progress))
        inner_pen = QPen(inner_glow, 1.1 + self._drag_progress * 0.5)
        inner_pen.setStyle(Qt.PenStyle.SolidLine)
        painter.setPen(inner_pen)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawRoundedRect(rect.adjusted(4, 4, -4, -4), 15, 15)
        painter.restore()

    def _paint_border(self, painter: QPainter, rect, border_color: QColor, accent_color: QColor) -> None:
        painter.save()

        base_pen = QPen(border_color, 2 + self._hover_progress * 0.35)
        base_pen.setStyle(Qt.PenStyle.DashLine)
        base_color = QColor(border_color)
        base_color.setAlpha(round(255 * max(0.12, 1.0 - self._drag_progress)))
        base_pen.setColor(base_color)
        painter.setPen(base_pen)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawRoundedRect(rect, 18, 18)

        if self._drag_progress > 0.0:
            active_color = QColor(accent_color)
            active_color.setAlpha(round(188 * self._drag_progress))
            active_pen = QPen(active_color, 2 + self._drag_progress * 0.8)
            active_pen.setStyle(Qt.PenStyle.SolidLine)
            painter.setPen(active_pen)
            painter.drawRoundedRect(rect, 18, 18)

        painter.restore()

    def _paint_wave_border(self, painter: QPainter, rect, accent_color: QColor) -> None:
        if self._drag_progress <= 0.0 or self._ripple_opacity <= 0.0:
            return

        painter.save()
        wave_rect = rect.adjusted(-4, -4, 4, 4)

        leading_color = QColor(accent_color)
        leading_color.setAlpha(round(112 * self._drag_progress * self._ripple_opacity))
        if leading_color.alpha() <= 0:
            painter.restore()
            return

        leading_pen = QPen(leading_color, 1.6 + self._drag_progress * 0.8)
        leading_pen.setStyle(Qt.PenStyle.CustomDashLine)
        leading_pen.setDashPattern([10, 8, 3, 8])
        leading_pen.setDashOffset(-self._ripple_progress * 34)
        painter.setPen(leading_pen)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawRoundedRect(wave_rect, 22, 22)

        trailing_color = QColor(accent_color)
        trailing_color.setAlpha(round(52 * self._drag_progress * max(0.0, self._ripple_opacity - 0.18)))
        if trailing_color.alpha() > 0:
            trailing_pen = QPen(trailing_color, 1.0 + self._drag_progress * 0.5)
            trailing_pen.setStyle(Qt.PenStyle.CustomDashLine)
            trailing_pen.setDashPattern([4, 10, 8, 10])
            trailing_pen.setDashOffset(self._ripple_progress * 18)
            painter.setPen(trailing_pen)
            painter.drawRoundedRect(wave_rect.adjusted(-2, -2, 2, 2), 24, 24)

        painter.restore()

    def _on_ripple_value_changed(self, value) -> None:
        self._ripple_progress = float(value)
        self._ripple_opacity = max(0.0, 1.0 - self._ripple_progress)
        if self._drag_progress > 0.0:
            self.update()

    def _refresh_visuals(self, *_args) -> None:
        self.update()

    @staticmethod
    def _mix_color(source: QColor, target: QColor, ratio: float) -> QColor:
        ratio = max(0.0, min(1.0, ratio))
        return QColor(
            round(source.red() * (1 - ratio) + target.red() * ratio),
            round(source.green() * (1 - ratio) + target.green() * ratio),
            round(source.blue() * (1 - ratio) + target.blue() * ratio),
            round(source.alpha() * (1 - ratio) + target.alpha() * ratio),
        )

    def get_hover_progress(self) -> float:
        return self._hover_progress

    def set_hover_progress(self, value: float) -> None:
        self._hover_progress = max(0.0, min(1.0, float(value)))
        self.update()

    hover_progress = Property(float, get_hover_progress, set_hover_progress)

    def get_drag_progress(self) -> float:
        return self._drag_progress

    def set_drag_progress(self, value: float) -> None:
        self._drag_progress = max(0.0, min(1.0, float(value)))
        if self._drag_progress <= 0.0 and self._ripple_animation.state() != QVariantAnimation.State.Running:
            self._ripple_progress = 0.0
            self._ripple_opacity = 0.0
        self.update()

    drag_progress = Property(float, get_drag_progress, set_drag_progress)

    @staticmethod
    def _extract_folder_path(event) -> Path | None:
        urls = event.mimeData().urls()
        if len(urls) != 1 or not urls[0].isLocalFile():
            return None

        folder_path = Path(urls[0].toLocalFile())
        if not folder_path.exists() or not folder_path.is_dir():
            return None
        return folder_path

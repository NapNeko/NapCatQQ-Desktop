# -*- coding: utf-8 -*-
"""通用拖拽文件夹组件。"""

# 标准库导入
from pathlib import Path

# 第三方库导入
from qfluentwidgets import BodyLabel, CaptionLabel, isDarkTheme, setFont, TransparentPushButton
from PySide6.QtCore import QDir, Qt, Signal
from PySide6.QtGui import QColor, QDragEnterEvent, QDragMoveEvent, QDropEvent, QPainter, QPen
from PySide6.QtWidgets import QFileDialog, QSizePolicy, QVBoxLayout, QWidget


class DropFolderWidget(QWidget):
    """支持拖拽或浏览文件夹的通用控件。"""

    folder_selected = Signal(object)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self._folder_path: Path | None = None
        self._drag_active = False
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

        self.browse_button.clicked.connect(self._browse_folder)

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

    def dragEnterEvent(self, event: QDragEnterEvent) -> None:
        folder_path = self._extract_folder_path(event)
        if folder_path is None:
            event.ignore()
            return

        self._drag_active = True
        event.acceptProposedAction()
        self.update()

    def dragMoveEvent(self, event: QDragMoveEvent) -> None:
        if self._extract_folder_path(event) is None:
            event.ignore()
            return
        event.acceptProposedAction()

    def dragLeaveEvent(self, event) -> None:
        self._drag_active = False
        self.update()
        super().dragLeaveEvent(event)

    def dropEvent(self, event: QDropEvent) -> None:
        folder_path = self._extract_folder_path(event)
        self._drag_active = False
        self.update()
        if folder_path is None:
            event.ignore()
            return

        self.set_folder_path(folder_path)
        event.acceptProposedAction()

    def paintEvent(self, event) -> None:
        super().paintEvent(event)

        rect = self.rect().adjusted(18, 18, -18, -18)
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        color = QColor("#3daee9" if self._drag_active else ("#5f6368" if isDarkTheme() else "#9aa0a6"))
        pen = QPen(color, 2)
        pen.setStyle(Qt.PenStyle.DashLine)
        painter.setPen(pen)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawRoundedRect(rect, 18, 18)

    def _browse_folder(self) -> None:
        self.browse_folder()

    @staticmethod
    def _extract_folder_path(event) -> Path | None:
        urls = event.mimeData().urls()
        if len(urls) != 1 or not urls[0].isLocalFile():
            return None

        folder_path = Path(urls[0].toLocalFile())
        if not folder_path.exists() or not folder_path.is_dir():
            return None
        return folder_path

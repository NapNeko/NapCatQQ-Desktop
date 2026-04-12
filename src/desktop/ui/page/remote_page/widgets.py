# -*- coding: utf-8 -*-
"""远程管理页面自定义控件。"""
from __future__ import annotations

from typing import TYPE_CHECKING

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QFrame, QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    ComboBox,
    LineEdit,
    PrimaryPushButton,
    PushButton,
    StrongBodyLabel,
    SubtitleLabel,
    ToolButton,
)

if TYPE_CHECKING:
    from PySide6.QtGui import QIcon


class ConnectionCard(QFrame):
    """连接信息卡片。"""

    def __init__(self, title: str, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("connectionCard")
        self._setup_ui(title)

    def _setup_ui(self, title: str) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(12)
        layout.setContentsMargins(16, 16, 16, 16)

        # 标题
        self.title_label = StrongBodyLabel(title, self)
        layout.addWidget(self.title_label)

        # 内容区域
        self.content = QWidget(self)
        self.content_layout = QVBoxLayout(self.content)
        self.content_layout.setSpacing(8)
        self.content_layout.setContentsMargins(0, 0, 0, 0)
        layout.addWidget(self.content)

    def set_content(self, widget: QWidget) -> None:
        """设置内容控件。"""
        # 清除旧内容
        while self.content_layout.count():
            item = self.content_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

        self.content_layout.addWidget(widget)


class FormRow(QWidget):
    """表单行。"""

    def __init__(
        self,
        label: str,
        widget: QWidget,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._setup_ui(label, widget)

    def _setup_ui(self, label: str, widget: QWidget) -> None:
        layout = QHBoxLayout(self)
        layout.setSpacing(12)
        layout.setContentsMargins(0, 0, 0, 0)

        self.label = BodyLabel(label, self)
        self.label.setFixedWidth(80)
        self.label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        layout.addWidget(self.label)

        layout.addWidget(widget, 1)


class StatusIndicator(QWidget):
    """状态指示器。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QHBoxLayout(self)
        layout.setSpacing(8)
        layout.setContentsMargins(0, 0, 0, 0)

        self.dot = BodyLabel("●", self)
        self.dot.setStyleSheet("color: #999;")

        self.text = BodyLabel("未连接", self)
        self.text.setStyleSheet("color: #999;")

        layout.addWidget(self.dot)
        layout.addWidget(self.text)
        layout.addStretch()

    def set_status(self, connected: bool, message: str | None = None) -> None:
        """设置状态。"""
        if connected:
            self.dot.setStyleSheet("color: #52c41a;")
            self.text.setStyleSheet("color: #52c41a;")
            self.text.setText(message or "已连接")
        else:
            self.dot.setStyleSheet("color: #999;")
            self.text.setStyleSheet("color: #999;")
            self.text.setText(message or "未连接")


class ActionButtonGroup(QWidget):
    """操作按钮组。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QHBoxLayout(self)
        layout.setSpacing(8)
        layout.setContentsMargins(0, 0, 0, 0)

        self.start_btn = PushButton("启动", self)
        self.stop_btn = PushButton("停止", self)
        self.restart_btn = PushButton("重启", self)
        self.log_btn = PushButton("日志", self)

        layout.addWidget(self.start_btn)
        layout.addWidget(self.stop_btn)
        layout.addWidget(self.restart_btn)
        layout.addWidget(self.log_btn)
        layout.addStretch()

    def set_enabled(self, enabled: bool) -> None:
        """设置按钮启用状态。"""
        self.start_btn.setEnabled(enabled)
        self.stop_btn.setEnabled(enabled)
        self.restart_btn.setEnabled(enabled)
        self.log_btn.setEnabled(enabled)


class ModeSelector(QWidget):
    """模式选择器。"""

    mode_changed = None  # 信号在外部连接

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QHBoxLayout(self)
        layout.setSpacing(12)
        layout.setContentsMargins(0, 0, 0, 0)

        self.label = BodyLabel("连接模式:", self)
        layout.addWidget(self.label)

        self.combo = ComboBox(self)
        self.combo.addItem("SSH 直连", "ssh")
        self.combo.addItem("Agent/Daemon", "agent")
        self.combo.setFixedWidth(150)
        layout.addWidget(self.combo)

        layout.addStretch()

    def current_mode(self) -> str:
        """获取当前模式。"""
        return self.combo.currentData()

    def set_mode(self, mode: str) -> None:
        """设置模式。"""
        index = self.combo.findData(mode)
        if index >= 0:
            self.combo.setCurrentIndex(index)

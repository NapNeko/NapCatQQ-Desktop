# -*- coding: utf-8 -*-
# 标准库导入
import subprocess
from pathlib import Path

# 第三方库导入
from qfluentwidgets import (
    ComboBox,
    HeaderCardWidget,
    TransparentPushButton,
    TransparentToolButton,
    isDarkTheme,
)
from qfluentwidgets.components.widgets.menu import MenuAnimationType
from qfluentwidgets import FluentIcon as FI
from PySide6.QtCore import QPoint, Qt
from PySide6.QtGui import QAction, QColor, QTextCharFormat, QTextCursor
from PySide6.QtWidgets import QPlainTextEdit, QSizePolicy, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.logging import LogLevel, LogOutputNotification, log_output_notification_center, logger
from src.ui.components.code_editor import CodeExibit, LogHighlighter
from src.ui.components.info_bar import error_bar, warning_bar


class LogLevelFilterComboBox(ComboBox):
    """避免在菜单项关闭后继续访问已销毁 item 的安全组合框。"""

    def _showComboMenu(self):
        if not self.items:
            return

        menu = self._createComboMenu()
        for index, item in enumerate(self.items):
            action = QAction(item.icon, item.text)
            action.setEnabled(item.isEnabled)
            action.triggered.connect(lambda checked=False, idx=index: self._onItemClicked(idx))
            menu.addAction(action)

        if menu.view.width() < self.width():
            menu.view.setMinimumWidth(self.width())
            menu.adjustSize()

        menu.setMaxVisibleItems(self.maxVisibleItems())
        menu.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        menu.closedSignal.connect(self._onDropMenuClosed)
        self.dropMenu = menu

        if self.currentIndex() >= 0 and self.items:
            menu.setDefaultAction(menu.actions()[self.currentIndex()])

        x = -menu.width() // 2 + menu.layout().contentsMargins().left() + self.width() // 2
        pd = self.mapToGlobal(QPoint(x, self.height()))
        hd = menu.view.heightForAnimation(pd, MenuAnimationType.DROP_DOWN)

        pu = self.mapToGlobal(QPoint(x, 0))
        hu = menu.view.heightForAnimation(pu, MenuAnimationType.PULL_UP)

        if hd >= hu:
            menu.view.adjustSize(pd, MenuAnimationType.DROP_DOWN)
            menu.exec(pd, aniType=MenuAnimationType.DROP_DOWN)
        else:
            menu.view.adjustSize(pu, MenuAnimationType.PULL_UP)
            menu.exec(pu, aniType=MenuAnimationType.PULL_UP)


class DesktopLog(QWidget):
    """Desktop 日志页面。"""

    def __init__(self, parent=None) -> None:
        super().__init__(parent=parent)

        self._log_path: Path | None = None
        self._log_offset: int = 0
        self._preview_entries: list[tuple[str | None, str]] = []
        self._pending_fallback_text: str | None = None
        self._is_refreshing_view = False
        self._view_refresh_requested = False

        self.view = HeaderCardWidget(self)
        self.log_view = CodeExibit(self.view)
        self.log_highlighter = LogHighlighter(self.log_view.document())
        self.level_filter_combo = LogLevelFilterComboBox(self.view)
        self.open_location_button = TransparentPushButton(FI.FOLDER, self.tr("打开位置"), self.view)
        self.font_enlarge_button = TransparentToolButton(FI.ADD, self.view)
        self.font_shrink_button = TransparentToolButton(FI.REMOVE, self.view)
        self.v_box_layout = QVBoxLayout(self)

        self.setObjectName("SetupDesktopLogWidget")
        self._setup_ui()
        self._connect_signal()
        self.reload_log_view()

    def _setup_ui(self) -> None:
        """初始化界面。"""
        self.view.setTitle(self.tr("Desktop 日志"))
        self.log_view.set_font_size(12)
        self.log_view.setLineWrapMode(QPlainTextEdit.LineWrapMode.WidgetWidth)
        self.log_view.setMinimumHeight(420)
        self.log_view.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.view.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.level_filter_combo.addItem(self.tr("全部等级"), userData=LogLevel.ALL_.name)
        self.level_filter_combo.addItem(self.tr("严重"), userData=LogLevel.CRIT.name)
        self.level_filter_combo.addItem(self.tr("错误"), userData=LogLevel.EROR.name)
        self.level_filter_combo.addItem(self.tr("警告"), userData=LogLevel.WARN.name)
        self.level_filter_combo.addItem(self.tr("信息"), userData=LogLevel.INFO.name)
        self.level_filter_combo.addItem(self.tr("调试"), userData=LogLevel.DBUG.name)
        self.level_filter_combo.addItem(self.tr("跟踪"), userData=LogLevel.TRCE.name)
        self.level_filter_combo.setCurrentIndex(0)
        self.level_filter_combo.setMinimumWidth(132)

        self.view.headerLayout.addStretch(1)
        self.view.headerLayout.addWidget(self.level_filter_combo)
        self.view.headerLayout.addWidget(self.open_location_button)
        self.view.headerLayout.addWidget(self.font_enlarge_button)
        self.view.headerLayout.addWidget(self.font_shrink_button)
        self.view.viewLayout.setContentsMargins(2, 4, 2, 0)
        self.view.viewLayout.addWidget(self.log_view, 1)

        self.v_box_layout.setContentsMargins(0, 0, 0, 8)
        self.v_box_layout.addWidget(self.view, 1)

        self._show_log_placeholder(self.tr("正在加载 Desktop 日志..."))

    def _connect_signal(self) -> None:
        """连接信号。"""
        self.level_filter_combo.currentIndexChanged.connect(lambda _index: self._on_level_filter_changed())
        self.open_location_button.clicked.connect(self.open_current_log_location)
        self.font_enlarge_button.clicked.connect(self.slot_font_enlarge_button)
        self.font_shrink_button.clicked.connect(self.slot_font_shrink_button)
        log_output_notification_center.log_output_created.connect(
            self._on_log_output_created,
            Qt.ConnectionType.QueuedConnection,
        )

    def reload_log_view(self) -> None:
        """完整重载当前 Desktop 日志文件。"""
        if (log_path := self._resolve_log_path()) is None:
            self._log_path = None
            self._log_offset = 0
            self._preview_entries = []
            self._request_view_refresh(self.tr("当前会话尚未创建 Desktop 日志文件"))
            return

        try:
            text = log_path.read_text(encoding="utf-8", errors="replace")
        except OSError as exc:
            self._log_path = None
            self._log_offset = 0
            self._preview_entries = []
            error_bar(self.tr(f"读取 Desktop 日志失败: {exc}"), parent=self)
            self._request_view_refresh(self.tr("读取 Desktop 日志失败"))
            return

        self._log_path = log_path
        self._log_offset = log_path.stat().st_size
        self._preview_entries = self._build_preview_entries(text)
        self._request_view_refresh(self.tr("当前日志文件为空"))

    def open_current_log_location(self) -> None:
        """打开当前日志文件位置。"""
        if (log_path := self._resolve_log_path()) is None:
            warning_bar(self.tr("当前会话尚未创建 Desktop 日志文件"), parent=self)
            return

        subprocess.Popen(f'explorer /select,"{log_path}"', shell=True)

    def slot_set_log_view(self, data: str) -> None:
        """设置当前日志内容。"""
        follow_tail = self._is_log_view_pinned_to_bottom()
        current_cursor = self.log_view.textCursor()
        scroll_bar = self.log_view.verticalScrollBar()
        previous_scroll_value = scroll_bar.value()

        QPlainTextEdit.setPlainText(self.log_view, data)
        self.log_view._apply_document_text_color(self.log_view._theme_text_color(self.log_view._is_dark_theme(None)))

        if follow_tail:
            scroll_bar.setValue(scroll_bar.maximum())
        else:
            self.log_view.setTextCursor(current_cursor)
            scroll_bar.setValue(min(previous_scroll_value, scroll_bar.maximum()))

    def slot_insert_log_view(self, data: str) -> None:
        """向日志视图追加内容。"""
        follow_tail = self._is_log_view_pinned_to_bottom()
        current_cursor = self.log_view.textCursor()
        scroll_bar = self.log_view.verticalScrollBar()
        previous_scroll_value = scroll_bar.value()

        cursor = QTextCursor(self.log_view.document())
        cursor.movePosition(QTextCursor.MoveOperation.End)
        fmt = QTextCharFormat()
        fmt.setForeground(QColor("#e6eaf2") if isDarkTheme() else QColor("#1f2937"))
        cursor.setCharFormat(fmt)
        cursor.insertText(data)

        if follow_tail:
            self.log_view.setTextCursor(cursor)
            scroll_bar.setValue(scroll_bar.maximum())
        else:
            self.log_view.setTextCursor(current_cursor)
            scroll_bar.setValue(previous_scroll_value)

    def slot_font_enlarge_button(self) -> None:
        """放大字体。"""
        self.log_view.set_font_size(self.log_view.font_size + 1)

    def slot_font_shrink_button(self) -> None:
        """缩小字体。"""
        self.log_view.set_font_size(self.log_view.font_size - 1)

    def _on_log_output_created(self, notification: LogOutputNotification) -> None:
        """接收日志域追加事件。"""
        if self._log_path is None:
            self.reload_log_view()
            return

        if notification.log_path != self._log_path:
            if notification.log_path == self._resolve_log_path():
                self.reload_log_view()
            return

        self._log_offset += len(notification.line_text.encode("utf-8", errors="replace"))
        new_entries = self._build_preview_entries(notification.line_text, self._preview_entries[-1][0] if self._preview_entries else None)
        self._preview_entries.extend(new_entries)
        self._request_view_refresh()

    def _resolve_log_path(self) -> Path | None:
        """解析当前会话日志文件路径。"""
        log_path = getattr(logger, "log_path", None)
        if log_path is None:
            return None

        path = Path(log_path)
        return path if path.exists() else None

    def _show_log_placeholder(self, text: str) -> None:
        """展示日志占位文本。"""
        self.slot_set_log_view(text)

    def _is_log_view_pinned_to_bottom(self) -> bool:
        """判断日志视图当前是否贴底。"""
        scroll_bar = self.log_view.verticalScrollBar()
        return scroll_bar.maximum() - scroll_bar.value() <= 2

    def _on_level_filter_changed(self) -> None:
        """切换日志等级筛选。"""
        self._request_view_refresh()

    def _request_view_refresh(self, fallback_text: str | None = None) -> None:
        """请求一次串行的日志视图刷新。"""
        if fallback_text is not None:
            self._pending_fallback_text = fallback_text

        if self._is_refreshing_view:
            self._view_refresh_requested = True
            return

        self._is_refreshing_view = True
        try:
            while True:
                self._view_refresh_requested = False
                self._flush_pending_view_refresh()
                if not self._view_refresh_requested:
                    break
        finally:
            self._is_refreshing_view = False

    def _flush_pending_view_refresh(self) -> None:
        """按当前筛选条件刷新日志预览。"""
        filtered_text = "".join(text for level, text in self._preview_entries if self._matches_selected_level(level))
        if filtered_text:
            self.slot_set_log_view(filtered_text)
            self._pending_fallback_text = None
            return

        if self._pending_fallback_text is not None:
            self._show_log_placeholder(self._pending_fallback_text)
            self._pending_fallback_text = None
            return

        self._show_log_placeholder(self.tr("当前筛选条件下没有日志"))

    def _selected_level_name(self) -> str | None:
        """返回当前筛选的日志等级。"""
        current_level = self.level_filter_combo.currentData()
        if current_level == LogLevel.ALL_.name:
            return None
        return current_level

    def _matches_selected_level(self, level_name: str | None) -> bool:
        """判断日志是否符合当前筛选条件。"""
        selected_level = self._selected_level_name()
        if selected_level is None:
            return True
        return level_name == selected_level

    @staticmethod
    def _build_preview_entries(text: str, previous_level: str | None = None) -> list[tuple[str | None, str]]:
        """将完整日志文本拆为可筛选的终端风格预览行。"""
        if not text:
            return []

        entries: list[tuple[str | None, str]] = []
        current_level = previous_level
        for line in text.splitlines(keepends=True):
            level_name, preview_line = DesktopLog._format_log_preview_line(line, current_level)
            if level_name is not None:
                current_level = level_name
            entries.append((current_level, preview_line))
        return entries

    @staticmethod
    def _format_log_preview_line(line: str, inherited_level: str | None = None) -> tuple[str | None, str]:
        """将单行完整日志压缩为 `时间 | 等级 | 消息`。"""
        newline = "\n" if line.endswith("\n") else ""
        raw_line = line.rstrip("\n")
        parts = raw_line.split(" | ", 5)
        if len(parts) != 6:
            return inherited_level, line

        time_text, level_text, _, _, _, message_text = parts
        level_name = level_text.strip()[1:-1].strip() if level_text.startswith("[") and level_text.endswith("]") else None
        return level_name, f"{time_text} | {level_text} | {message_text}{newline}"

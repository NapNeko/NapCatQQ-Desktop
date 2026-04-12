# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import isDarkTheme, setFont
from qfluentwidgets.components.widgets.menu import TextEditMenu
from PySide6.QtCore import Qt, QTimer, QUrl
from PySide6.QtGui import QColor, QDesktopServices, QMouseEvent, QPalette, QTextCharFormat, QTextCursor, QWheelEvent
from PySide6.QtWidgets import QTextBrowser, QWidget

# 项目内模块导入
from src.desktop.core.config import cfg
from src.desktop.ui.common.style_sheet import WidgetStyleSheet
from src.desktop.ui.components.code_editor.editor import CodeEditor
from src.desktop.ui.components.code_editor.smooth_scroll import SmoothTextScrollMixin


class CodeExibit(CodeEditor):

    def __init__(self, parent: QWidget) -> None:
        super().__init__(parent)
        self.setReadOnly(True)


class UpdateLogExhibit(SmoothTextScrollMixin, QTextBrowser):
    """更新日志页面使用的透明文本框"""

    def __init__(self, parent: QWidget) -> None:
        super().__init__(parent)
        self._raw_html = ""
        self.setReadOnly(True)
        self.setOpenExternalLinks(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        setFont(self, 16)
        self.viewport().setAutoFillBackground(False)
        self.viewport().setAttribute(Qt.WidgetAttribute.WA_StyledBackground, False)
        self.viewport().setStyleSheet("background: transparent;")

        WidgetStyleSheet.UPDATE_LOG_CARD.apply(self)
        self._base_style_sheet = self.styleSheet()
        self._init_smooth_scroll()
        self._apply_theme_palette()
        cfg.themeChanged.connect(self._queue_theme_palette_update)

    def setHtml(self, text: str) -> None:
        """为更新日志富文本注入主题相关的默认前景色。"""
        self._raw_html = text
        self._render_html()

    def wheelEvent(self, event: QWheelEvent) -> None:
        if self._handle_smooth_wheel_event(event):
            return
        super().wheelEvent(event)

    def _apply_theme_palette(self, *args) -> None:
        dark = self._is_dark_theme(args[0] if args else None)
        palette = self.palette()
        text_color = QColor("#e6eaf2") if dark else QColor("#1f2937")
        link_color = QColor("#7aa2f7") if dark else QColor("#2563eb")
        transparent = QColor(0, 0, 0, 0)
        palette.setColor(QPalette.ColorRole.Text, text_color)
        palette.setColor(QPalette.ColorRole.WindowText, text_color)
        palette.setColor(QPalette.ColorRole.Link, link_color)
        palette.setColor(QPalette.ColorRole.Base, transparent)
        palette.setColor(QPalette.ColorRole.Window, transparent)
        palette.setColor(QPalette.ColorRole.AlternateBase, transparent)
        self.setPalette(palette)
        self.setStyleSheet(self._base_style_sheet + self._theme_style_sheet(dark))
        fmt = QTextCharFormat()
        fmt.setForeground(text_color)
        self.setCurrentCharFormat(fmt)
        self._render_html(dark)

    def _queue_theme_palette_update(self, theme) -> None:
        QTimer.singleShot(0, lambda: self._apply_theme_palette(theme))

    def _render_html(self, dark: bool | None = None) -> None:
        if self._raw_html:
            super().setHtml(self._theme_css(dark) + self._raw_html)

    @staticmethod
    def _theme_css(dark: bool | None = None) -> str:
        dark = isDarkTheme() if dark is None else dark
        if dark:
            text_color = "#e6eaf2"
            muted_color = "#a9b4c8"
            link_color = "#7aa2f7"
            code_color = "#d7e3ff"
        else:
            text_color = "#1f2937"
            muted_color = "#4b5563"
            link_color = "#2563eb"
            code_color = "#1d4ed8"

        return (
            "<style>"
            "html, body { background: transparent; }"
            "body, p, li, span, div { color: " + text_color + "; }"
            "h1, h2, h3, h4, h5, h6, strong { color: " + text_color + "; }"
            "blockquote { color: " + muted_color + "; }"
            "a { color: " + link_color + "; }"
            "code, pre { color: " + code_color + "; }"
            "pre { white-space: pre-wrap; }"
            "</style>"
        )

    @staticmethod
    def _theme_style_sheet(dark: bool) -> str:
        text_color = "#e6eaf2" if dark else "#1f2937"
        link_color = "#7aa2f7" if dark else "#2563eb"
        return (
            "\nQTextBrowser {"
            f"color: {text_color};"
            f"selection-color: {text_color};"
            "background: transparent;"
            "}"
            "\nQTextBrowser > QWidget {"
            "background: transparent;"
            "}"
            "\nQTextBrowser a {"
            f"color: {link_color};"
            "}"
        )

    @staticmethod
    def _is_dark_theme(theme) -> bool:
        theme_name = getattr(theme, "name", "")
        if theme_name == "DARK":
            return True
        if theme_name == "LIGHT":
            return False
        return isDarkTheme()

    def mousePressEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton and self.anchorAt(event.pos()):
            QDesktopServices.openUrl(QUrl(self.anchorAt(event.pos())))
        else:
            super().mousePressEvent(event)

    def contextMenuEvent(self, event) -> None:
        menu = TextEditMenu(self)
        menu.exec(event.globalPos(), ani=True)

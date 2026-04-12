# -*- coding: utf-8 -*-

# 标准库导入
import os
import sys
import threading
from pathlib import Path

# 第三方库导入
import pytest
import main
from PySide6.QtCore import Qt, qInstallMessageHandler
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication, QPushButton

# 项目内模块导入
import src.desktop.core.installation.installers as install_func
import src.desktop.core.logging.log_func as log_func_module
from src.desktop.core.logging.notification_center import log_output_notification_center
from src.desktop.core.installation.installers import QQInstall
from src.desktop.core.logging.log_func import Logger


def create_test_logger(tmp_path: Path) -> Logger:
    """创建仅写入临时目录的测试日志器。"""
    test_logger = Logger()
    test_logger.load_config()
    test_logger.log_path = tmp_path / "app.log"
    test_logger.log_path.write_text("", encoding="utf-8")
    return test_logger


def ensure_exception_logging_app() -> main.ExceptionLoggingApplication:
    """创建或复用测试用 QApplication。"""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        return main.ExceptionLoggingApplication([])
    if isinstance(app, main.ExceptionLoggingApplication):
        return app

    pytest.skip("已有非 ExceptionLoggingApplication 的 QApplication 实例")


def test_logger_critical_serializes_level_and_message(tmp_path: Path) -> None:
    """critical 日志应带有 CRIT 等级并正确落盘。"""
    test_logger = create_test_logger(tmp_path)

    test_logger.critical("critical failure")

    content = test_logger.log_path.read_text(encoding="utf-8")
    assert "[CRIT]" in content
    assert "critical failure" in content


def test_logger_exception_writes_traceback(tmp_path: Path) -> None:
    """exception 日志应包含异常摘要和 traceback。"""
    test_logger = create_test_logger(tmp_path)

    try:
        raise ValueError("boom")
    except ValueError as exc:
        test_logger.exception("处理失败", exc)

    content = test_logger.log_path.read_text(encoding="utf-8")
    assert "处理失败: ValueError: boom" in content
    assert "Traceback" in content


def test_logger_publishes_log_output_notification(tmp_path: Path) -> None:
    """日志落盘后应广播实时输出事件。"""
    test_logger = create_test_logger(tmp_path)
    notifications: list[object] = []

    log_output_notification_center.log_output_created.connect(notifications.append)
    try:
        test_logger.info("stream hello")
    finally:
        log_output_notification_center.log_output_created.disconnect(notifications.append)

    assert notifications
    notification = notifications[-1]
    assert getattr(notification, "log_path") == test_logger.log_path
    assert "stream hello" in getattr(notification, "line_text")


def test_logger_trace_is_suppressed_without_developer_mode(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """未启用开发者模式时，TRACE 日志不应落盘。"""
    test_logger = create_test_logger(tmp_path)
    monkeypatch.setattr(log_func_module, "is_developer_mode_enabled", lambda: False)

    test_logger.trace("trace hidden")

    assert test_logger.log_path.read_text(encoding="utf-8") == ""


def test_logger_trace_is_emitted_in_developer_mode(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """启用开发者模式时，TRACE 日志应写入日志文件。"""
    test_logger = create_test_logger(tmp_path)
    monkeypatch.setattr(log_func_module, "is_developer_mode_enabled", lambda: True)

    test_logger.trace("trace visible")

    content = test_logger.log_path.read_text(encoding="utf-8")
    assert "[TRCE]" in content
    assert "trace visible" in content


def test_logger_trace_override_can_enable_without_developer_mode(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """运行时开关应允许在非开发者模式下临时启用 TRACE 日志。"""
    test_logger = create_test_logger(tmp_path)
    monkeypatch.setattr(log_func_module, "is_developer_mode_enabled", lambda: False)
    test_logger.set_trace_logging_enabled(True)

    test_logger.trace("trace by override")

    content = test_logger.log_path.read_text(encoding="utf-8")
    assert "[TRCE]" in content
    assert "trace by override" in content


def test_logger_trace_override_can_disable_in_developer_mode(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """运行时开关应允许在开发者模式下临时关闭 TRACE 日志。"""
    test_logger = create_test_logger(tmp_path)
    monkeypatch.setattr(log_func_module, "is_developer_mode_enabled", lambda: True)
    test_logger.set_trace_logging_enabled(False)

    test_logger.trace("trace disabled by override")

    assert test_logger.log_path.read_text(encoding="utf-8") == ""


def test_install_exception_hooks_write_unhandled_exception(tmp_path: Path) -> None:
    """安装全局异常钩子后，调用 sys.excepthook 应写出 critical 日志。"""
    test_logger = create_test_logger(tmp_path)
    previous_sys_excepthook = sys.excepthook
    previous_thread_excepthook = threading.excepthook
    previous_unraisablehook = sys.unraisablehook

    try:
        test_logger.install_exception_hooks()

        try:
            raise RuntimeError("hook boom")
        except RuntimeError as exc:
            sys.excepthook(type(exc), exc, exc.__traceback__)
    finally:
        sys.excepthook = previous_sys_excepthook
        threading.excepthook = previous_thread_excepthook
        sys.unraisablehook = previous_unraisablehook
        qInstallMessageHandler(test_logger._previous_qt_message_handler)

    content = test_logger.log_path.read_text(encoding="utf-8")
    assert "捕获未处理异常" in content
    assert "[CRIT]" in content
    assert "RuntimeError: hook boom" in content


def test_qq_install_logs_exception_on_failure(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """QQ 安装失败时应调用异常日志记录。"""
    installer = QQInstall(tmp_path / "QQ.exe")
    captured: dict[str, object] = {}

    def fake_exception(message: str, exc: BaseException, *args, **kwargs) -> None:
        captured["message"] = message
        captured["exc"] = exc

    def fake_run(*args, **kwargs):
        raise OSError("spawn failed")

    monkeypatch.setattr(install_func.logger, "info", lambda *args, **kwargs: None)
    monkeypatch.setattr(install_func.logger, "error", lambda *args, **kwargs: None)
    monkeypatch.setattr(install_func.logger, "exception", fake_exception)
    monkeypatch.setattr(install_func.subprocess, "run", fake_run)

    installer.execute()

    assert captured["message"] == "安装 QQ 失败"
    assert isinstance(captured["exc"], OSError)


def test_exception_logging_application_logs_button_handler_exceptions(monkeypatch: pytest.MonkeyPatch) -> None:
    """按钮事件处理异常应由 Qt 应用异常兜底记录。"""
    app = ensure_exception_logging_app()
    captured: dict[str, object] = {}

    def fake_log_unhandled_exception(
        trigger: str,
        message: str,
        exc: BaseException,
        exc_type: type[BaseException] | None = None,
        exc_traceback=None,
        log_source=None,
    ) -> None:
        captured["trigger"] = trigger
        captured["message"] = message
        captured["exc"] = exc
        captured["exc_type"] = exc_type
        captured["log_source"] = log_source

    monkeypatch.setattr(main.logger, "log_unhandled_exception", fake_log_unhandled_exception)

    class BoomButton(QPushButton):
        def mousePressEvent(self, event) -> None:
            raise RuntimeError("notify boom")

    button = BoomButton("boom")
    button.resize(120, 40)
    button.show()

    try:
        QTest.mouseClick(button, Qt.MouseButton.LeftButton)
        app.processEvents()
    finally:
        button.close()

    assert captured["trigger"] == "qt.notify"
    assert "Qt 事件处理未捕获异常" in str(captured["message"])
    assert isinstance(captured["exc"], RuntimeError)
    assert captured["exc_type"] is RuntimeError


def test_button_slot_exception_is_written_to_log(tmp_path: Path) -> None:
    """按钮槽函数抛错时，应通过 sys.excepthook 落到日志。"""
    app = ensure_exception_logging_app()
    test_logger = create_test_logger(tmp_path)
    previous_sys_excepthook = sys.excepthook
    previous_thread_excepthook = threading.excepthook
    previous_unraisablehook = sys.unraisablehook

    try:
        test_logger.install_exception_hooks()

        button = QPushButton("slot-boom")

        def raise_in_slot() -> None:
            raise RuntimeError("slot boom")

        button.clicked.connect(raise_in_slot)
        button.resize(120, 40)
        button.show()

        try:
            QTest.mouseClick(button, Qt.MouseButton.LeftButton)
            app.processEvents()
        finally:
            button.close()
    finally:
        sys.excepthook = previous_sys_excepthook
        threading.excepthook = previous_thread_excepthook
        sys.unraisablehook = previous_unraisablehook
        qInstallMessageHandler(test_logger._previous_qt_message_handler)

    content = test_logger.log_path.read_text(encoding="utf-8")
    assert "捕获未处理异常" in content
    assert "RuntimeError: slot boom" in content


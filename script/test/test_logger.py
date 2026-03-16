# -*- coding: utf-8 -*-

# 标准库导入
import sys
import threading
from pathlib import Path

# 第三方库导入
import pytest
from PySide6.QtCore import qInstallMessageHandler

# 项目内模块导入
import src.core.utils.install_func as install_func
from src.core.utils.install_func import QQInstall
from src.core.utils.logger.log_func import Logger


def create_test_logger(tmp_path: Path) -> Logger:
    """创建仅写入临时目录的测试日志器。"""
    test_logger = Logger()
    test_logger.load_config()
    test_logger.log_path = tmp_path / "app.log"
    test_logger.log_path.write_text("", encoding="utf-8")
    return test_logger


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

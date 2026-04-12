# -*- coding: utf-8 -*-

# 标准库导入
import os
import sys
from pathlib import Path
from types import SimpleNamespace

# 第三方库导入
import pytest

# 项目内模块导入
import main
import src.desktop.core.config as config_module
import src.desktop.core.platform.single_instance as mutex_module
import src.desktop.ui.common.font as font_module


@pytest.fixture
def logger_stub(monkeypatch: pytest.MonkeyPatch) -> None:
    """屏蔽启动流程中的真实日志写入。"""
    monkeypatch.setattr(main.logger, "trace", lambda *args, **kwargs: None)
    monkeypatch.setattr(main.logger, "info", lambda *args, **kwargs: None)
    monkeypatch.setattr(main.logger, "warning", lambda *args, **kwargs: None)


def setup_runtime_dependencies(
    monkeypatch: pytest.MonkeyPatch,
    *,
    dpi_scale: object,
    show_main_window: bool,
    eula_accepted: bool,
    single_instance: bool = False,
    developer_mode: bool = False,
    exit_code: int = 0,
) -> tuple[dict[str, object], SimpleNamespace]:
    """为 run_application 注入可控依赖。"""
    calls: dict[str, object] = {}
    runtime_options = SimpleNamespace(developer_mode=developer_mode)

    monkeypatch.setattr(main, "parse_runtime_launch_options", lambda argv: (runtime_options, ["main.py", "--qt-arg"]))
    monkeypatch.setattr(main, "apply_runtime_launch_options", lambda options: calls.setdefault("applied", options))
    monkeypatch.setattr(main, "resolve_app_base_path", lambda: Path("D:/fake-app"))

    class FakeQApplication:
        @staticmethod
        def setHighDpiScaleFactorRoundingPolicy(policy) -> None:
            calls["dpi_policy"] = policy

    monkeypatch.setattr(main, "QApplication", FakeQApplication)

    class FakeExceptionLoggingApplication:
        def __init__(self, argv) -> None:
            calls["app_argv"] = list(argv)

        def exec(self) -> int:
            calls["app_exec"] = True
            return exit_code

    monkeypatch.setattr(main, "ExceptionLoggingApplication", FakeExceptionLoggingApplication)

    fake_cfg = SimpleNamespace(
        dpi_scale="dpi_scale",
        main_window="main_window",
        elua_accepted="elua_accepted",
    )
    cfg_values = {
        fake_cfg.dpi_scale: dpi_scale,
        fake_cfg.main_window: show_main_window,
        fake_cfg.elua_accepted: eula_accepted,
    }
    fake_cfg.get = lambda item: cfg_values[item]
    monkeypatch.setattr(config_module, "cfg", fake_cfg)

    class FakeSingleInstanceApplication:
        def is_running(self) -> bool:
            return single_instance

    monkeypatch.setattr(mutex_module, "SingleInstanceApplication", FakeSingleInstanceApplication)
    monkeypatch.setattr(font_module.FontManager, "initialize_fonts", staticmethod(lambda: calls.setdefault("fonts", True)))

    path_func_controller = SimpleNamespace(path_validator=lambda: calls.setdefault("path_validated", True))
    main_window_controller = SimpleNamespace(initialize=lambda: calls.setdefault("window", "main"))
    guide_window_controller = SimpleNamespace(initialize=lambda: calls.setdefault("window", "guide"))

    def fake_it(cls):
        mapping = {
            "PathFunc": path_func_controller,
            "MainWindow": main_window_controller,
            "GuideWindow": guide_window_controller,
        }
        return mapping[cls.__name__]

    monkeypatch.setattr(sys.modules["creart"], "it", fake_it)

    return calls, runtime_options


def test_run_application_enters_main_window_with_manual_dpi(
    monkeypatch: pytest.MonkeyPatch, logger_stub
) -> None:
    """主窗口分支应设置手动 DPI 并保留过滤后的 Qt 参数。"""
    calls, runtime_options = setup_runtime_dependencies(
        monkeypatch,
        dpi_scale=1.5,
        show_main_window=True,
        eula_accepted=True,
        developer_mode=True,
        exit_code=7,
    )
    monkeypatch.setattr(sys, "argv", ["main.py", "--developer-mode", "--qt-arg"])
    monkeypatch.delenv("QT_SCALE_FACTOR", raising=False)

    result = main.run_application()

    assert result == 7
    assert runtime_options is calls["applied"]
    assert sys.argv == ["main.py", "--qt-arg"]
    assert os.environ["QT_SCALE_FACTOR"] == "1.5"
    assert calls["path_validated"] is True
    assert calls["fonts"] is True
    assert calls["window"] == "main"
    assert calls["app_argv"] == ["main.py", "--qt-arg"]


def test_run_application_enters_guide_window_with_auto_dpi(
    monkeypatch: pytest.MonkeyPatch, logger_stub
) -> None:
    """引导窗口分支应在 Auto DPI 下设置 Qt rounding policy。"""
    calls, _ = setup_runtime_dependencies(
        monkeypatch,
        dpi_scale="Auto",
        show_main_window=False,
        eula_accepted=False,
        exit_code=3,
    )
    monkeypatch.setattr(sys, "argv", ["main.py"])
    monkeypatch.delenv("QT_SCALE_FACTOR", raising=False)

    result = main.run_application()

    assert result == 3
    assert "dpi_policy" in calls
    assert "QT_SCALE_FACTOR" not in os.environ
    assert calls["window"] == "guide"


def test_run_application_exits_when_existing_instance_detected(
    monkeypatch: pytest.MonkeyPatch, logger_stub
) -> None:
    """检测到已有实例时应在创建 QApplication 前直接退出。"""
    calls, _ = setup_runtime_dependencies(
        monkeypatch,
        dpi_scale="Auto",
        show_main_window=True,
        eula_accepted=True,
        single_instance=True,
    )
    monkeypatch.setattr(sys, "argv", ["main.py"])

    result = main.run_application()

    assert result == 0
    assert "app_argv" not in calls
    assert "window" not in calls


def test_main_entry_returns_run_application_exit_code(monkeypatch: pytest.MonkeyPatch, logger_stub) -> None:
    """入口函数应透传 run_application 的退出码。"""
    installed: list[bool] = []
    monkeypatch.setattr(main.logger, "install_exception_hooks", lambda: installed.append(True))
    monkeypatch.setattr(main, "run_application", lambda: 9)

    assert main.main_entry() == 9
    assert installed == [True]


def test_main_entry_emits_crash_bundle_on_unhandled_exception(
    monkeypatch: pytest.MonkeyPatch, logger_stub
) -> None:
    """入口函数遇到未处理异常时应记录日志并返回 1。"""
    critical_messages: list[str] = []
    exception_messages: list[tuple[str, str]] = []
    crash_bundles: list[tuple[str, str]] = []

    monkeypatch.setattr(main.logger, "install_exception_hooks", lambda: None)
    monkeypatch.setattr(
        main,
        "run_application",
        lambda: (_ for _ in ()).throw(RuntimeError("startup boom")),
    )
    monkeypatch.setattr(main.logger, "critical", lambda message, **kwargs: critical_messages.append(message))
    monkeypatch.setattr(
        main.logger,
        "exception",
        lambda message, exc, **kwargs: exception_messages.append((message, str(exc))),
    )
    monkeypatch.setattr(
        main.logger,
        "emit_crash_bundle",
        lambda trigger, exc, exc_type, exc_traceback: crash_bundles.append((trigger, str(exc))),
    )

    assert main.main_entry() == 1
    assert critical_messages == ["应用启动或运行过程中出现未处理异常"]
    assert exception_messages == [("应用未处理异常详情", "startup boom")]
    assert crash_bundles == [("main", "startup boom")]


def test_main_entry_reraises_system_exit(monkeypatch: pytest.MonkeyPatch, logger_stub) -> None:
    """入口函数不应吞掉 SystemExit。"""
    monkeypatch.setattr(main.logger, "install_exception_hooks", lambda: None)
    monkeypatch.setattr(
        main,
        "run_application",
        lambda: (_ for _ in ()).throw(SystemExit(5)),
    )

    with pytest.raises(SystemExit) as exc_info:
        main.main_entry()

    assert exc_info.value.code == 5


# -*- coding: utf-8 -*-
# 标准库导入
import os
import sys

from PySide6.QtCore import QEvent, QObject, Qt
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.core.utils.app_path import resolve_app_base_path
from src.core.utils.logger import LogSource, logger
from src.core.utils.runtime_args import apply_runtime_launch_options, parse_runtime_launch_options


class ExceptionLoggingApplication(QApplication):
    """为 Qt 事件分发补充异常落盘，避免 UI 点击异常直接丢失上下文。"""

    @staticmethod
    def _describe_receiver(receiver: QObject | None) -> str:
        if receiver is None:
            return "receiver=<none>"

        receiver_type = type(receiver).__name__
        object_name = receiver.objectName() if receiver.objectName() else "<anonymous>"
        return f"receiver={receiver_type}(objectName={object_name})"

    @staticmethod
    def _describe_event(event: QEvent | None) -> str:
        if event is None:
            return "event=<none>"

        event_type = event.type()
        event_name = getattr(event_type, "name", None) or str(int(event_type))
        return f"event={event_name}"

    def notify(self, receiver: QObject | None, event: QEvent | None) -> bool:
        try:
            return super().notify(receiver, event)
        except Exception as exc:
            context = ", ".join([self._describe_receiver(receiver), self._describe_event(event)])
            logger.log_unhandled_exception(
                "qt.notify",
                f"Qt 事件处理未捕获异常({context})",
                exc,
                type(exc),
                exc.__traceback__,
                log_source=LogSource.UI,
            )
            return False


def run_application() -> int:
    """启动桌面应用并返回事件循环退出码。"""
    runtime_options, filtered_argv = parse_runtime_launch_options(sys.argv)
    apply_runtime_launch_options(runtime_options)
    sys.argv[:] = filtered_argv

    runtime_mode = "frozen" if getattr(sys, "frozen", False) else "source"
    base_path = resolve_app_base_path()
    logger.info(
        (
            f"应用启动: mode={runtime_mode}, developer_mode={runtime_options.developer_mode}, "
            f"base_path={base_path}, log_file={logger.log_path}"
        ),
        log_source=LogSource.CORE,
    )
    if runtime_options.developer_mode:
        logger.warning("开发者模式已启用 (--developer-mode)", log_source=LogSource.CORE)

    # 第三方库导入
    from creart import it

    # 项目内模块导入
    from src.core.config import cfg
    from src.core.utils.mutex import SingleInstanceApplication
    from src.core.utils.path_func import PathFunc
    from src.resource import resource
    from src.ui.common.font import FontManager

    _ = resource

    # 实现单实例应用程序检查
    if SingleInstanceApplication().is_running():
        logger.warning("检测到已有实例正在运行，当前实例退出", log_source=LogSource.CORE)
        return 0

    logger.info("单实例检查通过", log_source=LogSource.CORE)

    # 执行路径验证
    logger.info("开始执行路径校验", log_source=LogSource.CORE)
    it(PathFunc).path_validator()
    logger.info("路径校验完成", log_source=LogSource.CORE)

    # 设置 DPI 缩放
    dpi_scale = cfg.get(cfg.dpi_scale)
    if dpi_scale == "Auto":
        QApplication.setHighDpiScaleFactorRoundingPolicy(Qt.HighDpiScaleFactorRoundingPolicy.PassThrough)
        logger.info("DPI 缩放策略: Auto(PassThrough)", log_source=LogSource.CORE)
    else:
        os.environ["QT_SCALE_FACTOR"] = str(dpi_scale)
        logger.info(f"DPI 缩放策略: 手动({dpi_scale})", log_source=LogSource.CORE)

    app = ExceptionLoggingApplication(sys.argv)
    logger.info("QApplication 创建完成", log_source=LogSource.UI)

    # 初始化字体
    FontManager.initialize_fonts()
    logger.info("字体初始化完成", log_source=LogSource.UI)

    if cfg.get(cfg.main_window) and cfg.get(cfg.elua_accepted):
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        logger.info("进入主窗口初始化流程", log_source=LogSource.UI)
        it(MainWindow).initialize()
    else:
        # 项目内模块导入
        from src.ui.window.guide_window import GuideWindow

        logger.info("进入引导窗口初始化流程", log_source=LogSource.UI)
        it(GuideWindow).initialize()

    exit_code = app.exec()
    logger.info(f"事件循环退出, exit_code={exit_code}", log_source=LogSource.CORE)
    return exit_code


def main_entry() -> int:
    """CLI 入口，负责安装全局异常钩子并返回进程退出码。"""
    logger.install_exception_hooks()

    try:
        return run_application()
    except SystemExit:
        raise
    except Exception as exc:
        logger.critical("应用启动或运行过程中出现未处理异常", log_source=LogSource.CORE)
        logger.exception("应用未处理异常详情", exc, log_source=LogSource.CORE)
        logger.emit_crash_bundle("main", exc, type(exc), exc.__traceback__)
        return 1


if __name__ == "__main__":
    sys.exit(main_entry())

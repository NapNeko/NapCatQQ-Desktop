# -*- coding: utf-8 -*-
# 标准库导入
import sys
import threading
import traceback
from contextlib import contextmanager
from datetime import datetime
from pathlib import Path

from PySide6.QtCore import QtMsgType, qInstallMessageHandler

# 项目内模块导入
from src.core.utils.app_path import resolve_app_base_path
from src.core.utils.logger.crash_bundle import (
    CrashBundlePayload,
    build_crash_bundle,
    resolve_desktop_output_dir,
)
from src.core.utils.logger.log_data import Log, LogGroup, LogPosition
from src.core.utils.logger.log_enum import LogLevel, LogSource, LogType
from src.core.utils.logger.log_utils import capture_call_location


class Logger:
    """NCD 内部日志记录器"""

    log_buffer: list[Log | LogGroup]

    log_buffer_size: int
    log_buffer_delete_size: int
    log_save_day: int
    log_path: Path

    def __init__(self) -> None:
        """初始化日志记录器"""
        # Log 缓冲区
        self.log_buffer = []
        self._hooks_installed = False
        self._previous_qt_message_handler = None
        self._crash_bundle_lock = threading.Lock()
        self._crash_bundle_path: Path | None = None
        self._crash_bundle_active = False

    def load_config(self) -> None:
        """加载配置项"""
        self.log_buffer_size = 5000  # 日志缓冲区大小
        self.log_buffer_delete_size = 1000  # 删除缓冲区日志数量
        self.log_save_day = 7  # 日志保存天数

    def create_log_file(self) -> None:
        """
        ## 用于创建日志文件
            - 日志文件名格式为: {DATETIME}.log
        """

        # 定义日志文件路径
        if not (log_dir := resolve_app_base_path() / "log").exists():
            log_dir.mkdir(parents=True, exist_ok=True)
        self.log_path = log_dir / f"{datetime.now().strftime('%Y-%m-%d_%H-%M-%S')}.log"

        # 遍历日志文件夹, 删除过期日志文件(超过 7 天)
        for log_file in log_dir.iterdir():
            if not log_file.is_file():
                continue

            if (datetime.now() - datetime.fromtimestamp(log_file.stat().st_mtime)).days > self.log_save_day:
                log_file.unlink()

    def clear_buffer(self) -> None:
        """清理日志缓冲区

        当日志列表长度超过 日志缓冲区大小 时, 并根据 删除缓冲区日志数量 清理缓冲区
        """
        if len(self.log_buffer) >= self.log_buffer_size:
            self.log_buffer = self.log_buffer[self.log_buffer_delete_size :]

    def _format_exception_message(
        self,
        message: str,
        exc: BaseException,
        exc_type: type[BaseException] | None = None,
        exc_traceback=None,
    ) -> str:
        """构造包含异常栈的日志消息。"""
        resolved_type = exc_type or type(exc)
        details = f"{message}: {resolved_type.__name__}: {exc}"
        trace_text = "".join(traceback.format_exception(resolved_type, exc, exc_traceback or exc.__traceback__)).strip()

        if trace_text:
            return f"{details}\n{trace_text}"

        return details

    def _log(
        self,
        level: LogLevel,
        message: str,
        time: int | float,
        log_type: LogType,
        log_source: LogSource,
        log_position: LogPosition | None,
        log_group: LogGroup | None = None,
    ):
        """构造 Log 对象并添加到日志缓冲区

        Args:
            level (LogLevel): 日志等级
            message (str): 日志消息
            time (int | float): 日志时间戳
            log_type (LogType): 日志类型
            log_source (LogSource): 日志来源
            log_position (LogPosition | None): 日志位置
            log_group (LogGroup | None): 日志组，可为 None。
        """
        # 构造 Log
        log = Log(level, message, time, log_type, log_source, log_position)

        if log_group:
            # 如果提供了 log_group，将日志添加到它的内部
            log_group.add(log)
        else:
            # 否则直接添加到 log_buffer
            self.log_buffer.append(log)

        # 遍历日志列表, 追加到日志文件中
        with open(self.log_path, "a", encoding="utf-8") as f:
            f.write(log.to_string() + "\n")
        # 判断是否需要清理缓冲区
        self.clear_buffer()
        # 打印 log
        print(log)

    @capture_call_location
    def debug(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition | None = None,
        log_group: LogGroup | None = None,
    ):
        self._log(LogLevel.DBUG, message, datetime.now().timestamp(), log_type, log_source, log_position, log_group)

    @capture_call_location
    def info(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition | None = None,
        log_group: LogGroup | None = None,
    ):
        self._log(LogLevel.INFO, message, datetime.now().timestamp(), log_type, log_source, log_position, log_group)

    @capture_call_location
    def warning(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition | None = None,
        log_group: LogGroup | None = None,
    ):
        self._log(LogLevel.WARN, message, datetime.now().timestamp(), log_type, log_source, log_position, log_group)

    @capture_call_location
    def error(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition | None = None,
        log_group: LogGroup | None = None,
    ):
        self._log(LogLevel.EROR, message, datetime.now().timestamp(), log_type, log_source, log_position, log_group)

    @capture_call_location
    def critical(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition | None = None,
        log_group: LogGroup | None = None,
    ):
        self._log(LogLevel.CRIT, message, datetime.now().timestamp(), log_type, log_source, log_position, log_group)

    @capture_call_location
    def exception(
        self,
        message: str,
        exc: BaseException,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition | None = None,
        log_group: LogGroup | None = None,
    ):
        self._log(
            LogLevel.EROR,
            self._format_exception_message(message, exc),
            datetime.now().timestamp(),
            log_type,
            log_source,
            log_position,
            log_group,
        )

    def _handle_sys_exception(self, exc_type: type[BaseException], exc: BaseException, exc_traceback) -> None:
        """处理主线程未捕获异常。"""
        if issubclass(exc_type, KeyboardInterrupt):
            return

        self.log_unhandled_exception(
            "sys.excepthook",
            "捕获未处理异常",
            exc,
            exc_type,
            exc_traceback,
            log_source=LogSource.CORE,
        )

    def _handle_thread_exception(self, args: threading.ExceptHookArgs) -> None:
        """处理线程未捕获异常。"""
        if args.exc_type is None or args.exc_value is None:
            return

        thread_name = args.thread.name if args.thread is not None else "unknown"
        self.log_unhandled_exception(
            "threading.excepthook",
            f"线程未处理异常(thread={thread_name})",
            args.exc_value,
            args.exc_type,
            args.exc_traceback,
            log_source=LogSource.CORE,
        )

    def _handle_unraisable_exception(self, args) -> None:
        """处理无法传播到调用方的异常。"""
        if args.exc_value is None:
            return

        object_repr = repr(args.object) if getattr(args, "object", None) is not None else "unknown"
        message = f"捕获无法传播的异常(object={object_repr})"
        if getattr(args, "err_msg", None):
            message = f"{message}: {args.err_msg}"

        self.log_unhandled_exception(
            "sys.unraisablehook",
            message,
            args.exc_value,
            args.exc_type,
            args.exc_traceback,
            log_source=LogSource.CORE,
        )

    def log_unhandled_exception(
        self,
        trigger: str,
        message: str,
        exc: BaseException,
        exc_type: type[BaseException] | None = None,
        exc_traceback=None,
        log_source: LogSource = LogSource.CORE,
    ) -> None:
        """统一记录未处理异常并生成诊断包。"""
        self._log(
            LogLevel.CRIT,
            self._format_exception_message(message, exc, exc_type, exc_traceback),
            datetime.now().timestamp(),
            LogType.NONE_TYPE,
            log_source,
            None,
        )
        self.emit_crash_bundle(trigger, exc, exc_type, exc_traceback)

    def _handle_qt_message(self, msg_type, context, message: str) -> None:
        """接管 Qt 的 warning/critical/fatal 日志。"""
        level_mapping = {
            QtMsgType.QtDebugMsg: LogLevel.DBUG,
            QtMsgType.QtInfoMsg: LogLevel.INFO,
            QtMsgType.QtWarningMsg: LogLevel.WARN,
            QtMsgType.QtCriticalMsg: LogLevel.EROR,
            QtMsgType.QtFatalMsg: LogLevel.CRIT,
        }
        level = level_mapping.get(msg_type, LogLevel.INFO)

        file_name = Path(context.file).name if getattr(context, "file", None) else "<qt>"
        position = LogPosition(module=getattr(context, "category", "") or "qt", file=file_name, line=context.line or 0)
        self._log(
            level,
            f"Qt message: {message}",
            datetime.now().timestamp(),
            LogType.NONE_TYPE,
            LogSource.UI,
            position,
        )
        if msg_type == QtMsgType.QtFatalMsg:
            self.emit_crash_bundle(
                "qt.fatal",
                RuntimeError(f"Qt fatal message: {message}"),
                RuntimeError,
                None,
            )

    def emit_crash_bundle(
        self,
        trigger: str,
        exc: BaseException | None,
        exc_type: type[BaseException] | None = None,
        exc_traceback=None,
    ) -> Path | None:
        """生成一次性的桌面崩溃诊断包。"""
        return self._emit_crash_bundle(trigger, exc, exc_type, exc_traceback, remember_bundle=True)

    def emit_test_crash_bundle(self) -> Path | None:
        """生成一个不影响正式崩溃去重状态的测试诊断包。"""
        try:
            raise RuntimeError("Developer mode manual crash bundle export")
        except RuntimeError as exc:
            return self._emit_crash_bundle(
                "developer.manual",
                exc,
                type(exc),
                exc.__traceback__,
                remember_bundle=False,
            )

    def _emit_crash_bundle(
        self,
        trigger: str,
        exc: BaseException | None,
        exc_type: type[BaseException] | None = None,
        exc_traceback=None,
        remember_bundle: bool = True,
    ) -> Path | None:
        """生成崩溃诊断包，并按需记录首次崩溃产物。"""
        with self._crash_bundle_lock:
            if remember_bundle and self._crash_bundle_path is not None:
                return self._crash_bundle_path
            if self._crash_bundle_active:
                return None
            self._crash_bundle_active = True

        try:
            resolved_exc_type = exc_type or (type(exc) if exc is not None else RuntimeError)
            resolved_exc = exc or RuntimeError(f"Crash trigger: {trigger}")
            traceback_text = "".join(
                traceback.format_exception(resolved_exc_type, resolved_exc, exc_traceback or resolved_exc.__traceback__)
            ).strip()
            output_dir, output_source = resolve_desktop_output_dir(resolve_app_base_path())
            payload = CrashBundlePayload(
                trigger=trigger,
                created_at=datetime.now(),
                exception_type=resolved_exc_type.__name__,
                exception_message=str(resolved_exc),
                traceback_text=traceback_text,
                log_path=self.log_path,
                base_path=resolve_app_base_path(),
            )
            bundle_path = build_crash_bundle(output_dir, payload)
            if remember_bundle:
                self._crash_bundle_path = bundle_path

            level = LogLevel.INFO if output_source == "desktop" else LogLevel.WARN
            self._log(
                level,
                f"已生成崩溃诊断包: {bundle_path} (trigger={trigger}, output_source={output_source})",
                datetime.now().timestamp(),
                LogType.FILE_FUNC,
                LogSource.CORE,
                None,
            )
            return bundle_path
        except Exception as bundle_exc:
            self._log(
                LogLevel.EROR,
                f"生成崩溃诊断包失败: {type(bundle_exc).__name__}: {bundle_exc}",
                datetime.now().timestamp(),
                LogType.FILE_FUNC,
                LogSource.CORE,
                None,
            )
            return None
        finally:
            with self._crash_bundle_lock:
                self._crash_bundle_active = False

    def install_exception_hooks(self) -> None:
        """安装 Python 和 Qt 的全局异常日志钩子。"""
        if self._hooks_installed:
            return

        sys.excepthook = self._handle_sys_exception
        threading.excepthook = self._handle_thread_exception
        sys.unraisablehook = self._handle_unraisable_exception
        self._previous_qt_message_handler = qInstallMessageHandler(self._handle_qt_message)
        self._hooks_installed = True
        self.info("已安装全局异常和 Qt 日志钩子", log_source=LogSource.CORE)

    @contextmanager
    def group(self, name: str, log_type: LogType, log_source: LogSource):
        """创建一个带有开始/结束标记的逻辑日志组"""
        log_group = LogGroup(name, log_type, log_source)
        try:
            self.info(
                f"{'-' * 20} > {name} 开始 < {'-' * 20}", log_type=log_type, log_source=log_source, log_group=log_group
            )
            yield log_group
        finally:
            self.info(
                f"{'-' * 20} > {name} 结束 < {'-' * 20}", log_type=log_type, log_source=log_source, log_group=log_group
            )
            self.log_buffer.append(log_group)


# 实例化日志记录器
logger = Logger()
logger.load_config()
logger.create_log_file()

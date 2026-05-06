# -*- coding: utf-8 -*-
"""[`BotMigrationService`](src/core/operation/migration.py): Bot 运行位置迁移服务 (P3.W3.B).

把一个 Bot 在不同 ``runtime_target`` (本地 ↔ 远端, 远端 A ↔ 远端 B) 之间搬运:

1. 通过 [`resolve_backend_for_bot`](src/core/operation/resolver.py) 解析源/目标 backend
2. 列出源端 config 目录下所有 ``*_<qq_id>.json`` 配置文件
3. 把它们逐一复制到目标端 (通过 ``backend.read_file`` / ``backend.write_file``)
4. 删除源端配置文件 (避免遗留陈旧数据干扰下次切换)

设计要点:
- **不直接管 Bot 进程**: 停止源端 Bot 的责任在 UI 层 (主线程通过
  [`ManagerNapCatQQProcess.stop_process`](src/core/runtime/napcat.py) 完成),
  service 只关心 backend 之间的文件搬运
- **不依赖 update_config**: 调用方应在 service 之前/之后调用
  [`update_config`](src/core/config/operate_config.py) 完成 ``bot.json`` 的字段更新,
  service 自己只搬运 NapCat 派生配置 (onebot11/napcat JSON)
- **同步语义**: 暴露同步 ``execute(plan)``, Qt 信号桥接由
  [`BotMigrationRunnable`](src/core/operation/migration.py) 完成,
  避免 service 与 GUI 框架耦合
- **MVP 范围**: 仅迁移 NapCat 配置文件; NapCat 持久数据 (cache/database)
  路径未标准化, 留到 P4 阶段评估
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Callable

from PySide6.QtCore import QObject, QRunnable, Signal

from src.core.config.config_model import RUNTIME_TARGET_LOCAL
from src.core.logging import LogSource, LogType, logger

from .resolver import BackendResolutionError, resolve_backend_for_bot

if TYPE_CHECKING:
    from src.core.config.config_model import BotConfig

    from .backend import OperationBackend


# 进度回调签名: (message, percent_0_to_100)
ProgressCallback = Callable[[str, int], None]


@dataclass(slots=True)
class MigrationPlan:
    """迁移计划数据.

    Attributes:
        qq_id: 待迁移 Bot 的 QQ 号 (字符串形式, 与
            [`BotConfig.QQID`](src/core/config/config_model.py) 一致)
        source_target: 源端 ``runtime_target`` (``"local"`` 或 ``server_id``)
        dest_target: 目标端 ``runtime_target``
        move_persistent_data: 是否搬运 config 目录下与 ``<qq_id>`` 相关的
            非 onebot11/napcat 文件; MVP 阶段保留为 future flag,
            实际只用于在 UI 上传达用户意图
    """

    qq_id: str
    source_target: str
    dest_target: str
    move_persistent_data: bool = False

    def validate(self) -> None:
        """校验计划合法性."""
        if not self.qq_id or not self.qq_id.strip():
            raise ValueError("迁移计划 qq_id 不能为空")
        if not self.source_target or not self.dest_target:
            raise ValueError("迁移计划 source_target / dest_target 不能为空")
        if self.source_target == self.dest_target:
            raise ValueError(
                f"迁移计划源/目标相同, 无需迁移: target={self.source_target}"
            )


class BotMigrationError(RuntimeError):
    """Bot 迁移过程的统一错误类型.

    包含 ``stage`` 字段标记失败发生在哪个阶段, 便于上层日志与回滚定位.
    """

    def __init__(self, message: str, *, stage: str = "unknown") -> None:
        super().__init__(message)
        self.stage = stage


# 不允许迁移的特殊文件名 (例如全局 webui / config.json 等); 仅迁移 ``*_<qq>.json``
# 命名约定与 [`RemoteBackend.write_bot_runtime_config`](src/core/operation/remote_backend.py) 对齐
_QQ_BOUND_CONFIG_PATTERN = re.compile(r"^(onebot11|napcat)_(?P<qq>\d{4,12})\.json$")


def _build_qq_specific_pattern(qq_id: str) -> re.Pattern[str]:
    """构造仅匹配指定 qq_id 的 ``*_<qq>.json`` 正则."""
    safe_qq = re.escape(qq_id)
    return re.compile(rf"^(onebot11|napcat)_{safe_qq}\.json$")


class BotMigrationService(QObject):
    """Bot 运行位置迁移服务.

    Qt 信号:
        progress_signal (str, int): (人类可读消息, 0-100 进度百分比)
        finished_signal (bool, str): (是否成功, 完成消息)

    用法::

        service = BotMigrationService()
        service.progress_signal.connect(on_progress)
        service.finished_signal.connect(on_done)
        # 同步执行 (应在 QThreadPool worker 线程调用; UI 线程会被阻塞)
        service.execute(plan)
    """

    progress_signal = Signal(str, int)
    finished_signal = Signal(bool, str)

    def execute(self, plan: MigrationPlan) -> None:
        """**同步**执行迁移 (应在 [`QRunnable`](https://doc.qt.io/qt-6/qrunnable.html) 中调用).

        Raises:
            BotMigrationError: 任何阶段失败时抛出, 同时 emit ``finished_signal(False, ...)``
        """
        try:
            plan.validate()
        except ValueError as exc:
            self._emit_finished(False, f"迁移计划无效: {exc}")
            raise BotMigrationError(str(exc), stage="validate") from exc

        logger.info(
            (
                "开始迁移 Bot 配置: "
                f"qq_id={plan.qq_id}, source={plan.source_target}, "
                f"dest={plan.dest_target}, move_persistent_data={plan.move_persistent_data}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

        try:
            self._emit_progress("解析源端 backend", 5)
            source_backend = self._resolve_backend(plan.source_target)
            self._emit_progress("解析目标端 backend", 10)
            dest_backend = self._resolve_backend(plan.dest_target)

            # 远端 backend 需要先 connect (本地 backend connect 是 no-op)
            self._emit_progress("连接 backend", 15)
            self._safe_connect(source_backend)
            self._safe_connect(dest_backend)

            self._emit_progress("枚举源端配置文件", 25)
            files_to_migrate = self._list_qq_config_files(source_backend, plan.qq_id)
            if not files_to_migrate:
                logger.info(
                    f"源端无 qq_id={plan.qq_id} 相关配置文件需迁移; 仅切换 runtime_target",
                    LogType.NETWORK,
                    LogSource.CORE,
                )

            total = max(1, len(files_to_migrate))
            for idx, filename in enumerate(files_to_migrate, start=1):
                pct_base = 30 + int(50 * (idx - 1) / total)
                self._emit_progress(f"复制 {filename}", pct_base)
                self._copy_config_file(source_backend, dest_backend, filename)

            self._emit_progress("清理源端旧配置", 85)
            for filename in files_to_migrate:
                self._delete_config_file(source_backend, filename)

            self._emit_progress("迁移完成", 100)
            ok_msg = (
                f"已迁移 {len(files_to_migrate)} 个配置文件: "
                f"{plan.source_target} → {plan.dest_target}"
            )
            self._emit_finished(True, ok_msg)
            logger.info(ok_msg, LogType.NETWORK, LogSource.CORE)
        except BotMigrationError as exc:
            # 内部步骤 (connect / copy / cleanup) 抛 BotMigrationError 时, 外层尚未 emit finished
            # 需要在此补 emit, 让 UI 能感知失败 (与外层 ``except Exception`` 分支语义一致)
            err_msg = f"迁移失败 [{exc.stage}]: {exc}"
            self._emit_finished(False, err_msg)
            logger.warning(err_msg, LogType.NETWORK, LogSource.CORE)
            raise
        except BackendResolutionError as exc:
            err_msg = f"backend 解析失败: {exc}"
            self._emit_finished(False, err_msg)
            logger.warning(err_msg, LogType.NETWORK, LogSource.CORE)
            raise BotMigrationError(err_msg, stage="resolve") from exc
        except Exception as exc:  # noqa: BLE001 - 统一封装为 BotMigrationError
            err_msg = f"迁移失败: {type(exc).__name__}: {exc}"
            self._emit_finished(False, err_msg)
            logger.warning(err_msg, LogType.NETWORK, LogSource.CORE)
            raise BotMigrationError(err_msg, stage="execute") from exc

    # ==================== 内部 ====================
    def _resolve_backend(self, target: str) -> "OperationBackend":
        """生成最小化的 ``BotConfig``-like 对象供 resolver 路由."""

        class _BotShim:
            __slots__ = ("runtime_target",)

            def __init__(self, target: str) -> None:
                self.runtime_target = target

        return resolve_backend_for_bot(_BotShim(target))  # type: ignore[arg-type]

    @staticmethod
    def _safe_connect(backend: "OperationBackend") -> None:
        """connect 失败时包装成 BotMigrationError."""
        try:
            backend.connect()
        except Exception as exc:  # noqa: BLE001 - paramiko / 文件系统 异常多种
            raise BotMigrationError(
                f"backend connect 失败: {type(exc).__name__}: {exc}",
                stage="connect",
            ) from exc

    def _list_qq_config_files(
        self, backend: "OperationBackend", qq_id: str
    ) -> list[str]:
        """列出指定 backend 上 config_dir 下与 qq_id 相关的 ``*_<qq>.json`` 文件名."""
        config_dir = self._config_dir_for(backend)
        if not backend.file_exists(config_dir):
            return []

        try:
            entries = backend.list_dir(config_dir)
        except FileNotFoundError:
            return []

        pattern = _build_qq_specific_pattern(qq_id)
        return [entry.name for entry in entries if not entry.is_dir and pattern.match(entry.name)]

    def _copy_config_file(
        self,
        source_backend: "OperationBackend",
        dest_backend: "OperationBackend",
        filename: str,
    ) -> None:
        """通过 ``read_file`` / ``write_file`` 把单个配置文件从源端复制到目标端."""
        source_path = self._join_config_path(source_backend, filename)
        dest_path = self._join_config_path(dest_backend, filename)
        try:
            content = source_backend.read_file(source_path)
        except Exception as exc:  # noqa: BLE001
            raise BotMigrationError(
                f"读取源端配置失败: {filename}: {type(exc).__name__}: {exc}",
                stage="copy_read",
            ) from exc
        try:
            dest_backend.write_file(dest_path, content)
        except Exception as exc:  # noqa: BLE001
            raise BotMigrationError(
                f"写入目标端配置失败: {filename}: {type(exc).__name__}: {exc}",
                stage="copy_write",
            ) from exc

    def _delete_config_file(
        self, backend: "OperationBackend", filename: str
    ) -> None:
        """删除源端配置文件; 不存在时静默跳过, 仅记录 warning."""
        path = self._join_config_path(backend, filename)
        try:
            if backend.file_exists(path):
                backend.remove(path)
        except Exception as exc:  # noqa: BLE001 - 清理失败不阻断, 但需要 warning
            logger.warning(
                f"清理源端配置失败 (已迁移完成, 不影响目标端): {filename}: "
                f"{type(exc).__name__}: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )

    @staticmethod
    def _config_dir_for(backend: "OperationBackend") -> str:
        """返回该 backend 的 NapCat 配置目录路径 (字符串).

        - LocalBackend: ``PathFunc.napcat_config_path`` (Windows 风格)
        - RemoteBackend: ``LinuxCorePaths.config_dir`` (POSIX 风格, 含 ``$HOME``)
        """
        # 延迟导入避免循环
        from .local_backend import LocalBackend
        from .remote_backend import RemoteBackend

        if isinstance(backend, LocalBackend):
            from creart import it

            from src.core.runtime.paths import PathFunc

            return str(it(PathFunc).napcat_config_path)
        if isinstance(backend, RemoteBackend):
            return backend.paths.config_dir
        raise BotMigrationError(
            f"不支持的 backend 类型: {type(backend).__name__}",
            stage="config_dir",
        )

    @staticmethod
    def _join_config_path(backend: "OperationBackend", filename: str) -> str:
        """把 ``filename`` 拼到 backend 的 config_dir 后."""
        config_dir = BotMigrationService._config_dir_for(backend)
        # 本地用 Path 风格自然拼接, 远端用 POSIX 风格
        from .local_backend import LocalBackend

        if isinstance(backend, LocalBackend):
            from pathlib import Path as _Path

            return str(_Path(config_dir) / filename)
        # POSIX
        if config_dir.endswith("/"):
            return f"{config_dir}{filename}"
        return f"{config_dir}/{filename}"

    def _emit_progress(self, message: str, percent: int) -> None:
        self.progress_signal.emit(message, max(0, min(100, percent)))

    def _emit_finished(self, ok: bool, message: str) -> None:
        self.finished_signal.emit(ok, message)


def derive_plan_from_bot_config(
    *,
    qq_id: str,
    old_target: str,
    new_target: str,
    move_persistent_data: bool = False,
) -> MigrationPlan:
    """从旧/新 ``runtime_target`` 构造 [`MigrationPlan`](src/core/operation/migration.py).

    便利函数, 替 UI 层屏蔽 ``RUNTIME_TARGET_LOCAL`` 常量细节.
    """
    return MigrationPlan(
        qq_id=str(qq_id),
        source_target=old_target or RUNTIME_TARGET_LOCAL,
        dest_target=new_target or RUNTIME_TARGET_LOCAL,
        move_persistent_data=move_persistent_data,
    )


# ==================== Qt QRunnable wrapper ====================
class BotMigrationRunnableSignals(QObject):
    """[`BotMigrationRunnable`](src/core/operation/migration.py) 信号载体."""

    progress = Signal(str, int)  # message, percent
    finished = Signal(bool, str)  # ok, message


class BotMigrationRunnable(QRunnable):
    """把 [`BotMigrationService.execute`](src/core/operation/migration.py) 推到
    [`QThreadPool`](https://doc.qt.io/qt-6/qthreadpool.html) 后台线程.

    UI 端通过 ``signals.progress`` / ``signals.finished`` 订阅状态.
    """

    def __init__(self, plan: MigrationPlan) -> None:
        super().__init__()
        self.signals = BotMigrationRunnableSignals()
        self._plan = plan
        self.setAutoDelete(True)

    def run(self) -> None:  # noqa: D401 - 实现 QRunnable.run
        service = BotMigrationService()
        # 把 service 的同步信号桥接到 runnable 的信号 (跨线程)
        service.progress_signal.connect(self.signals.progress.emit)
        service.finished_signal.connect(self.signals.finished.emit)

        # 捕获 service.finished_signal 的 (success, message), 给 BackgroundTaskCenter 用
        result: dict[str, object] = {"success": False, "message": ""}

        def _capture_result(success: bool, message: str) -> None:
            result["success"] = bool(success)
            result["message"] = message or ""

        service.finished_signal.connect(_capture_result)

        # P3 perf: 上报到全局 BackgroundTaskCenter, 完成时把 success/message 传给
        # ProgressInfoBar 桥, ✅/❌ + 文案自动展示. 任意原因失败仍要 end, 用 try/finally 兜底.
        task_id = f"bot-migration-{self._plan.qq_id}"
        center = None
        try:
            from creart import it
            from src.core.runtime.background_tasks import BackgroundTaskCenter

            center = it(BackgroundTaskCenter)
            center.begin(
                task_id,
                f"迁移 Bot {self._plan.qq_id} ({self._plan.source_target} → {self._plan.dest_target})",
                content="正在搬运数据并切换 runtime_target…",
            )
        except Exception:  # noqa: BLE001 - center 不可用时不阻断主流程
            center = None

        try:
            service.execute(self._plan)
        except BotMigrationError as exc:
            # service.execute 已经 emit 过 finished(False), 这里仅记录
            logger.warning(
                f"BotMigrationRunnable 捕获迁移异常: stage={exc.stage}, msg={exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
        except Exception as exc:  # noqa: BLE001 - 兜底, 避免 worker 静默
            logger.warning(
                f"BotMigrationRunnable 未预期异常: {type(exc).__name__}: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            self.signals.finished.emit(False, f"迁移异常: {type(exc).__name__}: {exc}")
        finally:
            if center is not None:
                try:
                    success = bool(result.get("success"))
                    message = str(result.get("message") or "")
                    if success:
                        center.end(
                            task_id,
                            success=True,
                            message=message or f"Bot {self._plan.qq_id} 迁移完成",
                        )
                    else:
                        center.fail(task_id, message or "迁移失败")
                except Exception:  # noqa: BLE001
                    pass

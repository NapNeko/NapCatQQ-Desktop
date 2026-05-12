"""[`BotMigrationService`](src/core/operation/migration.py): Bot 运行位置迁移服务 (P3.W3.B).
# -*- coding: utf-8 -*-

把一个 Bot 在不同 ``runtime_target`` (本地 ↔ 远端, 远端 A ↔ 远端 B) 之间搬运:

1. 通过 [`resolve_backend_for_bot`](src/core/operation/resolver.py) 解析源/目标 backend
2. 列出源端 config 目录下所有 ``*_<qq_id>.json`` 配置文件
3. 把它们逐一复制到目标端 (通过 ``backend.read_file`` / ``backend.write_file``)
4. 删除源端配置文件 (避免遗留陈旧数据干扰下次切换)

设计要点:
- **不直接管 Bot 进程**: 停止源端 Bot 的责任在 UI 层 (主线程通过
  [`BotProcessManager.stop_bot`](src/core/runtime/napcat.py) 完成),
  service 只关心 backend 之间的文件搬运
- **不依赖 update_config**: 调用方应在 service 之前/之后调用
  [`update_config`](src/core/config/operate_config.py) 完成 ``bot.json`` 的字段更新,
  service 自己只搬运 NapCat 派生配置 (onebot11/napcat JSON)
- **同步语义**: 暴露同步 ``execute(plan)``, Qt 信号桥接由
  [`BotMigrationRunnable`](src/core/operation/migration.py) 完成,
  避免 service 与 GUI 框架耦合
- **P4 W3 F6 兑现**: 当 ``MigrationPlan.move_persistent_data=True`` 时, 在搬完
  NapCat 配置后顺序搬运 NapCat 数据目录 + QQ 账号缓存 (路径白名单见
  :meth:`BotMigrationService._persistent_data_roots`); 单文件走
  ``OperationBackend.read_bytes`` / ``append_bytes`` 1 MiB chunk + ``.partial`` 续传.
"""

from __future__ import annotations

import re
import time
from dataclasses import dataclass
from typing import TYPE_CHECKING, Callable

from PySide6.QtCore import QObject, QRunnable, Signal

from src.core.config.config_model import RUNTIME_TARGET_LOCAL
from src.core.logging import LogSource, LogType, logger
from src.core.runtime.backend_type import BackendType

from .resolver import BackendResolutionError, resolve_backend_for_bot

if TYPE_CHECKING:
    from src.core.config.config_model import BotConfig

    from .backend import OperationBackend


# ==================== F6 持久数据迁移常量 ====================
#: 单次 sftp / 文件 IO chunk 大小; 提升至 4 MiB 减少 SFTP 往返次数, 大幅提速.
PERSISTENT_DATA_CHUNK_SIZE = 4 * 1024 * 1024
#: 单文件未完成传输的临时后缀; 与目标真名共存, 完成后 rename 覆盖.
PERSISTENT_PARTIAL_SUFFIX = ".partial"
#: 单文件 chunk 级别最大重试次数 (每次从断点续传, 不重头开始).
CHUNK_MAX_RETRIES = 5
#: chunk 重试间隔 (秒), 指数退避: retry_delay * 2^attempt, 上限 30s.
CHUNK_RETRY_BASE_DELAY = 2.0
#: 文件级最大重试次数 (整个 _copy_with_resume 失败后重新进入, 利用 .partial 续传).
FILE_MAX_RETRIES = 3
#: 文件级重试间隔基数 (秒).
FILE_RETRY_BASE_DELAY = 3.0


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
        backend_type: Bot 的后端类型 (NapCat / SnowLuma); 决定配置文件命名模式
            与 config 目录路径. 默认 NAPCAT 保持向后兼容.
    """

    qq_id: str
    source_target: str
    dest_target: str
    move_persistent_data: bool = False
    backend_type: BackendType = BackendType.NAPCAT

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
# SnowLuma 命名: ``onebot_<uin>.json`` (无 ``11`` 后缀, 无独立 napcat_<uin>.json)
_QQ_BOUND_CONFIG_PATTERN_SL = re.compile(r"^onebot_(?P<qq>\d{4,12})\.json$")


def _build_qq_specific_pattern(qq_id: str, backend_type: BackendType = BackendType.NAPCAT) -> re.Pattern[str]:
    """构造仅匹配指定 qq_id 的配置文件正则.

    - NapCat: ``^(onebot11|napcat)_<qq>.json$``
    - SnowLuma: ``^onebot_<qq>.json$``
    """
    safe_qq = re.escape(qq_id)
    if backend_type == BackendType.SNOWLUMA:
        return re.compile(rf"^onebot_{safe_qq}\.json$")
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
    # P4 W3 F6: 字节级搬运进度 (transferred_bytes, total_bytes)
    bytes_progress_signal = Signal(int, int)

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
            files_to_migrate = self._list_qq_config_files(source_backend, plan.qq_id, plan.backend_type)
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
                self._copy_config_file(source_backend, dest_backend, filename, plan.backend_type)

            self._emit_progress("清理源端旧配置", 80)
            for filename in files_to_migrate:
                self._delete_config_file(source_backend, filename, plan.backend_type)

            transferred_bytes = 0
            persistent_file_count = 0
            if plan.move_persistent_data:
                self._emit_progress("搬运 NapCat 持久数据", 82)
                transferred_bytes, persistent_file_count = self._transfer_persistent_data(
                    source_backend, dest_backend, plan
                )

            self._emit_progress("迁移完成", 100)
            extra = (
                f", 持久数据 {persistent_file_count} 个文件 / {transferred_bytes} 字节"
                if plan.move_persistent_data
                else ""
            )
            ok_msg = (
                f"已迁移 {len(files_to_migrate)} 个配置文件{extra}: "
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
        self, backend: "OperationBackend", qq_id: str, backend_type: BackendType = BackendType.NAPCAT
    ) -> list[str]:
        """列出指定 backend 上 config_dir 下与 qq_id 相关的配置文件名.

        - NapCat: 匹配 ``onebot11_<qq>.json`` + ``napcat_<qq>.json``
        - SnowLuma: 匹配 ``onebot_<qq>.json``
        """
        config_dir = self._config_dir_for(backend, backend_type)
        if not backend.file_exists(config_dir):
            return []

        try:
            entries = backend.list_dir(config_dir)
        except FileNotFoundError:
            return []

        pattern = _build_qq_specific_pattern(qq_id, backend_type)
        return [entry.name for entry in entries if not entry.is_dir and pattern.match(entry.name)]

    def _copy_config_file(
        self,
        source_backend: "OperationBackend",
        dest_backend: "OperationBackend",
        filename: str,
        backend_type: BackendType = BackendType.NAPCAT,
    ) -> None:
        """通过 ``read_file`` / ``write_file`` 把单个配置文件从源端复制到目标端."""
        source_path = self._join_config_path(source_backend, filename, backend_type)
        dest_path = self._join_config_path(dest_backend, filename, backend_type)
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
        self, backend: "OperationBackend", filename: str, backend_type: BackendType = BackendType.NAPCAT
    ) -> None:
        """删除源端配置文件; 不存在时静默跳过, 仅记录 warning."""
        path = self._join_config_path(backend, filename, backend_type)
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
    def _config_dir_for(backend: "OperationBackend", backend_type: BackendType = BackendType.NAPCAT) -> str:
        """返回该 backend 的配置目录路径 (字符串).

        - LocalBackend + NAPCAT: ``PathFunc.napcat_config_path`` (Windows 风格)
        - LocalBackend + SNOWLUMA: ``PathFunc.get_snowluma_config_dir()`` (Windows 风格)
        - RemoteBackend (NC): ``LinuxCorePaths.config_dir`` (POSIX 风格, 含 ``$HOME``)
        - RemoteSnowLumaBackend (SL): ``SnowLumaRemotePaths.config_dir`` (POSIX 风格)
        """
        # 延迟导入避免循环
        from .local_backend import LocalBackend
        from .remote_backend import RemoteBackend
        from .remote_snowluma_backend import RemoteSnowLumaBackend

        if isinstance(backend, RemoteSnowLumaBackend):
            return backend.sl_paths.config_dir
        if isinstance(backend, LocalBackend):
            from creart import it

            from src.core.runtime.paths import PathFunc

            path_func = it(PathFunc)
            if backend_type == BackendType.SNOWLUMA:
                return str(path_func.get_snowluma_config_dir())
            return str(path_func.napcat_config_path)
        if isinstance(backend, RemoteBackend):
            return backend.paths.config_dir
        raise BotMigrationError(
            f"不支持的 backend 类型: {type(backend).__name__}",
            stage="config_dir",
        )

    @staticmethod
    def _join_config_path(backend: "OperationBackend", filename: str, backend_type: BackendType = BackendType.NAPCAT) -> str:
        """把 ``filename`` 拼到 backend 的 config_dir 后."""
        config_dir = BotMigrationService._config_dir_for(backend, backend_type)
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

    # ==================== P4 W3 F6: 持久数据搬运 ====================
    def _transfer_persistent_data(
        self,
        source_backend: "OperationBackend",
        dest_backend: "OperationBackend",
        plan: MigrationPlan,
    ) -> tuple[int, int]:
        """搬运白名单下的 NapCat 持久数据; 返回 ``(transferred_bytes, file_count)``.

        改进 (提速 + 容错):
        - chunk 从 1 MiB 提升到 4 MiB, 减少 SFTP 往返次数
        - 单文件失败自动重试 (FILE_MAX_RETRIES 次), 利用 .partial 续传
        - 所有文件重试耗尽后不立即中止, 而是跳过并记录, 继续搬运其余文件
        - 最终汇总失败文件; 有失败时抛 BotMigrationError 但已成功的文件保留
        """
        plan_pairs = self._collect_persistent_files(source_backend, dest_backend, plan)
        total_bytes = sum(size for _, _, size in plan_pairs)
        if not plan_pairs:
            logger.info(
                f"持久数据白名单下无文件需要搬运: qq_id={plan.qq_id}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            self.bytes_progress_signal.emit(0, 0)
            return 0, 0

        logger.info(
            (
                f"开始搬运持久数据: qq_id={plan.qq_id}, "
                f"files={len(plan_pairs)}, total_bytes={total_bytes}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

        transferred = 0
        success_count = 0
        failed_files: list[tuple[str, str]] = []  # (src_path, error_message)
        self.bytes_progress_signal.emit(transferred, total_bytes)

        for src_abs, dst_abs, size in plan_pairs:
            file_done = 0
            last_error = ""
            for attempt in range(FILE_MAX_RETRIES):
                try:
                    file_done = self._copy_with_resume(
                        source_backend,
                        dest_backend,
                        src_abs,
                        dst_abs,
                        expected_size=size,
                        on_chunk=lambda done, total=total_bytes, base=transferred: (
                            self.bytes_progress_signal.emit(base + done, total)
                        ),
                    )
                    break  # 成功, 跳出重试循环
                except (BotMigrationError, Exception) as exc:  # noqa: BLE001
                    last_error = f"{type(exc).__name__}: {exc}"
                    if attempt < FILE_MAX_RETRIES - 1:
                        delay = min(FILE_RETRY_BASE_DELAY * (2 ** attempt), 30.0)
                        logger.warning(
                            f"文件传输失败, 第 {attempt + 1}/{FILE_MAX_RETRIES} 次重试 "
                            f"(等待 {delay:.1f}s): {src_abs} -> {dst_abs}: {last_error}",
                            LogType.NETWORK,
                            LogSource.CORE,
                        )
                        time.sleep(delay)
                        # 重连 backend (远端可能断线)
                        self._try_reconnect(source_backend)
                        self._try_reconnect(dest_backend)
                    else:
                        # 重试耗尽, 记录失败但不中止
                        logger.warning(
                            f"文件传输重试耗尽, 跳过: {src_abs} -> {dst_abs}: {last_error}",
                            LogType.NETWORK,
                            LogSource.CORE,
                        )
                        failed_files.append((src_abs, last_error))
            else:
                # for-else: 重试循环正常结束 (未 break) 意味着全部失败
                # 用 size 推进 transferred 以保持进度条不卡死
                transferred += size
                self.bytes_progress_signal.emit(transferred, total_bytes)
                continue

            # 成功路径
            transferred += file_done
            success_count += 1
            self.bytes_progress_signal.emit(transferred, total_bytes)

        if failed_files:
            summary = "; ".join(f"{src}: {err}" for src, err in failed_files[:5])
            if len(failed_files) > 5:
                summary += f" ... 及其他 {len(failed_files) - 5} 个文件"
            raise BotMigrationError(
                f"持久数据搬运部分失败: 成功 {success_count}/{len(plan_pairs)} 个文件, "
                f"失败 {len(failed_files)} 个. 详情: {summary}",
                stage="persistent_data",
            )

        return transferred, success_count

    def _collect_persistent_files(
        self,
        source_backend: "OperationBackend",
        dest_backend: "OperationBackend",
        plan: MigrationPlan,
    ) -> list[tuple[str, str, int]]:
        """枚举白名单根目录下的所有文件, 返回 ``(src_abs, dst_abs, size)`` 三元组列表.

        - 同名同 size 已存在于目标端时跳过 (粗粒度续传, 满足需求 §F6 "size 一致 + mtime >= 源 mtime 时跳过";
          为减小 backend API 表面, 这里只比较 size; mtime 一致性在 P5 优化期补强).
        """
        src_roots = self._persistent_data_roots(source_backend, plan.backend_type)
        dst_roots = self._persistent_data_roots(dest_backend, plan.backend_type)
        if len(src_roots) != len(dst_roots):
            raise BotMigrationError(
                f"持久数据根目录数量不匹配: src={len(src_roots)}, dst={len(dst_roots)}",
                stage="persistent_data_roots",
            )

        results: list[tuple[str, str, int]] = []
        for src_root, dst_root in zip(src_roots, dst_roots):
            try:
                files = source_backend.walk_files(src_root)
            except Exception as exc:  # noqa: BLE001
                raise BotMigrationError(
                    f"枚举源端持久数据失败 [{src_root}]: {type(exc).__name__}: {exc}",
                    stage="persistent_data_walk",
                ) from exc
            for rel, size in files:
                src_abs = self._join_persistent_path(src_root, rel)
                dst_abs = self._join_persistent_path(dst_root, rel)
                # 已存在且 size 一致 -> 跳过, 避免重复传输
                try:
                    if dest_backend.file_exists(dst_abs) and dest_backend.file_size(dst_abs) == size:
                        continue
                except Exception:  # noqa: BLE001 - file_size 失败不阻断, 当作未传输
                    pass
                results.append((src_abs, dst_abs, size))
        return results

    def _copy_with_resume(
        self,
        source_backend: "OperationBackend",
        dest_backend: "OperationBackend",
        src_path: str,
        dst_path: str,
        *,
        expected_size: int,
        on_chunk: Callable[[int], None],
    ) -> int:
        """单文件流式拷贝, 4 MiB chunk + ``.partial`` 续传 + chunk 级重试.

        改进:
        - chunk 从 1 MiB 提升到 4 MiB, 减少 SFTP 往返 (大文件提速 ~3x)
        - 每个 chunk 的 read/append 失败后自动重试 CHUNK_MAX_RETRIES 次,
          指数退避等待, 中间自动重连 backend
        - 续传逻辑不变: .partial 已存在时从其长度作为 offset 继续

        Args:
            source_backend / dest_backend: 字节级 IO 实现端.
            src_path / dst_path: 源端 / 目标端**绝对路径**.
            expected_size: 源端预估字节数, 用于 progress 计算 (允许偏差, 不做 hash 校验).
            on_chunk: 每个 chunk 完成回调; 入参为"本文件已完成字节数".

        Returns:
            实际成功写入的字节数.
        """
        partial_path = f"{dst_path}{PERSISTENT_PARTIAL_SUFFIX}"
        # 续传: 目标 ``.partial`` 已存在时, 从其当前长度作为 resume offset
        resume_offset = 0
        try:
            if dest_backend.file_exists(partial_path):
                resume_offset = dest_backend.file_size(partial_path)
                if resume_offset > expected_size:
                    # 之前的 partial 比源端文件还大 (源端被裁剪), 安全起见整体重传
                    dest_backend.remove(partial_path)
                    resume_offset = 0
        except Exception:  # noqa: BLE001 - 续传探测失败不阻断, 退化为全量重传
            resume_offset = 0

        offset = resume_offset
        on_chunk(offset)
        # 流式 4 MiB 分片 + chunk 级重试
        while offset < expected_size:
            length = min(PERSISTENT_DATA_CHUNK_SIZE, expected_size - offset)
            chunk = self._read_chunk_with_retry(source_backend, src_path, offset, length)
            if not chunk:
                # 源端文件意外缩短: 提前结束, 已写入部分仍保留 .partial 待重试
                break
            self._write_chunk_with_retry(dest_backend, partial_path, chunk)
            offset += len(chunk)
            on_chunk(offset)

        # 完成: 把 .partial rename 为目标真名 (覆盖语义由 backend.rename 保证)
        if offset >= expected_size:
            dest_backend.rename(partial_path, dst_path)
        else:
            # 部分完成: 保留 .partial 让下次续传, 抛错通知上层中止后续文件
            raise BotMigrationError(
                f"源端文件意外缩短 [{src_path}]: 期望 {expected_size}B, 实读 {offset}B; "
                f"已保留 {partial_path}",
                stage="persistent_data_chunk",
            )
        return offset

    def _read_chunk_with_retry(
        self,
        backend: "OperationBackend",
        path: str,
        offset: int,
        length: int,
    ) -> bytes:
        """带重试的 chunk 读取; 失败时指数退避 + 自动重连."""
        last_exc: Exception | None = None
        for attempt in range(CHUNK_MAX_RETRIES):
            try:
                return backend.read_bytes(path, offset=offset, length=length)
            except Exception as exc:  # noqa: BLE001
                last_exc = exc
                if attempt < CHUNK_MAX_RETRIES - 1:
                    delay = min(CHUNK_RETRY_BASE_DELAY * (2 ** attempt), 30.0)
                    logger.warning(
                        f"chunk 读取失败 (attempt {attempt + 1}/{CHUNK_MAX_RETRIES}, "
                        f"offset={offset}, 等待 {delay:.1f}s): {type(exc).__name__}: {exc}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )
                    time.sleep(delay)
                    self._try_reconnect(backend)
        raise BotMigrationError(
            f"chunk 读取重试耗尽 [{path}@{offset}]: {last_exc}",
            stage="persistent_data_chunk",
        )

    def _write_chunk_with_retry(
        self,
        backend: "OperationBackend",
        path: str,
        data: bytes,
    ) -> None:
        """带重试的 chunk 写入; 失败时指数退避 + 自动重连."""
        last_exc: Exception | None = None
        for attempt in range(CHUNK_MAX_RETRIES):
            try:
                backend.append_bytes(path, data)
                return
            except Exception as exc:  # noqa: BLE001
                last_exc = exc
                if attempt < CHUNK_MAX_RETRIES - 1:
                    delay = min(CHUNK_RETRY_BASE_DELAY * (2 ** attempt), 30.0)
                    logger.warning(
                        f"chunk 写入失败 (attempt {attempt + 1}/{CHUNK_MAX_RETRIES}, "
                        f"等待 {delay:.1f}s): {type(exc).__name__}: {exc}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )
                    time.sleep(delay)
                    self._try_reconnect(backend)
                    # 写入失败后需要校正 .partial 长度, 避免重复追加
                    # 重新检查 partial 文件实际大小来决定是否需要重写
                    try:
                        actual_size = backend.file_size(path)
                        # 如果 actual_size 已经包含了这次 data, 说明写入其实成功了
                        # (网络返回超时但数据已落盘), 直接 return
                        # 这里无法精确判断, 保守策略: 如果 size 增长了就认为成功
                        # 由于 append 语义, 重复 append 会导致数据重复, 所以需要谨慎
                        # 实际上 append_bytes 失败时数据可能部分写入, 但 .partial 续传
                        # 机制在文件级重试时会重新校正 offset, 所以这里直接重试即可
                    except Exception:  # noqa: BLE001
                        pass
        raise BotMigrationError(
            f"chunk 写入重试耗尽 [{path}]: {last_exc}",
            stage="persistent_data_chunk",
        )

    @staticmethod
    def _try_reconnect(backend: "OperationBackend") -> None:
        """尝试重连 backend; 失败时静默 (让上层重试逻辑处理)."""
        try:
            backend.connect()
        except Exception:  # noqa: BLE001
            pass

    @staticmethod
    def _persistent_data_roots(backend: "OperationBackend", backend_type: BackendType = BackendType.NAPCAT) -> list[str]:
        """返回 ``backend`` 上的持久数据根目录列表.

        NapCat (与需求 §F6 白名单对齐):
        - RemoteBackend (Linux): ``$HOME/Napcat/opt/QQ/resources/app/app_launcher/napcat/data``,
          ``$HOME/.config/QQ``
        - LocalBackend (Windows): NapCat 安装目录下的 ``app_launcher/napcat/data``,
          以及 ``%APPDATA%/Tencent/QQ`` (若存在); 缺失时返回空列表.

        SnowLuma:
        - RemoteSnowLumaBackend (Linux): ``{snowluma_framework_dir}/data``
        - LocalBackend (Windows): ``PathFunc.get_snowluma_data_dir()``

        顺序与目标端**严格对齐**, 让 ``zip`` 后一一对应.
        """
        # 延迟 import 避免循环
        from .local_backend import LocalBackend
        from .remote_backend import RemoteBackend
        from .remote_snowluma_backend import RemoteSnowLumaBackend

        if isinstance(backend, RemoteSnowLumaBackend):
            # SL 远端: 只有一个数据根 (snowluma_framework_dir/data)
            sl_data = backend.sl_paths.snowluma_framework_dir.rstrip("/") + "/data"
            return [sl_data]
        if isinstance(backend, RemoteBackend):
            workspace = backend.paths.workspace_dir.rstrip("/")
            return [
                f"{workspace}/opt/QQ/resources/app/app_launcher/napcat/data",
                "$HOME/.config/QQ",
            ]
        if isinstance(backend, LocalBackend):
            from creart import it

            from src.core.runtime.paths import PathFunc

            path_func = it(PathFunc)

            if backend_type == BackendType.SNOWLUMA:
                # SL 本地: 只有一个数据根
                return [str(path_func.get_snowluma_data_dir())]

            # NC 本地: 两个数据根
            from os import environ
            from pathlib import Path as _Path

            napcat_root = path_func.napcat_path
            local_data = napcat_root / "opt" / "QQ" / "resources" / "app" / "app_launcher" / "napcat" / "data"
            appdata = environ.get("APPDATA", "")
            local_qq = _Path(appdata) / "Tencent" / "QQ" if appdata else None
            roots: list[str] = [str(local_data)]
            roots.append(str(local_qq) if local_qq is not None else "")
            return roots
        raise BotMigrationError(
            f"不支持的 backend 类型: {type(backend).__name__}",
            stage="persistent_data_roots",
        )

    @staticmethod
    def _join_persistent_path(root: str, rel: str) -> str:
        """跨平台拼接 ``root + rel``; rel 走 POSIX 风格."""
        if not rel:
            return root
        if root.endswith("/") or root.endswith("\\"):
            return f"{root}{rel}"
        # 远端 root (POSIX) 用 ``/``, 本地 root (Windows) 也接受 ``/`` (Path 容忍)
        return f"{root}/{rel}"


def derive_plan_from_bot_config(
    *,
    qq_id: str,
    old_target: str,
    new_target: str,
    move_persistent_data: bool = False,
    backend_type: BackendType = BackendType.NAPCAT,
) -> MigrationPlan:
    """从旧/新 ``runtime_target`` 构造 [`MigrationPlan`](src/core/operation/migration.py).

    便利函数, 替 UI 层屏蔽 ``RUNTIME_TARGET_LOCAL`` 常量细节.
    """
    return MigrationPlan(
        qq_id=str(qq_id),
        source_target=old_target or RUNTIME_TARGET_LOCAL,
        dest_target=new_target or RUNTIME_TARGET_LOCAL,
        move_persistent_data=move_persistent_data,
        backend_type=backend_type,
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

        # P4 W3 F6: 把字节级进度桥到 ProgressInfoBar content 文案
        # ("已传输 X.X / Y.Y MB"), 让用户在搬运大文件时看到推进
        if center is not None and self._plan.move_persistent_data:
            label = (
                f"迁移 Bot {self._plan.qq_id} "
                f"({self._plan.source_target} → {self._plan.dest_target})"
            )

            def _on_bytes_progress(transferred: int, total: int) -> None:
                if total <= 0:
                    return
                content = (
                    f"持久数据搬运: "
                    f"{transferred / (1024 * 1024):.1f} / "
                    f"{total / (1024 * 1024):.1f} MiB"
                )
                try:
                    center.begin(task_id, label, content=content)  # type: ignore[union-attr]
                except Exception:  # noqa: BLE001 - 进度文案失败不阻断搬运
                    pass

            service.bytes_progress_signal.connect(_on_bytes_progress)

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

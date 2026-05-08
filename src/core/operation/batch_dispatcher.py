# -*- coding: utf-8 -*-
"""[`BatchDispatcher`](src/core/operation/batch_dispatcher.py): 批量 Bot 操作派发器
(P4 W1.F2).

设计目标
--------

P4 之前, BotPage 的启动 / 停止 / 迁移 / 删除都是单 Bot 触发. 多 Bot (>= 5)
场景下用户必须重复点 N 次, 远端 Bot 启停尤其耗时. 本模块提供"一次提交 N 个,
聚合上报一次"的派发能力, 并复用 P3 perf 的:

- [`QThreadPool.globalInstance()`](https://doc.qt.io/qt-6/qthreadpool.html) 异步派发
- [`BackgroundTaskCenter`](src/core/runtime/background_tasks.py) 单条聚合任务
  (label 形如 ``"批量启动 (3/5)"``)
- ProgressInfoBar 桥既有信号通路 (任务结束时携带成败 + 文案)

约束
----

- 不**新增**任何 ``OperationBackend`` 公共 API; 调用方自行准备闭包.
- 不**新增**线程池实例; 共用 ``QThreadPool.globalInstance()``.
- 单个 batch 在 ``BackgroundTaskCenter`` 上**只**注册一个 task (用 batch_id),
  内部 N 个 worker 不重复登记 - 否则 N 大于 ProgressInfoBar 容量时 UI 会刷屏.
- 测试态走 ``executor=_inline_executor`` 走同步执行, 不依赖 QApplication / QThreadPool.

派发模型
----

::

    dispatcher = BatchDispatcher()
    dispatcher.progress_signal.connect(...)   # (done, total)
    dispatcher.finished_signal.connect(...)   # list[BatchOutcome]
    batch_id = dispatcher.dispatch(
        "批量启动",
        [(qq_id, lambda qq=qq_id: process_manager.create_napcat_process(...)) for ...],
        sequential=False,    # 启动 / 停止可并行; 迁移 / 删除应 sequential=True
    )
"""
from __future__ import annotations

# 标准库导入
import threading
import uuid
from abc import ABC
from collections.abc import Callable
from dataclasses import dataclass
from typing import Literal

# 第三方库导入
from creart import add_creator, exists_module, it
from creart.creator import AbstractCreator, CreateTargetInfo
from PySide6.QtCore import QObject, QRunnable, QThreadPool, Signal


BatchAction = Literal["start", "stop", "migrate", "delete", "custom"]


@dataclass(frozen=True)
class BatchOutcome:
    """单个 Bot 操作的最终结果快照."""

    key: str
    """通常是 ``str(qq_id)``; 也允许调用方用别的稳定 key (例如服务器 id)."""

    ok: bool
    error: str | None = None


# ==================== 执行器 ====================
# 抽象执行器: 接收 ``QRunnable``, 同步或异步分派. 默认走 QThreadPool, 测试走 inline.
Executor = Callable[[QRunnable], None]


def _qthreadpool_executor(runnable: QRunnable) -> None:
    """生产环境默认执行器: 派到全局 QThreadPool."""
    QThreadPool.globalInstance().start(runnable)


def _inline_executor(runnable: QRunnable) -> None:
    """同步执行 ``runnable.run()``, 不依赖 QApplication / QThreadPool.

    用途:

    - **测试**: 不进线程池, 单线程驱动 dispatcher 的状态机.
    - **生产**: 调用方明确知道 ``op`` 是非阻塞 (例如 P3 perf 之后的
      ``ManagerNapCatQQProcess.create_napcat_process`` / ``stop_process``,
      它们内部已经把耗时分支异步化), 强制留在主线程执行, 避免
      ``QProcess`` / ``QObject`` 在无事件循环的 worker 线程上被构造.
      公开别名见 :data:`inline_executor`.
    """
    runnable.run()


#: 公开别名: 与 :func:`_inline_executor` 等价, 供调用方在
#: ``BatchDispatcher.dispatch(..., executor=inline_executor)`` 显式选择
#: "同步 / 主线程执行" 模式. 见函数 docstring 的 "生产" 用途.
inline_executor = _inline_executor


# ==================== 内部 worker ====================
class _BatchItemRunnable(QRunnable):
    """单个 Bot 操作的 ``QRunnable`` 包装.

    职责:

    1. 执行调用方提供的 ``op`` 闭包.
    2. ``op`` 抛异常 -> 记录 friendly 文案 + ok=False; 否则 ok=True.
    3. 通过 ``_BatchTracker.report_done`` 回调汇报结果, 由 tracker 决定是否触发
       ``progress_signal`` / ``finished_signal``.

    约束:
    - 不直接 emit Qt 信号 (跨线程 emit 由 tracker 在持锁后转发, 信号自身的
      ``Qt::QueuedConnection`` 把投递切回主线程订阅者).
    - 不持有 ``BatchDispatcher`` 引用以避免循环引用.
    """

    __slots__ = ("_key", "_op", "_tracker")

    def __init__(self, key: str, op: Callable[[], None], tracker: "_BatchTracker") -> None:
        super().__init__()
        self._key = key
        self._op = op
        self._tracker = tracker

    def run(self) -> None:
        try:
            self._op()
            outcome = BatchOutcome(key=self._key, ok=True, error=None)
        except BaseException as exc:  # noqa: BLE001 - 任何错误都包成 outcome
            # 延迟 import 避免与 errors 模块循环
            from src.core.remote.friendly_errors import to_friendly

            try:
                friendly = to_friendly(exc)
            except Exception:  # noqa: BLE001
                friendly = str(exc) or type(exc).__name__
            outcome = BatchOutcome(key=self._key, ok=False, error=friendly)
        finally:
            self._tracker.report_done(self._key, outcome)


# ==================== 任务跟踪器 ====================
class _BatchTracker:
    """单个 batch 的进度状态机.

    把 ``BatchDispatcher`` 与每个 ``_BatchItemRunnable`` 之间的"全局状态"集中, 让
    BatchDispatcher 自身保持无状态 (一个 BatchDispatcher 实例可以同时跟多个 batch).
    """

    __slots__ = (
        "_dispatcher",
        "_batch_id",
        "_label",
        "_total",
        "_outcomes",
        "_done",
        "_lock",
        "_sequential",
        "_pending_items",
        "_executor",
    )

    def __init__(
        self,
        *,
        dispatcher: "BatchDispatcher",
        batch_id: str,
        label: str,
        total: int,
        sequential: bool,
        executor: Executor,
    ) -> None:
        self._dispatcher = dispatcher
        self._batch_id = batch_id
        self._label = label
        self._total = total
        self._outcomes: list[BatchOutcome] = []
        self._done = 0
        self._lock = threading.Lock()
        self._sequential = sequential
        self._pending_items: list[_BatchItemRunnable] = []
        self._executor = executor

    @property
    def batch_id(self) -> str:
        return self._batch_id

    def report_done(self, key: str, outcome: BatchOutcome) -> None:
        """worker 完成后回调 (可能在工作线程)."""
        with self._lock:
            self._outcomes.append(outcome)
            self._done += 1
            done = self._done
            total = self._total
            next_runnable = (
                self._pending_items.pop(0) if self._sequential and self._pending_items else None
            )
            finished = done >= total

        # 信号 emit 不持锁 (Qt::AutoConnection -> QueuedConnection 跨线程; 即使同线程订阅者
        # 也不应阻塞下一个 runnable 的派发).
        self._dispatcher.progress_signal.emit(done, total)

        # 把 BackgroundTaskCenter 的 content 同步刷新, ProgressInfoBar 会响应
        try:
            # 延迟 import 避免循环
            from src.core.runtime.background_tasks import BackgroundTaskCenter

            it(BackgroundTaskCenter).begin(
                self._batch_id,
                self._label,
                content=f"{done}/{total}",
            )
        except Exception:  # noqa: BLE001 - 测试环境无 creart 上下文时静默
            pass

        # sequential 模式下: 上一个完成后立即派下一个
        if next_runnable is not None:
            self._executor(next_runnable)

        if finished:
            self._finalize()

    def _finalize(self) -> None:
        """全部完成后聚合 emit + 关闭 BackgroundTaskCenter 任务."""
        # 复制一份 outcomes 以避免被外部 mutate
        outcomes = list(self._outcomes)
        success = sum(1 for o in outcomes if o.ok)
        failed = len(outcomes) - success

        try:
            from src.core.runtime.background_tasks import BackgroundTaskCenter

            center = it(BackgroundTaskCenter)
            if failed == 0:
                center.end(self._batch_id, success=True, message=f"全部完成 ({success}/{len(outcomes)})")
            else:
                center.end(
                    self._batch_id,
                    success=False,
                    message=f"成功 {success} / 失败 {failed}",
                )
        except Exception:  # noqa: BLE001
            pass

        self._dispatcher.finished_signal.emit(outcomes)
        # 让 dispatcher 释放对 tracker 的引用
        self._dispatcher._on_batch_finished(self._batch_id)


# ==================== Dispatcher 主类 ====================
class BatchDispatcher(QObject):
    """批量操作派发器.

    信号:

    - ``progress_signal(done, total)``: 每个子项完成时 emit
    - ``finished_signal(list[BatchOutcome])``: 整个 batch 完成时 emit (主线程接口)
    - ``batch_started_signal(batch_id, label, total)``: dispatch 入口同步 emit, UI 端可
      据此切换"批量进行中"态

    单例约定: 通过 [`creart`](https://github.com/GreyElaina/creart) 拿; 也可手工
    实例化用于测试.
    """

    progress_signal = Signal(int, int)
    finished_signal = Signal(list)
    batch_started_signal = Signal(str, str, int)

    def __init__(self) -> None:
        super().__init__()
        self._trackers: dict[str, _BatchTracker] = {}
        self._lock = threading.Lock()

    def dispatch(
        self,
        action_label: str,
        items: list[tuple[str, Callable[[], None]]],
        *,
        sequential: bool = False,
        executor: Executor | None = None,
        batch_id: str | None = None,
    ) -> str:
        """提交一个 batch 给执行器.

        Args:
            action_label: 用户可见的操作名, 形如 ``"批量启动"`` / ``"批量停止"``;
                tracker 会在 ProgressInfoBar 上拼成 ``"批量启动 (3/5)"`` 风格.
            items: ``[(key, op), ...]``; ``op`` 是无参 callable, 抛异常视为失败.
                ``key`` 通常是 ``str(qq_id)``, 也可以是任意稳定字符串.
            sequential: True 时排成 1-worker 链 (适合迁移 / 删除等不可并发场景);
                False 时所有 op 同时入池 (启动 / 停止可承受并发).
            executor: 可选自定义执行器, 测试用 :func:`_inline_executor` 同步跑;
                生产环境留默认 (None) 走 QThreadPool.
            batch_id: 可选, 显式指定; 否则 ``f"batch-{uuid4().hex[:8]}"``.

        Returns:
            ``batch_id``. ``items`` 为空时**也**返回一个 batch_id 并立即同步
            ``finished_signal.emit([])`` (UI 端不会因为 0 项卡死).
        """
        if executor is None:
            executor = _qthreadpool_executor

        bid = batch_id or f"batch-{uuid.uuid4().hex[:8]}"
        total = len(items)

        # 0 项: 直接 finalize, 不进 tracker
        if total == 0:
            self.batch_started_signal.emit(bid, action_label, 0)
            self.finished_signal.emit([])
            return bid

        tracker = _BatchTracker(
            dispatcher=self,
            batch_id=bid,
            label=action_label,
            total=total,
            sequential=sequential,
            executor=executor,
        )
        with self._lock:
            self._trackers[bid] = tracker

        runnables = [
            _BatchItemRunnable(key=key, op=op, tracker=tracker) for key, op in items
        ]

        # 进入 BackgroundTaskCenter (单条聚合任务)
        try:
            from src.core.runtime.background_tasks import BackgroundTaskCenter

            it(BackgroundTaskCenter).begin(bid, action_label, content=f"0/{total}")
        except Exception:  # noqa: BLE001
            pass

        self.batch_started_signal.emit(bid, action_label, total)

        if sequential:
            # 排队等待: 第一个进 executor, 其余暂存 tracker._pending_items
            tracker._pending_items.extend(runnables[1:])
            executor(runnables[0])
        else:
            for runnable in runnables:
                executor(runnable)

        return bid

    def active_batch_ids(self) -> list[str]:
        """当前在跟踪的 batch_id 列表 (主要用于诊断)."""
        with self._lock:
            return list(self._trackers.keys())

    # ==================== 私有 ====================
    def _on_batch_finished(self, batch_id: str) -> None:
        """tracker 完成后清理引用."""
        with self._lock:
            self._trackers.pop(batch_id, None)


# ==================== creart 单例 ====================
class BatchDispatcherCreator(AbstractCreator, ABC):
    """[`BatchDispatcher`](src/core/operation/batch_dispatcher.py) 单例创建器."""

    targets = (
        CreateTargetInfo("src.core.operation.batch_dispatcher", "BatchDispatcher"),
    )

    @staticmethod
    def available() -> bool:
        return exists_module("src.core.operation.batch_dispatcher")

    @staticmethod
    def create(create_type):
        return create_type()


add_creator(BatchDispatcherCreator)


__all__: tuple[str, ...] = (
    "BatchAction",
    "BatchOutcome",
    "BatchDispatcher",
    "Executor",
)

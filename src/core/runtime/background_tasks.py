# -*- coding: utf-8 -*-
"""[`BackgroundTaskCenter`](src/core/runtime/background_tasks.py): 跨线程后台任务聚合中心 (P3 perf).

设计目标
--------

把分散在多个 [`QRunnable`](https://doc.qt.io/qt-6/qrunnable.html) (启动 / 停止 / 同步配置 /
迁移 / 部署 / 强制更新 / 回滚 / ...) 中的"我现在正在做某件 SSH 事"事件统一汇总,
让 UI 层 (例如 [`BackgroundTaskBar`](src/ui/page/bot_page/widget/background_task_bar.py))
可以用单一信号源渲染"当前后台任务数 + 最早一项标签", 解决"按完按钮没反应"的体验问题.

关键约束
--------

- 任意工作线程都可以调用 :meth:`BackgroundTaskCenter.begin` / :meth:`end`;
  线程安全靠内部 ``threading.Lock`` 互斥, Qt Signal 的跨线程派发负责把
  ``count_changed_signal`` 投递到主线程订阅者.
- :meth:`active_count` / :meth:`active_tasks` 是只读快照, 也可以在任意线程调用.
- 该类**不要**持有 QObject 子节点引用 (例如 widget); 它只对外发信号.
- 使用 [`creart`](https://github.com/GreyElaina/creart) 单例, 与
  [`ManagerNapCatQQProcess`](src/core/runtime/napcat.py) 等 P2/P3 既有 manager 一致.
"""
from __future__ import annotations

# 标准库导入
import threading
from abc import ABC
from contextlib import contextmanager
from dataclasses import dataclass
from typing import Iterator

# 第三方库导入
from creart import add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from PySide6.QtCore import QObject, Signal


@dataclass(frozen=True)
class BackgroundTask:
    """后台任务条目快照 (不可变).

    ``content`` 是 ProgressInfoBar 的 "进行中" 描述文案, 用于桥接
    [`ProgressInfoBarBridge`](src/ui/components/progress_info_bar_bridge.py).
    """

    task_id: str
    label: str
    content: str = ""


class BackgroundTaskCenter(QObject):
    """跨线程后台任务聚合中心.

    对外暴露的信号:

    - ``task_started_signal(task_id, label, content)``: 新任务进入跟踪 (P3 perf 升级:
      additional ``content`` 字段供 [`ProgressInfoBar`](https://github.com/zhiyiYo/PyQt-Fluent-Widgets)
      显示进行中描述, 历史调用 ``begin(task_id, label)`` 透明兼容, ``content=""``)
    - ``task_finished_signal(task_id)``: 任务结束 (兼容已存在的"只关心结束"订阅者)
    - ``task_completed_signal(task_id, success, message)``: 任务结束的扩展信号,
      携带成败 + 完成文案, 供 ``ProgressInfoBar.setComplete`` 桥使用
    - ``count_changed_signal(count)``: 当前活跃任务总数变化
    """

    task_started_signal = Signal(str, str, str)
    task_finished_signal = Signal(str)
    task_completed_signal = Signal(str, bool, str)
    count_changed_signal = Signal(int)

    def __init__(self) -> None:
        super().__init__()
        self._tasks: dict[str, BackgroundTask] = {}
        self._lock = threading.Lock()

    # ==================== 公共接口 ====================
    def begin(self, task_id: str, label: str, *, content: str = "") -> None:
        """登记一个新任务并 emit 信号; 重复 ``task_id`` 视为标签 / 描述更新.

        Args:
            task_id: 任务唯一 id.
            label: ProgressInfoBar 标题 / 状态条主标签.
            content: 可选, ProgressInfoBar 进行中描述; 空串保留, 便于桥侧自决文案.
        """
        with self._lock:
            self._tasks[task_id] = BackgroundTask(task_id=task_id, label=label, content=content)
            count = len(self._tasks)
        self.task_started_signal.emit(task_id, label, content)
        self.count_changed_signal.emit(count)

    def end(self, task_id: str, *, success: bool = True, message: str = "") -> None:
        """注销任务并 emit 信号; 不存在的 ``task_id`` 静默忽略.

        Args:
            task_id: 任务唯一 id.
            success: 任务最终成败. ``False`` 会让 ProgressInfoBar 切换到 ❌ 配色.
            message: 完成文案; 空串时桥会回退到原 label.
        """
        with self._lock:
            existed = self._tasks.pop(task_id, None)
            count = len(self._tasks)
        if existed is None:
            return
        self.task_finished_signal.emit(task_id)
        self.task_completed_signal.emit(task_id, bool(success), message or "")
        self.count_changed_signal.emit(count)

    def fail(self, task_id: str, message: str = "") -> None:
        """:meth:`end` 的失败语义糖, 等价于 ``end(task_id, success=False, message=message)``."""
        self.end(task_id, success=False, message=message)

    def active_count(self) -> int:
        """当前活跃任务数量."""
        with self._lock:
            return len(self._tasks)

    def active_tasks(self) -> list[BackgroundTask]:
        """当前活跃任务的有序列表 (按登记顺序; Python 3.7+ dict 保序)."""
        with self._lock:
            return list(self._tasks.values())

    @contextmanager
    def track(
        self, task_id: str, label: str, *, content: str = "", success_message: str = ""
    ) -> Iterator[None]:
        """``with center.track(task_id, label):`` 包装, 异常仍会触发 :meth:`end`.

        若 ``with`` 块抛异常, 自动 ``end(success=False, message=str(exc))``;
        正常退出则 ``end(success=True, message=success_message)``.
        """
        self.begin(task_id, label, content=content)
        try:
            yield
        except BaseException as exc:  # noqa: BLE001 - 上抛, 但先打"失败"标记
            self.end(task_id, success=False, message=str(exc) or type(exc).__name__)
            raise
        else:
            self.end(task_id, success=True, message=success_message)

    def reset_for_test(self) -> None:
        """测试辅助: 清空所有任务记录, **仅供单元测试**.

        生产代码调用会破坏 UI 状态条与 RunnableE 间的时序假设; 故 hard 标记为 test-only.
        """
        with self._lock:
            self._tasks.clear()
            count = 0
        self.count_changed_signal.emit(count)


# ==================== 创建器 ====================
class BackgroundTaskCenterCreator(AbstractCreator, ABC):
    """[`BackgroundTaskCenter`](src/core/runtime/background_tasks.py) 单例创建器."""

    targets = (
        CreateTargetInfo("src.core.runtime.background_tasks", "BackgroundTaskCenter"),
    )

    @staticmethod
    def available() -> bool:
        return exists_module("src.core.runtime.background_tasks")

    @staticmethod
    def create(create_type):
        return create_type()


add_creator(BackgroundTaskCenterCreator)

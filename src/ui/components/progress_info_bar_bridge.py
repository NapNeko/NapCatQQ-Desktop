# -*- coding: utf-8 -*-
"""[`ProgressInfoBarBridge`](src/ui/components/progress_info_bar_bridge.py): 把
[`BackgroundTaskCenter`](src/core/runtime/background_tasks.py) 的任务事件桥接到
[`ProgressInfoBar`](https://github.com/zhiyiYo/PyQt-Fluent-Widgets) 弹窗 (P3 perf).

设计目标
--------

P3 perf 之前 BotPage Header 自己渲染了一条聚合状态条, BotCard 也在卡片内显示
``IndeterminateProgressBar``. 用户提出"用组件库的 ProgressInfoBar / ProgressToast",
桥接思路是:

- 任意工作线程调 ``BackgroundTaskCenter.begin(task_id, label, content=...)`` →
  桥在主窗口右上 spawn 一个不确定模式 ``ProgressInfoBar``, 旋转环 + 标题 + 描述
- 任务完成调 ``end(task_id, success=..., message=...)`` →
  桥找到对应 InfoBar 调用 ``setComplete(success, content=message)``,
  自动切换 ✅/❌ 配色, 1.5s 后淡出关闭

UI 一致性收益:

- 多任务并发自动堆叠 (InfoBarManager 负责堆叠 / 位移)
- 启动 / 配置同步 / 部署 / 迁移 / 测试连接全部走同一展示路径
- BotCard / Header 不再自己画进度, 视觉杂噪降低

线程模型
--------

- ``BackgroundTaskCenter`` 信号支持跨线程 (Qt 自动 ``QueuedConnection``),
  桥内的 slot 一定在 UI 主线程执行, 可以放心操作 widget.
- 桥本身是 :class:`QObject`, 必须在 UI 线程构造并由 ``parent`` widget 决定生命周期;
  ``parent`` 析构后桥自动断开订阅, 不会泄漏.
"""
from __future__ import annotations

# 标准库导入
import weakref
from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from PySide6.QtCore import QObject, Qt
from PySide6.QtWidgets import QWidget
from qfluentwidgets import ProgressInfoBar

# 项目内模块导入
from src.core.logging import LogSource, logger
from src.core.runtime.background_tasks import BackgroundTaskCenter
from src.ui.components.managers import NCDInfoBarPosition

if TYPE_CHECKING:
    pass


class ProgressInfoBarBridge(QObject):
    """订阅 [`BackgroundTaskCenter`](src/core/runtime/background_tasks.py),
    在指定 ``parent`` 上 spawn / 收尾 [`ProgressInfoBar`](https://github.com/zhiyiYo/PyQt-Fluent-Widgets).

    Args:
        parent: 承载 ProgressInfoBar 的父 widget, 一般是 ``MainWindow`` 或 ``BotPage``.
            ``InfoBarManager`` 会基于该 parent 的 size 计算条目位置, 因此应当是
            一个稳定可见的全屏级 / 页面级 widget.
        position: 默认 [`NCDInfoBarPosition.TOP_RIGHT`](src/ui/components/managers.py),
            与项目 ``error_bar`` / ``warning_bar`` 同款 — InfoBar 会落在 (margin, margin+42),
            避开 [`CustomTitleBar`](src/ui/window/window.py) 与 NavigationView. 传原版
            ``qfluentwidgets.InfoBarPosition.TOP_RIGHT`` 会走默认 manager, 直接顶在标题栏下方.
        auto_close_after: 任务完成后 ProgressInfoBar 淡出的延迟 (毫秒);
            ``< 0`` 表示不自动关闭, 由用户手动点 ✕.

    生命周期:

    - 桥构造后立即连接 [`BackgroundTaskCenter`](src/core/runtime/background_tasks.py) 信号.
    - ``parent`` 销毁会触发桥的 ``destroyed``, 信号会被 Qt 自动断开 (因为 connect 的
      slot 是 ``self`` 上的方法, ``self`` 也作为 ``parent`` 的子节点被同时销毁).
    - 同一进程允许有多个 bridge 实例 (例如调试时挂在不同窗口), 它们各自管理 own InfoBar.
    """

    def __init__(
        self,
        parent: QWidget,
        *,
        position: NCDInfoBarPosition = NCDInfoBarPosition.TOP_RIGHT,
        auto_close_after: int = 1500,
    ) -> None:
        super().__init__(parent)
        self._parent_ref = weakref.ref(parent)
        self._position = position
        self._auto_close_after = int(auto_close_after)
        self._bars: dict[str, weakref.ReferenceType[ProgressInfoBar]] = {}

        center = it(BackgroundTaskCenter)
        # 跨线程 emit 默认 ``QueuedConnection``, slot 始终在 UI 线程执行.
        center.task_started_signal.connect(self._on_task_started, Qt.QueuedConnection)
        center.task_completed_signal.connect(self._on_task_completed, Qt.QueuedConnection)

        # 启动时如果 Center 已有任务在跑 (例如热重载场景), 先把它们拉起来.
        for task in center.active_tasks():
            self._on_task_started(task.task_id, task.label, task.content)

    # ==================== 信号槽 ====================
    def _on_task_started(self, task_id: str, label: str, content: str) -> None:
        """新任务进入跟踪 → spawn 一个不确定模式 ProgressInfoBar.

        重复 ``task_id``: 已存在则只更新文案, 不重复弹.
        """
        existing = self._resolve_bar(task_id)
        if existing is not None:
            existing.setTitle(label)
            if content:
                existing.setContent(content)
            return

        parent = self._parent_ref()
        if parent is None:
            # parent 已被销毁; 不再尝试展示.
            return

        try:
            bar = ProgressInfoBar.indeterminate(
                title=label,
                content=content or "正在处理…",
                isClosable=False,  # 长任务不允许用户提前关掉, 防止误操作
                duration=-1,
                position=self._position,
                parent=parent,
            )
        except Exception as exc:  # noqa: BLE001 - InfoBar 构造异常不应让任务挂掉
            logger.warning(
                f"ProgressInfoBar 构造失败 (task={task_id}, label={label}): "
                f"{type(exc).__name__}: {exc}",
                log_source=LogSource.UI,
            )
            return

        self._bars[task_id] = weakref.ref(bar)

    def _on_task_completed(self, task_id: str, success: bool, message: str) -> None:
        """任务结束 → 把对应的 ProgressInfoBar 切换到完成态."""
        bar = self._resolve_bar(task_id)
        self._bars.pop(task_id, None)
        if bar is None:
            return

        # message 空时回退到当前 title 作为完成内容, 避免空白
        content = message or bar.title or ""
        try:
            bar.setComplete(
                success=success,
                content=content,
                autoCloseAfter=self._auto_close_after,
            )
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"ProgressInfoBar.setComplete 失败 (task={task_id}, success={success}): "
                f"{type(exc).__name__}: {exc}",
                log_source=LogSource.UI,
            )

    # ==================== 内部辅助 ====================
    def _resolve_bar(self, task_id: str) -> ProgressInfoBar | None:
        """从 weakref 字典里取回仍存活的 ProgressInfoBar; 已销毁则清掉条目."""
        ref = self._bars.get(task_id)
        if ref is None:
            return None
        bar = ref()
        if bar is None:
            self._bars.pop(task_id, None)
        return bar

    # ==================== 测试辅助 ====================
    def active_task_ids(self) -> list[str]:
        """返回当前桥仍在跟踪的 task_id 列表 (供测试 / 调试使用)."""
        return [task_id for task_id, ref in self._bars.items() if ref() is not None]

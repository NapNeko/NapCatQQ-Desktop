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
        position: 默认 [`NCDInfoBarPosition.BOTTOM_RIGHT`](src/ui/components/managers.py),
            与 ``success_bar`` / ``info_bar`` 同款 - chip 从右下角往上堆叠,
            **远离 BotCard header 区域**, 避免多任务并发时 chip 覆盖到第二行卡片标题
            (P4 W4 修复: 之前 TOP_RIGHT 第二/三条 chip y≈126/184 与 BotPage 卡片重叠).
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
        position: NCDInfoBarPosition = NCDInfoBarPosition.BOTTOM_RIGHT,
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
        """任务结束 → 关闭 indeterminate ProgressInfoBar + 弹出新的 success/error InfoBar.

        P4 W4 设计决策:
        ----------------
        ``ProgressInfoBar.setComplete`` 是 "原地复用" 模式 - 切换 IconWidget 但保留
        widget 实例; 这导致 indeterminate 状态的 sizeHint / layout / 内嵌 label 的
        尺寸约束会**继续生效**, 即使我们调 ``setMinimumWidth`` / ``adjustSize`` /
        ``layout.invalidate`` 都救不回来 (实测 chip 本体撑到 320px 后内部
        contentLabel 仍按旧 sizeHint 渲染, 文本被裁切).

        放弃复用. 完成态走与 ``success_bar`` 完全一样的路径:
          1. 关闭旧的 indeterminate ProgressInfoBar (立刻 close, 不等淡出);
          2. 用 ``InfoBar.success/error`` 重新弹一个常规 InfoBar, 由 InfoBarManager
             按 ``success_bar`` 同款堆叠到 BOTTOM_RIGHT 栈;
          3. 新 InfoBar 因为是全新创建, sizeHint 完全按文本自适应,
             与同栈的 ``success_bar`` 视觉一致.
        """
        bar = self._resolve_bar(task_id)
        self._bars.pop(task_id, None)
        if bar is None:
            return

        # 收集旧 bar 的展示参数, 用作新 InfoBar 的输入
        title = bar.title or ""
        content = message or bar.content or ""
        position = bar.position
        parent = bar.parent()

        # 1) 立刻关闭老 chip (跳过 1.5s 淡出); manager.remove 会自动顺移其他 chip
        try:
            bar.close()
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"关闭旧 ProgressInfoBar 失败 (task={task_id}): "
                f"{type(exc).__name__}: {exc}",
                log_source=LogSource.UI,
            )

        if parent is None:
            return

        # 2) 根据 success / error 走对应的 InfoBar 工厂; 与项目里 success_bar / error_bar
        #    使用同一套渲染路径, sizeHint / 颜色 / 图标都一致.
        from qfluentwidgets import InfoBar
        from PySide6.QtCore import Qt

        factory = InfoBar.success if success else InfoBar.error
        try:
            factory(
                title=title,
                content=content,
                orient=Qt.Orientation.Horizontal,
                isClosable=True,
                duration=max(self._auto_close_after, 1500),
                position=position,
                parent=parent,
            )
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"创建完成态 InfoBar 失败 (task={task_id}, success={success}): "
                f"{type(exc).__name__}: {exc}",
                log_source=LogSource.UI,
            )

    # ==================== 内部辅助 ====================
    def _relayout_bar(self, bar: "ProgressInfoBar") -> None:
        """setComplete 后强制 InfoBar 自适应宽度 + 通知 manager 重新堆叠位置."""
        old_w, old_x = bar.width(), bar.x()

        # 1) 让 InfoBar 按新 content 重算文字区域 (setTitle/setContent 已调过, 但
        #    Qt 的 sizeHint 在 layout 重新激活前不一定立刻生效, 这里再强制刷一次).
        try:
            bar._adjustText()
        except Exception:  # noqa: BLE001 - 私有 API, 不存在就跳过, 继续走 adjustSize
            pass
        # 让所有嵌套 layout 失效, 强制重算 sizeHint (光 adjustSize 不够 - qfluentwidgets
        # 的 InfoBar 在 setComplete 后 sizeHint 仍然是 indeterminate 状态时的值).
        layout = bar.layout()
        if layout is not None:
            layout.invalidate()
            layout.activate()
        bar.updateGeometry()
        bar.adjustSize()

        # 2) Qt 的 adjustSize 仍然受到 sizeHint 限制. 如果 chip 实际文本宽于 sizeHint
        #    (常见于 ProgressInfoBar 完成态比 indeterminate 文案更长), 用 fontMetrics
        #    估算所需宽度并 setMinimumWidth 兜底, 保证文本完整显示.
        self._ensure_min_width_for_text(bar)

        new_w = bar.width()

        # 2) 通知 InfoBarManager 重新定位本 chip + 同栈其他 chip.
        try:
            from qfluentwidgets import InfoBarManager

            manager = InfoBarManager.make(bar.position)
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"InfoBarManager.make({bar.position!r}) 失败, 跳过重排: "
                f"{type(exc).__name__}: {exc}",
                log_source=LogSource.UI,
            )
            return

        parent = bar.parent()
        new_pos = manager._pos(bar)
        if parent is None:
            bar.move(new_pos)
        else:
            for peer in list(manager.infoBars.get(parent, [])):
                try:
                    peer.move(manager._pos(peer))
                except Exception:  # noqa: BLE001
                    continue

        logger.debug(
            f"ProgressInfoBar 重排 (mgr={type(manager).__name__}): "
            f"width {old_w}->{new_w}, x {old_x}->{new_pos.x()}, y={new_pos.y()}, "
            f"parent={type(parent).__name__ if parent else None}, "
            f"parent_w={parent.width() if parent else '?'}",
            log_source=LogSource.UI,
        )

    # 完成态 chip 的兜底最小宽度. 与项目里 success_bar / info_bar 渲染出的常见
    # InfoBar 视觉宽度对齐 (~320 px), 避免 ProgressInfoBar 比同栈的 success_bar
    # 视觉上明显窄一截.
    _MIN_COMPLETE_WIDTH = 320

    def _ensure_min_width_for_text(self, bar: "ProgressInfoBar") -> None:
        """强制 ProgressInfoBar 完成态宽度足以容纳 title / content 文本.

        qfluentwidgets 的 ``InfoBar.adjustSize`` 在 setComplete 切换 IconWidget 之后
        计算出的 sizeHint 仍然是 indeterminate 状态时的(更短)值, 导致完成态文案被裁切.
        策略:
          1. 基线: ``_MIN_COMPLETE_WIDTH`` (与同栈 success_bar 宽度对齐)
          2. 文本兜底: fontMetrics 量出 title / content 实际像素宽度 + chrome 预算,
             如果超过基线则按文本宽度.
          3. 上限: 不超过 parent 可视宽度的 85%, 极端长文案不会贴满窗口.
        """
        try:
            title_label = bar.titleLabel
            content_label = bar.contentLabel
            title_w = title_label.fontMetrics().horizontalAdvance(title_label.text())
            content_w = content_label.fontMetrics().horizontalAdvance(content_label.text())
        except Exception:  # noqa: BLE001 - 量不出来就放弃兜底, 不阻塞主流程
            title_w = content_w = 0

        # 预算:
        #   左侧 padding(16) + icon(30) + icon→text gap(16) +
        #   右侧 close button(30) + button padding(16) + 安全余量(20)
        chrome_budget = 16 + 30 + 16 + 30 + 16 + 20
        text_w = max(title_w, content_w)
        desired_w = max(self._MIN_COMPLETE_WIDTH, text_w + chrome_budget)

        parent = bar.parent()
        if parent is not None:
            cap = max(int(parent.width() * 0.85) - 24, self._MIN_COMPLETE_WIDTH)
            desired_w = min(desired_w, cap)

        logger.debug(
            f"ProgressInfoBar 兜底宽度: title_w={title_w}, content_w={content_w}, "
            f"chrome={chrome_budget}, desired={desired_w}, current={bar.width()}",
            log_source=LogSource.UI,
        )

        if desired_w > bar.width():
            bar.setMinimumWidth(desired_w)
            bar.resize(desired_w, bar.height())

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

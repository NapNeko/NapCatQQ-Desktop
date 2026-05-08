# -*- coding: utf-8 -*-
"""[`StatusOverviewDialog`](src/ui/components/status_overview_dialog.py): 状态聚合面板 (P4 W2.F4).

设计要点
========

三栏只读对话框, 在 RemotePage 工具栏新增按钮入口打开.

- **栏 1: 服务器列表** - 名称 / 部署状态 / 资源水位 (CPU / Mem / Disk).
  数据由 [`ServerManager.list_servers()`](src/core/remote/server_manager.py) 提供,
  水位实时来自 [`ResourceMonitorService.latest`](src/core/remote/resource_monitor.py).
- **栏 2: 远端 Bot 列表** - 名称 / runtime_target / 进程状态.
  数据通过 [`read_config()`](src/core/config/operate_config.py) 拉本地 + 远端 Bot 配置,
  进程状态消费 [`ManagerNapCatQQProcess.process_changed_signal`](src/core/runtime/napcat.py).
- **栏 3: 后台任务** - 直接渲染
  [`BackgroundTaskCenter.active_tasks()`](src/core/runtime/background_tasks.py).

不引入新单例 / 不写持久化; 所有信号订阅都在 ``showEvent`` 时挂上, 关闭后断开,
避免对话框生命周期外泄.
"""
from __future__ import annotations

# 标准库导入
from typing import Any

# 第三方库导入
from PySide6.QtCore import QProcess, Qt
from PySide6.QtWidgets import (
    QFrame,
    QHBoxLayout,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)
from creart import it
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    MessageBoxBase,
    ScrollArea,
    StrongBodyLabel,
    SubtitleLabel,
)


# ==================== metric / state 文案辅助 ====================
_DEPLOY_STATE_LABEL = {
    "undeployed": "未部署",
    "deploying": "部署中",
    "deployed": "已部署",
    "failed": "部署失败",
}

_PROCESS_STATE_LABEL = {
    QProcess.ProcessState.NotRunning: "未运行",
    QProcess.ProcessState.Starting: "启动中",
    QProcess.ProcessState.Running: "运行中",
}


def _format_resource_line(sample: Any) -> str:
    """格式化 ``ResourceSample`` 为单行 ``CPU x% . MEM y% . DISK z%``."""
    if sample is None:
        return "—"
    return (
        f"CPU {sample.cpu_percent:.0f}% · "
        f"MEM {sample.mem_percent:.0f}% · "
        f"DISK {sample.disk_percent:.0f}%"
    )


# ==================== 子组件: 列表行 / 空态 ====================
class _OverviewItem(QFrame):
    """单条数据卡, 两行显示: primary 主名称 + secondary 副信息.

    Args:
        primary: 主文案 (BodyLabel, 如服务器名 / Bot 名 / 任务标签).
        secondary: 副文案 (CaptionLabel, 灰色; 如状态 / runtime_target / 资源水位).
            None 或空字符串时不显示该行.
    """

    def __init__(self, primary: str, secondary: str | None = None, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("overviewItem")
        # 风格参考 PyQt-Fluent-Widgets-Pro 的 RoundTableWidget: 每行独立圆角矩形 "pill",
        # 无分隔线, 行间留空隙靠 container_layout 的 spacing 控制; hover 时色块加深.
        self.setStyleSheet(
            "#overviewItem {"
            "  background-color: rgba(0, 0, 0, 0.035);"
            "  border: none;"
            "  border-radius: 6px;"
            "}"
            "#overviewItem:hover {"
            "  background-color: rgba(0, 0, 0, 0.06);"
            "}"
        )

        layout = QVBoxLayout(self)
        layout.setContentsMargins(12, 10, 12, 10)
        layout.setSpacing(3)

        primary_label = BodyLabel(primary, self)
        primary_label.setWordWrap(True)
        layout.addWidget(primary_label)

        if secondary:
            secondary_label = CaptionLabel(secondary, self)
            secondary_label.setWordWrap(True)
            secondary_label.setStyleSheet("color: rgba(0, 0, 0, 0.55);")
            layout.addWidget(secondary_label)


class _EmptyItem(QFrame):
    """空态占位, 居中显示弱化文案."""

    def __init__(self, text: str, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(4, 24, 4, 24)
        layout.setSpacing(0)

        label = CaptionLabel(text, self)
        label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        label.setWordWrap(True)
        label.setStyleSheet("color: rgba(0, 0, 0, 0.45);")
        layout.addWidget(label)


# ==================== 主对话框 ====================
class StatusOverviewDialog(MessageBoxBase):
    """三栏状态聚合面板, 在 RemotePage 工具栏入口打开."""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._signal_connections: list[tuple[Any, Any, Any]] = []
        self._setup_ui()
        self._wire_buttons()
        self._wire_signals()
        self.refresh_full()
        # 默认尺寸: 略放宽宽度以容纳 Bot 栏的 "名称 (QQID)" + "服务器 . 状态" 两行;
        # 高度收紧到 380, 数据多时由 ScrollArea 自动滚动.
        self.widget.setMinimumSize(780, 380)

    # ==================== UI ====================
    def _setup_ui(self) -> None:
        self._title_label = SubtitleLabel(self.tr("远端状态总览"), self)
        self._caption_label = CaptionLabel(
            self.tr("聚合服务器在线状态、远端 Bot 进程状态与后台任务进度"),
            self,
        )
        self._caption_label.setWordWrap(True)
        self._caption_label.setStyleSheet("color: rgba(0, 0, 0, 0.55);")

        columns = QWidget(self)
        columns_layout = QHBoxLayout(columns)
        columns_layout.setContentsMargins(0, 0, 0, 0)
        columns_layout.setSpacing(16)

        # P4 W4 重构: 移除每栏副标题占位灰字, 标题单独成行;
        # 列表区改用 QScrollArea + QVBoxLayout 容器, 单条数据用 _OverviewItem 两行展示.
        self._server_layout, server_panel = self._build_column(self.tr("服务器"))
        self._bot_layout, bot_panel = self._build_column(self.tr("远端 Bot"))
        self._task_layout, task_panel = self._build_column(self.tr("后台任务"))

        # 中间 Bot 栏需要更多宽度容纳 "名字 (QQID)" + "服务器名 . 进程状态" 两行
        columns_layout.addWidget(server_panel, 3)
        columns_layout.addWidget(bot_panel, 5)
        columns_layout.addWidget(task_panel, 3)

        self.viewLayout.addWidget(self._title_label)
        self.viewLayout.addWidget(self._caption_label)
        self.viewLayout.addSpacing(10)
        self.viewLayout.addWidget(columns, 1)

    def _build_column(self, title: str) -> tuple[QVBoxLayout, QWidget]:
        """构造单栏 (标题 + 滚动列表区).

        Returns:
            (container_layout, panel): container_layout 是滚动区内 inner widget 的
            QVBoxLayout, ``_refresh_*`` 直接 ``addWidget`` / 清空它来更新数据.
        """
        panel = QWidget(self)
        panel_layout = QVBoxLayout(panel)
        panel_layout.setContentsMargins(0, 0, 0, 0)
        panel_layout.setSpacing(10)

        # 标题左 padding 4px 与下方 pill 内文字左对齐 (pill 自身 contentsMargin-left 12,
        # 这里 panel 不缩进 4 看着错位较少; 较小值避免标题过度内缩).
        title_label = StrongBodyLabel(title, panel)
        title_label.setContentsMargins(4, 0, 0, 0)

        # 滚动区: 透明背景, 无边框; 通过 ScrollArea + inner widget 实现垂直列表
        scroll = ScrollArea(panel)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setStyleSheet(
            "QScrollArea { background-color: transparent; border: none; }"
            "QScrollArea > QWidget > QWidget { background-color: transparent; }"
        )
        scroll.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        inner = QWidget(scroll)
        container_layout = QVBoxLayout(inner)
        container_layout.setContentsMargins(0, 0, 0, 0)
        # 行间 6px 空隙: 让每个 _OverviewItem 作为独立 "pill" 呈现 (RoundTableWidget 风格)
        container_layout.setSpacing(6)
        container_layout.addStretch(1)
        scroll.setWidget(inner)

        panel_layout.addWidget(title_label)
        panel_layout.addWidget(scroll, 1)
        return container_layout, panel

    @staticmethod
    def _clear_layout(layout: QVBoxLayout) -> None:
        """清空 layout 全部 item (widget + stretch), 重新追加单个 stretch.

        必须把旧 stretch 也清掉, 否则每次 refresh 后 ``addStretch`` 会累积,
        导致 ``_append_item`` 插入的 widget 被前序 stretch 挤到列中央.

        注意: 旧 widget 在 layout 中是可见的, 直接 ``setParent(None)`` 会让
        它们以 ``isVisible()=true`` 的状态脱离父级, 在 ``deleteLater`` 处理前
        会被 Qt 提升为顶层窗口 (标题 "python"), 表现为 "刷新时弹窗" 的 bug;
        因此先 ``hide()`` 再断开父级.
        """
        while layout.count() > 0:
            item = layout.takeAt(0)
            if item is None:
                continue
            widget = item.widget()
            if widget is not None:
                widget.hide()
                widget.setParent(None)
                widget.deleteLater()
        layout.addStretch(1)

    @staticmethod
    def _append_item(layout: QVBoxLayout, widget: QWidget) -> None:
        """把单条 item 插到 layout 末尾 stretch 之前."""
        # stretch 永远在最后; 在它前一个位置插入
        layout.insertWidget(layout.count() - 1, widget)

    def _wire_buttons(self) -> None:
        # MessageBoxBase 默认 yes/cancel 按钮; 这里只保留 "刷新" + "关闭"
        self.yesButton.setText(self.tr("刷新"))
        self.cancelButton.setText(self.tr("关闭"))
        # yes 不关闭对话框, 改成手动刷新
        self.yesButton.clicked.disconnect()
        self.yesButton.clicked.connect(self.refresh_full)

    # ==================== 信号订阅 ====================
    def _wire_signals(self) -> None:
        """挂接现有四个数据源的信号 -> 对应栏目刷新; 关闭时统一断开."""
        try:
            from src.core.remote.server_manager import ServerManager

            mgr = it(ServerManager)
            self._safe_connect(mgr.server_added, self._refresh_servers)
            self._safe_connect(mgr.server_updated, self._refresh_servers)
            self._safe_connect(mgr.server_removed, self._refresh_servers)
            self._safe_connect(mgr.server_state_changed, self._refresh_servers)
        except Exception:  # noqa: BLE001
            pass

        try:
            from src.core.remote.resource_monitor import ResourceMonitorService

            svc = it(ResourceMonitorService)
            self._safe_connect(svc.sample_arrived, self._refresh_servers)
        except Exception:  # noqa: BLE001
            pass

        try:
            from src.core.runtime.napcat import ManagerNapCatQQProcess

            mgr = it(ManagerNapCatQQProcess)
            self._safe_connect(mgr.process_changed_signal, self._refresh_bots)
        except Exception:  # noqa: BLE001
            pass

        try:
            from src.core.runtime.background_tasks import BackgroundTaskCenter

            center = it(BackgroundTaskCenter)
            self._safe_connect(center.task_started_signal, self._refresh_tasks)
            self._safe_connect(center.task_finished_signal, self._refresh_tasks)
        except Exception:  # noqa: BLE001
            pass

    def _safe_connect(self, signal: Any, slot: Any) -> None:
        """记录连接, 关闭时统一断开."""
        try:
            signal.connect(slot, Qt.ConnectionType.QueuedConnection)
            self._signal_connections.append((signal, slot, None))
        except Exception:  # noqa: BLE001
            pass

    def _disconnect_all(self) -> None:
        for signal, slot, _ in self._signal_connections:
            try:
                signal.disconnect(slot)
            except Exception:  # noqa: BLE001
                pass
        self._signal_connections.clear()

    # ==================== 数据刷新 ====================
    def refresh_full(self) -> None:
        self._refresh_servers()
        self._refresh_bots()
        self._refresh_tasks()

    # ServerManager / ResourceMonitorService 信号都路由到这里; 兼容 0~2 个位置参
    def _refresh_servers(self, *_args: Any, **_kwargs: Any) -> None:
        self._clear_layout(self._server_layout)
        try:
            from src.core.remote.resource_monitor import ResourceMonitorService
            from src.core.remote.server_manager import ServerManager

            servers = it(ServerManager).list_servers()
            monitor = self._safe_get(ResourceMonitorService)
        except Exception:  # noqa: BLE001
            servers, monitor = [], None

        if not servers:
            self._append_item(self._server_layout, _EmptyItem(self.tr("尚无服务器档案"), self))
            return

        for profile in servers:
            state_text = _DEPLOY_STATE_LABEL.get(
                profile.deployment_state.value, profile.deployment_state.value
            )
            sample = monitor.latest(profile.id) if monitor is not None else None
            res_text = _format_resource_line(sample)
            secondary = f"{state_text} · {res_text}"
            self._append_item(self._server_layout, _OverviewItem(profile.name, secondary, self))

    def _refresh_bots(self, *_args: Any, **_kwargs: Any) -> None:
        self._clear_layout(self._bot_layout)
        try:
            from src.core.config.operate_config import read_config
            from src.core.runtime.napcat import ManagerNapCatQQProcess

            configs = read_config()
            mgr = it(ManagerNapCatQQProcess)
        except Exception:  # noqa: BLE001
            configs, mgr = [], None

        # 仅显示远端 Bot (本地 Bot 由 BotPage 主面板覆盖)
        remote_bots = [c for c in configs if c.bot.is_remote]
        if not remote_bots:
            self._append_item(
                self._bot_layout, _EmptyItem(self.tr("当前没有远端运行的 Bot"), self)
            )
            return

        # 一次性拉服务器名映射, 把 runtime_target (server_id UUID) 解析为可读名;
        # 失败时回退到 UUID 截短.
        server_name_map = self._collect_server_name_map()

        for cfg in remote_bots:
            qq_id = str(cfg.bot.QQID)
            state = QProcess.ProcessState.NotRunning
            if mgr is not None:
                record = mgr.remote_process_dict.get(qq_id)
                if record is not None:
                    state = record.state
            state_text = _PROCESS_STATE_LABEL.get(state, str(state))

            target_id = cfg.bot.runtime_target or ""
            target_label = server_name_map.get(target_id) or self._truncate_uuid(target_id)

            primary = f"{cfg.bot.name} ({qq_id})"
            secondary = f"{target_label} · {state_text}" if target_label else state_text
            self._append_item(self._bot_layout, _OverviewItem(primary, secondary, self))

    def _refresh_tasks(self, *_args: Any, **_kwargs: Any) -> None:
        self._clear_layout(self._task_layout)
        try:
            from src.core.runtime.background_tasks import BackgroundTaskCenter

            tasks = it(BackgroundTaskCenter).active_tasks()
        except Exception:  # noqa: BLE001
            tasks = []

        if not tasks:
            self._append_item(
                self._task_layout, _EmptyItem(self.tr("没有进行中的后台任务"), self)
            )
            return
        for task in tasks:
            content = task.content or ""
            self._append_item(
                self._task_layout, _OverviewItem(task.label, content or None, self)
            )

    # ---------- 名称解析辅助 ----------
    @staticmethod
    def _collect_server_name_map() -> dict[str, str]:
        """返回 ``{server_id: server_name}`` 映射; 失败时返回空字典."""
        try:
            from src.core.remote.server_manager import ServerManager

            return {s.id: s.name for s in it(ServerManager).list_servers()}
        except Exception:  # noqa: BLE001
            return {}

    @staticmethod
    def _truncate_uuid(value: str) -> str:
        """UUID 太长时截短为前 8 位, 避免压垮列宽."""
        if not value:
            return ""
        if len(value) <= 12:
            return value
        return value[:8] + "…"

    @staticmethod
    def _safe_get(klass: Any) -> Any | None:
        try:
            return it(klass)
        except Exception:  # noqa: BLE001
            return None

    # ==================== 生命周期 ====================
    def closeEvent(self, event: Any) -> None:  # noqa: N802 - Qt 命名固定
        self._disconnect_all()
        super().closeEvent(event)

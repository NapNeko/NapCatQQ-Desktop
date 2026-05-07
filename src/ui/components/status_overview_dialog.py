# -*- coding: utf-8 -*-
"""[`StatusOverviewDialog`](src/ui/components/status_overview_dialog.py): 状态聚合面板 (P4 W2·F4).

设计要点
========

三栏只读对话框, 在 RemotePage 工具栏新增按钮入口打开.

- **栏 1: 服务器列表** — 名称 / 部署状态 / 资源水位 (CPU / Mem / Disk).
  数据由 [`ServerManager.list_servers()`](src/core/remote/server_manager.py) 提供,
  水位实时来自 [`ResourceMonitorService.latest`](src/core/remote/resource_monitor.py).
- **栏 2: 远端 Bot 列表** — 名称 / runtime_target / 进程状态.
  数据通过 [`read_config()`](src/core/config/operate_config.py) 拉本地 + 远端 Bot 配置,
  进程状态消费 [`ManagerNapCatQQProcess.process_changed_signal`](src/core/runtime/napcat.py).
- **栏 3: 后台任务** — 直接渲染
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
    QHBoxLayout,
    QListWidget,
    QListWidgetItem,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)
from creart import it
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    FluentIcon,
    MessageBoxBase,
    PrimaryPushButton,
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
    """格式化 ``ResourceSample`` 为单行 ``CPU x% · MEM y% · DISK z%``."""
    if sample is None:
        return "—"
    return (
        f"CPU {sample.cpu_percent:.0f}% · "
        f"MEM {sample.mem_percent:.0f}% · "
        f"DISK {sample.disk_percent:.0f}%"
    )


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
        # 适当扩大默认尺寸; MessageBoxBase 内部 widget 是 ``self.widget``.
        self.widget.setMinimumSize(820, 460)

    # ==================== UI ====================
    def _setup_ui(self) -> None:
        self._title_label = SubtitleLabel(self.tr("远端状态总览"), self)
        self._caption_label = CaptionLabel(
            self.tr("聚合服务器在线状态、远端 Bot 进程状态与后台任务进度."),
            self,
        )
        self._caption_label.setWordWrap(True)

        columns = QWidget(self)
        columns_layout = QHBoxLayout(columns)
        columns_layout.setContentsMargins(0, 0, 0, 0)
        columns_layout.setSpacing(12)

        self._server_list, server_panel = self._build_column(
            self.tr("服务器"),
            self.tr("名称 · 状态 · 资源水位"),
        )
        self._bot_list, bot_panel = self._build_column(
            self.tr("远端 Bot"),
            self.tr("名称 · 运行位置 · 进程状态"),
        )
        self._task_list, task_panel = self._build_column(
            self.tr("后台任务"),
            self.tr("正在进行中的批量 / 部署 / 迁移操作"),
        )

        columns_layout.addWidget(server_panel, 1)
        columns_layout.addWidget(bot_panel, 1)
        columns_layout.addWidget(task_panel, 1)

        self.viewLayout.addWidget(self._title_label)
        self.viewLayout.addWidget(self._caption_label)
        self.viewLayout.addSpacing(6)
        self.viewLayout.addWidget(columns, 1)

    def _build_column(self, title: str, subtitle: str) -> tuple[QListWidget, QWidget]:
        panel = QWidget(self)
        panel_layout = QVBoxLayout(panel)
        panel_layout.setContentsMargins(0, 0, 0, 0)
        panel_layout.setSpacing(4)

        title_label = StrongBodyLabel(title, panel)
        subtitle_label = CaptionLabel(subtitle, panel)
        subtitle_label.setStyleSheet("color: #6b7280;")

        list_widget = QListWidget(panel)
        list_widget.setSelectionMode(QListWidget.SelectionMode.NoSelection)
        list_widget.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        list_widget.setStyleSheet(
            "QListWidget { background-color: transparent; border: none; }"
            "QListWidget::item { padding: 6px 4px; }"
        )

        panel_layout.addWidget(title_label)
        panel_layout.addWidget(subtitle_label)
        panel_layout.addWidget(list_widget, 1)
        return list_widget, panel

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
        self._server_list.clear()
        try:
            from src.core.remote.resource_monitor import ResourceMonitorService
            from src.core.remote.server_manager import ServerManager

            servers = it(ServerManager).list_servers()
            monitor = self._safe_get(ResourceMonitorService)
        except Exception:  # noqa: BLE001
            servers, monitor = [], None

        if not servers:
            self._server_list.addItem(QListWidgetItem(self.tr("尚无服务器档案")))
            return

        for profile in servers:
            state_text = _DEPLOY_STATE_LABEL.get(
                profile.deployment_state.value, profile.deployment_state.value
            )
            sample = monitor.latest(profile.id) if monitor is not None else None
            res_text = _format_resource_line(sample)
            line = f"{profile.name} · {state_text} · {res_text}"
            self._server_list.addItem(QListWidgetItem(line))

    def _refresh_bots(self, *_args: Any, **_kwargs: Any) -> None:
        self._bot_list.clear()
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
            self._bot_list.addItem(QListWidgetItem(self.tr("当前没有远端运行的 Bot")))
            return

        for cfg in remote_bots:
            qq_id = str(cfg.bot.QQID)
            state = QProcess.ProcessState.NotRunning
            if mgr is not None:
                record = mgr.remote_process_dict.get(qq_id)
                if record is not None:
                    state = record.state
            state_text = _PROCESS_STATE_LABEL.get(state, str(state))
            target = cfg.bot.runtime_target
            line = f"{cfg.bot.name} ({qq_id}) · {target} · {state_text}"
            self._bot_list.addItem(QListWidgetItem(line))

    def _refresh_tasks(self, *_args: Any, **_kwargs: Any) -> None:
        self._task_list.clear()
        try:
            from src.core.runtime.background_tasks import BackgroundTaskCenter

            tasks = it(BackgroundTaskCenter).active_tasks()
        except Exception:  # noqa: BLE001
            tasks = []

        if not tasks:
            self._task_list.addItem(QListWidgetItem(self.tr("没有进行中的后台任务")))
            return
        for task in tasks:
            content = task.content or ""
            line = f"{task.label} · {content}" if content else task.label
            self._task_list.addItem(QListWidgetItem(line))

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

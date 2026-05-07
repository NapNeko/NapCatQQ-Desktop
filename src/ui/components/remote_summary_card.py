# -*- coding: utf-8 -*-
"""[`RemoteSummaryCard`](src/ui/components/remote_summary_card.py): 首页远端概览卡片 (P4 W2·F4).

设计要点
========

- 数据源全部消费现有单例信号, **不**新增持久存储:
  - [`ServerManager`](src/core/remote/server_manager.py): 服务器总数 / 在线服务器数
    (deployment_state == DEPLOYED 视为"在线")
  - [`ManagerNapCatQQProcess`](src/core/runtime/napcat.py): 在线远端 Bot 数
    (``remote_process_dict`` 中 ``state == Running``)
  - [`ResourceMonitorService`](src/core/remote/resource_monitor.py): 最近一条阈值告警 (24h 内)
- 空态 (``ServerManager.list_servers() == []``) 折叠为单行
  "尚未添加远端服务器, 点此添加".
- 整卡可点击; 点击发射 ``navigate_to_remote_signal(server_id_or_empty: str)``,
  由 HomeWidget / MainWindow 接管路由 (本卡片不直接持有 RemotePage 引用,
  保持 UI 解耦).
- 不在 ``__init__`` 触发 ``ResourceMonitorService.bind_to_server_manager()``;
  只接 ``threshold_breached`` 信号读取最近告警, 由谁先 bind 决定 worker 是否启动.
"""
from __future__ import annotations

# 标准库导入
import time
from typing import Any

# 第三方库导入
from PySide6.QtCore import QProcess, Qt, Signal
from PySide6.QtGui import QMouseEvent
from PySide6.QtWidgets import QHBoxLayout, QLabel, QSizePolicy, QVBoxLayout
from creart import it
from qfluentwidgets import BodyLabel, FluentIcon, IconWidget, SimpleCardWidget, StrongBodyLabel


# ==================== 工具: 友好 metric 文案 ====================
_METRIC_LABEL = {"cpu": "CPU", "mem": "内存", "disk": "磁盘"}


def _format_breach(server_name: str, metric: str, value: float, ts: float) -> str:
    """生成最近告警的单行展示文案."""
    label = _METRIC_LABEL.get(metric, metric.upper())
    elapsed_min = max(0, int((time.time() - ts) // 60))
    suffix = f"{elapsed_min} 分钟前" if elapsed_min > 0 else "刚刚"
    return f"{server_name} · {label} {value:.0f}% · {suffix}"


# ==================== 主组件 ====================
class RemoteSummaryCard(SimpleCardWidget):
    """首页顶部远端概览卡片 (P4 F4).

    Signals:
        navigate_to_remote_signal (str): 用户点击卡片时发射, 参数为 server_id;
            空字符串表示"打开 RemotePage 但不指定服务器".
    """

    BREACH_VALID_WINDOW = 24 * 3600  # 24h 内的告警才显示

    navigate_to_remote_signal = Signal(str)

    def __init__(self, parent: Any = None) -> None:
        super().__init__(parent)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
        # (server_id, metric, value, ts) — 最近一条; None 表示无告警
        self._latest_breach: tuple[str, str, float, float] | None = None

        self._create_widgets()
        self._set_layout()
        self._wire_signals()
        self.refresh()

    # ---------- UI ----------
    def _create_widgets(self) -> None:
        self._title_icon = IconWidget(FluentIcon.GLOBE, self)
        self._title_icon.setFixedSize(18, 18)
        self._title_label = StrongBodyLabel(self.tr("远端概览"), self)
        # 数字行: 服务器 N · 在线 X · 远端 Bot Y
        self._summary_label = BodyLabel("", self)
        self._summary_label.setWordWrap(True)
        # 告警行 / 空态行
        self._secondary_label = BodyLabel("", self)
        self._secondary_label.setWordWrap(True)

    def _set_layout(self) -> None:
        outer = QVBoxLayout(self)
        outer.setContentsMargins(20, 14, 20, 14)
        outer.setSpacing(6)

        title_row = QHBoxLayout()
        title_row.setSpacing(8)
        title_row.setContentsMargins(0, 0, 0, 0)
        title_row.addWidget(self._title_icon, 0, Qt.AlignmentFlag.AlignVCenter)
        title_row.addWidget(self._title_label, 0, Qt.AlignmentFlag.AlignVCenter)
        title_row.addStretch(1)

        outer.addLayout(title_row)
        outer.addWidget(self._summary_label)
        outer.addWidget(self._secondary_label)

    def _wire_signals(self) -> None:
        # ServerManager: 服务器增删改 / 部署状态 -> 计数刷新
        try:
            from src.core.remote.server_manager import ServerManager

            manager = it(ServerManager)
            manager.server_added.connect(lambda _id: self.refresh())
            manager.server_removed.connect(lambda _id: self.refresh())
            manager.server_updated.connect(lambda _id: self.refresh())
            manager.server_state_changed.connect(lambda _id, _state: self.refresh())
        except Exception:  # noqa: BLE001 - UI 容错: 单例不可用时降级到只读静态文案
            pass

        # ManagerNapCatQQProcess: 远端 Bot 状态变化 -> 计数刷新
        try:
            from src.core.runtime.napcat import ManagerNapCatQQProcess

            it(ManagerNapCatQQProcess).process_changed_signal.connect(
                lambda _qq, _state: self.refresh()
            )
        except Exception:  # noqa: BLE001
            pass

        # ResourceMonitorService: 阈值告警 -> 缓存最近一次 + 刷新
        try:
            from src.core.remote.resource_monitor import ResourceMonitorService

            it(ResourceMonitorService).threshold_breached.connect(self._on_threshold_breached)
        except Exception:  # noqa: BLE001
            pass

    # ---------- 数据装配 ----------
    def refresh(self) -> None:
        """重新计算计数与告警行, 写到 UI."""
        total, online, online_remote_bots = self._collect_counts()

        if total == 0:
            self._render_empty_state()
            return

        self._summary_label.setText(
            self.tr("服务器 {total} · 在线 {online} · 远端 Bot {bots}").format(
                total=total, online=online, bots=online_remote_bots
            )
        )

        breach_text = self._format_recent_breach()
        if breach_text:
            self._secondary_label.setText(breach_text)
        else:
            self._secondary_label.setText(self.tr("点击查看远端服务器详情"))
        self._secondary_label.setVisible(True)

    def _render_empty_state(self) -> None:
        """无远端服务器时折叠为引导式单行."""
        self._summary_label.setText(self.tr("尚未添加远端服务器, 点此添加"))
        self._secondary_label.setText("")
        self._secondary_label.setVisible(False)

    def _collect_counts(self) -> tuple[int, int, int]:
        """返回 (服务器总数, 在线服务器数, 在线远端 Bot 数)."""
        total = 0
        online = 0
        try:
            from src.core.remote.server_manager import ServerManager
            from src.core.remote.servers import DeploymentState

            manager = it(ServerManager)
            servers = manager.list_servers()
            total = len(servers)
            online = sum(1 for s in servers if s.deployment_state == DeploymentState.DEPLOYED)
        except Exception:  # noqa: BLE001
            pass

        bots = 0
        try:
            from src.core.runtime.napcat import ManagerNapCatQQProcess

            mgr = it(ManagerNapCatQQProcess)
            bots = sum(
                1
                for record in mgr.remote_process_dict.values()
                if record.state == QProcess.ProcessState.Running
            )
        except Exception:  # noqa: BLE001
            pass

        return total, online, bots

    def _format_recent_breach(self) -> str:
        """24h 内最近一条告警的展示文案; 不存在或已过期返回空."""
        if self._latest_breach is None:
            return ""
        server_id, metric, value, ts = self._latest_breach
        if (time.time() - ts) > self.BREACH_VALID_WINDOW:
            return ""
        # 解析服务器名称, 找不到时回落到 server_id
        server_name = self._resolve_server_name(server_id) or server_id
        return _format_breach(server_name, metric, value, ts)

    def _resolve_server_name(self, server_id: str) -> str | None:
        try:
            from src.core.remote.server_manager import ServerManager

            profile = it(ServerManager).get_server(server_id)
            return profile.name if profile is not None else None
        except Exception:  # noqa: BLE001
            return None

    # ---------- 信号处理 ----------
    def _on_threshold_breached(self, server_id: str, metric: str, value: float) -> None:
        """缓存最近告警并立即刷新 UI."""
        self._latest_breach = (server_id, metric, value, time.time())
        self.refresh()

    # ---------- 鼠标交互: 整卡可点击 ----------
    def mousePressEvent(self, event: QMouseEvent) -> None:  # noqa: N802 - Qt 命名固定
        if event.button() == Qt.MouseButton.LeftButton:
            target_id = ""
            if self._latest_breach is not None:
                # 24h 内有告警: 点击直接定位到告警相关服务器
                _ts_now = time.time()
                if (_ts_now - self._latest_breach[3]) <= self.BREACH_VALID_WINDOW:
                    target_id = self._latest_breach[0]
            self.navigate_to_remote_signal.emit(target_id)
        super().mousePressEvent(event)

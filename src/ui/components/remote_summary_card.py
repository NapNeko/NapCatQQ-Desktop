# -*- coding: utf-8 -*-
"""[`RemoteSummaryCard`](src/ui/components/remote_summary_card.py): 首页远端概览卡片 (P4 W2·F4 / P4 W4 视觉重构).

设计要点
========

- 数据源全部消费现有单例信号, **不**新增持久存储:
  - [`ServerManager`](src/core/remote/server_manager.py): 服务器总数 / 在线服务器数
    (deployment_state == DEPLOYED 视为"在线")
  - [`ManagerNapCatQQProcess`](src/core/runtime/napcat.py): 在线远端 Bot 数
    (``remote_process_dict`` 中 ``state == Running``)
  - [`ResourceMonitorService`](src/core/remote/resource_monitor.py): 最近一条阈值告警 (24h 内)
- 视觉布局 (P4 W4 重构, 修字体重叠 + 提升信息密度):
    1. **标题行**: 图标 + "远端概览" 标题 + 右侧 `›` 指示卡片可点击.
    2. **KPI 行**: 3 列等宽 ``_MetricBlock`` (大数字 + 小标签); 大数字用
       ``SubtitleLabel`` 区分主次, 小标签用 ``CaptionLabel`` 弱化.
    3. **告警行 (条件显示)**: 仅在 24h 内有 ``threshold_breached`` 时出现单行 ⓘ 文案;
       无告警时整行收起, 避免与右上 chevron 重复的"点击查看"引导文案.
- 空态 (``ServerManager.list_servers() == []``) 折叠为单行
  "尚未添加远端服务器, 点此添加"; KPI / 告警行整体隐藏.
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
from PySide6.QtWidgets import QHBoxLayout, QSizePolicy, QVBoxLayout, QWidget
from creart import it
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    FluentIcon,
    IconWidget,
    ImageLabel,
    SimpleCardWidget,
    StrongBodyLabel,
    SubtitleLabel,
)


# ==================== 工具: 友好 metric 文案 ====================
_METRIC_LABEL = {"cpu": "CPU", "mem": "内存", "disk": "磁盘"}


def _format_breach(server_name: str, metric: str, value: float, ts: float) -> str:
    """生成最近告警的单行展示文案."""
    label = _METRIC_LABEL.get(metric, metric.upper())
    elapsed_min = max(0, int((time.time() - ts) // 60))
    suffix = f"{elapsed_min} 分钟前" if elapsed_min > 0 else "刚刚"
    return f"{server_name} · {label} {value:.0f}% · {suffix}"


# ==================== 子组件: KPI 块 ====================
class _MetricBlock(QWidget):
    """单个 KPI 块: 一个大数字 (SubtitleLabel) + 一个小标签 (CaptionLabel).

    视觉层级:
        - value: SubtitleLabel, 默认字号 ~20px, 强调读数
        - label: CaptionLabel, 默认字号 ~12px, 弱化为辅助文案
    """

    def __init__(self, value: str, label: str, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.value_label = SubtitleLabel(value, self)
        self.value_label.setAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter)
        self.caption_label = CaptionLabel(label, self)
        self.caption_label.setAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        # 大数字与小标签之间小间距, 避免视觉粘连
        layout.setSpacing(2)
        layout.addWidget(self.value_label)
        layout.addWidget(self.caption_label)

    def set_value(self, value: str) -> None:
        self.value_label.setText(value)


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
        # 鼠标悬停时显示手型, 提示卡片整体可点击
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        # (server_id, metric, value, ts) — 最近一条; None 表示无告警
        self._latest_breach: tuple[str, str, float, float] | None = None

        self._create_widgets()
        self._set_layout()
        self._wire_signals()
        self.refresh()

    # ---------- UI ----------
    def _create_widgets(self) -> None:
        # 标题行
        self._title_icon = ImageLabel(FluentIcon.GLOBE.path(), self)
        self._title_icon.scaledToWidth(16)
        self._title_label = StrongBodyLabel(self.tr("远端概览"), self)

        # KPI 三栏
        self._metric_servers = _MetricBlock("0", self.tr("服务器"), self)
        self._metric_online = _MetricBlock("0", self.tr("在线"), self)
        self._metric_bots = _MetricBlock("0", self.tr("远端 Bot"), self)

        # 告警 / 引导行
        self._secondary_icon = IconWidget(FluentIcon.INFO, self)
        self._secondary_icon.setFixedSize(14, 14)
        self._secondary_label = BodyLabel("", self)
        self._secondary_label.setWordWrap(True)
        # 默认隐藏图标; refresh 中根据告警态显示
        self._secondary_icon.hide()

        # 空态行 (KPI / 告警行被隐藏时显示)
        self._empty_label = BodyLabel(self.tr("尚未添加远端服务器, 点此添加"), self)
        self._empty_label.setWordWrap(True)
        self._empty_label.hide()

    def _set_layout(self) -> None:
        # 外层 22/16 边距 + 14 spacing 防止文字粘连
        outer = QVBoxLayout(self)
        outer.setContentsMargins(22, 16, 22, 16)
        outer.setSpacing(14)

        # 标题行
        title_row = QHBoxLayout()
        title_row.setSpacing(10)
        title_row.setContentsMargins(0, 0, 0, 0)
        title_row.addWidget(self._title_icon, 0, Qt.AlignmentFlag.AlignVCenter)
        title_row.addWidget(self._title_label, 0, Qt.AlignmentFlag.AlignVCenter)
        title_row.addStretch(1)
        outer.addLayout(title_row)

        # 空态行 (默认隐藏, refresh 中显隐)
        outer.addWidget(self._empty_label)

        # KPI 行: 三个等宽 metric block
        self._kpi_row = QHBoxLayout()
        self._kpi_row.setSpacing(20)
        self._kpi_row.setContentsMargins(0, 0, 0, 0)
        self._kpi_row.addWidget(self._metric_servers, 1)
        self._kpi_row.addWidget(self._metric_online, 1)
        self._kpi_row.addWidget(self._metric_bots, 1)
        outer.addLayout(self._kpi_row)

        # 告警 / 引导行
        self._secondary_row = QHBoxLayout()
        self._secondary_row.setSpacing(6)
        self._secondary_row.setContentsMargins(0, 0, 0, 0)
        self._secondary_row.addWidget(self._secondary_icon, 0, Qt.AlignmentFlag.AlignVCenter)
        self._secondary_row.addWidget(self._secondary_label, 1)
        outer.addLayout(self._secondary_row)

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

        # 离开空态: 隐藏 empty_label, 显示 KPI + secondary
        self._empty_label.hide()
        self._set_kpi_visible(True)

        self._metric_servers.set_value(str(total))
        self._metric_online.set_value(str(online))
        self._metric_bots.set_value(str(online_remote_bots))

        # 告警行: 仅在 24h 内有 ``threshold_breached`` 时出现; 无告警则整行收起,
        # 避免渲染 "点击查看远端服务器详情" 这种与右上 chevron 重复的引导文案.
        breach_text = self._format_recent_breach()
        if breach_text:
            self._secondary_icon.setIcon(FluentIcon.INFO)
            self._secondary_icon.show()
            self._secondary_label.setText(breach_text)
            self._secondary_label.setVisible(True)
        else:
            self._secondary_icon.hide()
            self._secondary_label.setText("")
            self._secondary_label.setVisible(False)

    def _render_empty_state(self) -> None:
        """无远端服务器时折叠为引导式单行."""
        self._set_kpi_visible(False)
        self._secondary_icon.hide()
        self._secondary_label.setText("")
        self._secondary_label.setVisible(False)
        self._empty_label.show()

    def _set_kpi_visible(self, visible: bool) -> None:
        """统一控制 KPI 三块的可见性, 避免重复代码."""
        for block in (self._metric_servers, self._metric_online, self._metric_bots):
            block.setVisible(visible)

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

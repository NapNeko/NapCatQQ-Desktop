# -*- coding: utf-8 -*-
"""AI 供应商管理主面板.

左右分栏布局容器: 左侧为固定宽度的供应商列表面板, 右侧为选中供应商的详细配置面板,
中间以垂直分隔线分隔. 界面加载时自动选中第一个供应商.
"""
from __future__ import annotations

from creart import it
from PySide6.QtCore import Signal
from PySide6.QtWidgets import QFrame, QHBoxLayout, QWidget

from src.core.agent.provider import ProviderRegistry

from .provider_detail_panel import ProviderDetailPanel
from .provider_list_panel import ProviderListPanel


class ProviderPanel(QWidget):
    """AI 供应商管理主面板 -- 左右分栏布局容器.

    左侧放置 ProviderListPanel(固定宽度 260px), 右侧放置 ProviderDetailPanel(stretch=1),
    中间添加垂直分隔线. 通过信号连接实现左右面板状态同步.

    Signals:
        provider_selected: 选中供应商时发射, 携带 provider_id.
        provider_updated: 供应商状态变更时发射, 携带 provider_id.
    """

    provider_selected = Signal(str)
    provider_updated = Signal(str)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self.setObjectName("SetupAgentProviderWidget")
        self._setup_ui()
        self._connect_signals()

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建主面板水平布局: 左侧列表 + 分隔线 + 右侧详情."""
        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # 左侧: 供应商列表面板(固定宽度 260px)
        self._list_panel = ProviderListPanel(self)
        layout.addWidget(self._list_panel)

        # 中间: 垂直分隔线 - 用 QFrame + QSS 控制可见性,
        # QFrame.VLine + Sunken 在透明背景下几乎看不出, 改用 1px 实色描边
        separator = QFrame(self)
        separator.setObjectName("ProviderPanelSeparator")
        separator.setFixedWidth(1)
        separator.setStyleSheet(
            "QFrame#ProviderPanelSeparator {"
            "  background-color: rgba(128, 128, 128, 0.18);"
            "  border: none;"
            "}"
        )
        layout.addWidget(separator)

        # 右侧: 供应商详情面板(stretch=1 占据剩余空间)
        self._detail_panel = ProviderDetailPanel(self)
        layout.addWidget(self._detail_panel, 1)

    def _connect_signals(self) -> None:
        """连接左右面板之间的信号.

        - ProviderListPanel.item_clicked → ProviderDetailPanel.load_provider
        - ProviderDetailPanel.provider_changed → ProviderListPanel.refresh_item
        """
        self._list_panel.item_clicked.connect(self._detail_panel.load_provider)
        self._detail_panel.provider_changed.connect(self._list_panel.refresh_item)

    # ------------------------------------------------------------------
    # 公共方法
    # ------------------------------------------------------------------

    def select_first_provider(self) -> None:
        """自动选中第一个供应商.

        从 ProviderRegistry 获取所有供应商列表, 若非空则选中第一个,
        同时更新左侧列表高亮和右侧详情面板内容.
        """
        registry: ProviderRegistry = it(ProviderRegistry)
        providers = registry.list_all()
        if providers:
            first_id = providers[0].provider_id
            self._list_panel.set_selected(first_id)
            self._detail_panel.load_provider(first_id)

    # ------------------------------------------------------------------
    # 事件重写
    # ------------------------------------------------------------------

    def showEvent(self, event) -> None:
        """面板显示时刷新列表并自动选中第一个供应商."""
        super().showEvent(event)

        # ProviderRegistryCreator 已在创建时从磁盘加载配置,
        # 此处仅作为防御性兜底: 若 registry 仍为空则注入默认供应商
        registry: ProviderRegistry = it(ProviderRegistry)
        if not registry.list_all():
            from src.core.agent.default_providers import get_default_providers

            for provider in get_default_providers():
                try:
                    registry.register(provider)
                except Exception:
                    pass

        self._list_panel.refresh()
        self.select_first_provider()

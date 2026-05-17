# -*- coding: utf-8 -*-
"""左侧供应商列表面板.

提供供应商列表的搜索, 过滤, 选中和添加功能.
顶部为搜索框, 中部为组件库 ListWidget 列表, 底部为透明"添加供应商"按钮.
支持拖拽排序: 使用 QListWidget InternalMove 模式,
释放到原位不触发排序变更, 异常时恢复到拖拽前排序状态.
"""
from __future__ import annotations

from creart import it
from PySide6.QtCore import QEvent, QSize, Qt, Signal
from PySide6.QtWidgets import (
    QAbstractItemView,
    QListWidgetItem,
    QVBoxLayout,
    QWidget,
)
from qfluentwidgets import (
    FluentIcon,
    ListWidget,
    ScrollBarHandleDisplayMode,
    TransparentPushButton,
    setCustomStyleSheet,
)

from src.core.logging import LogSource, logger

from src.core.agent.provider import Provider, ProviderRegistry
from src.ui.components.subtle_search_edit import SubtleSearchEdit

from .add_provider_dialog import AddProviderDialog
from .provider_list_item_widget import ProviderListItemWidget


class ProviderListPanel(QWidget):
    """左侧供应商列表面板 - 搜索 + ListWidget + 添加按钮.

    固定宽度 260px, 顶部搜索框实时过滤, 中部使用 qfluentwidgets.ListWidget
    展示供应商条目, 底部透明"添加供应商"按钮弹出 AddProviderDialog.
    支持拖拽排序: InternalMove 模式 + 异常恢复.
    """

    # 信号: 点击供应商条目时发射, 携带 provider_id
    item_clicked = Signal(str)
    # 信号: 拖拽排序完成后发射, 携带新的 provider_id 顺序列表
    sort_changed = Signal(list)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._selected_id: str | None = None
        # 拖拽排序: 存储拖拽前的排序状态, 用于异常恢复
        self._pre_drag_order: list[str] = []
        self.setFixedWidth(260)
        self._setup_ui()
        self._connect_signals()
        # 安装事件过滤器以拦截拖拽事件 (在 model.rowsMoved 之前快照排序状态)
        self._list_widget.viewport().installEventFilter(self)

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建面板 UI 布局."""
        self._layout = QVBoxLayout(self)
        self._layout.setContentsMargins(8, 8, 8, 8)
        self._layout.setSpacing(8)

        # 顶部: 轻量搜索框 (低视觉权重, 不抢眼)
        self._search_edit = SubtleSearchEdit(self)
        self._search_edit.setPlaceholderText(self.tr("搜索供应商..."))
        self._layout.addWidget(self._search_edit)

        # 中部: 使用组件库自带的 ListWidget, 启用 InternalMove 拖拽模式
        self._list_widget = ListWidget(self)
        self._list_widget.setDragDropMode(
            QAbstractItemView.DragDropMode.InternalMove
        )
        self._list_widget.setDefaultDropAction(Qt.DropAction.MoveAction)

        self._list_widget.setDragEnabled(True)
        self._list_widget.setAcceptDrops(True)
        # 拖拽占位符指示器 - 使用 QListWidget 内置的 drop indicator
        self._list_widget.setDropIndicatorShown(True)
        # 滚动条按需显示: 仅在鼠标进入或滚动时浮出, 平时收起为细线
        # SmoothScrollDelegate 的 vScrollBar 暴露了 ON_HOVER 模式
        try:
            self._list_widget.scrollDelegate.vScrollBar.setHandleDisplayMode(
                ScrollBarHandleDisplayMode.ON_HOVER
            )
        except AttributeError:  # pragma: no cover - 兼容老版本组件库
            pass
        # qfluentwidgets 的滚动条是浮层样式 (SmoothScrollDelegate),
        # 会盖在 item 内容右侧. 用 viewportMargins 让出 4px, 给收起的
        # 细线留出最小空间, 且鼠标悬停展开时不会突兀挤压 item.
        self._list_widget.setViewportMargins(0, 0, 4, 0)
        self._layout.addWidget(self._list_widget, 1)

        # 底部: 添加供应商按钮 (带线框, 视觉上更明确)
        self._add_button = TransparentPushButton(FluentIcon.ADD, self.tr("添加供应商"), self)
        self._add_button.setObjectName("addProviderButton")
        # 通过 setCustomStyleSheet 覆盖组件库默认按钮样式, 支持主题自动切换
        _LIGHT_QSS = (
            "TransparentPushButton {"
            "  border: 1px solid rgba(0, 0, 0, 0.12);"
            "  border-radius: 6px;"
            "  padding: 4px 12px;"
            "  background-color: transparent;"
            "}"
            "TransparentPushButton:hover {"
            "  background-color: rgba(0, 0, 0, 0.03);"
            "  border: 1px solid rgba(0, 0, 0, 0.18);"
            "}"
            "TransparentPushButton:pressed {"
            "  background-color: rgba(0, 0, 0, 0.06);"
            "  border: 1px solid rgba(0, 0, 0, 0.18);"
            "}"
        )
        _DARK_QSS = (
            "TransparentPushButton {"
            "  border: 1px solid rgba(255, 255, 255, 0.12);"
            "  border-radius: 6px;"
            "  padding: 4px 12px;"
            "  background-color: transparent;"
            "}"
            "TransparentPushButton:hover {"
            "  background-color: rgba(255, 255, 255, 0.05);"
            "  border: 1px solid rgba(255, 255, 255, 0.18);"
            "}"
            "TransparentPushButton:pressed {"
            "  background-color: rgba(255, 255, 255, 0.08);"
            "  border: 1px solid rgba(255, 255, 255, 0.18);"
            "}"
        )
        setCustomStyleSheet(self._add_button, _LIGHT_QSS, _DARK_QSS)
        self._layout.addWidget(self._add_button)

    def _connect_signals(self) -> None:
        """连接内部信号."""
        self._search_edit.textChanged.connect(self._on_search_changed)
        self._add_button.clicked.connect(self._on_add_clicked)
        self._list_widget.currentItemChanged.connect(self._on_current_item_changed)
        # 拖拽排序: 监听 model 的 rowsMoved 信号检测排序变更
        self._list_widget.model().rowsMoved.connect(self._on_rows_moved)

    # ------------------------------------------------------------------
    # 公共方法
    # ------------------------------------------------------------------

    def refresh(self) -> None:
        """从 ProviderRegistry 获取所有供应商并重建列表."""
        registry: ProviderRegistry = it(ProviderRegistry)
        providers = registry.list_all()
        self._rebuild_list(providers)

    def refresh_item(self, provider_id: str) -> None:
        """更新单个条目的显示.

        Args:
            provider_id: 要更新的供应商 ID.
        """
        registry: ProviderRegistry = it(ProviderRegistry)
        try:
            provider = registry.get(provider_id)
        except KeyError:
            return

        for i in range(self._list_widget.count()):
            item = self._list_widget.item(i)
            if item and item.data(Qt.ItemDataRole.UserRole) == provider_id:
                # 更新自定义 widget 的启用状态
                widget = self._list_widget.itemWidget(item)
                if isinstance(widget, ProviderListItemWidget):
                    widget.update_enabled_state(provider.enabled)
                break

    def set_selected(self, provider_id: str) -> None:
        """高亮选中条目.

        Args:
            provider_id: 要选中的供应商 ID.
        """
        self._selected_id = provider_id
        self._list_widget.blockSignals(True)
        for i in range(self._list_widget.count()):
            item = self._list_widget.item(i)
            if item and item.data(Qt.ItemDataRole.UserRole) == provider_id:
                self._list_widget.setCurrentItem(item)
                break
        self._list_widget.blockSignals(False)

    def filter_by_text(self, text: str) -> None:
        """按名称过滤显示供应商条目.

        不区分大小写匹配供应商名称.

        Args:
            text: 过滤文本.
        """
        keyword = text.strip().lower()
        for i in range(self._list_widget.count()):
            item = self._list_widget.item(i)
            if item:
                # 从 item widget 获取名称, 或从 UserRole 数据获取
                widget = self._list_widget.itemWidget(item)
                if isinstance(widget, ProviderListItemWidget):
                    name = widget._provider.name.lower()
                else:
                    name = (item.text() or "").lower()
                visible = not keyword or keyword in name
                item.setHidden(not visible)

    # ------------------------------------------------------------------
    # 内部方法
    # ------------------------------------------------------------------

    def _rebuild_list(self, providers: list[Provider]) -> None:
        """重建供应商条目列表.

        Args:
            providers: 供应商列表.
        """
        # 重建期间阻止 currentItemChanged 信号, 避免意外触发
        self._list_widget.blockSignals(True)
        self._list_widget.clear()

        for provider in providers:
            item = QListWidgetItem()
            item.setData(Qt.ItemDataRole.UserRole, provider.provider_id)
            # 设置 item 大小以容纳自定义 widget
            item.setSizeHint(QSize(0, 36))
            self._list_widget.addItem(item)
            # 使用自定义 ProviderListItemWidget 替代纯文本
            item_widget = ProviderListItemWidget(provider, self._list_widget)
            # 让 widget 填满 item 行高, 内部 layout 的 AlignVCenter 负责居中内容
            item_widget.setFixedHeight(36)
            self._list_widget.setItemWidget(item, item_widget)

        self._list_widget.blockSignals(False)

        # 恢复选中状态
        if self._selected_id:
            self.set_selected(self._selected_id)

    def _on_current_item_changed(
        self, current: QListWidgetItem | None, _previous: QListWidgetItem | None
    ) -> None:
        """ListWidget 选中项变化时发射 item_clicked 信号.

        Args:
            current: 当前选中的 item.
            _previous: 之前选中的 item.
        """
        if current is None:
            return
        provider_id = current.data(Qt.ItemDataRole.UserRole)
        if provider_id:
            self._selected_id = provider_id
            self.item_clicked.emit(provider_id)

    def _on_search_changed(self, text: str) -> None:
        """搜索框文本变化时过滤列表.

        Args:
            text: 搜索文本.
        """
        self.filter_by_text(text)

    def _on_add_clicked(self) -> None:
        """点击"添加供应商"按钮, 弹出 AddProviderDialog."""
        dialog = AddProviderDialog(self.window())
        if dialog.exec():
            data = dialog.get_data()
            registry: ProviderRegistry = it(ProviderRegistry)
            # 注册新供应商 (使用默认模型)
            from src.core.agent.provider import ModelEntry

            new_provider = Provider(
                provider_id=data["provider_id"],
                name=data["name"],
                api_base_url=data["api_base_url"],
                api_key_ref=data["api_key_ref"],
                models=[ModelEntry(model_id="default", max_tokens=4096)],
            )
            registry.register(new_provider)
            self.refresh()
            # 选中新添加的供应商
            self.set_selected(data["provider_id"])
            self.item_clicked.emit(data["provider_id"])

    # ------------------------------------------------------------------
    # 拖拽排序相关方法
    # ------------------------------------------------------------------

    def _get_current_order(self) -> list[str]:
        """获取当前列表中所有条目的 provider_id 顺序.

        Returns:
            provider_id 列表, 按当前列表顺序排列.
        """
        order: list[str] = []
        for i in range(self._list_widget.count()):
            item = self._list_widget.item(i)
            if item:
                pid = item.data(Qt.ItemDataRole.UserRole)
                if pid:
                    order.append(pid)
        return order

    def _store_pre_drag_order(self) -> None:
        """在拖拽开始前存储当前排序状态, 用于异常恢复."""
        self._pre_drag_order = self._get_current_order()

    def _restore_pre_drag_order(self) -> None:
        """恢复到拖拽前的排序状态 (异常恢复).

        不修改持久化存储, 仅恢复 UI 列表顺序.
        """
        if not self._pre_drag_order:
            return
        try:
            registry: ProviderRegistry = it(ProviderRegistry)
            # 按 pre_drag_order 排序 providers
            providers = registry.list_all()
            order_map = {pid: idx for idx, pid in enumerate(self._pre_drag_order)}
            sorted_providers = sorted(
                providers, key=lambda p: order_map.get(p.provider_id, len(order_map))
            )
            self._rebuild_list(sorted_providers)
        except Exception as _exc:
            logger.exception("恢复拖拽前排序状态失败", _exc)

    def _on_rows_moved(self, *_args) -> None:
        """QListWidget model 的 rowsMoved 信号处理.

        拖拽完成后:
        1. 检测是否为无操作 (释放到原位), 若是则不触发排序变更
        2. 更新各 Provider 的 sort_order 字段
        3. 通过 ProviderRegistry 将新排序写入 ConfigPersistence
        4. 发射 sort_changed 信号
        若过程中发生异常则恢复到拖拽前排序状态, 不修改持久化存储.
        """
        try:
            new_order = self._get_current_order()
            # 检测释放到原位: 新顺序与拖拽前顺序相同
            if new_order == self._pre_drag_order:
                # 无操作, 不触发排序变更
                return

            # 更新 ProviderRegistry 中各 Provider 的 sort_order 字段
            registry: ProviderRegistry = it(ProviderRegistry)
            registry.reorder_providers(new_order)

            # 持久化新排序到 ConfigPersistence
            self._persist_provider_order(registry)

            # 更新 _pre_drag_order 为新顺序
            self._pre_drag_order = new_order

            # 发射排序变更信号
            self.sort_changed.emit(new_order)
        except Exception as _exc:
            logger.exception("拖拽排序处理异常, 恢复到拖拽前状态", _exc)
            self._restore_pre_drag_order()

    def _persist_provider_order(self, registry: ProviderRegistry) -> None:
        """将当前 Provider 排序持久化到配置文件.

        通过 ConfigPersistence 将 ProviderRegistry 中的所有 Provider
        (包含更新后的 sort_order) 写入 agent_config.json.

        Args:
            registry: ProviderRegistry 实例.
        """
        from src.core.agent.config_persistence import ConfigPersistence
        from src.core.runtime.paths import PathFunc

        try:
            path_func: PathFunc = it(PathFunc)
            config_file_path = path_func.config_dir_path / "agent_config.json"
            persistence = ConfigPersistence(config_file_path)

            # 加载现有配置并更新 providers 列表
            config_data = persistence.load()
            config_data.providers = registry.list_all()
            persistence.save(config_data)
        except Exception as exc:
            logger.error(f"持久化 Provider 排序失败: {exc}")

    # ------------------------------------------------------------------
    # 事件过滤器 - 拖拽前快照排序状态
    # ------------------------------------------------------------------

    def eventFilter(self, obj, event: QEvent) -> bool:
        """事件过滤器: 拖拽开始时存储拖拽前排序状态."""
        if obj is self._list_widget.viewport():
            event_type = event.type()
            if event_type in (
                QEvent.Type.DragEnter,
                QEvent.Type.MouseButtonPress,
            ):
                self._store_pre_drag_order()

        return super().eventFilter(obj, event)

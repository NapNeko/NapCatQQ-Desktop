# -*- coding: utf-8 -*-
"""模型列表管理组件.

展示供应商的模型列表, 支持搜索过滤, 删除模型和添加模型.
顶部标题栏显示模型数量 + 搜索按钮 + 添加按钮, 中部为模型条目列表.
"""
from __future__ import annotations

from PySide6.QtCore import Signal
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    FluentIcon,
    InfoBar,
    LineEdit,
    TransparentToolButton,
)

from src.core.agent.provider import ModelEntry, Provider
from src.ui.common.style_sheet import WidgetStyleSheet

from .add_model_dialog import AddModelDialog


class ModelListWidget(QWidget):
    """模型列表管理组件 — 标题栏 + 搜索 + 列表 + 添加.

    提供模型的可视化列表管理, 包括搜索过滤, 删除和添加功能.
    顶部标题栏显示模型数量 + 搜索按钮 + 添加按钮, 参考图二设计.
    """

    # 信号: 模型被添加/移除时发射, 携带 model_id
    model_added = Signal(str)
    model_removed = Signal(str)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._provider: Provider | None = None
        self._model_items: list[QWidget] = []
        self._search_visible = False
        self._setup_ui()
        self._connect_signals()
        WidgetStyleSheet.MODEL_LIST_WIDGET.apply(self)

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建组件 UI 布局."""
        self._layout = QVBoxLayout(self)
        self._layout.setContentsMargins(0, 0, 0, 0)
        self._layout.setSpacing(8)

        # 顶部: 模型标题 + 数量 badge + 搜索按钮 + 添加按钮
        header_layout = QHBoxLayout()
        header_layout.setContentsMargins(0, 0, 0, 0)
        header_layout.setSpacing(8)

        self._title_label = BodyLabel(self.tr("模型"), self)
        header_layout.addWidget(self._title_label)

        self._count_label = CaptionLabel("", self)
        self._count_label.setObjectName("modelCountBadge")
        header_layout.addWidget(self._count_label)

        header_layout.addStretch()

        # 搜索切换按钮
        self._search_toggle_btn = TransparentToolButton(FluentIcon.SEARCH, self)
        self._search_toggle_btn.setFixedSize(28, 28)
        header_layout.addWidget(self._search_toggle_btn)

        # 添加模型按钮
        self._add_btn = TransparentToolButton(FluentIcon.ADD, self)
        self._add_btn.setFixedSize(28, 28)
        header_layout.addWidget(self._add_btn)

        self._layout.addLayout(header_layout)

        # 搜索框 (默认隐藏, 点击搜索按钮时显示)
        self._search_edit = LineEdit(self)
        self._search_edit.setPlaceholderText(self.tr("搜索模型..."))
        self._search_edit.setClearButtonEnabled(True)
        self._search_edit.setVisible(False)
        self._layout.addWidget(self._search_edit)

        # 中部: 模型条目列表容器
        self._list_container = QWidget(self)
        self._list_layout = QVBoxLayout(self._list_container)
        self._list_layout.setContentsMargins(0, 0, 0, 0)
        self._list_layout.setSpacing(4)
        self._layout.addWidget(self._list_container)

    def _connect_signals(self) -> None:
        """连接内部信号."""
        self._search_edit.textChanged.connect(self._on_search_changed)
        self._search_toggle_btn.clicked.connect(self._on_toggle_search)
        self._add_btn.clicked.connect(self._on_add_model_clicked)

    # ------------------------------------------------------------------
    # 公共方法
    # ------------------------------------------------------------------

    def set_provider(self, provider: Provider) -> None:
        """加载供应商的模型列表.

        Args:
            provider: 要展示模型列表的供应商实例.
        """
        self._provider = provider
        self._search_edit.clear()
        self._rebuild_list()

    def refresh(self) -> None:
        """刷新当前模型列表显示."""
        if self._provider is not None:
            self._rebuild_list()

    # ------------------------------------------------------------------
    # 内部方法
    # ------------------------------------------------------------------

    def _rebuild_list(self) -> None:
        """重建模型条目列表."""
        # 清除旧条目
        for item in self._model_items:
            item.setParent(None)
            item.deleteLater()
        self._model_items.clear()

        if self._provider is None:
            self._count_label.setText("")
            return

        # 更新数量统计
        count = len(self._provider.models)
        self._count_label.setText(self.tr("模型数量: {}").format(count))

        # 创建模型条目
        for model in self._provider.models:
            item_widget = self._create_model_item(model)
            self._list_layout.addWidget(item_widget)
            self._model_items.append(item_widget)

    def _create_model_item(self, model: ModelEntry) -> QWidget:
        """创建单个模型条目组件.

        Args:
            model: 模型条目数据.

        Returns:
            包含模型名称和删除按钮的 QWidget.
        """
        item = QWidget(self._list_container)
        layout = QHBoxLayout(item)
        layout.setContentsMargins(4, 4, 4, 4)
        layout.setSpacing(8)

        # 模型名称: 优先显示 display_name, 为空则显示 model_id
        display_text = model.display_name if model.display_name else model.model_id
        name_label = BodyLabel(display_text, item)
        layout.addWidget(name_label, 1)

        # 删除按钮
        delete_btn = TransparentToolButton(FluentIcon.DELETE, item)
        delete_btn.setFixedSize(24, 24)
        delete_btn.clicked.connect(lambda checked=False, mid=model.model_id: self._on_remove_model(mid))
        layout.addWidget(delete_btn)

        # 存储 model_id 用于搜索过滤
        item.setProperty("model_id", model.model_id)
        item.setProperty("display_name", model.display_name)

        return item

    def _on_toggle_search(self) -> None:
        """切换搜索框的显示/隐藏状态."""
        self._search_visible = not self._search_visible
        self._search_edit.setVisible(self._search_visible)
        if self._search_visible:
            self._search_edit.setFocus()
        else:
            self._search_edit.clear()

    def _on_search_changed(self, text: str) -> None:
        """搜索框文本变化时过滤模型列表.

        不区分大小写, 匹配 model_id 或 display_name.

        Args:
            text: 搜索文本.
        """
        keyword = text.strip().lower()
        for item in self._model_items:
            model_id = (item.property("model_id") or "").lower()
            display_name = (item.property("display_name") or "").lower()
            visible = not keyword or keyword in model_id or keyword in display_name
            item.setVisible(visible)

    def _on_remove_model(self, model_id: str) -> None:
        """移除指定模型.

        如果是最后一个模型则阻止删除并显示警告.

        Args:
            model_id: 要移除的模型 ID.
        """
        if self._provider is None:
            return

        # 检查是否为最后一个模型
        if len(self._provider.models) <= 1:
            InfoBar.warning(
                "",
                self.tr("至少保留一个模型"),
                duration=3000,
                parent=self.window(),
            )
            return

        # 从 provider.models 中移除
        self._provider.models = [m for m in self._provider.models if m.model_id != model_id]
        self.model_removed.emit(model_id)
        self._rebuild_list()

    def _on_add_model_clicked(self) -> None:
        """点击"添加模型"按钮, 弹出 AddModelDialog."""
        dialog = AddModelDialog(self.window())
        if dialog.exec():
            data = dialog.get_data()
            if self._provider is not None:
                new_model = ModelEntry(
                    model_id=data["model_id"],
                    display_name=data["display_name"],
                    max_tokens=data["max_tokens"],
                )
                self._provider.models.append(new_model)
                self.model_added.emit(data["model_id"])
                self._rebuild_list()

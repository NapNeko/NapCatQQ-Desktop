# -*- coding: utf-8 -*-
"""自定义 HTTP 请求头对话框.

允许用户为某个供应商配置额外的 HTTP 请求头, 适配器在构建请求时会把这些
请求头与默认认证头合并 (``custom_headers`` 字段写回到 Provider).

UI:
    Header Name | Header Value | 删除
    ...
    + 添加一行
"""
from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QHBoxLayout,
    QListWidget,
    QListWidgetItem,
    QWidget,
)
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    FluentIcon,
    LineEdit,
    MessageBoxBase,
    SubtitleLabel,
    TransparentPushButton,
    TransparentToolButton,
)


class CustomHeadersDialog(MessageBoxBase):
    """编辑自定义请求头键值对的对话框.

    Args:
        parent: 父窗口.
        initial_headers: 当前已存的自定义请求头字典.
    """

    def __init__(
        self,
        parent: QWidget,
        initial_headers: dict[str, str] | None = None,
    ) -> None:
        super().__init__(parent=parent)
        self._setup_ui()
        self._populate(initial_headers or {})
        self.widget.setMinimumWidth(560)

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建对话框 UI."""
        title = SubtitleLabel(self.tr("自定义请求头"), self)
        self.viewLayout.addWidget(title)

        hint = CaptionLabel(
            self.tr(
                "这些键值对会与适配器生成的认证请求头合并, "
                "用户配置优先级更高 (鉴权头通常不应被覆盖)."
            ),
            self,
        )
        hint.setWordWrap(True)
        self.viewLayout.addWidget(hint)
        self.viewLayout.addSpacing(8)

        self._list_widget = QListWidget(self)
        self._list_widget.setMinimumHeight(220)
        self._list_widget.setSelectionMode(QListWidget.SelectionMode.NoSelection)
        self.viewLayout.addWidget(self._list_widget)

        # 添加新行按钮
        add_btn = TransparentPushButton(FluentIcon.ADD, self.tr("添加请求头"), self)
        add_btn.clicked.connect(lambda: self._append_row("", ""))
        self.viewLayout.addWidget(add_btn)

        self.yesButton.setText(self.tr("保存"))
        self.cancelButton.setText(self.tr("取消"))

    def _populate(self, headers: dict[str, str]) -> None:
        """以已有键值对填充列表."""
        if not headers:
            # 给一个空行, 方便用户直接编辑
            self._append_row("", "")
            return
        for name, value in headers.items():
            self._append_row(name, value)

    # ------------------------------------------------------------------
    # 列表项管理
    # ------------------------------------------------------------------

    def _append_row(self, name: str, value: str) -> None:
        """新增一行 (Header Name + Header Value + 删除)."""
        item = QListWidgetItem()
        row_widget = QWidget(self._list_widget)
        row_layout = QHBoxLayout(row_widget)
        row_layout.setContentsMargins(8, 4, 8, 4)
        row_layout.setSpacing(6)

        name_edit = LineEdit(row_widget)
        name_edit.setPlaceholderText(self.tr("Header Name (如 X-Custom-Token)"))
        name_edit.setText(name)
        row_layout.addWidget(name_edit, 2)

        value_edit = LineEdit(row_widget)
        value_edit.setPlaceholderText(self.tr("Header Value"))
        value_edit.setText(value)
        row_layout.addWidget(value_edit, 3)

        remove_btn = TransparentToolButton(FluentIcon.DELETE, row_widget)
        remove_btn.setFixedSize(28, 28)
        remove_btn.setToolTip(self.tr("移除该行"))
        remove_btn.clicked.connect(lambda _checked=False, it=item: self._remove_item(it))
        row_layout.addWidget(remove_btn, 0)

        # 把输入框引用绑定到 item 上, 提交时再读取
        item.setData(Qt.ItemDataRole.UserRole, (name_edit, value_edit))

        item.setSizeHint(row_widget.sizeHint())
        self._list_widget.addItem(item)
        self._list_widget.setItemWidget(item, row_widget)

    def _remove_item(self, item: QListWidgetItem) -> None:
        """删除指定行."""
        row = self._list_widget.row(item)
        if row >= 0:
            self._list_widget.takeItem(row)

    # ------------------------------------------------------------------
    # 结果获取
    # ------------------------------------------------------------------

    def add_blank_row(self) -> None:
        """在最末尾添加一行空白行 (供外部需要时调用)."""
        self._append_row("", "")

    def get_headers(self) -> dict[str, str]:
        """返回当前所有非空键的请求头字典.

        - 若同名键出现多次, 后定义的覆盖前面.
        - 自动 strip Header Name, 保留 Header Value 原始内容 (允许前后空格).
        """
        result: dict[str, str] = {}
        for i in range(self._list_widget.count()):
            item = self._list_widget.item(i)
            if item is None:
                continue
            data = item.data(Qt.ItemDataRole.UserRole)
            if not data:
                continue
            name_edit, value_edit = data
            name = name_edit.text().strip()
            value = value_edit.text()
            if name:
                result[name] = value
        return result

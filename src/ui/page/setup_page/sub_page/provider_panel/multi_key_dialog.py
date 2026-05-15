# -*- coding: utf-8 -*-
"""多 API 密钥配置对话框.

让用户以列表形式管理逗号分隔的多个 API 密钥, 提交时将所有非空密钥
拼接成 ``key1,key2,...`` 写回 Provider.api_key_ref 字段.
后端发起请求时会从中随机选择一个密钥, 减轻单密钥的速率限制压力.
"""
from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QHBoxLayout,
    QListWidget,
    QListWidgetItem,
    QVBoxLayout,
    QWidget,
)
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    FluentIcon,
    LineEdit,
    MessageBoxBase,
    PasswordLineEdit,
    SubtitleLabel,
    TransparentToolButton,
)


class MultiKeyDialog(MessageBoxBase):
    """以列表方式增删 API 密钥的对话框.

    输入区可以输入新密钥, 点击 ``+`` 加入下方列表; 列表行右侧的删除按钮
    把对应密钥移除. 提交时按列表顺序拼接为逗号分隔字符串, 调用方读取
    ``get_keys_string()`` 获取最终结果.

    Args:
        parent: 父窗口, 一般传当前供应商详情面板的 ``window()``.
        initial_keys: 初始密钥列表, 通常由 ``provider.api_key_ref.split(",")`` 得到.
    """

    def __init__(self, parent: QWidget, initial_keys: list[str]) -> None:
        super().__init__(parent=parent)
        self._setup_ui()
        self._populate(initial_keys)
        self.widget.setMinimumWidth(520)

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建对话框 UI."""
        title = SubtitleLabel(self.tr("配置多个 API 密钥"), self)
        self.viewLayout.addWidget(title)

        hint = CaptionLabel(
            self.tr("发起请求时会随机选择一条密钥, 适合分散单 Key 速率限制."),
            self,
        )
        hint.setWordWrap(True)
        self.viewLayout.addWidget(hint)
        self.viewLayout.addSpacing(8)

        # 输入新密钥行: PasswordLineEdit + 添加按钮
        input_row = QHBoxLayout()
        input_row.setContentsMargins(0, 0, 0, 0)
        input_row.setSpacing(8)

        self._new_key_edit = PasswordLineEdit(self)
        self._new_key_edit.setPlaceholderText(self.tr("输入新密钥后回车或点击 +"))
        self._new_key_edit.returnPressed.connect(self._on_add_clicked)
        input_row.addWidget(self._new_key_edit, 1)

        add_btn = TransparentToolButton(FluentIcon.ADD, self)
        add_btn.setFixedSize(32, 32)
        add_btn.setToolTip(self.tr("添加密钥"))
        add_btn.clicked.connect(self._on_add_clicked)
        input_row.addWidget(add_btn, 0)

        self.viewLayout.addLayout(input_row)
        self.viewLayout.addSpacing(8)

        # 已添加的密钥列表
        self.viewLayout.addWidget(BodyLabel(self.tr("已配置密钥"), self))
        self._list_widget = QListWidget(self)
        self._list_widget.setMinimumHeight(200)
        self._list_widget.setSelectionMode(QListWidget.SelectionMode.NoSelection)
        self.viewLayout.addWidget(self._list_widget)

        # 按钮文案
        self.yesButton.setText(self.tr("保存"))
        self.cancelButton.setText(self.tr("取消"))

    def _populate(self, initial_keys: list[str]) -> None:
        """以初始密钥填充列表."""
        for key in initial_keys:
            key = key.strip()
            if key:
                self._append_item(key)

    # ------------------------------------------------------------------
    # 列表项管理
    # ------------------------------------------------------------------

    def _append_item(self, key: str) -> None:
        """追加一个密钥行 (脱敏显示 + 删除按钮)."""
        item = QListWidgetItem()
        # 用 UserRole 存原始值, 显示用脱敏文本
        item.setData(Qt.ItemDataRole.UserRole, key)

        row_widget = QWidget(self._list_widget)
        row_layout = QHBoxLayout(row_widget)
        row_layout.setContentsMargins(8, 4, 8, 4)
        row_layout.setSpacing(8)

        masked = self._mask_key(key)
        # 用一个只读的 LineEdit 来展示, 避免长密钥溢出且支持复制
        display = LineEdit(row_widget)
        display.setReadOnly(True)
        display.setText(masked)
        row_layout.addWidget(display, 1)

        remove_btn = TransparentToolButton(FluentIcon.DELETE, row_widget)
        remove_btn.setFixedSize(28, 28)
        remove_btn.setToolTip(self.tr("移除该密钥"))
        remove_btn.clicked.connect(lambda _checked=False, it=item: self._remove_item(it))
        row_layout.addWidget(remove_btn, 0)

        item.setSizeHint(row_widget.sizeHint())
        self._list_widget.addItem(item)
        self._list_widget.setItemWidget(item, row_widget)

    def _remove_item(self, item: QListWidgetItem) -> None:
        """从列表中删除指定行."""
        row = self._list_widget.row(item)
        if row >= 0:
            self._list_widget.takeItem(row)

    def _mask_key(self, key: str) -> str:
        """脱敏显示密钥, 仅保留前后 4 位."""
        if len(key) <= 8:
            return "•" * len(key)
        return f"{key[:4]}{'•' * (len(key) - 8)}{key[-4:]}"

    # ------------------------------------------------------------------
    # 事件
    # ------------------------------------------------------------------

    def _on_add_clicked(self) -> None:
        """把输入框中的密钥追加到列表."""
        text = self._new_key_edit.text().strip()
        if not text:
            return
        # 允许一次粘贴 "key1,key2,key3" 拆成多行
        for piece in (p.strip() for p in text.split(",")):
            if piece:
                self._append_item(piece)
        self._new_key_edit.clear()

    # ------------------------------------------------------------------
    # 结果获取
    # ------------------------------------------------------------------

    def get_keys(self) -> list[str]:
        """返回当前列表中所有密钥的列表 (按显示顺序)."""
        keys: list[str] = []
        for i in range(self._list_widget.count()):
            item = self._list_widget.item(i)
            if item is None:
                continue
            key = item.data(Qt.ItemDataRole.UserRole)
            if key:
                keys.append(str(key))
        return keys

    def get_keys_string(self) -> str:
        """返回逗号分隔的密钥字符串, 可直接写回 ``Provider.api_key_ref``."""
        return ",".join(self.get_keys())

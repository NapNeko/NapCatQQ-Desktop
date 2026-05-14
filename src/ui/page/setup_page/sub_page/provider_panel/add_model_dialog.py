# -*- coding: utf-8 -*-
"""添加模型对话框."""
from __future__ import annotations

from PySide6.QtWidgets import QWidget
from qfluentwidgets import BodyLabel, InfoBar, LineEdit, MessageBoxBase, SubtitleLabel


class AddModelDialog(MessageBoxBase):
    """添加模型表单对话框.

    包含 model_id, display_name, max_tokens 三个字段,
    其中 model_id 为必填, max_tokens 需为正整数.
    """

    def __init__(self, parent: QWidget) -> None:
        super().__init__(parent=parent)
        self._setup_ui()
        self.widget.setMinimumWidth(420)

    def _setup_ui(self) -> None:
        self.title_label = SubtitleLabel(self.tr("添加模型"), self)
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addSpacing(8)

        # Model ID(必填)
        self.model_id_edit = LineEdit(self)
        self.model_id_edit.setPlaceholderText(self.tr("模型标识 (如: gpt-4o, deepseek-chat)"))
        self.model_id_edit.setClearButtonEnabled(True)
        self._add_row(self.tr("模型 ID"), self.model_id_edit)

        # Display Name(选填)
        self.display_name_edit = LineEdit(self)
        self.display_name_edit.setPlaceholderText(self.tr("显示名称 (可选)"))
        self.display_name_edit.setClearButtonEnabled(True)
        self._add_row(self.tr("显示名称"), self.display_name_edit)

        # Max Tokens(必填, 正整数)
        self.max_tokens_edit = LineEdit(self)
        self.max_tokens_edit.setPlaceholderText("4096")
        self.max_tokens_edit.setClearButtonEnabled(True)
        self._add_row(self.tr("最大 Tokens"), self.max_tokens_edit)

        self.yesButton.setText(self.tr("添加"))
        self.cancelButton.setText(self.tr("取消"))

    def _add_row(self, label_text: str, widget: QWidget) -> None:
        """添加一行表单字段(标签 + 输入框)."""
        label = BodyLabel(label_text, self)
        self.viewLayout.addWidget(label)
        self.viewLayout.addWidget(widget)
        self.viewLayout.addSpacing(4)

    def validate(self) -> bool:
        """验证表单字段.

        检查 model_id 非空且 max_tokens 为正整数.
        """
        if not self.model_id_edit.text().strip():
            InfoBar.error("", self.tr("模型 ID 不能为空"), duration=3000, parent=self)
            return False

        max_tokens_text = self.max_tokens_edit.text().strip()
        if not max_tokens_text:
            # 未填写时使用默认值
            max_tokens_text = "4096"

        try:
            max_tokens = int(max_tokens_text)
        except ValueError:
            InfoBar.error("", self.tr("最大 Tokens 必须为整数"), duration=3000, parent=self)
            return False

        if max_tokens < 1:
            InfoBar.error("", self.tr("最大 Tokens 必须为正整数"), duration=3000, parent=self)
            return False

        return True

    def get_data(self) -> dict:
        """返回表单数据.

        Returns:
            包含 model_id, display_name, max_tokens 的字典,
            其中 max_tokens 为整数类型.
        """
        max_tokens_text = self.max_tokens_edit.text().strip()
        if not max_tokens_text:
            max_tokens_text = "4096"

        return {
            "model_id": self.model_id_edit.text().strip(),
            "display_name": self.display_name_edit.text().strip(),
            "max_tokens": int(max_tokens_text),
        }

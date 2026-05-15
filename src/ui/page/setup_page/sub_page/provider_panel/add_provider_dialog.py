# -*- coding: utf-8 -*-
"""添加供应商对话框.

提供表单让用户输入新供应商的基本信息: provider_id, name, api_base_url, api_key_ref.
提交时验证所有字段非空, 并检查 provider_id 是否已存在.
"""
from __future__ import annotations

from typing import TYPE_CHECKING

from creart import it
from PySide6.QtWidgets import QWidget
from qfluentwidgets import (
    BodyLabel,
    LineEdit,
    MessageBoxBase,
    PasswordLineEdit,
    SubtitleLabel,
)

from src.ui.components.info_bar import error_bar

if TYPE_CHECKING:
    from src.core.agent.provider import ProviderRegistry


class AddProviderDialog(MessageBoxBase):
    """添加供应商表单对话框.

    包含 provider_id, name, api_base_url, api_key_ref 四个必填字段.
    提交时校验字段非空并检查 provider_id 唯一性.
    """

    def __init__(self, parent: QWidget) -> None:
        super().__init__(parent=parent)
        self._setup_ui()
        self.widget.setMinimumWidth(480)

        # 覆盖确认按钮行为, 加入校验逻辑
        try:
            self.yesButton.clicked.disconnect()
        except (TypeError, RuntimeError):
            pass
        self.yesButton.clicked.connect(self._on_yes_clicked)

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建表单 UI."""
        self.title_label = SubtitleLabel(self.tr("添加供应商"), self)
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addSpacing(8)

        # Provider ID
        self.provider_id_edit = LineEdit(self)
        self.provider_id_edit.setPlaceholderText(self.tr("唯一标识 (如: openai, deepseek)"))
        self.provider_id_edit.setClearButtonEnabled(True)
        self._add_field(self.tr("供应商 ID"), self.provider_id_edit)

        # Name
        self.name_edit = LineEdit(self)
        self.name_edit.setPlaceholderText(self.tr("显示名称 (如: OpenAI, DeepSeek)"))
        self.name_edit.setClearButtonEnabled(True)
        self._add_field(self.tr("名称"), self.name_edit)

        # API Base URL
        self.api_base_url_edit = LineEdit(self)
        self.api_base_url_edit.setPlaceholderText(self.tr("https://api.openai.com/v1"))
        self.api_base_url_edit.setClearButtonEnabled(True)
        self._add_field(self.tr("API Base URL"), self.api_base_url_edit)

        # API Key
        self.api_key_edit = PasswordLineEdit(self)
        self.api_key_edit.setPlaceholderText(self.tr("sk-..."))
        self._add_field(self.tr("API Key"), self.api_key_edit)

        # 按钮文本
        self.yesButton.setText(self.tr("添加"))
        self.cancelButton.setText(self.tr("取消"))

    def _add_field(self, label_text: str, widget: QWidget) -> None:
        """添加一行表单字段(标签 + 输入框)."""
        label = BodyLabel(label_text, self)
        self.viewLayout.addWidget(label)
        self.viewLayout.addWidget(widget)
        self.viewLayout.addSpacing(4)

    # ------------------------------------------------------------------
    # 校验与数据
    # ------------------------------------------------------------------

    def validate(self) -> bool:
        """检查所有字段非空.

        Returns:
            True 表示校验通过, False 表示存在空字段.
        """
        if not self.provider_id_edit.text().strip():
            error_bar(content=self.tr("供应商 ID 不能为空"), title="", duration=3000, parent=self)
            return False
        if not self.name_edit.text().strip():
            error_bar(content=self.tr("名称不能为空"), title="", duration=3000, parent=self)
            return False
        if not self.api_base_url_edit.text().strip():
            error_bar(content=self.tr("API Base URL 不能为空"), title="", duration=3000, parent=self)
            return False
        if not self.api_key_edit.text().strip():
            error_bar(content=self.tr("API Key 不能为空"), title="", duration=3000, parent=self)
            return False
        return True

    def get_data(self) -> dict:
        """返回表单数据字典.

        Returns:
            包含 provider_id, name, api_base_url, api_key_ref 的字典.
        """
        return {
            "provider_id": self.provider_id_edit.text().strip(),
            "name": self.name_edit.text().strip(),
            "api_base_url": self.api_base_url_edit.text().strip(),
            "api_key_ref": self.api_key_edit.text().strip(),
        }

    # ------------------------------------------------------------------
    # 事件处理
    # ------------------------------------------------------------------

    def _on_yes_clicked(self) -> None:
        """确认按钮点击: 校验 -> 检查重复 -> 接受对话框."""
        if not self.validate():
            return

        # 检查 provider_id 是否已存在
        from src.core.agent.provider import ProviderRegistry

        registry: ProviderRegistry = it(ProviderRegistry)
        provider_id = self.provider_id_edit.text().strip()
        try:
            registry.get(provider_id)
            # 如果没有抛出 KeyError, 说明已存在
            error_bar(content=self.tr("该供应商 ID 已存在"), title="", duration=3000, parent=self)
            return
        except KeyError:
            pass

        self.accept()
        self.accepted.emit()

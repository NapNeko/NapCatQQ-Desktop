# -*- coding: utf-8 -*-
"""API 选项配置对话框.

以模态弹窗形式展示供应商的 6 个 API 兼容性选项开关, 用户切换开关后
立即通过 ProviderRegistry.update_provider 持久化变更. 若持久化失败,
开关恢复到变更前状态并显示错误 InfoBar.

当 Provider.api_options 为 None 时, 按 ProviderApiOptions() 默认值显示开关状态.
"""
from __future__ import annotations

from creart import it
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QWidget
from qfluentwidgets import (
    CaptionLabel,
    MessageBoxBase,
    StrongBodyLabel,
    SubtitleLabel,
    SwitchButton,
)

from src.core.agent.provider import ProviderApiOptions, ProviderRegistry
from src.ui.components.info_bar import error_bar


# 选项定义: (field_name, label, default_value)
_SWITCH_OPTIONS: list[tuple[str, str, bool]] = [
    ("supports_array_content", "支持数组格式消息内容", True),
    ("supports_stream_options", "支持 stream_options 参数", True),
    ("supports_developer_role", "支持 developer 角色消息", False),
    ("supports_service_tier", "支持 service_tier 参数", False),
    ("supports_enable_thinking", "支持 enable_thinking 参数", True),
    ("supports_verbosity", "支持 verbosity 参数", True),
]


class ApiOptionsDialog(MessageBoxBase):
    """API 兼容性选项弹窗.

    展示 6 个 SwitchButton 开关, 用户切换后即时持久化到 ProviderRegistry.

    Args:
        parent: 父窗口.
        provider_id: 当前供应商 ID.
    """

    def __init__(self, parent: QWidget, provider_id: str) -> None:
        super().__init__(parent=parent)
        self._provider_id = provider_id
        self._switches: dict[str, SwitchButton] = {}
        self._setup_ui()
        self._setup_switches()
        self.widget.setMinimumWidth(480)

        # 隐藏确认/取消按钮, 本弹窗为即时保存模式
        self.yesButton.setText(self.tr("关闭"))
        self.cancelButton.hide()

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建对话框标题和说明."""
        title = SubtitleLabel(self.tr("API 选项"), self)
        self.viewLayout.addWidget(title)

        hint = CaptionLabel(
            self.tr("控制适配器在构建请求时是否包含特定参数, 关闭不兼容的选项可避免 API 报错."),
            self,
        )
        hint.setWordWrap(True)
        self.viewLayout.addWidget(hint)
        self.viewLayout.addSpacing(12)

    def _setup_switches(self) -> None:
        """为每个选项创建标签+开关行."""
        registry = it(ProviderRegistry)
        provider = registry.get(self._provider_id)
        api_options = provider.api_options or ProviderApiOptions()

        for field_name, label_text, _default in _SWITCH_OPTIONS:
            row_widget = QWidget(self)
            row_layout = QHBoxLayout(row_widget)
            row_layout.setContentsMargins(0, 4, 0, 4)
            row_layout.setSpacing(12)

            label = StrongBodyLabel(label_text, row_widget)
            row_layout.addWidget(label, 1)

            switch = SwitchButton(row_widget)
            checked = getattr(api_options, field_name)
            switch.setChecked(checked)
            switch.checkedChanged.connect(
                lambda state, fn=field_name: self._on_switch_toggled(fn, state)
            )
            row_layout.addWidget(switch, 0, Qt.AlignmentFlag.AlignRight)

            self._switches[field_name] = switch
            self.viewLayout.addWidget(row_widget)

    # ------------------------------------------------------------------
    # 开关切换处理
    # ------------------------------------------------------------------

    def _on_switch_toggled(self, field_name: str, checked: bool) -> None:
        """开关切换回调, 通过 ProviderRegistry 持久化变更.

        Args:
            field_name: ProviderApiOptions 的字段名.
            checked: 开关新状态.
        """
        registry = it(ProviderRegistry)
        try:
            provider = registry.get(self._provider_id)
            current_options = provider.api_options or ProviderApiOptions()
            updated_options = current_options.model_copy(update={field_name: checked})
            registry.update_provider(self._provider_id, api_options=updated_options)
        except Exception:
            # 持久化失败: 恢复开关状态并显示错误
            switch = self._switches[field_name]
            switch.blockSignals(True)
            switch.setChecked(not checked)
            switch.blockSignals(False)
            error_bar(
                content=self.tr("API 选项保存失败, 请稍后重试"),
                parent=self,
            )

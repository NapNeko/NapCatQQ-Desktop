# -*- coding: utf-8 -*-
"""编辑模型对话框.

自定义 MessageBoxBase 对话框, 去掉默认底部按钮区域, 改为内嵌保存按钮.
布局参考设计稿:
- 模型 ID (只读 + 复制按钮)
- 模型名称 (可编辑)
- 分组名称 (可编辑, 默认自动解析)
- [更多设置] 折叠区 + [保存] 按钮
- 模型类型 (Pill 标签多选)
- 支持增量文本输出 (Switch)
- 币种 (ComboBox)
- 输入价格 / 输出价格
"""
from __future__ import annotations

from PySide6.QtCore import QSize, Qt, Signal
from PySide6.QtGui import QGuiApplication
from PySide6.QtWidgets import (
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    ComboBox,
    FluentIcon,
    LineEdit,
    MessageBoxBase,
    PillPushButton,
    PrimaryPushButton,
    StrongBodyLabel,
    SubtitleLabel,
    SwitchButton,
    TransparentPushButton,
    TransparentToolButton,
    isDarkTheme,
)

from src.core.agent.provider import ModelEntry
from src.core.config import cfg
from src.ui.components.info_bar import success_bar


def _parse_group_name(model_id: str) -> str:
    """从 model_id 解析默认分组名 (与 model_list_widget 中逻辑一致)."""
    parts = model_id.split("-")
    prefix: list[str] = []
    for part in parts:
        if not part:
            continue
        if part.isdigit():
            break
        if part[0].isdigit():
            break
        prefix.append(part)
    return "-".join(prefix) if prefix else model_id


class EditModelDialog(MessageBoxBase):
    """编辑模型对话框 - 无底部按钮, 内嵌保存.

    Signals:
        saved: 保存成功时发射, 携带更新后的 ModelEntry.
    """

    saved = Signal(object)  # ModelEntry

    def __init__(self, model: ModelEntry, parent: QWidget) -> None:
        self._model = model
        super().__init__(parent=parent)
        self._setup_content()
        self._populate(model)
        self.widget.setMinimumWidth(520)

        # 隐藏默认底部按钮区域
        self.yesButton.setVisible(False)
        self.cancelButton.setVisible(False)
        self.buttonGroup.setVisible(False)

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_content(self) -> None:
        """构建对话框内容."""
        # 标题行: "编辑模型" + 关闭按钮
        title_row = QHBoxLayout()
        title_row.setContentsMargins(0, 0, 0, 0)
        self._title = SubtitleLabel(self.tr("编辑模型"), self)
        title_row.addWidget(self._title)
        title_row.addStretch()
        self._close_btn = TransparentToolButton(FluentIcon.CLOSE, self)
        self._close_btn.setFixedSize(32, 32)
        self._close_btn.clicked.connect(self.reject)
        title_row.addWidget(self._close_btn)
        self.viewLayout.addLayout(title_row)
        self.viewLayout.addSpacing(12)

        # --- 基础字段 ---
        # 模型 ID (只读 + 复制)
        self._model_id_edit = LineEdit(self)
        self._model_id_edit.setReadOnly(True)
        self._copy_btn = TransparentToolButton(FluentIcon.COPY, self)
        self._copy_btn.setFixedSize(28, 28)
        self._copy_btn.setToolTip(self.tr("复制模型 ID"))
        self._copy_btn.clicked.connect(self._on_copy_id)
        self._add_field_row(
            self.tr("模型 ID"), self._model_id_edit, suffix_widget=self._copy_btn, required=True
        )

        # 模型名称
        self._name_edit = LineEdit(self)
        self._name_edit.setPlaceholderText(self.tr("显示名称 (可选)"))
        self._name_edit.setClearButtonEnabled(True)
        self._add_field_row(self.tr("模型名称"), self._name_edit)

        # 分组名称
        self._group_edit = LineEdit(self)
        self._group_edit.setPlaceholderText(self.tr("自动解析"))
        self._group_edit.setClearButtonEnabled(True)
        self._add_field_row(self.tr("分组名称"), self._group_edit)

        self.viewLayout.addSpacing(8)

        # --- 更多设置 + 保存 按钮行 ---
        action_row = QHBoxLayout()
        action_row.setContentsMargins(0, 0, 0, 0)
        action_row.setSpacing(12)

        self._expand_btn = TransparentPushButton(self.tr("更多设置"), self)
        self._expand_btn.setIcon(FluentIcon.CHEVRON_DOWN_MED)
        self._expand_btn.setCheckable(True)
        self._expand_btn.clicked.connect(self._on_toggle_advanced)
        action_row.addWidget(self._expand_btn)

        action_row.addStretch()

        self._save_btn = PrimaryPushButton(FluentIcon.SAVE, self.tr("保存"), self)
        self._save_btn.clicked.connect(self._on_save)
        action_row.addWidget(self._save_btn)

        self.viewLayout.addLayout(action_row)

        # --- 分割线 ---
        self._adv_divider = QFrame(self)
        self._adv_divider.setFrameShape(QFrame.Shape.NoFrame)
        self._adv_divider.setFixedHeight(1)
        self._apply_divider_style()
        cfg.themeChanged.connect(self._apply_divider_style)
        self.viewLayout.addSpacing(8)
        self.viewLayout.addWidget(self._adv_divider)
        self.viewLayout.addSpacing(8)

        # --- 高级区域 (默认隐藏) ---
        self._advanced_widget = QWidget(self)
        adv_layout = QVBoxLayout(self._advanced_widget)
        adv_layout.setContentsMargins(0, 0, 0, 0)
        adv_layout.setSpacing(12)

        # 模型类型 - Pill 标签
        type_header = QHBoxLayout()
        type_header.setSpacing(4)
        type_label = StrongBodyLabel(self.tr("模型类型"), self._advanced_widget)
        type_header.addWidget(type_label)
        self._type_warning = TransparentToolButton(FluentIcon.INFO, self._advanced_widget)
        self._type_warning.setFixedSize(18, 18)
        self._type_warning.setToolTip(self.tr("能力标签影响模型路由策略"))
        type_header.addWidget(self._type_warning)
        type_header.addStretch()
        adv_layout.addLayout(type_header)

        # Pill 标签行
        pill_row = QHBoxLayout()
        pill_row.setSpacing(8)
        self._pills: dict[str, PillPushButton] = {}
        pill_defs = [
            ("vision", self.tr("视觉"), FluentIcon.VIEW, "#4CAF50"),
            ("web", self.tr("联网"), FluentIcon.GLOBE, "#2196F3"),
            ("reasoning", self.tr("推理"), FluentIcon.BRIGHTNESS, "#9C27B0"),
            ("tools", self.tr("工具"), FluentIcon.DEVELOPER_TOOLS, "#FF9800"),
            ("rerank", self.tr("重排"), None, ""),
            ("embedding", self.tr("嵌入"), None, ""),
        ]
        for key, text, icon, _color in pill_defs:
            pill = PillPushButton(self._advanced_widget)
            pill.setText(text)
            if icon:
                pill.setIcon(icon)
            pill.setCheckable(True)
            self._pills[key] = pill
            pill_row.addWidget(pill)
        pill_row.addStretch()
        adv_layout.addLayout(pill_row)

        adv_layout.addSpacing(4)

        # 支持增量文本输出
        streaming_row = QHBoxLayout()
        streaming_row.setContentsMargins(0, 0, 0, 0)
        streaming_label = BodyLabel(self.tr("支持增量文本输出"), self._advanced_widget)
        streaming_row.addWidget(streaming_label)
        self._streaming_help = TransparentToolButton(FluentIcon.INFO, self._advanced_widget)
        self._streaming_help.setFixedSize(18, 18)
        self._streaming_help.setToolTip(self.tr("即 Streaming 模式, 逐 token 返回"))
        streaming_row.addWidget(self._streaming_help)
        streaming_row.addStretch()
        self._streaming_switch = SwitchButton(self._advanced_widget)
        streaming_row.addWidget(self._streaming_switch)
        adv_layout.addLayout(streaming_row)

        adv_layout.addSpacing(4)

        # 币种 + 价格
        price_grid = QGridLayout()
        price_grid.setContentsMargins(0, 0, 0, 0)
        price_grid.setHorizontalSpacing(12)
        price_grid.setVerticalSpacing(8)

        price_grid.addWidget(BodyLabel(self.tr("币种"), self._advanced_widget), 0, 0)
        self._currency_combo = ComboBox(self._advanced_widget)
        self._currency_combo.addItems(["$", "¥", "€"])
        self._currency_combo.setFixedWidth(80)
        price_grid.addWidget(self._currency_combo, 0, 1)

        price_grid.addWidget(BodyLabel(self.tr("输入价格"), self._advanced_widget), 1, 0)
        self._input_price_edit = LineEdit(self._advanced_widget)
        self._input_price_edit.setPlaceholderText("0.00")
        self._input_price_edit.setFixedWidth(100)
        price_grid.addWidget(self._input_price_edit, 1, 1)
        price_grid.addWidget(
            CaptionLabel(self.tr("$ / 百万 Token"), self._advanced_widget), 1, 2
        )

        price_grid.addWidget(BodyLabel(self.tr("输出价格"), self._advanced_widget), 2, 0)
        self._output_price_edit = LineEdit(self._advanced_widget)
        self._output_price_edit.setPlaceholderText("0.00")
        self._output_price_edit.setFixedWidth(100)
        price_grid.addWidget(self._output_price_edit, 2, 1)
        price_grid.addWidget(
            CaptionLabel(self.tr("$ / 百万 Token"), self._advanced_widget), 2, 2
        )

        adv_layout.addLayout(price_grid)

        self.viewLayout.addWidget(self._advanced_widget)
        # 默认折叠高级区域
        self._advanced_widget.setVisible(False)
        self._adv_divider.setVisible(False)

    # ------------------------------------------------------------------
    # 辅助方法
    # ------------------------------------------------------------------

    def _add_field_row(
        self,
        label_text: str,
        edit_widget: QWidget,
        suffix_widget: QWidget | None = None,
        required: bool = False,
    ) -> None:
        """添加一行表单: [标签] [输入框] [可选后缀]."""
        row = QHBoxLayout()
        row.setContentsMargins(0, 0, 0, 0)
        row.setSpacing(12)

        label = BodyLabel(label_text, self)
        if required:
            label.setText("* " + label_text)
            label.setStyleSheet("QLabel { color: #4CAF50; }")
        label.setFixedWidth(80)
        row.addWidget(label)

        row.addWidget(edit_widget, 1)

        if suffix_widget:
            row.addWidget(suffix_widget)

        self.viewLayout.addLayout(row)
        self.viewLayout.addSpacing(8)

    def _apply_divider_style(self, *_args) -> None:
        color = "rgba(255,255,255,0.06)" if isDarkTheme() else "rgba(0,0,0,0.06)"
        self._adv_divider.setStyleSheet(
            f"QFrame {{ background-color: {color}; border: none; }}"
        )

    # ------------------------------------------------------------------
    # 数据填充
    # ------------------------------------------------------------------

    def _populate(self, model: ModelEntry) -> None:
        """用 ModelEntry 数据填充所有控件."""
        self._model_id_edit.setText(model.model_id)
        self._name_edit.setText(model.display_name or model.model_id)
        self._group_edit.setText(
            model.group_name if model.group_name else _parse_group_name(model.model_id)
        )

        # Pill 标签
        self._pills["vision"].setChecked(model.supports_vision)
        self._pills["web"].setChecked(model.supports_web)
        self._pills["reasoning"].setChecked(model.supports_reasoning)
        self._pills["tools"].setChecked(model.supports_tools)
        self._pills["rerank"].setChecked(model.supports_rerank)
        self._pills["embedding"].setChecked(model.supports_embedding)

        # Streaming
        self._streaming_switch.setChecked(model.supports_streaming)

        # 价格
        currency_map = {"USD": "$", "CNY": "¥", "EUR": "€"}
        symbol = currency_map.get(model.currency, "$")
        idx = self._currency_combo.findText(symbol)
        if idx >= 0:
            self._currency_combo.setCurrentIndex(idx)
        self._input_price_edit.setText(
            f"{model.input_price:.2f}" if model.input_price else ""
        )
        self._output_price_edit.setText(
            f"{model.output_price:.2f}" if model.output_price else ""
        )

    # ------------------------------------------------------------------
    # 槽函数
    # ------------------------------------------------------------------

    def _on_copy_id(self) -> None:
        clipboard = QGuiApplication.clipboard()
        if clipboard:
            clipboard.setText(self._model_id_edit.text())
            success_bar(
                content=self.tr("已复制"), title="", duration=1500, parent=self
            )

    def _on_toggle_advanced(self) -> None:
        expanded = self._expand_btn.isChecked()
        self._advanced_widget.setVisible(expanded)
        self._adv_divider.setVisible(expanded)
        self._expand_btn.setIcon(
            FluentIcon.CHEVRON_DOWN_MED if not expanded else FluentIcon.UP
        )

    def _on_save(self) -> None:
        """收集表单数据, 更新 ModelEntry 并发射 saved 信号."""
        # 收集基础字段
        display_name = self._name_edit.text().strip()
        group_name = self._group_edit.text().strip()

        # 收集能力标签
        supports_vision = self._pills["vision"].isChecked()
        supports_web = self._pills["web"].isChecked()
        supports_reasoning = self._pills["reasoning"].isChecked()
        supports_tools = self._pills["tools"].isChecked()
        supports_rerank = self._pills["rerank"].isChecked()
        supports_embedding = self._pills["embedding"].isChecked()

        # Streaming
        supports_streaming = self._streaming_switch.isChecked()

        # 价格
        symbol_map = {"$": "USD", "¥": "CNY", "€": "EUR"}
        currency = symbol_map.get(self._currency_combo.currentText(), "USD")

        input_price = 0.0
        output_price = 0.0
        try:
            input_price = float(self._input_price_edit.text().strip() or "0")
        except ValueError:
            pass
        try:
            output_price = float(self._output_price_edit.text().strip() or "0")
        except ValueError:
            pass

        # 更新 model (pydantic model_copy)
        updated = self._model.model_copy(
            update={
                "display_name": display_name,
                "group_name": group_name,
                "supports_vision": supports_vision,
                "supports_web": supports_web,
                "supports_reasoning": supports_reasoning,
                "supports_tools": supports_tools,
                "supports_rerank": supports_rerank,
                "supports_embedding": supports_embedding,
                "supports_streaming": supports_streaming,
                "currency": currency,
                "input_price": input_price,
                "output_price": output_price,
            }
        )

        self.saved.emit(updated)
        self.accept()

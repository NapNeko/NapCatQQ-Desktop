# -*- coding: utf-8 -*-
"""编辑模型对话框.

自定义 MessageBoxBase 对话框, 去掉默认底部按钮区域, 改为内嵌保存按钮.
布局使用 QFormLayout 风格的 QGridLayout 实现标签-控件对齐:
- 模型 ID (只读 + 复制按钮)
- 模型名称 (可编辑)
- 分组名称 (可编辑, 默认自动解析)
- [更多设置] 折叠区 + [保存] 按钮
- Temperature / Top P (slider + spinbox)
- Max Tokens
- 推理强度
- 模型类型 (Pill 标签多选)
- 支持增量文本输出 (Switch)
- 币种 / 输入价格 / 输出价格
"""
from __future__ import annotations

from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QGuiApplication
from PySide6.QtWidgets import (
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QVBoxLayout,
    QWidget,
)
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    ComboBox,
    DoubleSpinBox,
    FluentIcon,
    LineEdit,
    MessageBoxBase,
    PillPushButton,
    PrimaryPushButton,
    Slider,
    SpinBox,
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

from src.core.logging import LogSource, logger


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

    # 布局常量
    _LABEL_WIDTH = 90       # 表单标签固定宽度
    _SPINBOX_WIDTH = 110    # spinbox 统一宽度
    _FIELD_SPACING = 16     # 表单行间距
    _SECTION_SPACING = 20   # 区块间距

    def __init__(self, model: ModelEntry, parent: QWidget) -> None:
        self._model = model
        self._syncing = False
        super().__init__(parent=parent)
        self._setup_content()
        self._populate(model)
        self.widget.setMinimumWidth(580)

        # 隐藏默认底部按钮区域
        self.yesButton.setVisible(False)
        self.cancelButton.setVisible(False)
        self.buttonGroup.setVisible(False)

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_content(self) -> None:
        """构建对话框内容."""
        self.viewLayout.setSpacing(0)

        # 标题行: "编辑模型" + 关闭按钮
        self._setup_title_row()

        # 基础字段 (Grid 布局)
        self._setup_basic_fields()

        # 更多设置 + 保存 按钮行
        self.viewLayout.addSpacing(self._SECTION_SPACING)
        self._setup_action_row()

        # 分割线
        self._setup_divider()

        # 高级区域 (默认隐藏)
        self._setup_advanced_section()

    def _setup_title_row(self) -> None:
        """标题行: '编辑模型' + 关闭按钮."""
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
        self.viewLayout.addSpacing(self._SECTION_SPACING)

    def _setup_basic_fields(self) -> None:
        """基础字段区域 - 使用 QGridLayout 对齐标签和输入框."""
        grid = QGridLayout()
        grid.setContentsMargins(0, 0, 0, 0)
        grid.setHorizontalSpacing(12)
        grid.setVerticalSpacing(self._FIELD_SPACING)
        grid.setColumnMinimumWidth(0, self._LABEL_WIDTH)

        row = 0

        # 模型 ID (只读 + 复制)
        id_label = BodyLabel(self.tr("* 模型 ID"), self)
        id_label.setStyleSheet("QLabel { color: #4CAF50; }")
        grid.addWidget(id_label, row, 0, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)

        id_row = QHBoxLayout()
        id_row.setSpacing(8)
        self._model_id_edit = LineEdit(self)
        self._model_id_edit.setReadOnly(True)
        id_row.addWidget(self._model_id_edit, 1)
        self._copy_btn = TransparentToolButton(FluentIcon.COPY, self)
        self._copy_btn.setFixedSize(32, 32)
        self._copy_btn.setToolTip(self.tr("复制模型 ID"))
        self._copy_btn.clicked.connect(self._on_copy_id)
        id_row.addWidget(self._copy_btn)
        grid.addLayout(id_row, row, 1)

        row += 1

        # 模型名称
        name_label = BodyLabel(self.tr("模型名称"), self)
        grid.addWidget(name_label, row, 0, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        self._name_edit = LineEdit(self)
        self._name_edit.setPlaceholderText(self.tr("显示名称 (可选)"))
        self._name_edit.setClearButtonEnabled(True)
        grid.addWidget(self._name_edit, row, 1)

        row += 1

        # 分组名称
        group_label = BodyLabel(self.tr("分组名称"), self)
        grid.addWidget(group_label, row, 0, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        self._group_edit = LineEdit(self)
        self._group_edit.setPlaceholderText(self.tr("自动解析"))
        self._group_edit.setClearButtonEnabled(True)
        grid.addWidget(self._group_edit, row, 1)

        self.viewLayout.addLayout(grid)

    def _setup_action_row(self) -> None:
        """更多设置 + 保存 按钮行."""
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

    def _setup_divider(self) -> None:
        """分割线 (高级区域展开时可见)."""
        self._adv_divider = QFrame(self)
        self._adv_divider.setFrameShape(QFrame.Shape.NoFrame)
        self._adv_divider.setFixedHeight(1)
        self._apply_divider_style()
        cfg.themeChanged.connect(self._apply_divider_style)
        self.viewLayout.addSpacing(12)
        self.viewLayout.addWidget(self._adv_divider)
        self.viewLayout.addSpacing(12)
        self._adv_divider.setVisible(False)

    def _setup_advanced_section(self) -> None:
        """高级区域 (默认隐藏)."""
        self._advanced_widget = QWidget(self)
        adv_layout = QVBoxLayout(self._advanced_widget)
        adv_layout.setContentsMargins(0, 0, 0, 0)
        adv_layout.setSpacing(0)

        # 参数调节区 - 使用 Grid 对齐
        param_grid = QGridLayout()
        param_grid.setContentsMargins(0, 0, 0, 0)
        param_grid.setHorizontalSpacing(12)
        param_grid.setVerticalSpacing(14)
        param_grid.setColumnMinimumWidth(0, self._LABEL_WIDTH)

        row = 0

        # Temperature (0.0 - 2.0)
        temp_label = CaptionLabel(self.tr("Temperature (0.0 - 2.0)"), self._advanced_widget)
        param_grid.addWidget(
            temp_label, row, 0, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter
        )
        temp_row = QHBoxLayout()
        temp_row.setSpacing(12)
        self._temp_slider = Slider(Qt.Orientation.Horizontal, self._advanced_widget)
        self._temp_slider.setRange(0, 20)  # 0.0-2.0, step 0.1
        self._temp_slider.setValue(7)
        self._temp_spinbox = DoubleSpinBox(self._advanced_widget)
        self._temp_spinbox.setRange(0.0, 2.0)
        self._temp_spinbox.setSingleStep(0.1)
        self._temp_spinbox.setDecimals(1)
        self._temp_spinbox.setValue(0.7)
        self._temp_spinbox.setFixedWidth(self._SPINBOX_WIDTH)
        temp_row.addWidget(self._temp_slider, 1)
        temp_row.addWidget(self._temp_spinbox, 0)
        param_grid.addLayout(temp_row, row, 1)

        self._temp_slider.valueChanged.connect(self._on_temp_slider_changed)
        self._temp_spinbox.valueChanged.connect(self._on_temp_spinbox_changed)

        row += 1

        # Top P (0.0 - 1.0)
        top_p_label = CaptionLabel(self.tr("Top P (0.0 - 1.0)"), self._advanced_widget)
        param_grid.addWidget(
            top_p_label, row, 0, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter
        )
        top_p_row = QHBoxLayout()
        top_p_row.setSpacing(12)
        self._top_p_slider = Slider(Qt.Orientation.Horizontal, self._advanced_widget)
        self._top_p_slider.setRange(0, 20)  # 0.0-1.0, step 0.05
        self._top_p_slider.setValue(20)
        self._top_p_spinbox = DoubleSpinBox(self._advanced_widget)
        self._top_p_spinbox.setRange(0.0, 1.0)
        self._top_p_spinbox.setSingleStep(0.05)
        self._top_p_spinbox.setDecimals(2)
        self._top_p_spinbox.setValue(1.0)
        self._top_p_spinbox.setFixedWidth(self._SPINBOX_WIDTH)
        top_p_row.addWidget(self._top_p_slider, 1)
        top_p_row.addWidget(self._top_p_spinbox, 0)
        param_grid.addLayout(top_p_row, row, 1)

        self._top_p_slider.valueChanged.connect(self._on_top_p_slider_changed)
        self._top_p_spinbox.valueChanged.connect(self._on_top_p_spinbox_changed)

        row += 1

        # Max Tokens
        max_tokens_label = CaptionLabel(self.tr("Max Tokens"), self._advanced_widget)
        param_grid.addWidget(
            max_tokens_label, row, 0, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter
        )
        self._max_tokens_spinbox = SpinBox(self._advanced_widget)
        self._max_tokens_spinbox.setRange(1, 10_000_000)
        self._max_tokens_spinbox.setSingleStep(1024)
        self._max_tokens_spinbox.setKeyboardTracking(False)
        self._max_tokens_spinbox.setFixedWidth(160)
        param_grid.addWidget(self._max_tokens_spinbox, row, 1, Qt.AlignmentFlag.AlignLeft)

        row += 1

        # Reasoning Effort (根据 supports_reasoning 显示/隐藏)
        self._reasoning_label = CaptionLabel(
            self.tr("推理强度"), self._advanced_widget
        )
        param_grid.addWidget(
            self._reasoning_label, row, 0,
            Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter,
        )
        self._reasoning_combo = ComboBox(self._advanced_widget)
        self._reasoning_combo.addItems([
            self.tr("不设置"),
            self.tr("低"),
            self.tr("中"),
            self.tr("高"),
        ])
        self._reasoning_combo.setCurrentIndex(0)
        self._reasoning_combo.setFixedWidth(160)
        param_grid.addWidget(self._reasoning_combo, row, 1, Qt.AlignmentFlag.AlignLeft)

        adv_layout.addLayout(param_grid)
        adv_layout.addSpacing(self._SECTION_SPACING)

        # 模型类型 - Pill 标签
        type_header = QHBoxLayout()
        type_header.setContentsMargins(0, 0, 0, 0)
        type_header.setSpacing(6)
        type_label = StrongBodyLabel(self.tr("模型类型"), self._advanced_widget)
        type_header.addWidget(type_label)
        self._type_warning = TransparentToolButton(FluentIcon.INFO, self._advanced_widget)
        self._type_warning.setFixedSize(20, 20)
        self._type_warning.setToolTip(self.tr("能力标签影响模型路由策略"))
        type_header.addWidget(self._type_warning)
        type_header.addStretch()
        adv_layout.addLayout(type_header)
        adv_layout.addSpacing(10)

        # Pill 标签行 (允许换行)
        pill_row = QHBoxLayout()
        pill_row.setSpacing(8)
        pill_row.setContentsMargins(0, 0, 0, 0)
        self._pills: dict[str, PillPushButton] = {}
        pill_defs = [
            ("vision", self.tr("视觉"), FluentIcon.VIEW),
            ("web", self.tr("联网"), FluentIcon.GLOBE),
            ("reasoning", self.tr("推理"), FluentIcon.BRIGHTNESS),
            ("tools", self.tr("工具"), FluentIcon.DEVELOPER_TOOLS),
            ("rerank", self.tr("重排"), None),
            ("embedding", self.tr("嵌入"), None),
        ]
        for key, text, icon in pill_defs:
            pill = PillPushButton(self._advanced_widget)
            pill.setText(text)
            if icon:
                pill.setIcon(icon)
            pill.setCheckable(True)
            self._pills[key] = pill
            pill_row.addWidget(pill)
        pill_row.addStretch()
        adv_layout.addLayout(pill_row)
        adv_layout.addSpacing(self._FIELD_SPACING)

        # 支持增量文本输出
        streaming_row = QHBoxLayout()
        streaming_row.setContentsMargins(0, 0, 0, 0)
        streaming_row.setSpacing(6)
        streaming_label = BodyLabel(self.tr("支持增量文本输出"), self._advanced_widget)
        streaming_row.addWidget(streaming_label)
        self._streaming_help = TransparentToolButton(FluentIcon.INFO, self._advanced_widget)
        self._streaming_help.setFixedSize(20, 20)
        self._streaming_help.setToolTip(self.tr("即 Streaming 模式, 逐 token 返回"))
        streaming_row.addWidget(self._streaming_help)
        streaming_row.addStretch()
        self._streaming_switch = SwitchButton(self._advanced_widget)
        streaming_row.addWidget(self._streaming_switch)
        adv_layout.addLayout(streaming_row)
        adv_layout.addSpacing(self._SECTION_SPACING)

        # 价格区域
        self._setup_price_section(adv_layout)

        self.viewLayout.addWidget(self._advanced_widget)
        self._advanced_widget.setVisible(False)

    def _setup_price_section(self, parent_layout: QVBoxLayout) -> None:
        """价格设置区域 - 使用 Grid 对齐."""
        price_title = StrongBodyLabel(self.tr("价格设置"), self._advanced_widget)
        parent_layout.addWidget(price_title)
        parent_layout.addSpacing(10)

        price_grid = QGridLayout()
        price_grid.setContentsMargins(0, 0, 0, 0)
        price_grid.setHorizontalSpacing(12)
        price_grid.setVerticalSpacing(12)
        price_grid.setColumnMinimumWidth(0, self._LABEL_WIDTH)

        # 币种
        currency_label = BodyLabel(self.tr("币种"), self._advanced_widget)
        price_grid.addWidget(
            currency_label, 0, 0, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter
        )
        self._currency_combo = ComboBox(self._advanced_widget)
        self._currency_combo.addItems(["$", "¥", "€"])
        self._currency_combo.setFixedWidth(80)
        price_grid.addWidget(self._currency_combo, 0, 1, Qt.AlignmentFlag.AlignLeft)

        # 输入价格
        input_label = BodyLabel(self.tr("输入价格"), self._advanced_widget)
        price_grid.addWidget(
            input_label, 1, 0, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter
        )
        input_price_row = QHBoxLayout()
        input_price_row.setSpacing(8)
        self._input_price_edit = LineEdit(self._advanced_widget)
        self._input_price_edit.setPlaceholderText("0.00")
        self._input_price_edit.setFixedWidth(120)
        input_price_row.addWidget(self._input_price_edit)
        input_price_row.addWidget(
            CaptionLabel(self.tr("/ 百万 Token"), self._advanced_widget)
        )
        input_price_row.addStretch()
        price_grid.addLayout(input_price_row, 1, 1)

        # 输出价格
        output_label = BodyLabel(self.tr("输出价格"), self._advanced_widget)
        price_grid.addWidget(
            output_label, 2, 0, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter
        )
        output_price_row = QHBoxLayout()
        output_price_row.setSpacing(8)
        self._output_price_edit = LineEdit(self._advanced_widget)
        self._output_price_edit.setPlaceholderText("0.00")
        self._output_price_edit.setFixedWidth(120)
        output_price_row.addWidget(self._output_price_edit)
        output_price_row.addWidget(
            CaptionLabel(self.tr("/ 百万 Token"), self._advanced_widget)
        )
        output_price_row.addStretch()
        price_grid.addLayout(output_price_row, 2, 1)

        parent_layout.addLayout(price_grid)

    # ------------------------------------------------------------------
    # 辅助方法
    # ------------------------------------------------------------------

    def _apply_divider_style(self, *_args) -> None:
        """应用分割线样式, 适配主题."""
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

        # Temperature
        temp_value = model.temperature if model.temperature is not None else 0.7
        self._syncing = True
        self._temp_slider.setValue(round(temp_value * 10))
        self._temp_spinbox.setValue(temp_value)
        self._syncing = False

        # Top P
        top_p_value = model.top_p if model.top_p is not None else 1.0
        self._syncing = True
        self._top_p_slider.setValue(round(top_p_value / 0.05))
        self._top_p_spinbox.setValue(top_p_value)
        self._syncing = False

        # Max Tokens
        self._max_tokens_spinbox.setValue(model.max_tokens)

        # Reasoning Effort
        effort_map = {"low": 1, "medium": 2, "high": 3}
        idx = effort_map.get(model.reasoning_effort, 0) if model.reasoning_effort else 0
        self._reasoning_combo.setCurrentIndex(idx)

        # 根据 supports_reasoning 控制可见性
        reasoning_visible = model.supports_reasoning
        self._reasoning_label.setVisible(reasoning_visible)
        self._reasoning_combo.setVisible(reasoning_visible)

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

        # 连接 reasoning pill 切换 → 动态显示/隐藏 reasoning_effort
        self._pills["reasoning"].toggled.connect(self._on_reasoning_pill_toggled)

    # ------------------------------------------------------------------
    # 槽函数
    # ------------------------------------------------------------------

    def _on_copy_id(self) -> None:
        """复制模型 ID 到剪贴板."""
        clipboard = QGuiApplication.clipboard()
        if clipboard:
            clipboard.setText(self._model_id_edit.text())
            success_bar(
                content=self.tr("已复制"), title="", duration=1500, parent=self
            )

    def _on_toggle_advanced(self) -> None:
        """切换高级区域展开/折叠."""
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

        # Temperature / Top P / Max Tokens
        temperature = round(self._temp_spinbox.value(), 1)
        top_p = round(self._top_p_spinbox.value(), 2)
        max_tokens = self._max_tokens_spinbox.value()

        # Reasoning Effort
        effort_index = self._reasoning_combo.currentIndex()
        effort_map = {0: None, 1: "low", 2: "medium", 3: "high"}
        reasoning_effort = effort_map[effort_index]

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
                "max_tokens": max_tokens,
                "temperature": temperature,
                "top_p": top_p,
                "reasoning_effort": reasoning_effort,
                "currency": currency,
                "input_price": input_price,
                "output_price": output_price,
            }
        )

        self._persist_config()
        self.saved.emit(updated)
        self.accept()

    # ------------------------------------------------------------------
    # Temperature 双向同步
    # ------------------------------------------------------------------

    def _on_temp_slider_changed(self, value: int) -> None:
        """Temperature 滑块变更 -> 更新 spinbox."""
        if self._syncing:
            return
        self._syncing = True
        actual = round(value * 0.1, 1)
        self._temp_spinbox.setValue(actual)
        self._syncing = False

    def _on_temp_spinbox_changed(self, value: float) -> None:
        """Temperature spinbox 变更 -> 更新滑块."""
        if self._syncing:
            return
        self._syncing = True
        slider_value = round(value * 10)
        self._temp_slider.setValue(slider_value)
        self._syncing = False

    # ------------------------------------------------------------------
    # Top P 双向同步
    # ------------------------------------------------------------------

    def _on_top_p_slider_changed(self, value: int) -> None:
        """Top P 滑块变更 -> 更新 spinbox."""
        if self._syncing:
            return
        self._syncing = True
        actual = round(value * 0.05, 2)
        self._top_p_spinbox.setValue(actual)
        self._syncing = False

    def _on_top_p_spinbox_changed(self, value: float) -> None:
        """Top P spinbox 变更 -> 更新滑块."""
        if self._syncing:
            return
        self._syncing = True
        slider_value = round(value / 0.05)
        self._top_p_slider.setValue(slider_value)
        self._syncing = False

    # ------------------------------------------------------------------
    # Reasoning Effort 动态显示/隐藏
    # ------------------------------------------------------------------

    def _on_reasoning_pill_toggled(self, checked: bool) -> None:
        """reasoning pill 切换时动态显示/隐藏 reasoning_effort 控件."""
        self._reasoning_label.setVisible(checked)
        self._reasoning_combo.setVisible(checked)
        if not checked:
            self._reasoning_combo.setCurrentIndex(0)

    # ------------------------------------------------------------------
    # 持久化
    # ------------------------------------------------------------------

    def _persist_config(self) -> None:
        """触发 ConfigPersistence.save() 将当前配置写入磁盘."""
        try:
            from creart import it

            from src.core.agent.config_persistence import ConfigPersistence
            from src.core.agent.provider import ProviderRegistry
            from src.core.runtime.paths import PathFunc

            path_func: PathFunc = it(PathFunc)
            config_file_path = path_func.config_dir_path / "agent_config.json"
            persistence = ConfigPersistence(config_file_path)

            config_data = persistence.load()
            registry: ProviderRegistry = it(ProviderRegistry)
            config_data.providers = registry.list_all()

            persistence.save(config_data)
        except Exception:
            logger.error("EditModelDialog 持久化配置失败")

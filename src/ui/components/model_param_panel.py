# -*- coding: utf-8 -*-
"""模型参数调节面板.

提供 temperature、top_p、max_tokens 的滑块+输入框控件，
实时同步参数变更并通过信号通知外部控制器。
"""

from __future__ import annotations

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import (
    CaptionLabel,
    DoubleSpinBox,
    PrimaryPushButton,
    Slider,
    SpinBox,
    StrongBodyLabel,
)

from src.core.agent.model_param_utils import (
    clamp_max_tokens,
    clamp_temperature,
    clamp_top_p,
)
from src.core.agent.provider import ModelConfig, ModelEntry


class ModelParamPanel(QWidget):
    """模型参数调节面板 — temperature / top_p / max_tokens 滑块+输入框.

    通过 param_changed 信号通知参数变更，由外部控制器负责持久化。
    """

    param_changed = Signal(str, object)  # (field_name, new_value)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._syncing = False  # 防止双向同步时循环触发
        self._max_tokens_limit = 4096  # 当前模型的 max_tokens 上限

        self._setup_ui()
        self._connect_signals()

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建面板 UI 布局."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(16)

        # Temperature
        self._temp_label = StrongBodyLabel("Temperature", self)
        self._temp_hint = CaptionLabel("0.0 - 2.0", self)
        self._temp_slider = Slider(Qt.Orientation.Horizontal, self)
        self._temp_slider.setRange(0, 20)
        self._temp_slider.setValue(7)  # 0.7 * 10
        self._temp_spinbox = DoubleSpinBox(self)
        self._temp_spinbox.setRange(0.0, 2.0)
        self._temp_spinbox.setSingleStep(0.1)
        self._temp_spinbox.setDecimals(1)
        self._temp_spinbox.setValue(0.7)

        temp_row = QHBoxLayout()
        temp_row.setSpacing(12)
        temp_row.addWidget(self._temp_slider, 1)
        temp_row.addWidget(self._temp_spinbox, 0)

        layout.addWidget(self._temp_label)
        layout.addWidget(self._temp_hint)
        layout.addLayout(temp_row)

        # Top P
        self._top_p_label = StrongBodyLabel("Top P", self)
        self._top_p_hint = CaptionLabel("0.0 - 1.0", self)
        self._top_p_slider = Slider(Qt.Orientation.Horizontal, self)
        self._top_p_slider.setRange(0, 20)
        self._top_p_slider.setValue(20)  # 1.0 / 0.05 = 20
        self._top_p_spinbox = DoubleSpinBox(self)
        self._top_p_spinbox.setRange(0.0, 1.0)
        self._top_p_spinbox.setSingleStep(0.05)
        self._top_p_spinbox.setDecimals(2)
        self._top_p_spinbox.setValue(1.0)

        top_p_row = QHBoxLayout()
        top_p_row.setSpacing(12)
        top_p_row.addWidget(self._top_p_slider, 1)
        top_p_row.addWidget(self._top_p_spinbox, 0)

        layout.addWidget(self._top_p_label)
        layout.addWidget(self._top_p_hint)
        layout.addLayout(top_p_row)

        # Max Tokens
        self._max_tokens_label = StrongBodyLabel("Max Tokens", self)
        self._max_tokens_hint = CaptionLabel("1 - 4096", self)
        self._max_tokens_slider = Slider(Qt.Orientation.Horizontal, self)
        self._max_tokens_slider.setRange(1, 4096)
        self._max_tokens_slider.setValue(4096)
        self._max_tokens_spinbox = SpinBox(self)
        self._max_tokens_spinbox.setRange(1, 4096)
        self._max_tokens_spinbox.setSingleStep(1)
        self._max_tokens_spinbox.setValue(4096)

        max_tokens_row = QHBoxLayout()
        max_tokens_row.setSpacing(12)
        max_tokens_row.addWidget(self._max_tokens_slider, 1)
        max_tokens_row.addWidget(self._max_tokens_spinbox, 0)

        layout.addWidget(self._max_tokens_label)
        layout.addWidget(self._max_tokens_hint)
        layout.addLayout(max_tokens_row)

        # 重置按钮
        self._reset_button = PrimaryPushButton(self.tr("重置为默认值"), self)
        layout.addSpacing(8)
        layout.addWidget(self._reset_button, 0, Qt.AlignmentFlag.AlignLeft)

    # ------------------------------------------------------------------
    # 信号连接
    # ------------------------------------------------------------------

    def _connect_signals(self) -> None:
        """连接滑块和输入框的双向同步信号."""
        # Temperature
        self._temp_slider.valueChanged.connect(self._on_temp_slider_changed)
        self._temp_spinbox.valueChanged.connect(self._on_temp_spinbox_changed)

        # Top P
        self._top_p_slider.valueChanged.connect(self._on_top_p_slider_changed)
        self._top_p_spinbox.valueChanged.connect(self._on_top_p_spinbox_changed)

        # Max Tokens
        self._max_tokens_slider.valueChanged.connect(self._on_max_tokens_slider_changed)
        self._max_tokens_spinbox.valueChanged.connect(self._on_max_tokens_spinbox_changed)

        # 重置按钮
        self._reset_button.clicked.connect(self._on_reset_clicked)

    # ------------------------------------------------------------------
    # Temperature 双向同步
    # ------------------------------------------------------------------

    def _on_temp_slider_changed(self, value: int) -> None:
        """滑块变更 → 更新 spinbox 并发射信号."""
        if self._syncing:
            return
        self._syncing = True
        actual = round(value * 0.1, 1)
        clamped = clamp_temperature(actual)
        self._temp_spinbox.setValue(clamped)
        self._syncing = False
        self.param_changed.emit("temperature", clamped)

    def _on_temp_spinbox_changed(self, value: float) -> None:
        """Spinbox 变更 → 更新滑块并发射信号."""
        if self._syncing:
            return
        self._syncing = True
        clamped = clamp_temperature(value)
        slider_value = round(clamped * 10)
        self._temp_slider.setValue(slider_value)
        if clamped != value:
            self._temp_spinbox.setValue(clamped)
        self._syncing = False
        self.param_changed.emit("temperature", clamped)

    # ------------------------------------------------------------------
    # Top P 双向同步
    # ------------------------------------------------------------------

    def _on_top_p_slider_changed(self, value: int) -> None:
        """滑块变更 → 更新 spinbox 并发射信号."""
        if self._syncing:
            return
        self._syncing = True
        actual = round(value * 0.05, 2)
        clamped = clamp_top_p(actual)
        self._top_p_spinbox.setValue(clamped)
        self._syncing = False
        self.param_changed.emit("top_p", clamped)

    def _on_top_p_spinbox_changed(self, value: float) -> None:
        """Spinbox 变更 → 更新滑块并发射信号."""
        if self._syncing:
            return
        self._syncing = True
        clamped = clamp_top_p(value)
        slider_value = round(clamped / 0.05)
        self._top_p_slider.setValue(slider_value)
        if clamped != value:
            self._top_p_spinbox.setValue(clamped)
        self._syncing = False
        self.param_changed.emit("top_p", clamped)

    # ------------------------------------------------------------------
    # Max Tokens 双向同步
    # ------------------------------------------------------------------

    def _on_max_tokens_slider_changed(self, value: int) -> None:
        """滑块变更 → 更新 spinbox 并发射信号."""
        if self._syncing:
            return
        self._syncing = True
        clamped = clamp_max_tokens(value, self._max_tokens_limit)
        self._max_tokens_spinbox.setValue(clamped)
        self._syncing = False
        self.param_changed.emit("max_tokens", clamped)

    def _on_max_tokens_spinbox_changed(self, value: int) -> None:
        """Spinbox 变更 → 更新滑块并发射信号."""
        if self._syncing:
            return
        self._syncing = True
        clamped = clamp_max_tokens(value, self._max_tokens_limit)
        self._max_tokens_slider.setValue(clamped)
        if clamped != value:
            self._max_tokens_spinbox.setValue(clamped)
        self._syncing = False
        self.param_changed.emit("max_tokens", clamped)

    # ------------------------------------------------------------------
    # 重置
    # ------------------------------------------------------------------

    def _on_reset_clicked(self) -> None:
        """重置按钮点击 → 委托给 reset_to_defaults (使用当前 max_tokens_limit)."""
        # 构造一个临时 ModelEntry 用于重置
        entry = ModelEntry(model_id="temp", max_tokens=self._max_tokens_limit)
        self.reset_to_defaults(entry)

    # ------------------------------------------------------------------
    # 公共接口
    # ------------------------------------------------------------------

    def load_config(self, model_config: ModelConfig, model_entry: ModelEntry) -> None:
        """加载模型配置到控件.

        Args:
            model_config: 当前模型的参数配置。
            model_entry: 当前模型条目（用于获取 max_tokens 上限）。
        """
        self._syncing = True
        self._max_tokens_limit = model_entry.max_tokens

        # 更新 max_tokens 范围
        self._max_tokens_slider.setRange(1, model_entry.max_tokens)
        self._max_tokens_spinbox.setRange(1, model_entry.max_tokens)
        self._max_tokens_hint.setText(f"1 - {model_entry.max_tokens}")

        # 设置 temperature
        temp = clamp_temperature(model_config.temperature)
        self._temp_slider.setValue(round(temp * 10))
        self._temp_spinbox.setValue(temp)

        # 设置 top_p
        top_p = clamp_top_p(model_config.top_p)
        self._top_p_slider.setValue(round(top_p / 0.05))
        self._top_p_spinbox.setValue(top_p)

        # 设置 max_tokens
        max_tokens = clamp_max_tokens(model_config.max_tokens, model_entry.max_tokens)
        self._max_tokens_slider.setValue(max_tokens)
        self._max_tokens_spinbox.setValue(max_tokens)

        self._syncing = False

    def reset_to_defaults(self, model_entry: ModelEntry) -> None:
        """重置所有参数为默认值.

        Args:
            model_entry: 当前模型条目（用于获取 max_tokens 默认值）。
        """
        self._syncing = True
        self._max_tokens_limit = model_entry.max_tokens

        # 更新 max_tokens 范围
        self._max_tokens_slider.setRange(1, model_entry.max_tokens)
        self._max_tokens_spinbox.setRange(1, model_entry.max_tokens)
        self._max_tokens_hint.setText(f"1 - {model_entry.max_tokens}")

        # 设置默认值
        self._temp_slider.setValue(7)  # 0.7
        self._temp_spinbox.setValue(0.7)

        self._top_p_slider.setValue(20)  # 1.0
        self._top_p_spinbox.setValue(1.0)

        self._max_tokens_slider.setValue(model_entry.max_tokens)
        self._max_tokens_spinbox.setValue(model_entry.max_tokens)

        self._syncing = False

        # 发射参数变更信号
        self.param_changed.emit("temperature", 0.7)
        self.param_changed.emit("top_p", 1.0)
        self.param_changed.emit("max_tokens", model_entry.max_tokens)

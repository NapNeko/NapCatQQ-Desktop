# -*- coding: utf-8 -*-
"""编辑模型对话框 - 管理单个模型的显示名称、max_tokens 及高级参数设置。"""
from __future__ import annotations

from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    ComboBox,
    DoubleSpinBox,
    FluentIcon,
    LineEdit,
    MessageBoxBase,
    Slider,
    SpinBox,
    SubtitleLabel,
    ToolButton,
)
from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

from src.core.agent.provider import ModelEntry

from src.core.logging import LogSource, logger


class EditModelDialog(MessageBoxBase):
    """编辑模型对话框，包含"更多设置"折叠区。

    提供 display_name、max_tokens 基础表单，以及可折叠的高级参数区域
    （temperature slider、top_p slider、max_tokens input、reasoning_effort combo）。

    Signals:
        saved: 保存成功时发射，携带更新后的 ModelEntry。
    """

    saved = Signal(object)  # ModelEntry

    def __init__(self, model_entry: ModelEntry, parent: QWidget) -> None:
        super().__init__(parent=parent)
        self._model_entry = model_entry
        self._syncing = False  # 防止双向同步循环触发

        self._setup_ui()
        self._load_values()
        self.widget.setMinimumWidth(520)

        # 连接保存按钮到 save() 方法
        self.yesButton.clicked.disconnect()
        self.yesButton.clicked.connect(self._on_save_clicked)

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建对话框 UI。"""
        # 标题
        self.title_label = SubtitleLabel(self.tr("编辑模型"), self)
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addSpacing(12)

        # --- 基础表单 ---
        self._setup_basic_form()

        # --- 更多设置折叠区 ---
        self.viewLayout.addSpacing(12)
        self._setup_advanced_section()

        # 配置按钮文本
        self.yesButton.setText(self.tr("保存"))
        self.cancelButton.setText(self.tr("取消"))

    def _setup_basic_form(self) -> None:
        """构建基础表单：display_name 输入框、max_tokens 输入框。"""
        # Display Name
        self.display_name_label = BodyLabel(self.tr("显示名称"), self)
        self.display_name_edit = LineEdit(self)
        self.display_name_edit.setPlaceholderText(
            self.tr("模型显示名称 (留空则使用 model_id)")
        )
        self.display_name_edit.setClearButtonEnabled(True)
        self.viewLayout.addWidget(self.display_name_label)
        self.viewLayout.addWidget(self.display_name_edit)
        self.viewLayout.addSpacing(8)

        # Max Tokens
        self.max_tokens_label = BodyLabel(self.tr("最大 Token 数"), self)
        self.max_tokens_spinbox = SpinBox(self)
        self.max_tokens_spinbox.setRange(1, 10_000_000)
        self.max_tokens_spinbox.setSingleStep(1024)
        self.max_tokens_spinbox.setKeyboardTracking(False)
        self.viewLayout.addWidget(self.max_tokens_label)
        self.viewLayout.addWidget(self.max_tokens_spinbox)

    def _setup_advanced_section(self) -> None:
        """构建"更多设置"折叠区：temperature slider, top_p slider, reasoning_effort combo。"""
        # 折叠区标题行：点击可展开/收起
        header_row = QHBoxLayout()
        header_row.setContentsMargins(0, 0, 0, 0)

        self._advanced_toggle_btn = ToolButton(FluentIcon.CHEVRON_RIGHT, self)
        self._advanced_toggle_btn.setFixedSize(28, 28)
        self._advanced_toggle_btn.clicked.connect(self._toggle_advanced_section)

        self._advanced_title = BodyLabel(self.tr("更多设置"), self)

        header_row.addWidget(self._advanced_toggle_btn)
        header_row.addWidget(self._advanced_title)
        header_row.addStretch(1)
        self.viewLayout.addLayout(header_row)

        # 折叠区内容容器
        self._advanced_widget = QWidget(self)
        self._advanced_layout = QVBoxLayout(self._advanced_widget)
        self._advanced_layout.setContentsMargins(4, 8, 4, 0)
        self._advanced_layout.setSpacing(12)

        # Temperature
        self._setup_temperature_control()

        # Top P
        self._setup_top_p_control()

        # Reasoning Effort
        self._setup_reasoning_effort_control()

        self.viewLayout.addWidget(self._advanced_widget)

        # 默认收起
        self._advanced_expanded = False
        self._advanced_widget.setVisible(False)

    def _setup_temperature_control(self) -> None:
        """构建 temperature 滑块控件（范围 0.0–2.0，步长 0.1，默认 0.7）。"""
        temp_label = CaptionLabel(self.tr("Temperature (0.0 - 2.0)"), self._advanced_widget)
        self._advanced_layout.addWidget(temp_label)

        temp_row = QHBoxLayout()
        temp_row.setSpacing(12)

        self.temp_slider = Slider(Qt.Orientation.Horizontal, self._advanced_widget)
        self.temp_slider.setRange(0, 20)  # 0.0 到 2.0，步长 0.1
        self.temp_slider.setValue(7)  # 默认 0.7

        self.temp_spinbox = DoubleSpinBox(self._advanced_widget)
        self.temp_spinbox.setRange(0.0, 2.0)
        self.temp_spinbox.setSingleStep(0.1)
        self.temp_spinbox.setDecimals(1)
        self.temp_spinbox.setValue(0.7)

        temp_row.addWidget(self.temp_slider, 1)
        temp_row.addWidget(self.temp_spinbox, 0)
        self._advanced_layout.addLayout(temp_row)

        # 双向同步
        self.temp_slider.valueChanged.connect(self._on_temp_slider_changed)
        self.temp_spinbox.valueChanged.connect(self._on_temp_spinbox_changed)

    def _setup_top_p_control(self) -> None:
        """构建 top_p 滑块控件（范围 0.0–1.0，步长 0.05，默认 1.0）。"""
        top_p_label = CaptionLabel(self.tr("Top P (0.0 - 1.0)"), self._advanced_widget)
        self._advanced_layout.addWidget(top_p_label)

        top_p_row = QHBoxLayout()
        top_p_row.setSpacing(12)

        self.top_p_slider = Slider(Qt.Orientation.Horizontal, self._advanced_widget)
        self.top_p_slider.setRange(0, 20)  # 0.0 到 1.0，步长 0.05 → 20 steps
        self.top_p_slider.setValue(20)  # 默认 1.0

        self.top_p_spinbox = DoubleSpinBox(self._advanced_widget)
        self.top_p_spinbox.setRange(0.0, 1.0)
        self.top_p_spinbox.setSingleStep(0.05)
        self.top_p_spinbox.setDecimals(2)
        self.top_p_spinbox.setValue(1.0)

        top_p_row.addWidget(self.top_p_slider, 1)
        top_p_row.addWidget(self.top_p_spinbox, 0)
        self._advanced_layout.addLayout(top_p_row)

        # 双向同步
        self.top_p_slider.valueChanged.connect(self._on_top_p_slider_changed)
        self.top_p_spinbox.valueChanged.connect(self._on_top_p_spinbox_changed)

    def _setup_reasoning_effort_control(self) -> None:
        """构建 reasoning_effort 下拉控件。

        根据 ModelEntry.supports_reasoning 动态显示/隐藏。
        """
        self._reasoning_label = CaptionLabel(
            self.tr("推理强度 (Reasoning Effort)"), self._advanced_widget
        )
        self._advanced_layout.addWidget(self._reasoning_label)

        self.reasoning_effort_combo = ComboBox(self._advanced_widget)
        self.reasoning_effort_combo.addItems([
            self.tr("不设置"),
            self.tr("低"),
            self.tr("中"),
            self.tr("高"),
        ])
        self.reasoning_effort_combo.setCurrentIndex(0)
        self._advanced_layout.addWidget(self.reasoning_effort_combo)

        # 根据 supports_reasoning 控制可见性
        visible = self._model_entry.supports_reasoning
        self._reasoning_label.setVisible(visible)
        self.reasoning_effort_combo.setVisible(visible)

    # ------------------------------------------------------------------
    # supports_reasoning 动态切换
    # ------------------------------------------------------------------

    def _on_reasoning_toggled(self, checked: bool) -> None:
        """supports_reasoning 标签切换时动态显示/隐藏 reasoning_effort 控件。

        当外部或内部切换模型的 supports_reasoning 能力标签时调用此方法，
        以动态控制 reasoning_effort 下拉控件及其标签的可见性。
        """
        self._reasoning_label.setVisible(checked)
        self.reasoning_effort_combo.setVisible(checked)
        # 隐藏时重置为"不设置"
        if not checked:
            self.reasoning_effort_combo.setCurrentIndex(0)

    def set_supports_reasoning(self, supports: bool) -> None:
        """外部接口：更新 supports_reasoning 状态并刷新控件可见性。

        Args:
            supports: 模型是否支持推理能力。
        """
        self._on_reasoning_toggled(supports)

    # ------------------------------------------------------------------
    # 折叠区展开/收起
    # ------------------------------------------------------------------

    def _toggle_advanced_section(self) -> None:
        """切换"更多设置"折叠区的展开/收起状态。"""
        self._advanced_expanded = not self._advanced_expanded
        self._advanced_widget.setVisible(self._advanced_expanded)

        # 更新箭头图标方向
        if self._advanced_expanded:
            self._advanced_toggle_btn.setIcon(FluentIcon.CHEVRON_DOWN)
        else:
            self._advanced_toggle_btn.setIcon(FluentIcon.CHEVRON_RIGHT)

    # ------------------------------------------------------------------
    # 数据加载
    # ------------------------------------------------------------------

    def _load_values(self) -> None:
        """从 ModelEntry 加载已保存的值到控件。"""
        entry = self._model_entry

        # 基础表单
        self.display_name_edit.setText(entry.display_name)
        self.max_tokens_spinbox.setValue(entry.max_tokens)

        # 高级参数 - temperature
        temp_value = entry.temperature if entry.temperature is not None else 0.7
        self._syncing = True
        self.temp_slider.setValue(round(temp_value * 10))
        self.temp_spinbox.setValue(temp_value)
        self._syncing = False

        # 高级参数 - top_p
        top_p_value = entry.top_p if entry.top_p is not None else 1.0
        self._syncing = True
        self.top_p_slider.setValue(round(top_p_value / 0.05))
        self.top_p_spinbox.setValue(top_p_value)
        self._syncing = False

        # 高级参数 - reasoning_effort
        effort_map = {"low": 1, "medium": 2, "high": 3}
        idx = effort_map.get(entry.reasoning_effort, 0) if entry.reasoning_effort else 0
        self.reasoning_effort_combo.setCurrentIndex(idx)

    # ------------------------------------------------------------------
    # Temperature 双向同步
    # ------------------------------------------------------------------

    def _on_temp_slider_changed(self, value: int) -> None:
        """Temperature 滑块变更 → 更新 spinbox。"""
        if self._syncing:
            return
        self._syncing = True
        actual = round(value * 0.1, 1)
        self.temp_spinbox.setValue(actual)
        self._syncing = False

    def _on_temp_spinbox_changed(self, value: float) -> None:
        """Temperature spinbox 变更 → 更新滑块。"""
        if self._syncing:
            return
        self._syncing = True
        slider_value = round(value * 10)
        self.temp_slider.setValue(slider_value)
        self._syncing = False

    # ------------------------------------------------------------------
    # Top P 双向同步
    # ------------------------------------------------------------------

    def _on_top_p_slider_changed(self, value: int) -> None:
        """Top P 滑块变更 → 更新 spinbox。"""
        if self._syncing:
            return
        self._syncing = True
        actual = round(value * 0.05, 2)
        self.top_p_spinbox.setValue(actual)
        self._syncing = False

    def _on_top_p_spinbox_changed(self, value: float) -> None:
        """Top P spinbox 变更 → 更新滑块。"""
        if self._syncing:
            return
        self._syncing = True
        slider_value = round(value / 0.05)
        self.top_p_slider.setValue(slider_value)
        self._syncing = False

    # ------------------------------------------------------------------
    # 公共接口
    # ------------------------------------------------------------------

    def get_updated_entry(self) -> ModelEntry:
        """收集控件值，返回更新后的 ModelEntry。

        Returns:
            包含用户修改后参数的 ModelEntry 实例。
        """
        # 收集 reasoning_effort
        effort_index = self.reasoning_effort_combo.currentIndex()
        effort_map = {0: None, 1: "low", 2: "medium", 3: "high"}
        reasoning_effort = effort_map[effort_index]

        # 收集 temperature / top_p（如果与默认值相同则存为 None 以保持向后兼容）
        temperature = round(self.temp_spinbox.value(), 1)
        top_p = round(self.top_p_spinbox.value(), 2)

        return self._model_entry.model_copy(update={
            "display_name": self.display_name_edit.text().strip(),
            "max_tokens": self.max_tokens_spinbox.value(),
            "temperature": temperature,
            "top_p": top_p,
            "reasoning_effort": reasoning_effort,
        })

    # ------------------------------------------------------------------
    # 保存与持久化
    # ------------------------------------------------------------------

    def _on_save_clicked(self) -> None:
        """保存按钮点击处理：收集数据、发射信号、触发持久化、关闭对话框。"""
        updated_entry = self.save()
        self.saved.emit(updated_entry)
        self.accept()

    def save(self) -> ModelEntry:
        """收集所有控件值，更新 ModelEntry 字段，并触发 ConfigPersistence.save() 持久化。

        Returns:
            更新后的 ModelEntry 实例。
        """
        updated_entry = self.get_updated_entry()

        # 触发配置持久化
        self._persist_config()

        return updated_entry

    def _persist_config(self) -> None:
        """触发 ConfigPersistence.save() 将当前配置写入磁盘。

        使用 creart 获取 PathFunc 和 ProviderRegistry 单例，
        将完整的 providers 列表和活跃状态持久化到 agent_config.json。
        如果持久化失败，记录 error 日志但不阻塞 UI。
        """
        try:
            from creart import it

            from src.core.agent.config_persistence import ConfigPersistence
            from src.core.agent.provider import ProviderRegistry
            from src.core.runtime.paths import PathFunc

            path_func: PathFunc = it(PathFunc)
            config_file_path = path_func.config_dir_path / "agent_config.json"
            persistence = ConfigPersistence(config_file_path)

            # 加载现有配置并同步 providers 列表
            config_data = persistence.load()
            registry: ProviderRegistry = it(ProviderRegistry)
            config_data.providers = registry.list_all()

            persistence.save(config_data)
        except Exception:
            logger.error("EditModelDialog 持久化配置失败")

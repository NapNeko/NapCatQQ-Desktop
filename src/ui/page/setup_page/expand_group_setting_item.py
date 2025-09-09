# -*- coding: utf-8 -*-
"""
设置项组件模块

提供各种类型的设置项组件，用于配置界面。
包括文本框、开关、下拉框、范围滑块和文件选择器等。
"""

# 第三方库导入
from qfluentwidgets import (
    BodyLabel,
    ComboBox,
    ConfigItem,
    FluentIcon,
    LineEdit,
    PushButton,
    RangeConfigItem,
    Slider,
    SwitchButton,
)
from PySide6.QtCore import QDir, Qt, Signal, Slot
from PySide6.QtWidgets import QFileDialog, QHBoxLayout, QWidget

# 项目内模块导入
from src.core.config import cfg


class ItemBase(QWidget):
    """设置项组件的基类"""

    def __init__(self, title: str, parent: QWidget | None = None) -> None:
        """
        初始化设置项基类

        Args:
            title: 设置项标题文本
            parent: 父组件
        """
        super().__init__(parent=parent)

        self.title_label = BodyLabel(title, self)
        self.h_box_layout = QHBoxLayout(self)

        # 布局配置
        self.setFixedHeight(64)
        self.h_box_layout.addWidget(self.title_label, 0, Qt.AlignmentFlag.AlignLeft)
        self.h_box_layout.addStretch(1)
        self.h_box_layout.setContentsMargins(32, 12, 32, 12)

    def set_title(self, title: str) -> None:
        """
        设置标题文本

        Args:
            title: 新的标题文本
        """
        self.title_label.setText(title)

    def set_value(self, value) -> None:
        """
        设置组件值（需要在子类中实现）

        Args:
            value: 要设置的值
        """
        pass


class LineEditItem(ItemBase):
    """文本输入框设置项"""

    def __init__(
        self, config_item: ConfigItem, title: str, placeholder_text: str = "", parent: QWidget | None = None
    ) -> None:
        """
        初始化文本输入框设置项

        Args:
            config_item: 配置项对象
            title: 设置项标题
            placeholder_text: 输入框占位文本
            parent: 父组件
        """
        super().__init__(title, parent=parent)

        self.config_item = config_item
        self.line_edit = LineEdit(self)
        self.line_edit.setPlaceholderText(placeholder_text)

        # 初始化值
        if config_item:
            self.set_value(cfg.get(config_item))

        # 控件配置
        self.line_edit.editingFinished.connect(self._on_editing_finished)
        self.line_edit.setFixedWidth(175)

        # 布局添加
        self.h_box_layout.addWidget(self.line_edit, 0, Qt.AlignmentFlag.AlignRight)
        self.h_box_layout.addSpacing(16)

    def _on_editing_finished(self) -> None:
        """编辑完成时的槽函数"""
        self.set_value(self.line_edit.text())

    @Slot(str)
    def set_value(self, value: str) -> None:
        """
        设置输入框的值并更新配置

        Args:
            value: 要设置的文本值
        """
        if self.config_item:
            cfg.set(self.config_item, value)

        self.line_edit.setText(value)


class SwitchItem(ItemBase):
    """开关按钮设置项"""

    checked_changed_signal = Signal(bool)

    def __init__(self, config_item: ConfigItem, title: str, parent: QWidget | None = None) -> None:
        """
        初始化开关按钮设置项

        Args:
            config_item: 配置项对象
            title: 设置项标题
            parent: 父组件
        """
        super().__init__(title, parent=parent)

        self.config_item = config_item
        self.switch_button = SwitchButton(self)

        # 初始化状态
        if config_item:
            self.set_value(cfg.get(config_item))
            config_item.valueChanged.connect(self.set_value)

        # 布局添加
        self.h_box_layout.addWidget(self.switch_button, 0, Qt.AlignmentFlag.AlignRight)
        self.h_box_layout.addSpacing(2)

        # 信号连接
        self.switch_button.checkedChanged.connect(self._on_checked_changed)

    def _on_checked_changed(self, value: bool) -> None:
        """
        开关状态变化处理

        Args:
            value: 新的开关状态
        """
        self.set_value(value)
        self.checked_changed_signal.emit(value)

    @Slot(bool)
    def set_value(self, value: bool) -> None:
        """
        设置开关状态并更新配置

        Args:
            value: 要设置的开关状态
        """
        if self.config_item:
            cfg.set(self.config_item, value)

        self.switch_button.setChecked(value)

    def set_checked(self, value: bool) -> None:
        """
        设置选中状态

        Args:
            value: 要设置的选中状态
        """
        self.set_value(value)

    def is_checked(self) -> bool:
        """
        获取当前选中状态

        Returns:
            当前的开关状态
        """
        return self.switch_button.isChecked()


class ComboBoxItem(ItemBase):
    """下拉选择框设置项"""

    def __init__(
        self, config_item: ConfigItem, title: str, texts: list[str] | None = None, parent: QWidget | None = None
    ) -> None:
        """
        初始化下拉选择框设置项

        Args:
            config_item: 配置项对象
            title: 设置项标题
            texts: 选项显示文本列表
            parent: 父组件
        """
        super().__init__(title, parent=parent)

        self.config_item = config_item
        self.combo_box = ComboBox(self)

        # 布局添加
        self.h_box_layout.addWidget(self.combo_box, 0, Qt.AlignmentFlag.AlignRight)
        self.h_box_layout.addSpacing(16)

        # 选项映射
        self.option_to_text = {option: text for option, text in zip(config_item.options, texts)}
        for text, option in zip(texts, config_item.options):
            self.combo_box.addItem(text, userData=option)

        # 控件配置
        self.combo_box.setCurrentText(self.option_to_text[cfg.get(config_item)])
        self.combo_box.currentIndexChanged.connect(self._on_current_index_changed)
        self.combo_box.setFixedWidth(135)

    @Slot(int)
    def _on_current_index_changed(self, index: int) -> None:
        """
        当前选项变化处理

        Args:
            index: 新的选项索引
        """
        cfg.set(self.config_item, self.combo_box.itemData(index))

    def set_value(self, value) -> None:
        """
        设置选中值并更新配置

        Args:
            value: 要设置的选项值
        """
        if value not in self.option_to_text:
            return

        self.combo_box.setCurrentText(self.option_to_text[value])
        cfg.set(self.config_item, value)


class RangeItem(ItemBase):
    """范围滑块设置项"""

    value_changed_signal = Signal(int)

    def __init__(self, config_item: RangeConfigItem, title: str, parent: QWidget | None = None) -> None:
        """
        初始化范围滑块设置项

        Args:
            config_item: 范围配置项对象
            title: 设置项标题
            parent: 父组件
        """
        super().__init__(title, parent=parent)

        self.config_item = config_item
        self.slider = Slider(Qt.Orientation.Horizontal, self)
        self.value_label = BodyLabel(self)

        # 控件配置
        self.value_label.setObjectName("valueLabel")
        self.slider.setMinimumWidth(260)
        self.slider.setRange(*config_item.range)
        self.slider.setValue(config_item.value)
        self.slider.setFixedWidth(200)
        self.value_label.setNum(config_item.value)

        # 布局配置
        self.h_box_layout.addStretch(1)
        self.h_box_layout.addWidget(self.value_label, 0, Qt.AlignmentFlag.AlignRight)
        self.h_box_layout.addSpacing(6)
        self.h_box_layout.addWidget(self.slider, 0, Qt.AlignmentFlag.AlignRight)
        self.h_box_layout.addSpacing(16)

        # 信号连接
        config_item.valueChanged.connect(self.set_value)
        self.slider.valueChanged.connect(self._on_value_changed)

    @Slot(int)
    def _on_value_changed(self, value: int) -> None:
        """
        滑块值变化处理

        Args:
            value: 新的滑块值
        """
        self.set_value(value)
        self.value_changed_signal.emit(value)

    def set_value(self, value: int) -> None:
        """
        设置滑块值并更新配置

        Args:
            value: 要设置的数值
        """
        cfg.set(self.config_item, value)
        self.value_label.setNum(value)
        self.value_label.adjustSize()
        self.slider.setValue(value)


class FileItem(ItemBase):
    """文件选择器设置项"""

    file_changed_signal = Signal(str)  # 文件路径信号

    def __init__(self, config_item: ConfigItem, title: str, parent: QWidget | None = None) -> None:
        """
        初始化文件选择器设置项

        Args:
            config_item: 配置项对象
            title: 设置项标题
            parent: 父组件
        """
        super().__init__(title, parent=parent)

        self.config_item = config_item
        self.title_text = title
        self.button = PushButton(self.tr("选择文件"), self, FluentIcon.FOLDER_ADD)
        self.button.clicked.connect(self._on_button_clicked)

        # 控件配置
        self.button.setFixedWidth(135)
        if cfg.get(self.config_item):
            self.title_label.setText(f"{self.title_text}: {cfg.get(self.config_item)}")

        # 布局添加
        self.h_box_layout.addWidget(self.button, 0, Qt.AlignmentFlag.AlignRight)
        self.h_box_layout.addSpacing(16)

    @Slot()
    def _on_button_clicked(self) -> None:
        """按钮点击打开文件对话框"""
        file_path, _ = QFileDialog.getOpenFileName(
            self, self.tr("选择文件"), QDir.homePath(), "Image Files (*.png *.jpg *.jpeg)"
        )

        if file_path:
            cfg.set(self.config_item, file_path)
            self.title_label.setText(f"{self.title_text}: {file_path}")
            self.file_changed_signal.emit(file_path)

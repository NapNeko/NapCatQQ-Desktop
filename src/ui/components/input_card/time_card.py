# -*- coding: utf-8 -*-

"""
UI 组件：时间选择卡片
"""


# 标准库导入
from ast import Dict

# 第三方库导入
from qfluentwidgets import ComboBox, CompactSpinBox, FluentIconBase, LineEdit, SettingCard
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config.config_enum import TimeUnitEnum


class IntervalTimeConfigCard(SettingCard):
    """间隔时间选择卡片"""

    def __init__(
        self, icon: FluentIconBase, title: str, content: str | None = None, parent: QWidget | None = None
    ) -> None:
        """初始化

        Args:
            icon (FluentIconBase): 图标
            title (str): 标题
            content (str | None, optional): 呢容. Defaults to None.
            parent (QWidget | None, optional): 父控件. Defaults to None.
        """
        super().__init__(icon, title, content, parent)
        # 创建组件
        self.time_unit_combo_box = ComboBox(self)
        self.duration_spin_box = CompactSpinBox(self)

        # 设置组件
        self.time_unit_combo_box.addItem(self.tr("分钟"), userData=TimeUnitEnum.MINUTE)
        self.time_unit_combo_box.addItem(self.tr("小时"), userData=TimeUnitEnum.HOUR)
        self.time_unit_combo_box.addItem(self.tr("天"), userData=TimeUnitEnum.DAY)
        self.time_unit_combo_box.addItem(self.tr("月"), userData=TimeUnitEnum.MONTH)
        self.time_unit_combo_box.addItem(self.tr("年"), userData=TimeUnitEnum.YEAR)
        self.time_unit_combo_box.setCurrentIndex(1)

        self.duration_spin_box.setRange(1, 999)
        self.duration_spin_box.setValue(6)

        # 添加组件到布局
        self.hBoxLayout.addWidget(self.duration_spin_box, alignment=Qt.AlignmentFlag.AlignLeft)
        self.hBoxLayout.addSpacing(8)
        self.hBoxLayout.addWidget(self.time_unit_combo_box, alignment=Qt.AlignmentFlag.AlignLeft)
        self.hBoxLayout.addSpacing(16)

    def fill_value(self, time_unit: TimeUnitEnum, duration: int) -> None:
        """填充时间值

        Args:
            time_unit (TimeUnitEnum): 时间单位
            duration (int): 时间长度
        """
        self.time_unit_combo_box.setCurrentIndex(self.time_unit_combo_box.findData(time_unit))
        self.duration_spin_box.setValue(duration)

    def get_value(self) -> tuple[TimeUnitEnum, int]:
        """获取时间值

        这里返回的是元组, 第一个元素是时间单位, 第二个元素是时间长度

        Returns:
            tuple: (TimeUnitEnum, int)
        """
        return (self.time_unit_combo_box.currentData(), self.duration_spin_box.value())

    def clear(self) -> None:
        """清空时间值"""
        self.time_unit_combo_box.setCurrentIndex(1)
        self.duration_spin_box.setValue(0)

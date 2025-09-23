# -*- coding: utf-8 -*-

"""
UI 组件：时间选择卡片
"""


# 第三方库导入
from qfluentwidgets import FluentIconBase, SettingCard
from PySide6.QtCore import Qt, QTime
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.ui.components.time_picker import IntervalTimePicker


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
        self.interval_time_picker = IntervalTimePicker(self)

        self.hBoxLayout.addWidget(self.interval_time_picker, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def fill_value(self, value: str) -> None:
        """填充时间值

        Args:
            value (QTime): 时间值
        """
        self.interval_time_picker.setTime(value)

    def get_value(self) -> QTime:
        """获取时间值

        这里返回的是 QTime 对象, 但实际上表示的是一个时间间隔

        Returns:
            QTime: 时间值
        """
        return self.interval_time_picker.getTime().toPython

    def clear(self) -> None:
        pass

# -*- coding: utf-8 -*-
# 第三方库导入
from httpx import stream
from qfluentwidgets import PickerBase
from qfluentwidgets.components.date_time.picker_base import DigitFormatter
from qfluentwidgets.components.date_time.time_picker import MiniuteFormatter
from PySide6.QtCore import Property, QDate, QTime
from PySide6.QtWidgets import QWidget


class IntervalTimePicker(PickerBase):
    """间隔时间选择器"""

    def __init__(self, parent: QWidget) -> None:
        super().__init__(parent)
        self._day = self.tr("天")

        # 添加列, 包括年 月 日 时 分 秒
        self.addColumn(self.tr("数值"), range(0, 32), 60, formatter=DigitFormatter())

    def setTime(self, time) -> None:
        """设置当前时间

        Args:
            time (QTime): 当前时间
        """
        if not time.isValid() or time.isNull():
            return

        self._time = time

        time = time.toPython()

        self.setColumnValue(0, time.y)

    def _onConfirmed(self, value: list) -> None:
        """处理确认事件

        解析选择的值, 并更新当前时间

        Args:
            value (list): 选择的值列表
        """
        super()._onConfirmed(value)

        d = self.decodeValue(0, value[0])
        h = self.decodeValue(1, value[1])
        m = self.decodeValue(2, value[2])
        s = self.decodeValue(3, value[3])

        time = QDateTime(d * 24 + h, m, s)

        self.setTime(time)

    def panelInitialValue(self) -> list:
        """获取面板的初始值

        如果当前有值, 则返回当前值, 否则返回当前时间的编码值

        Returns:
            list: 初始值列表
        """
        if any(self.value()):
            return self.value()

        time = QTime.currentTime()

        d = int(self.encodeValue(0, 0))
        h = int(self.encodeValue(1, time.hour()))
        m = int(self.encodeValue(2, time.minute()))
        s = int(self.encodeValue(3, time.second()))

        # 检查小时是否大于等于24
        if h >= 24:
            # 如果小时数大于等于24, 则转换为天和小时
            days = h // 24
            hours = h % 24
            d = int(self.encodeValue(0, days))
            h = int(self.encodeValue(1, hours))

        return [d, h, m, s]

    def getTime(self) -> QTime:
        """获取当前时间

        Returns:
            QTime: 当前时间
        """
        return self._time

    time = Property(QTime, getTime, setTime)

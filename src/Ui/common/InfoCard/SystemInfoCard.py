# -*- coding: utf-8 -*-
import sys
from typing import Optional

import psutil
from PySide6.QtCore import Qt, QRectF, QPoint, QTimer
from PySide6.QtGui import QPainter, QColor, QPen
from PySide6.QtWidgets import QApplication, QWidget, QVBoxLayout
from qfluentwidgets import BodyLabel, setFont, SimpleCardWidget, InfoBadgeManager, IconInfoBadge, FluentIcon


class DashboardBase(QWidget):
    """
    ## 仪表盘进度条
    """

    def __init__(self, info: str):
        super().__init__()

        # 创建要显示的标签和布局
        self.timer: Optional[QTimer] = None
        self.view = SimpleCardWidget(self)
        self.progressBar = SemiCircularProgressBar(self.view)
        self.warningBadge = IconInfoBadge.warning(FluentIcon.UP, self, self.view, "SystemInfo")
        self.vBoxLayout = QVBoxLayout()

        # 设置 标签 以及 SimpleCardWidget 的一些属性
        self.view.setFixedSize(190, 190)
        self.progressBar.setFixedSize(155, 155)
        self.progressBar.setInfo(info)
        self.setFixedSize(self.view.width() + 10, self.view.height() + 10)
        self.view.move(0, self.height() - self.view.height())
        self.warningBadge.hide()

        # 调用方法
        self._setLayout()

    def setValue(self, value: int | float):
        self.progressBar.setValue(value)

    def onMonitor(self):
        """
        ## 打开监控
        """
        # 创建 QTimer 对象
        self.timer = QTimer()
        # 将计时器超时信号连接到槽函数
        self.timer.timeout.connect(self._monitor)
        # 设置计时器每隔 1000 毫秒（即 1 秒）超时一次
        self.timer.start(1000)

    def _monitor(self):
        """
        ## 监控实现逻辑, 请继承实现
        """
        pass

    def _setLayout(self) -> None:
        """
        ## 将控件添加到布局
        """
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.setContentsMargins(10, 15, 10, 10)
        self.vBoxLayout.addWidget(self.progressBar, 0, Qt.AlignmentFlag.AlignCenter)
        self.view.setLayout(self.vBoxLayout)


class SemiCircularProgressBar(QWidget):

    def __init__(self, parent=None):
        super().__init__(parent)
        self._value: int | float = 0
        self._max_value = 100
        self._bar_color = QColor("#009FAA")
        self._background_color = QColor("#D0D2D4")
        self._pen_width = 12

        # 创建QLabel来显示进度数字和类型文本
        self.value_label = BodyLabel(self)
        self.info_label = BodyLabel(self)
        # 设置 Label 样式
        setFont(self.value_label, 26)
        setFont(self.info_label, 18)
        self.value_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.info_label.setAlignment(Qt.AlignmentFlag.AlignCenter)

        # 创建布局并添加标签
        layout = QVBoxLayout(self)
        layout.addWidget(self.value_label)
        layout.addWidget(self.info_label)
        layout.setContentsMargins(0, 0, 0, 0)
        self.setLayout(layout)

    def setInfo(self, info: str):
        self.info_label.setText(info)

    def setValue(self, value: int | float):
        self._value = value
        self.value_label.setText(f"{value} %")
        self.update()

    def paintEvent(self, event):
        """
        ## 绘制进度条
        """
        # 获取当前窗口的宽度和高度, 边缘距离, 绘制区域，减去边缘距离
        width, height, margin = self.width(), self.height(), 10
        rect = QRectF(margin, margin, width - 2 * margin, height - 2 * margin)

        # 起始角度和跨度角度，单位为1/16度
        start_angle = 235 * 16  # 起始角度为235度
        span_angle = -290 * 16  # 逆时针跨度角为290度

        # 创建QPainter对象，用于绘图
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # 绘制背景弧线
        # 设置画笔，颜色为背景色，宽度为_pen_width，实线，圆形线帽实现圆角
        pen = QPen(
            self._background_color, self._pen_width,
            Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap
        )
        painter.setPen(pen)
        # 在rect区域内，从start_angle开始，绘制跨度为span_angle的弧线
        painter.drawArc(rect, start_angle, span_angle)
        # 计算当前值对应的弧线跨度角度
        progress_span_angle = int(span_angle * (self._value / self._max_value))

        # 绘制进度弧线
        pen.setColor(self._bar_color)  # 设置画笔颜色为进度条颜色
        painter.setPen(pen)  # 将画笔设置为当前画笔
        # 在rect区域内，从start_angle开始，绘制跨度为progress_span_angle的弧线
        painter.drawArc(rect, start_angle, progress_span_angle)

        self.value_label.setGeometry(0, height // 2 - 15, self.width(), 30)
        self.info_label.setGeometry(0, height - 30, self.height(), 30)


class CPUDashboard(DashboardBase):
    """
    ## 实现 CPU 占用仪表盘
    """

    def __init__(self):
        """
        ## 初始化
        """
        super().__init__("CPU")
        self.onMonitor()

    def _monitor(self):
        """
        ## 实现监控 CPU 信息
        """
        self.setValue(psutil.cpu_percent(interval=0))


class MemoryDashboard(DashboardBase):
    """
    ## 实现 Memory 占用仪表盘
    """

    def __init__(self):
        """
        ## 初始化
        """
        super().__init__("Memory")
        self.onMonitor()

    def _monitor(self):
        """
        ## 实现监控 Memory 信息
        """
        self.setValue(psutil.virtual_memory().percent)

    def paintEvent(self, event):
        """
        ## 调整 infoLabel 大小
        """
        super().paintEvent(event)
        setFont(self.progressBar.info_label, 13)


@InfoBadgeManager.register('SystemInfo')
class NewVersionInfoBadgeManager(InfoBadgeManager):
    """
    ## 更新图标显示位置调整
    """

    def position(self) -> QPoint:
        pos = self.target.geometry().topRight()
        x = pos.x() - self.badge.width() // 2 - 5
        y = pos.y() - self.badge.height() // 2 + 5
        return QPoint(x, y)


if __name__ == "__main__":
    app = QApplication(sys.argv)
    progressBar = SemiCircularProgressBar()
    progressBar.setValue(50)
    progressBar.resize(175, 175)
    progressBar.info_label.setText("CPU")
    progressBar.show()
    sys.exit(app.exec())

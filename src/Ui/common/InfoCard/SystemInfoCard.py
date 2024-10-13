# -*- coding: utf-8 -*-
# 标准库导入
import time
from typing import Optional

# 第三方库导入
import psutil
from anyio import value
from qfluentwidgets import (
    BodyLabel,
    FluentIcon,
    IconInfoBadge,
    ToolTipFilter,
    HeaderCardWidget,
    InfoBadgeManager,
    SimpleCardWidget,
    setFont,
    themeColor,
    isDarkTheme,
)
from PySide6.QtGui import QPen, QFont, QColor, QPainter
from PySide6.QtCore import Qt, QPoint, QRectF, QTimer
from PySide6.QtWidgets import QWidget, QFormLayout, QVBoxLayout

# 项目内模块导入
from src.Core import timer
from src.Core.Config import cfg


class DashboardBase(QWidget):
    """
    ## 仪表盘进度条
    """

    def __init__(self, info: str, parent=None) -> None:
        super().__init__(parent=parent)

        # 创建要显示的标签和布局
        self.timer: Optional[QTimer] = None
        self.view = SimpleCardWidget(self)
        self.progressBar = SemiCircularProgressBar(self.view)
        self.warningBadge = IconInfoBadge.warning(FluentIcon.UP, self, self.view, "SystemInfo")
        self.vBoxLayout = QVBoxLayout()

        # 设置 标签 以及 SimpleCardWidget 的一些属性
        self.view.setFixedSize(250, 250)
        self.progressBar.setFixedSize(205, 205)
        self.progressBar.setInfo(info)
        self.setFixedSize(self.view.width() + 10, self.view.height() + 10)
        self.view.move(0, self.height() - self.view.height())
        self.installEventFilter(ToolTipFilter(self))
        self.warningBadge.hide()

        # 调用方法
        self._setLayout()

    def setValue(self, value: int | float) -> None:
        self.progressBar.setValue(value)

    def _setLayout(self) -> None:
        """
        ## 将控件添加到布局
        """
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.setContentsMargins(12, 15, 10, 10)
        self.vBoxLayout.addWidget(self.progressBar, 0, Qt.AlignmentFlag.AlignCenter)
        self.view.setLayout(self.vBoxLayout)


class SemiCircularProgressBar(QWidget):

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self._value: int | float = 0
        self._max_value = 100
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

    def setInfo(self, info: str) -> None:
        self.info_label.setText(info)

    def setValue(self, value: int | float) -> None:
        self._value = value
        self.value_label.setText(f"{value} %")
        self.update()

    def paintEvent(self, event) -> None:
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
            QColor(255, 255, 255, 13) if isDarkTheme() else QColor(208, 210, 212, 170),
            self._pen_width,
            Qt.PenStyle.SolidLine,
            Qt.PenCapStyle.RoundCap,
        )
        painter.setPen(pen)
        # 在rect区域内，从start_angle开始，绘制跨度为span_angle的弧线
        painter.drawArc(rect, start_angle, span_angle)
        # 计算当前值对应的弧线跨度角度
        progress_span_angle = int(span_angle * (self._value / self._max_value))

        # 绘制进度弧线
        pen.setColor(themeColor())  # 设置画笔颜色为进度条颜色
        painter.setPen(pen)  # 将画笔设置为当前画笔
        # 在rect区域内，从start_angle开始，绘制跨度为progress_span_angle的弧线
        painter.drawArc(rect, start_angle, progress_span_angle)

        self.value_label.setGeometry(0, height // 2 - 15, self.width(), 30)
        self.info_label.setGeometry(0, height - 30, self.height(), 30)


class CPUDashboard(DashboardBase):
    """
    ## 实现 CPU 占用仪表盘
    """

    def __init__(self, parent=None) -> None:
        """
        ## 初始化
        """
        super().__init__("CPU", parent)
        self.monitor()

    @timer(1000)
    def monitor(self) -> None:
        """
        ## 实现监控 CPU 信息
        """
        self.setValue(psutil.cpu_percent(interval=0))
        # 获取每个 CPU 核心的使用率
        cpu_usages = psutil.cpu_percent(interval=0, percpu=True)
        # 获取总 CPU 数
        total_cpus = len(cpu_usages)
        max_rows = (total_cpus + 8 - 1) // 8
        lines = []
        for i in range(max_rows):
            line = []
            for j in range(i, total_cpus, max_rows):
                core_num = j + 1
                usage = cpu_usages[j]
                if usage < 10:
                    line.append(f"CPU {core_num:03d} 使用率:  {usage:5.0f}%")
                elif usage < 100:
                    line.append(f"CPU {core_num:03d} 使用率: {usage:5.0f}%")
                else:
                    line.append(f"CPU {core_num:03d} 使用率:{usage:5.0f}%")
            lines.append(str(" " * 10).join(line))
        self.setToolTip(self.tr("CPU 占用率:\n\n{}".format("\n".join(lines))))


class MemoryDashboard(DashboardBase):
    """
    ## 实现 Memory 占用仪表盘
    """

    def __init__(self, parent=None) -> None:
        """
        ## 初始化
        """
        super().__init__("Memory", parent)
        self.monitor()

    @timer(3000)
    def monitor(self) -> None:
        """
        ## 实现监控 Memory 信息
        """
        self.setValue(psutil.virtual_memory().percent)
        # 获取系统内存信息
        virtual_mem = psutil.virtual_memory()
        total_mem = virtual_mem.total / (1024**3)
        used_mem = virtual_mem.used / (1024**3)

        # 构建输出字符串
        tool_tip_string = self.tr(
            f"内存大小: {used_mem:.0f}G/{total_mem:.0f}G\n"
            f"内存使用情况: \n"
            f"{' ' * 8}NapCat Desktop: {psutil.Process().memory_info().rss / (1024 ** 2):.2f} MB"
        )

        self.setToolTip(tool_tip_string)


@InfoBadgeManager.register("SystemInfo")
class DashboardInfoBadgeManager(InfoBadgeManager):
    """
    ## 更新图标显示位置调整
    """

    def position(self) -> QPoint:
        pos = self.target.geometry().topRight()
        x = pos.x() - self.badge.width() // 2 - 5
        y = pos.y() - self.badge.height() // 2 + 5
        return QPoint(x, y)

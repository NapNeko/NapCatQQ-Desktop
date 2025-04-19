# -*- coding: utf-8 -*-

# 第三方库导入
from qfluentwidgets import BodyLabel
from qfluentwidgets import FluentIcon as FIcon
from qfluentwidgets import SegmentedWidget
from qfluentwidgets.common import setFont
from qfluentwidgets.window.stacked_widget import StackedWidget
from PySide6.QtGui import QFont
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout, QStackedWidget

# 项目内模块导入
from src.ui.unit_page.general import General
from src.ui.unit_page.famework import Framework


class UnitPage(QWidget):
    """组件页面"""

    def __init__(self, parent=None) -> None:
        """构造函数"""
        super().__init__(parent)

        # 调用方法
        self.createComponent()
        self.setComponent()
        self.setPiovt()
        self.setupLayout()

        # 连接信号与槽
        self._connectSignalToSlot()

    def createComponent(self) -> None:
        """创建组件"""
        self.vBoxLayout = QVBoxLayout()
        self.pivotBoxLayout = QHBoxLayout()
        self.titleLabel = BodyLabel(self.tr("组件"))
        self.pivot = SegmentedWidget()

        self.view = StackedWidget(self)
        self.general = General()
        self.framework = QWidget()

    def setComponent(self) -> None:
        """设置组件"""
        # 设置属性
        self.setObjectName("UnitPage")
        setFont(self.titleLabel, 32, QFont.Weight.DemiBold)

        self.view.addWidget(self.general)
        self.view.addWidget(self.framework)
        self.view.setCurrentIndex(0)

    def setPiovt(self) -> None:
        """设置分段控件"""
        self.pivot.addItem(
            routeKey=self.view.widget(0).objectName(),
            text=self.tr("通用"),
            icon=FIcon.EMOJI_TAB_SYMBOLS,
            onClick=lambda: self.view.setCurrentWidget(self.view.widget(0)),
        )
        self.pivot.addItem(
            routeKey=self.view.widget(1).objectName(),
            text=self.tr("框架"),
            icon=FIcon.EXPRESSIVE_INPUT_ENTRY,
            onClick=lambda: self.view.setCurrentWidget(self.view.widget(1)),
        )
        self.pivot.setCurrentItem(self.view.widget(0).objectName())

    def setupLayout(self) -> None:
        """设置布局"""
        # 分段控件布局
        self.pivotBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.pivotBoxLayout.addWidget(self.pivot, 1)
        self.pivotBoxLayout.addStretch(2)

        # 整体布局
        self.vBoxLayout.setContentsMargins(36, 24, 36, 16)
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.addWidget(self.titleLabel, 0, Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignLeft)
        self.vBoxLayout.addSpacing(16)
        self.vBoxLayout.addLayout(self.pivotBoxLayout, 0)
        self.vBoxLayout.addSpacing(12)
        self.vBoxLayout.addWidget(self.view, 1)
        self.setLayout(self.vBoxLayout)

    def _connectSignalToSlot(self):
        """连接信号与槽"""
        ...


__all__ = ["UnitPage"]

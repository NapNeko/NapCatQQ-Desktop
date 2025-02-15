# -*- coding: utf-8 -*-

# 第三方库导入
from qfluentwidgets import BodyLabel
from qfluentwidgets import FluentIcon as FIcon
from qfluentwidgets import ToolButton, ToolTipFilter, SegmentedWidget
from qfluentwidgets import ToolTipPosition as TTPosition
from qfluentwidgets.common import setFont
from PySide6.QtGui import QFont
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout, QStackedWidget

# 项目内模块导入
from src.ui.settings_page.separator import Separator


class SettingsPage(QWidget):
    """设置页面"""

    def __init__(self, parent=None) -> None:
        """构造函数"""
        super().__init__(parent)

        # 调用方法
        self.createComponent()
        self.setComponent()
        self.setPiovt()
        self.setupLayout()

    def createComponent(self) -> None:
        """创建组件"""
        self.vBoxLayout = QVBoxLayout()
        self.pivotBoxLayout = QHBoxLayout()
        self.titleLabel = BodyLabel(self.tr("设置"))
        self.pivot = SegmentedWidget()
        self.separator = Separator(self)
        self.clearConfigButton = ToolButton(FIcon.DELETE)
        self.exportConfigButton = ToolButton(FIcon.UP)
        self.importConfigButton = ToolButton(FIcon.DOWN)
        self.view = QStackedWidget()

    def setComponent(self) -> None:
        """设置组件"""
        self.setObjectName("SettingsPage")
        setFont(self.titleLabel, 36, QFont.Weight.DemiBold)

        # 设置提示
        self.clearConfigButton.setToolTip(self.tr("清除所有配置(不可逆喔😣)"))
        self.clearConfigButton.installEventFilter(ToolTipFilter(self.clearConfigButton, 300, TTPosition.TOP))
        self.exportConfigButton.setToolTip(self.tr("🏀 导出配置 (JSON)"))
        self.exportConfigButton.installEventFilter(ToolTipFilter(self.exportConfigButton, 300, TTPosition.TOP))
        self.importConfigButton.setToolTip(self.tr("🐔 导入配置 (JSON)"))
        self.importConfigButton.installEventFilter(ToolTipFilter(self.importConfigButton, 300, TTPosition.TOP))

    def setPiovt(self) -> None:
        """设置分段控件"""

        self.pivot.addItem(
            routeKey="settings_pivot_personalized",
            text=self.tr("个性化"),
            icon=FIcon.EMOJI_TAB_SYMBOLS,
        )
        self.pivot.addItem(
            routeKey="settings_pivot_other",
            text=self.tr("其他"),
            icon=FIcon.EXPRESSIVE_INPUT_ENTRY,
        )
        self.pivot.addItem(
            routeKey="settings_pivot_log",
            text=self.tr("日志"),
            icon=FIcon.CODE,
        )
        self.pivot.setCurrentItem("settings_pivot_personalized")

    def setupLayout(self) -> None:
        """设置布局"""

        # 分段控件布局
        self.pivotBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.pivotBoxLayout.addWidget(self.pivot, 2)
        self.pivotBoxLayout.addStretch(3)
        self.pivotBoxLayout.addWidget(self.clearConfigButton, 0)
        self.pivotBoxLayout.addSpacing(4)
        self.pivotBoxLayout.addWidget(self.separator, 0)
        self.pivotBoxLayout.addSpacing(4)
        self.pivotBoxLayout.addWidget(self.exportConfigButton, 0)
        self.pivotBoxLayout.addSpacing(4)
        self.pivotBoxLayout.addWidget(self.importConfigButton, 0)

        # 整体布局
        self.vBoxLayout.setContentsMargins(36, 24, 36, 16)
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.addWidget(self.titleLabel, 0, Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignLeft)
        self.vBoxLayout.addSpacing(16)
        self.vBoxLayout.addLayout(self.pivotBoxLayout, 0)
        self.vBoxLayout.addSpacing(12)
        self.vBoxLayout.addWidget(self.view, 1, Qt.AlignmentFlag.AlignHCenter)
        self.setLayout(self.vBoxLayout)

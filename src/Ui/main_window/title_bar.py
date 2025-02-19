# -*- coding: utf-8 -*-


# 第三方库导入
from qfluentwidgets import FluentIcon as FIcon
from qfluentwidgets import SearchLineEdit, FluentStyleSheet, MSFluentTitleBar, TransparentToolButton
from qframelesswindow.titlebar import TitleBarBase
from qfluentwidgets.common.icon import toQIcon
from qfluentwidgets.window.fluent_title_bar import MaxBtn, MinBtn, CloseBtn
from PySide6.QtCore import Qt, QSize
from PySide6.QtWidgets import QLabel, QHBoxLayout

# 项目内模块导入
from src.ui.icon import NCDIcon
from src.core.config import cfg
from src.ui.common.signal_bus import settingsSignalBus


class NCDTitleBar(TitleBarBase):
    """标题栏"""

    def __init__(self, parent=None) -> None:
        """构造函数"""
        super().__init__(parent)

        # 移除旧按钮
        self.removeOldButton()

        # 调用方法创建,设置组件并添加到布局
        self.createComponent()
        self.setCompoennt()
        self.setupLayout()

        # 连接信号与槽
        self._connectSignalToSlot()

        # 引用样式表
        FluentStyleSheet.FLUENT_WINDOW.apply(self)

    def createComponent(self) -> None:
        """创建组件"""
        self.iconLabel = QLabel(self)
        self.backButton = TransparentToolButton(FIcon.LEFT_ARROW, self)
        self.forwardButton = TransparentToolButton(FIcon.RIGHT_ARROW, self)
        self.searchLineEdit = SearchLineEdit(self)
        self.minBtn = MinBtn(self)
        self.maxBtn = MaxBtn(self)
        self.closeBtn = CloseBtn(self)

    def setCompoennt(self) -> None:
        """设置组件"""

        # 设置标题栏
        self.setFixedHeight(48)

        # 设置图标
        self.iconLabel.setFixedSize(24, 24)
        self.iconLabel.setPixmap(toQIcon(NCDIcon.LOGO).pixmap(24, 24))

        # 返回按钮
        self.backButton.setIconSize(QSize(12, 12))
        self.backButton.setVisible(cfg.get(cfg.commandCenter))

        # 前进按钮
        self.forwardButton.setIconSize(QSize(12, 12))
        self.forwardButton.setVisible(cfg.get(cfg.commandCenter))
        self.forwardButton.setEnabled(False)

        # 命令中心
        self.searchLineEdit.setPlaceholderText(f"{' ' * 25} 🔎 NapCatQQ Doucment")
        self.searchLineEdit.setVisible(cfg.get(cfg.commandCenter))
        self.searchLineEdit.setFixedHeight(28)
        self.searchLineEdit.setMinimumWidth(400)

    def setupLayout(self) -> None:
        """设置布局"""
        # 创建布局
        self.hBoxLayout = QHBoxLayout()
        self.buttonLayout = QHBoxLayout()
        self.centerLayout = QHBoxLayout()

        # 设置布局
        self.hBoxLayout.setSpacing(0)
        self.hBoxLayout.setContentsMargins(24, 0, 8, 0)

        self.centerLayout.setSpacing(8)
        self.centerLayout.setContentsMargins(0, 0, 0, 0)

        self.buttonLayout.setSpacing(8)
        self.buttonLayout.setAlignment(Qt.AlignmentFlag.AlignVCenter)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)

        # 添加到布局
        self.centerLayout.addWidget(self.backButton)
        self.centerLayout.addWidget(self.forwardButton)
        self.centerLayout.addWidget(self.searchLineEdit)

        self.buttonLayout.addWidget(self.minBtn)
        self.buttonLayout.addWidget(self.maxBtn)
        self.buttonLayout.addWidget(self.closeBtn)

        self.hBoxLayout.addWidget(self.iconLabel)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addLayout(self.centerLayout)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addLayout(self.buttonLayout)

        # 应用布局
        self.setLayout(self.hBoxLayout)

    def removeOldButton(self) -> None:
        """移除旧按钮"""
        self.minBtn.close()
        self.maxBtn.close()
        self.closeBtn.close()

    def _connectSignalToSlot(self):
        """连接信号与槽"""
        settingsSignalBus.commandCenterSingal.connect(
            lambda value: (
                self.backButton.setVisible(value),
                self.forwardButton.setVisible(value),
                self.searchLineEdit.setVisible(value),
            )
        )
        self.minBtn.clicked.connect(self.window().showMinimized)
        self.maxBtn.clicked.connect(
            lambda: self.window().showNormal() if self.window().isMaximized() else self.window().showMaximized()
        )
        self.closeBtn.clicked.connect(self.window().close)

# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt, Slot
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout
from creart import it
from qfluentwidgets import FluentIcon, TitleLabel, CaptionLabel, ToolTipFilter, TransparentToolButton


class DownloadTopCard(QWidget):
    """
    ## DownloadViewWidget 顶部展示的 InputCard
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)

        # 创建所需控件
        self.titleLabel = TitleLabel(self.tr("Download"), self)
        self.subtitleLabel = CaptionLabel(self.tr("Download the required components"), self)
        self.returnButton = TransparentToolButton(FluentIcon.RETURN, self)  # 返回按钮

        self.hBoxLayout = QHBoxLayout()
        self.labelLayout = QVBoxLayout()
        self.buttonLayout = QHBoxLayout()

        # 连接信号
        self.returnButton.clicked.connect(self.returnButtonSlot)

        # 调用方法
        self._addTooltips()
        self._setLayout()

    @Slot()
    def returnButtonSlot(self):
        """
        ## 返回按钮槽函数
        """
        from src.Ui.HomePage.Home import HomeWidget

        it(HomeWidget).setCurrentWidget(it(HomeWidget).contentView)

    def _addTooltips(self) -> None:
        """
        ## 为按钮添加悬停提示
        """
        # 添加提示
        self.returnButton.setToolTip(self.tr("Tap Back to Home"))
        self.returnButton.installEventFilter(ToolTipFilter(self.returnButton))

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.labelLayout.setSpacing(0)
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.addWidget(self.titleLabel)
        self.labelLayout.addSpacing(5)
        self.labelLayout.addWidget(self.subtitleLabel)

        self.buttonLayout.setSpacing(0)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)
        self.buttonLayout.addWidget(self.returnButton)
        self.buttonLayout.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignBottom)

        self.hBoxLayout.addLayout(self.labelLayout)
        self.hBoxLayout.addLayout(self.buttonLayout)
        self.hBoxLayout.setContentsMargins(0, 0, 0, 0)

        self.setLayout(self.hBoxLayout)

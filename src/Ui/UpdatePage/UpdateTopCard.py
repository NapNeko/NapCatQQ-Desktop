# -*- coding: utf-8 -*-
from PySide6.QtCore import Slot
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from creart import it
from qfluentwidgets import CaptionLabel, TitleLabel


class UpdateTopCard(QWidget):
    """
    ## DownloadViewWidget 顶部展示的 InputCard
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)

        # 创建所需控件
        self.titleLabel = TitleLabel(self.tr("Update"), self)
        self.subtitleLabel = CaptionLabel(self.tr("Update the required components"), self)

        self.hBoxLayout = QHBoxLayout()
        self.labelLayout = QVBoxLayout()
        self.buttonLayout = QHBoxLayout()


        # 调用方法
        self._setLayout()

    @Slot()
    def returnButtonSlot(self):
        """
        ## 返回按钮槽函数
        """
        from src.Ui.HomePage.Home import HomeWidget
        it(HomeWidget).setCurrentWidget(it(HomeWidget).contentView)

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.labelLayout.setSpacing(0)
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.addWidget(self.titleLabel)
        self.labelLayout.addSpacing(5)
        self.labelLayout.addWidget(self.subtitleLabel)

        self.hBoxLayout.addLayout(self.labelLayout)
        self.hBoxLayout.setContentsMargins(0, 0, 0, 0)

        self.setLayout(self.hBoxLayout)

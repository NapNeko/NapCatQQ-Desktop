# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import CaptionLabel, TitleLabel, SegmentedWidget, PrimaryPushButton, FluentIcon


class UpdateTopCard(QWidget):
    """
    ## DownloadViewWidget 顶部展示的 InputCard
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)

        # 创建所需控件
        self.titleLabel = TitleLabel(self.tr("更新组件"), self)
        self.subtitleLabel = CaptionLabel(self.tr("及时更新 Napcat 和 QQ 以获取全新的特性(或者是特性👀)"), self)
        self.pivot = SegmentedWidget()
        self.updateButton = PrimaryPushButton(FluentIcon.UPDATE, self.tr("刷新"))

        # 创建布局
        self.hBoxLayout = QHBoxLayout()
        self.labelLayout = QVBoxLayout()
        self.buttonLayout = QHBoxLayout()

        # 调用方法
        self._setLayout()

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        # 对 Label 区域进行布局
        self.labelLayout.setSpacing(0)
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.addWidget(self.titleLabel)
        self.labelLayout.addSpacing(5)
        self.labelLayout.addWidget(self.subtitleLabel)
        self.labelLayout.addSpacing(4)
        self.labelLayout.addWidget(self.pivot)

        # 对 Button 区域进行布局
        self.buttonLayout.addSpacing(4)
        self.buttonLayout.addWidget(self.updateButton)
        self.buttonLayout.setAlignment(Qt.AlignmentFlag.AlignBottom | Qt.AlignmentFlag.AlignLeft.AlignLeft)

        self.hBoxLayout.addLayout(self.labelLayout)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addLayout(self.buttonLayout)
        self.hBoxLayout.setContentsMargins(1, 0, 1, 5)

        self.setLayout(self.hBoxLayout)

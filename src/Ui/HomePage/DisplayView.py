# -*- coding: utf-8 -*-
from PySide6.QtGui import QFont, QDesktopServices
from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout
from qfluentwidgets.common import FluentIcon, setFont
from qfluentwidgets.components import ImageLabel, PushButton, TitleLabel, PrimaryPushButton

from src.Ui.StyleSheet import StyleSheet
from src.Core.NetworkFunc import Urls


class DisplayViewWidget(QWidget):
    # 开信号接口,将信号向上级传递
    goBtnSignal = Signal()

    def __init__(self) -> None:
        """
        初始化
        """
        super().__init__()
        self.setObjectName("display_view")

        # 创建控件
        self.vboxLayout = QVBoxLayout(self)
        self.logoImage = ImageLabel(self)
        self.logoLabel = TitleLabel("NapCatQQ-Desktop", self)
        self.buttonGroup = ButtonGroup(self)

        # 设置控件
        self.logoImage.setImage(":Global/logo.png")
        self.logoImage.scaledToWidth(self.width() // 5)
        self.buttonGroup.goBtnSignal.connect(self.goBtnSignal.emit)

        # 进行布局
        self._setLayout()

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

    def _setLayout(self) -> None:
        """
        对 ViewWidget 内控件进行布局
        """
        self.vboxLayout.addStretch(1)
        self.vboxLayout.addWidget(self.logoImage, alignment=Qt.AlignmentFlag.AlignCenter)
        self.vboxLayout.addSpacing(20)
        self.vboxLayout.setStretch(2, 0)
        self.vboxLayout.addWidget(self.logoLabel, alignment=Qt.AlignmentFlag.AlignCenter)
        self.vboxLayout.addSpacing(20)
        self.vboxLayout.addWidget(self.buttonGroup)
        self.vboxLayout.addStretch(1)

        self.vboxLayout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.setLayout(self.vboxLayout)

    def resizeEvent(self, event):
        """
        重写实现自动缩放
        """
        super().resizeEvent(event)
        # 缩放 Logo
        self.logoImage.scaledToWidth(self.width() // 5)
        # 缩放字体
        new_font_size = max(28, self.width() // 30)
        setFont(self.logoLabel, new_font_size, QFont.Weight.DemiBold)
        # 重绘页面
        self.update()


class ButtonGroup(QWidget):
    # 开信号接口,将信号向上级传递
    goBtnSignal = Signal()

    def __init__(self, parent: DisplayViewWidget) -> None:
        """
        初始化
        """
        super().__init__(parent=parent)

        # 创建控件
        self.hBoxLayout = QHBoxLayout(self)
        self.githubButton = PushButton(self)
        self.goButton = PrimaryPushButton(self)

        # 设置控件
        self.githubButton.setText(self.tr("GitHub"))
        self.goButton.setText(self.tr("Start Using"))
        self.githubButton.setIcon(FluentIcon.GITHUB)

        # 链接按钮信号
        self.goButton.clicked.connect(self.goBtnSignal.emit)
        self.githubButton.clicked.connect(lambda: QDesktopServices.openUrl(Urls.NAPCATQQ_REPO.value))

        # 进行布局
        self._setLayout()

    def _setLayout(self) -> None:
        """
        对 ButtonGroup 进行布局
        """
        self.hBoxLayout.addWidget(self.githubButton, alignment=Qt.AlignmentFlag.AlignCenter)
        self.hBoxLayout.addWidget(self.goButton, alignment=Qt.AlignmentFlag.AlignCenter)
        self.hBoxLayout.setSpacing(25)
        self.setLayout(self.hBoxLayout)

    def resizeEvent(self, event) -> None:
        """
        重写实现自动调整按钮大小
        """
        super().resizeEvent(event)
        # 重新计算大小
        new_button_width = max(100, self.parent().width() // 10)
        new_button_height = max(30, self.parent().height() // 20)
        self.githubButton.setFixedSize(new_button_width, new_button_height)
        self.goButton.setFixedSize(new_button_width, new_button_height)
        # 重绘画面
        self.update()

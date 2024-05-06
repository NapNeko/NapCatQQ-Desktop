# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt, QUrl, Signal
from PySide6.QtGui import QFont, QDesktopServices
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout
from qfluentwidgets.common import setFont, FluentIcon
from qfluentwidgets.components import ImageLabel, TitleLabel, PushButton, PrimaryPushButton

from src.Ui.StyleSheet import StyleSheet


class DisplayViewWidget(QWidget):
    # 开信号接口,将信号向上级传递
    go_btn_signal = Signal()

    def __init__(self) -> None:
        """
        初始化
        """
        super().__init__()
        self.setObjectName("display_view")

        # 创建控件
        self.v_box_layout = QVBoxLayout(self)
        self.logo_image = ImageLabel(self)
        self.logo_label = TitleLabel("NapCatQQ-Desktop", self)
        self.button_group = ButtonGroup(self)

        # 设置控件
        self.logo_image.setImage(":Global/logo.png")
        self.logo_image.scaledToWidth(self.width() // 5)
        self.button_group.go_btn_signal.connect(self.go_btn_signal.emit)

        # 进行布局
        self.__setlayout_()

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

    def __setlayout_(self) -> None:
        """
        对 ViewWidget 内控件进行布局
        """
        self.v_box_layout.addStretch(1)
        self.v_box_layout.addWidget(
            self.logo_image, alignment=Qt.AlignmentFlag.AlignCenter
        )
        self.v_box_layout.addSpacing(20)
        self.v_box_layout.setStretch(2, 0.5)
        self.v_box_layout.addWidget(
            self.logo_label, alignment=Qt.AlignmentFlag.AlignCenter
        )
        self.v_box_layout.addSpacing(20)
        self.v_box_layout.addWidget(self.button_group)
        self.v_box_layout.addStretch(1)

        self.v_box_layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.setLayout(self.v_box_layout)

    def resizeEvent(self, event):
        """
        重写实现自动缩放
        """
        super().resizeEvent(event)
        # 缩放 Logo
        self.logo_image.scaledToWidth(self.width() // 5)
        # 缩放字体
        new_font_size = max(28, self.width() // 30)
        setFont(self.logo_label, new_font_size, QFont.Weight.DemiBold)
        # 重绘页面
        self.update()


class ButtonGroup(QWidget):
    # 开信号接口,将信号向上级传递
    go_btn_signal = Signal()

    def __init__(self, parent: "DisplayViewWidget") -> None:
        """
        初始化
        """
        super().__init__()
        self.setParent(parent)

        # 创建控件
        self.h_box_layout = QHBoxLayout(self)
        self.github_btn = PushButton(self)
        self.go_btn = PrimaryPushButton(self)

        # 设置控件
        self.github_btn.setText(self.tr("GitHub"))
        self.go_btn.setText(self.tr("Start Using"))
        self.github_btn.setIcon(FluentIcon.GITHUB)

        # 链接按钮信号
        self.go_btn.clicked.connect(self.go_btn_signal.emit)
        self.github_btn.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl(
                r"https://github.com/NapNeko/NapCatQQ"
            ))
        )

        # 进行布局
        self.set_layout()

    def set_layout(self) -> None:
        """
        对 ButtonGroup 进行布局
        """
        self.h_box_layout.addWidget(
            self.github_btn, alignment=Qt.AlignmentFlag.AlignCenter
        )
        self.h_box_layout.addWidget(
            self.go_btn, alignment=Qt.AlignmentFlag.AlignCenter
        )

        self.h_box_layout.setSpacing(25)

        self.setLayout(self.h_box_layout)

    def resizeEvent(self, event) -> None:
        """
        重写实现自动调整按钮大小
        """
        super().resizeEvent(event)
        # 重新计算大小
        new_button_width = max(100, self.parent().width() // 10)
        new_button_height = max(30, self.parent().height() // 20)
        self.github_btn.setFixedSize(new_button_width, new_button_height)
        self.go_btn.setFixedSize(new_button_width, new_button_height)
        # 重绘画面
        self.update()

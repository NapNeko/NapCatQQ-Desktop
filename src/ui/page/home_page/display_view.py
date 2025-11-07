# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets.common import FluentIcon, setFont
from qfluentwidgets.components import ImageLabel, PrimaryPushButton, PushButton, TitleLabel
from PySide6.QtCore import Qt
from PySide6.QtGui import QDesktopServices, QFont
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.network.urls import Urls
from src.ui.components.background import DottedBackground
from src.ui.common.icon import StaticIcon
from src.ui.common.style_sheet import PageStyleSheet

"""
这里我是真没招了,这里是这个项目最早的代码了,当时也没什么经验
所以代码风格和现在不太一样,而且也没什么注释,后续有时间再来重构吧
目前只重构了代码风格,加上了一些注释(bushi),后续再说吧
"""


class DisplayViewWidget(DottedBackground):

    def __init__(self) -> None:
        """初始化"""
        super().__init__()
        self.setObjectName("display_view")

        # 创建控件
        self.v_box_layout = QVBoxLayout(self)
        self.logo_image = ImageLabel(self)
        self.logo_label = TitleLabel("NapCatQQ-Desktop", self)
        self.button_group = ButtonGroup(self)

        # 设置控件
        self.logo_image.setImage(StaticIcon.NAPCAT.path())
        self.logo_image.scaledToHeight(self.width() // 4)

        # 进行布局
        self._set_layout()

        # 应用样式表
        PageStyleSheet.HOME.apply(self)

    def _set_layout(self) -> None:
        """对 ViewWidget 内控件进行布局"""
        self.v_box_layout.addStretch(1)
        self.v_box_layout.addWidget(self.logo_image, alignment=Qt.AlignmentFlag.AlignCenter)
        self.v_box_layout.addSpacing(20)
        self.v_box_layout.setStretch(2, 0)
        self.v_box_layout.addWidget(self.logo_label, alignment=Qt.AlignmentFlag.AlignCenter)
        self.v_box_layout.addSpacing(20)
        self.v_box_layout.addWidget(self.button_group)
        self.v_box_layout.addStretch(1)

        self.v_box_layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.setLayout(self.v_box_layout)

    def resizeEvent(self, event):
        """重写实现自动缩放"""
        super().resizeEvent(event)
        # 缩放 Logo
        self.logo_image.scaledToHeight(self.width() // 4)
        # 缩放字体
        setFont(self.logo_label, max(28, self.width() // 30), QFont.Weight.DemiBold)
        # 重绘页面
        self.update()


class ButtonGroup(QWidget):

    def __init__(self, parent: DisplayViewWidget) -> None:
        """初始化"""
        super().__init__(parent=parent)

        # 创建控件
        self.h_box_layout = QHBoxLayout(self)
        self.github_button = PushButton(self)
        self.go_button = PrimaryPushButton(self)

        # 设置控件
        self.github_button.setText(self.tr("GitHub"))
        self.go_button.setText(self.tr("开始使用"))
        self.github_button.setIcon(FluentIcon.GITHUB)

        # 链接按钮信号
        self.github_button.clicked.connect(lambda: QDesktopServices.openUrl(Urls.NAPCATQQ_REPO.value))

        # 进行布局
        self._set_layout()

    def _set_layout(self) -> None:
        """对 ButtonGroup 进行布局"""
        self.h_box_layout.addWidget(self.github_button, alignment=Qt.AlignmentFlag.AlignCenter)
        self.h_box_layout.addWidget(self.go_button, alignment=Qt.AlignmentFlag.AlignCenter)
        self.h_box_layout.setSpacing(25)
        self.setLayout(self.h_box_layout)

    def resizeEvent(self, event) -> None:
        """重写实现自动调整按钮大小"""
        super().resizeEvent(event)
        # 重新计算大小
        new_button_width = max(100, self.parent().width() // 10)
        new_button_height = max(30, self.parent().height() // 20)
        self.github_button.setFixedSize(new_button_width, new_button_height)
        self.go_button.setFixedSize(new_button_width, new_button_height)
        # 重绘画面
        self.update()

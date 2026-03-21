# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self

# 第三方库导入
from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout

# 项目内模块导入
from src.ui.common.style_sheet import PageStyleSheet
from src.ui.page.home_page.hello_card import HelloCard
from src.ui.page.home_page.notice_card import NoticeCard
from src.ui.components.background import DottedBackground

# 避免在模块顶层导入 MainWindowCreator 以防止循环导入；
# 如果需要将 creator 之间的依赖关系声明给 creart，可在其他不产生循环的位置添加。

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import MainWindow


class HomeWidget(DottedBackground):

    def __init__(self) -> None:
        super().__init__()

    def initialize(self, parent: "MainWindow") -> Self:
        """初始化"""
        # 创建控件
        self.hello_card = HelloCard()
        self.notice_card = NoticeCard()

        # 设置控件
        self.setParent(parent)
        self.setObjectName("home_page")

        # 设置布局
        self.v_box_layout = QVBoxLayout()
        self.v_box_layout.addWidget(self.hello_card)
        self.v_box_layout.addSpacing(4)
        self.v_box_layout.addWidget(self.notice_card)

        self.h_box_layout = QHBoxLayout(self)
        self.h_box_layout.addLayout(self.v_box_layout)
        self.h_box_layout.addWidget(QWidget(), 1)  # 占位

        self.h_box_layout.setContentsMargins(24, 48, 24, 24)

        # 应用样式表
        PageStyleSheet.HOME.apply(self)
        self.hello_card.attach_floating_icon(self)

        return self


class HomePageCreator(AbstractCreator, ABC):
    """Home 页面创建器"""

    targets = (
        CreateTargetInfo(
            module="src.ui.page.home_page.home",
            identify="HomeWidget",
            humanized_name="Home 页面",
            description="NapCatQQ Desktop 主页面",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断 Home 页面模块是否可用"""
        return exists_module("src.ui.page.home_page.home")

    @staticmethod
    def create(create_type):
        """创建 Home 页面实例"""
        return create_type()


add_creator(HomePageCreator)

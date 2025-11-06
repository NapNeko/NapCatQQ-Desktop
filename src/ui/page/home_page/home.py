# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self

# 第三方库导入
from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module

# 项目内模块导入
from src.ui.common.style_sheet import PageStyleSheet
from src.ui.components.stacked_widget import TransparentStackedWidget
from src.ui.page.home_page.display_view import DisplayViewWidget

# 避免在模块顶层导入 MainWindowCreator 以防止循环导入；
# 如果需要将 creator 之间的依赖关系声明给 creart，可在其他不产生循环的位置添加。

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import MainWindow


class HomeWidget(TransparentStackedWidget):

    display_view: DisplayViewWidget

    def __init__(self) -> None:
        super().__init__()

    def initialize(self, parent: "MainWindow") -> Self:
        """初始化"""
        # 创建控件
        self.display_view = DisplayViewWidget()

        # 设置控件
        self.setParent(parent)
        self.setObjectName("home_page")
        self.addWidget(self.display_view)
        self.setCurrentWidget(self.display_view)

        # 链接信号
        self.display_view.button_group.go_button.clicked.connect(lambda: self.parent().setCurrentIndex(1))

        # 应用样式表
        PageStyleSheet.HOME.apply(self)

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
    def create(create_type: type[HomeWidget]) -> HomeWidget:
        """创建 Home 页面实例"""
        return create_type()


add_creator(HomePageCreator)

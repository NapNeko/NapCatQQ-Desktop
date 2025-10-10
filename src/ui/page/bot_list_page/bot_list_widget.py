# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING, List, Optional, Self

from PySide6.QtWidgets import QVBoxLayout, QWidget

# 项目内模块导入
from src.core.utils.singleton import singleton
from src.ui.components.stacked_widget import TransparentStackedWidget
from src.ui.page.bot_list_page.bot_list import BotList
from src.ui.page.bot_list_page.bot_top_card import BotTopCard

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.page.bot_list_page.bot_card import BotCard
    from src.ui.page.bot_list_page.bot_widget import BotWidget
    from src.ui.window.main_window import MainWindow


@singleton
class BotListWidget(QWidget):
    """机器人列表页面主控件, 显示所有已配置的机器人卡片"""

    view: Optional[TransparentStackedWidget]
    top_card: Optional[BotTopCard]
    bot_list: Optional[BotList]
    v_box_layout: Optional[QVBoxLayout]

    def __init__(self) -> None:
        """初始化 BotListWidget"""
        super().__init__()
        self.view = None
        self.top_card = None
        self.bot_list = None
        self.v_box_layout = None

    def initialize(self, parent: "MainWindow") -> Self:
        """初始化控件并进行配置

        Args:
            parent (MainWindow): 父控件

        Returns:
            Self: 返回自身实例用于链式调用
        """
        self.setParent(parent)
        self.setObjectName("bot_list_page")

        self._create_widgets()
        self._setup_layout()
        self._apply_styles()

        return self

    def _create_widgets(self) -> None:
        """创建并初始化所有子控件"""
        self.v_box_layout = QVBoxLayout(self)
        self.top_card = BotTopCard(self)
        self.view = TransparentStackedWidget(self)
        self.view.setObjectName("BotListStackedWidget")

        self.bot_list = BotList(self.view)
        self.view.addWidget(self.bot_list)
        self.view.setCurrentWidget(self.bot_list)

    def _setup_layout(self) -> None:
        """设置控件布局"""
        self.v_box_layout.addWidget(self.top_card)
        self.v_box_layout.addWidget(self.view)
        self.v_box_layout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.v_box_layout)

    def _apply_styles(self) -> None:
        """应用样式表"""
        ...

    # ==================== 公共方法 ====================
    def run_all_bot(self) -> None:
        """运行所有已配置的机器人"""
        # 项目内模块导入
        from src.ui.page.bot_list_page.bot_widget import BotWidget

        for card in self.bot_list.bot_card_list:
            if card.bot_widget is None:
                card.bot_widget = BotWidget(card.config)
                self.view.addWidget(card.bot_widget)

            card.bot_widget.on_run_button()

    def stop_all_bot(self) -> None:
        """停止所有正在运行的机器人"""
        for card in self.bot_list.bot_card_list:
            if not card.bot_widget:
                continue
            if card.bot_widget.is_run:
                card.bot_widget.stop_button.click()

    def get_bot_is_run(self) -> bool:
        """检查是否有机器人正在运行

        Returns:
            bool: 如果有机器人正在运行返回 True, 否则返回 False
        """
        # TODO 此处需要调整为检查 bot_widget 的 is_run 状态
        # for card in self.bot_list.bot_card_list:
        #     if not card.bot_widget:
        #         # 如果没有创建则表示没有运行
        #         continue
        #     if card.bot_widget.is_run:
        #         return True
        # return False

        return False

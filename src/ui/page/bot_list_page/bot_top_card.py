# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import BreadcrumbBar, CaptionLabel, FluentIcon, ToolTipFilter, TransparentToolButton, setFont
from PySide6.QtCore import Qt, Slot
from PySide6.QtGui import QFont
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.page.bot_list_page.bot_list_widget import BotListWidget


class BotTopCard(QWidget):
    """BotListWidget 顶部展示的 InputCard, 用于展示面包屑导航和操作按钮"""

    def __init__(self, parent: "BotListWidget") -> None:
        """初始化 BotTopCard

        Args:
            parent: 父控件 BotListWidget
        """
        super().__init__(parent=parent)

        # 创建控件
        self.breadcrumb_bar = BreadcrumbBar(self)
        self.subtitle_label = CaptionLabel(self.tr("您可以在此对机器人进行配置、启动以及管理"), self)
        self.update_list_button = TransparentToolButton(FluentIcon.SYNC, self)  # 刷新列表按钮

        self.h_box_layout = QHBoxLayout()
        self.label_layout = QVBoxLayout()
        self.button_layout = QHBoxLayout()

        # 设置控件属性
        setFont(self.breadcrumb_bar, 28, QFont.Weight.DemiBold)
        self.breadcrumb_bar.addItem(routeKey="BotTopCardTitle", text=self.tr("机器人列表"))
        self.breadcrumb_bar.setSpacing(15)
        self.update_list_button.clicked.connect(self._on_update_list_button_clicked)
        self.breadcrumb_bar.currentIndexChanged.connect(self._on_breadcrumb_bar_index_changed)

        # 初始化布局和提示
        self._add_tool_tips()
        self._setup_layout()

    def _setup_layout(self) -> None:
        """设置内部控件布局"""
        # 标签布局
        self.label_layout.setSpacing(0)
        self.label_layout.setContentsMargins(0, 0, 0, 0)
        self.label_layout.addWidget(self.breadcrumb_bar)
        self.label_layout.addSpacing(5)
        self.label_layout.addWidget(self.subtitle_label)

        # 按钮布局
        self.button_layout.setSpacing(0)
        self.button_layout.setContentsMargins(0, 0, 0, 0)
        self.button_layout.addWidget(self.update_list_button)
        self.button_layout.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignBottom)

        # 主布局
        self.h_box_layout.addLayout(self.label_layout)
        self.h_box_layout.addLayout(self.button_layout)
        self.h_box_layout.setContentsMargins(0, 0, 20, 0)

        self.setLayout(self.h_box_layout)

    def _add_tool_tips(self) -> None:
        """为按钮添加悬停提示"""
        self.update_list_button.setToolTip(self.tr("点击刷新列表"))
        self.update_list_button.installEventFilter(ToolTipFilter(self.update_list_button))

    # ==================== 公共方法 ====================
    def add_item(self, route_key: str) -> None:
        """给面包屑导航添加项目

        Args:
            route_key: 路由键, 用于标识项目
        """
        self.breadcrumb_bar.clear()
        self.breadcrumb_bar.addItem(routeKey="BotTopCardTitle", text=self.tr("机器人列表"))
        self.breadcrumb_bar.addItem(route_key, route_key)

    # ==================== 槽函数 ====================
    @Slot(int)
    def _on_breadcrumb_bar_index_changed(self, index: int) -> None:
        """面包屑导航索引变化槽函数

        判断用户是否点击的是 Bot List, 如果是则返回 Bot List 页面

        Args:
            index: 当前选中的索引
        """
        if index == 0:
            # 项目内模块导入
            from src.ui.page.bot_list_page.bot_list_widget import BotListWidget

            BotListWidget().view.setCurrentIndex(index)
            self.update_list_button.show()

    @Slot()
    def _on_update_list_button_clicked(self) -> None:
        """更新列表按钮点击槽函数"""
        # 项目内模块导入
        from src.ui.page.bot_list_page.bot_list_widget import BotListWidget

        BotListWidget().bot_list.update_list()

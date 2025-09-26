# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING, Any, Dict, Optional, Self

from PySide6.QtCore import Slot
from PySide6.QtWidgets import QVBoxLayout, QWidget

# 项目内模块导入
from src.core.utils.singleton import singleton
from src.ui.common.style_sheet import StyleSheet
from src.ui.components.stacked_widget import TransparentStackedWidget
from src.ui.page.add_page.add_page_enum import ConnectType
from src.ui.page.add_page.add_top_card import AddTopCard
from src.ui.page.add_page.advanced import AdvancedWidget
from src.ui.page.add_page.bot_widget import BotWidget
from src.ui.page.add_page.connect import ConnectWidget
from src.ui.page.add_page.msg_box import (
    ChooseConfigTypeDialog,
    HttpClientConfigDialog,
    HttpServerConfigDialog,
    HttpSSEServerConfigDialog,
    WebsocketClientConfigDialog,
    WebsocketServerConfigDialog,
)
from src.ui.page.add_page.signal_bus import add_page_singal_bus

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import MainWindow


@singleton
class AddWidget(QWidget):
    """Add Bot 页面主控件，包含基本配置、连接配置和高级配置三个标签页"""

    view: Optional[TransparentStackedWidget]
    top_card: Optional[AddTopCard]
    v_box_layout: Optional[QVBoxLayout]
    bot_widget: Optional[BotWidget]
    connect_widget: Optional[ConnectWidget]
    advanced_widget: Optional[AdvancedWidget]

    def __init__(self) -> None:
        """初始化 AddWidget"""
        super().__init__()
        self.view = None
        self.top_card = None
        self.v_box_layout = None
        self.bot_widget = None
        self.connect_widget = None
        self.advanced_widget = None

    def initialize(self, parent: "MainWindow") -> Self:
        """初始化 AddWidget 所需要的控件并进行配置

        Args:
            parent (MainWindow): 父控件

        Returns:
            Self: 返回自身实例用于链式调用
        """
        self.setParent(parent)
        self.setObjectName("add_page")

        self._create_widgets()
        self._setup_layout()
        self._connect_signals()
        self._apply_styles()

        return self

    def _create_widgets(self) -> None:
        """创建并初始化所有子控件"""
        self.v_box_layout = QVBoxLayout(self)
        self.top_card = AddTopCard(self)
        self._create_view()

    def _create_view(self) -> None:
        """创建并配置堆叠视图，包含三个配置页面"""
        self.view = TransparentStackedWidget()
        self.view.setObjectName("AddView")

        self.bot_widget = BotWidget(self)
        self.connect_widget = ConnectWidget(self)
        self.advanced_widget = AdvancedWidget(self)

        self.view.addWidget(self.bot_widget)
        self.view.addWidget(self.connect_widget)
        self.view.addWidget(self.advanced_widget)

        # 添加标签页项
        self.top_card.pivot.addItem(
            routeKey=self.bot_widget.objectName(),
            text=self.tr("基本配置"),
            onClick=lambda: self.view.setCurrentWidget(self.bot_widget),
        )
        self.top_card.pivot.addItem(
            routeKey=self.connect_widget.objectName(),
            text=self.tr("连接配置"),
            onClick=lambda: self.view.setCurrentWidget(self.connect_widget),
        )
        self.top_card.pivot.addItem(
            routeKey=self.advanced_widget.objectName(),
            text=self.tr("高级配置"),
            onClick=lambda: self.view.setCurrentWidget(self.advanced_widget),
        )

        # 连接信号并初始化当前标签页
        self.view.currentChanged.connect(self.on_current_index_changed)
        self.view.currentChanged.connect(add_page_singal_bus.add_widget_view_change_signal.emit)
        self.view.setCurrentWidget(self.bot_widget)
        self.top_card.pivot.setCurrentItem(self.bot_widget.objectName())

    def _setup_layout(self) -> None:
        """设置控件布局"""
        self.v_box_layout.addWidget(self.top_card)
        self.v_box_layout.addWidget(self.view)
        self.v_box_layout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.v_box_layout)

    def _connect_signals(self) -> None:
        """连接所有信号与槽"""
        # 视图切换信号
        self.view.currentChanged.connect(self.on_current_index_changed)
        self.view.currentChanged.connect(add_page_singal_bus.add_widget_view_change_signal.emit)

        # 业务逻辑信号
        add_page_singal_bus.add_connect_config_button_clicked_signal.connect(self._on_add_connect_config_button_clicked)
        add_page_singal_bus.choose_connect_type_signal.connect(self._on_show_type_dialog)

    def _apply_styles(self) -> None:
        """应用样式表"""
        StyleSheet.ADD_WIDGET.apply(self)

    def get_config(self) -> Dict[str, Any]:
        """返回配置结果

        Returns:
            Dict[str, Any]: 包含 bot、connect、advanced 三个部分配置的字典
        """
        return {
            "bot": self.bot_widget.get_value(),
            "connect": self.connect_widget.get_value(),
            "advanced": self.advanced_widget.get_value(),
        }

    @Slot(int)
    def on_current_index_changed(self, index: int) -> None:
        """视图切换时同步更新标签页选中状态

        Args:
            index: 当前视图索引
        """
        widget = self.view.widget(index)
        if widget:
            self.top_card.pivot.setCurrentItem(widget.objectName())

    @Slot()
    def _on_add_connect_config_button_clicked(self) -> None:
        """处理添加连接配置按钮点击事件"""
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        ChooseConfigTypeDialog(MainWindow()).exec()

    @Slot(ConnectType)
    def _on_show_type_dialog(self, connect_type: ConnectType) -> None:
        """根据连接类型显示对应的配置对话框

        Args:
            connect_type: 连接类型枚举值
        """
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        dialog_map = {
            ConnectType.HTTP_SERVER: HttpServerConfigDialog,
            ConnectType.HTTP_SSE_SERVER: HttpSSEServerConfigDialog,
            ConnectType.HTTP_CLIENT: HttpClientConfigDialog,
            ConnectType.WEBSOCKET_SERVER: WebsocketServerConfigDialog,
            ConnectType.WEBSOCKET_CLIENT: WebsocketClientConfigDialog,
        }

        dialog_class = dialog_map.get(connect_type)
        if dialog_class:
            dialog = dialog_class(MainWindow())
            if dialog.exec():
                self.connect_widget.add_card(dialog.get_config())

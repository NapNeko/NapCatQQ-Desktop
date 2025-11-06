# -*- coding: utf-8 -*-
"""
Bot 配置页面
"""
# 标准库导入
from enum import Enum

# 第三方库导入
from creart import it
from qfluentwidgets import FluentIcon, SegmentedWidget, TransparentPushButton
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config.config_model import Config
from src.core.config.operate_config import update_config
from src.ui.components.info_bar import error_bar, success_bar
from src.ui.components.stacked_widget import TransparentStackedWidget
from src.ui.page.bot_page.utils.enum import ConnectType
from src.ui.page.bot_page.widget import (
    ChooseConfigTypeDialog,
    HttpClientConfigDialog,
    HttpServerConfigDialog,
    HttpSSEServerConfigDialog,
    WebsocketClientConfigDialog,
    WebsocketServerConfigDialog,
)
from src.ui.page.bot_page.widget.config import AdvancedConfigWidget, BotConfigWidget, ConnectConfigWidget


class ConfigPage(QWidget):
    """配置机器人页面"""

    CONNECT_TYPE_AND_DIALOG = {
        ConnectType.HTTP_SERVER: HttpServerConfigDialog,
        ConnectType.HTTP_SSE_SERVER: HttpSSEServerConfigDialog,
        ConnectType.HTTP_CLIENT: HttpClientConfigDialog,
        ConnectType.WEBSOCKET_SERVER: WebsocketServerConfigDialog,
        ConnectType.WEBSOCKET_CLIENT: WebsocketClientConfigDialog,
    }

    class PageEnum(Enum):
        """页面枚举"""

        BOT_WIDGET = 0
        CONNECT_WIDGET = 1
        ADVANCED_WIDGET = 2

    def __init__(self, parent: QWidget | None = None):
        """初始化页面"""
        super().__init__(parent)
        # 设置属性
        self._config = None

        # 创建控件
        self.piovt = SegmentedWidget(self)
        self.view = TransparentStackedWidget()
        self.bot_widget = BotConfigWidget(self)
        self.connect_widget = ConnectConfigWidget(self)
        self.advanced_widget = AdvancedConfigWidget(self)
        self.return_button = TransparentPushButton(FluentIcon.LEFT_ARROW, self.tr("返回"), self)
        self.add_connect_button = TransparentPushButton(FluentIcon.ADD, self.tr("添加"), self)
        self.save_config_button = TransparentPushButton(FluentIcon.SAVE, self.tr("保存"), self)

        # 设置控件
        self.view.addWidget(self.bot_widget)
        self.view.addWidget(self.connect_widget)
        self.view.addWidget(self.advanced_widget)
        self.view.setCurrentWidget(self.bot_widget)

        self.piovt.addItem(
            routeKey=f"bot_widget",
            text=self.tr("基本配置"),
            onClick=lambda: self.view.setCurrentWidget(self.bot_widget),
        )
        self.piovt.addItem(
            routeKey="connect_widget",
            text=self.tr("连接配置"),
            onClick=lambda: self.view.setCurrentWidget(self.connect_widget),
        )
        self.piovt.addItem(
            routeKey=f"advanced_widget",
            text=self.tr("高级配置"),
            onClick=lambda: self.view.setCurrentWidget(self.advanced_widget),
        )
        self.piovt.setCurrentItem("bot_widget")

        self.add_connect_button.hide()

        # 设置布局
        self.top_layout = QHBoxLayout()
        self.top_layout.setContentsMargins(0, 0, 0, 0)
        self.top_layout.addWidget(self.piovt)
        self.top_layout.addStretch(1)
        self.top_layout.addWidget(self.add_connect_button)
        self.top_layout.addWidget(self.save_config_button)
        self.top_layout.addWidget(self.return_button)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.addLayout(self.top_layout)
        self.v_box_layout.addWidget(self.view, 1)

        # 链接信号
        self.view.currentChanged.connect(self.slot_view_current_index_changed)
        self.add_connect_button.clicked.connect(self.slot_add_connect_button)
        self.save_config_button.clicked.connect(self.slot_save_config_button)
        self.return_button.clicked.connect(self.slot_return_button)

    # ==================== 公共函数===================
    def get_config(self) -> Config:
        """返回所有配置"""
        return Config(
            **{
                "bot": self.bot_widget.get_config(),
                "connect": self.connect_widget.get_config(),
                "advanced": self.advanced_widget.get_config(),
            }
        )

    def fill_config(self, config: Config | None = None):
        """填充配置"""
        if config is None:
            return

        self._config = config
        self.bot_widget.fill_config(self._config.bot)
        self.connect_widget.fill_config(self._config.connect)
        self.advanced_widget.fill_config(self._config.advanced)

    def clear_config(self) -> None:
        """清空配置"""
        self.bot_widget.clear_config()
        self.connect_widget.clear_config()
        self.advanced_widget.clear_config()

    # ==================== 槽函数 ====================
    def slot_view_current_index_changed(self, index: int) -> None:
        """当 view 切换时更新 piovt 的选中状态

        Args:
            index (int): 当前索引
        """
        match self.PageEnum(index):
            case self.PageEnum.BOT_WIDGET:
                self.piovt.setCurrentItem("bot_widget")
                self.add_connect_button.hide()
            case self.PageEnum.CONNECT_WIDGET:
                self.piovt.setCurrentItem("connect_widget")
                self.add_connect_button.show()
            case self.PageEnum.ADVANCED_WIDGET:
                self.piovt.setCurrentItem("advanced_widget")
                self.add_connect_button.hide()

    def slot_return_button(self) -> None:
        """返回按钮槽函数"""
        # 项目内模块导入
        from src.ui.page.bot_page import BotPage

        page = it(BotPage)
        page.view.setCurrentWidget(page.bot_list_page)

    def slot_save_config_button(self) -> None:
        """保存按钮槽函数"""

        if update_config(self.get_config()):
            # 项目内模块导入
            from src.ui.page.bot_page import BotPage

            it(BotPage).bot_list_page.update_bot_list()
            success_bar(self.tr("保存配置成功"))
        else:
            error_bar(self.tr("保存配置文件时引发错误"))

    def slot_add_connect_button(self) -> None:
        """添加连接配置按钮槽函数"""
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        if not (_choose_connect_type_box := ChooseConfigTypeDialog(it(MainWindow))).exec():
            # 获取用户选择的结果并判断是否取消
            return

        if (_connect_type := _choose_connect_type_box.get_value()) == ConnectType.NO_TYPE:
            # 判断用户选择的类型, 如果没有选择则直接退出
            return

        if not (_connect_config_box := self.CONNECT_TYPE_AND_DIALOG.get(_connect_type)(it(MainWindow))).exec():
            # 判断用户在配置的时候是否选择了取消
            return

        # 拿到配置项添加卡片
        self.connect_widget.add_card(_connect_config_box.get_config())

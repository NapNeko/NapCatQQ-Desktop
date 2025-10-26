# -*- coding: utf-8 -*-
"""
Bot 配置页面
"""
# 第三方库导入
from enum import Enum
from qfluentwidgets import ExpandLayout, FluentIcon, ScrollArea, SegmentedWidget, TransparentPushButton
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout

# 项目内模块导入
from src.core.config.config_model import Config, BotConfig
from src.ui.common.icon import NapCatDesktopIcon
from src.ui.components.input_card import ComboBoxConfigCard, LineEditConfigCard, SwitchConfigCard
from src.ui.components.stacked_widget import TransparentStackedWidget


class BotConfigPage(ScrollArea):
    """Bot 设置页面"""

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        # 创建控件
        self.view = QWidget()

        self.bot_name_card = LineEditConfigCard(
            icon=FluentIcon.ROBOT,
            title=self.tr("Bot 名称"),
            content=self.tr("设置机器人的名称"),
            placeholder_text=self.tr("QIAO Bot"),
            parent=self.view,
        )
        self.bot_qq_id_card = LineEditConfigCard(
            icon=NapCatDesktopIcon.QQ,
            title=self.tr("Bot QQ"),
            content=self.tr("设置机器人 QQ 号, 不能为空"),
            placeholder_text=self.tr("114514"),
            parent=self.view,
        )
        self.music_sign_url_card = LineEditConfigCard(
            icon=FluentIcon.MUSIC,
            title=self.tr("音乐签名URL"),
            content=self.tr("用于处理音乐相关请求, 为空则使用默认签名服务器"),
            placeholder_text=self.tr("https://example.com/music"),
            parent=self.view,
        )
        # self.auto_restart_dialog = ShowDialogCard(
        #     dialog=AutoRestartDialog,
        #     icon=FluentIcon.IOT,
        #     title=self.tr("自动重启"),
        #     content=self.tr("设置自动重启 Bot 的相关选项"),
        #     parent=self.view,
        # )

        # 设置属性
        self._config = None
        self.cards = [getattr(self, attr) for attr in dir(self) if attr.endswith("_card")]

        # 设置控件
        self.setWidget(self.view)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setWidgetResizable(True)

        # 创建布局
        self.card_layout = ExpandLayout(self.view)
        self.card_layout.setContentsMargins(0, 0, 0, 0)
        self.card_layout.setSpacing(2)
        for card in self.cards:
            self.card_layout.addWidget(card)
        self.adjustSize()

    # ==================== 公共方法 ====================
    def get_config(self) -> BotConfig:
        """获取配置"""
        return BotConfig(
            **{
                "name": self.bot_name_card.get_value(),
                "QQID": self.bot_qq_id_card.get_value(),
                "musicSignUrl": self.music_sign_url.get_value(),
                "autoRestartSchedule": self.auto_restart_dialog.get_value(),
            }
        )

    def fill_config(self, config: BotConfig | None = None) -> None:
        """填充配置"""
        if config is None:
            return

        self._config = config
        self.bot_name_card.fill_value(self._config.name)
        self.bot_qq_id_card.fill_value(self._config.QQID)
        self.music_sign_url_card.fill_value(self._config.musicSignUrl)

    def clear_config(self) -> None:
        """清空配置"""
        for card in self.cards:
            card.clear()

    # ==================== 重写方法 ====================
    def adjustSize(self) -> None:
        """重写方法以调整控件大小适应内容高度"""
        self.resize(self.width(), self.card_layout.heightForWidth(self.width()) + 46)


class ConfigPage(QWidget):
    """配置机器人页面"""

    class PageEnum(Enum):
        """页面枚举"""

        BOT_WIDGET = 0

    def __init__(self, parent: QWidget | None = None):
        """初始化页面"""
        super().__init__(parent)
        # 设置属性
        self._config = None

        # 创建控件
        self.piovt = SegmentedWidget(self)
        self.view = TransparentStackedWidget()
        self.bot_widget = BotConfigPage(self)

        # 设置控件
        self.view.addWidget(self.bot_widget)
        self.view.setCurrentWidget(self.bot_widget)

        self.piovt.addItem(
            routeKey=f"bot_widget",
            text=self.tr("基本配置"),
            onClick=self.view.setCurrentWidget(self.bot_widget),
        )
        self.piovt.setCurrentItem("bot_widget")

        self.return_button = TransparentPushButton(FluentIcon.LEFT_ARROW, self.tr("返回"), self)

        # 设置布局
        self.top_layout = QHBoxLayout()
        self.top_layout.setContentsMargins(0, 0, 0, 0)
        self.top_layout.addWidget(self.piovt, 1, Qt.AlignmentFlag.AlignLeft)
        self.top_layout.addWidget(self.return_button, 1, Qt.AlignmentFlag.AlignRight)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.addLayout(self.top_layout)
        self.v_box_layout.addWidget(self.view, 1)

        # 链接信号
        self.view.currentChanged.connect(self.slot_view_current_index_changed)
        self.return_button.clicked.connect(self.slot_return_button)

    # ==================== 公共函数===================
    def fill_config(self, config: Config | None = None):
        """填充配置"""
        if config is None:
            return

        self._config = config
        self.bot_widget.fill_config(self._config.bot)

    # ==================== 槽函数 ====================
    def slot_view_current_index_changed(self, index: int) -> None:
        """当 view 切换时更新 piovt 的选中状态

        Args:
            index (int): 当前索引
        """
        match (index := self.PageEnum(index)):
            case self.PageEnum.BOT_WIDGET:
                self.piovt.setCurrentItem("bot_widget")

    def slot_return_button(self) -> None:
        """返回按钮槽函数"""
        from src.ui.page.bot_page import BotPage

        BotPage().view.setCurrentWidget(BotPage().bot_list_page)

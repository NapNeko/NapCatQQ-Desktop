# -*- coding: utf-8 -*-
# 类型注解导入
# 标准库导入
from typing import Any, Dict, List, Optional

# 第三方库导入
from qfluentwidgets import ExpandLayout, FluentIcon, ScrollArea
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config.config_model import BotConfig
from src.ui.common.icon import NapCatDesktopIcon
from src.ui.components.input_card import LineEditConfigCard


class BotWidget(ScrollArea):
    """Bot 基本配置页面, 包含名称、QQ号、音乐签名等配置项"""

    def __init__(self, parent: Optional[QWidget] = None, config: Optional[BotConfig] = None) -> None:
        """初始化 Bot 配置页面

        Args:
            parent: 父控件
            config: 可选的基础配置数据, 如果提供则填充表单
        """
        super().__init__(parent=parent)
        self.config: Optional[BotConfig] = config
        self.cards: List[LineEditConfigCard] = []

        self._setup_ui()
        self._set_layout()

    def _setup_ui(self) -> None:
        """初始化界面控件"""
        self.setObjectName("bot_widget")
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setWidgetResizable(True)

        # 创建视图容器
        self.view = QWidget()
        self.view.setObjectName("BotWidgetView")
        self.setWidget(self.view)

        # 创建布局
        self.card_layout = ExpandLayout(self.view)

        # 创建配置卡片
        self._create_cards()

        # 如果提供了配置数据, 则填充表单
        if self.config is not None:
            self.fill_value()

    def _create_cards(self) -> None:
        """创建所有配置卡片控件"""
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

        self.music_sign_url = LineEditConfigCard(
            icon=FluentIcon.MUSIC,
            title=self.tr("音乐签名URL"),
            content=self.tr("用于处理音乐相关请求, 为空则使用默认签名服务器"),
            placeholder_text=self.tr("https://example.com/music"),
            parent=self.view,
        )

        self.cards = [self.bot_name_card, self.bot_qq_id_card, self.music_sign_url]

    def _set_layout(self) -> None:
        """设置控件布局"""
        self.card_layout.setContentsMargins(0, 0, 0, 0)
        self.card_layout.setSpacing(2)

        for card in self.cards:
            self.card_layout.addWidget(card)

        self.view.setLayout(self.card_layout)
        self.adjustSize()

    # ==================== 公共方法 ====================
    def get_value(self) -> Dict[str, Any]:
        """获取当前所有配置项的值

        Returns:
            Dict[str, Any]: 包含名称、QQID、音乐签名URL的配置字典
        """
        return {
            "name": self.bot_name_card.get_value(),
            "QQID": self.bot_qq_id_card.get_value(),
            "musicSignUrl": self.music_sign_url.get_value(),
        }

    def fill_value(self) -> None:
        """使用配置数据填充表单

        Raises:
            AttributeError: 当 config 为 None 时调用此方法
        """
        if self.config is None:
            raise AttributeError("Cannot fill value without config")

        self.bot_name_card.fill_value(self.config.name)
        self.bot_qq_id_card.fill_value(self.config.QQID)
        self.music_sign_url.fill_value(self.config.musicSignUrl)

    def clear_values(self) -> None:
        """清空所有配置项的值"""
        for card in self.cards:
            card.clear()

    def adjustSize(self) -> None:
        """调整控件大小以适应内容高度

        Returns:
            None: 调整自身大小(我也不知道为什么这个函数要有返回值)
        """
        h = self.card_layout.heightForWidth(self.width()) + 46
        return self.resize(self.width(), h)

# -*- coding: utf-8 -*-
# 标准库导入
from typing import Any, Dict, List

# 第三方库导入
from qfluentwidgets import ExpandLayout, FluentIcon, ScrollArea
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config.config_model import AdvancedConfig
from src.ui.components.input_card import ComboBoxConfigCard, LineEditConfigCard, SwitchConfigCard


class AdvancedWidget(ScrollArea):
    """高级配置页面，包含各种高级设置选项"""

    view: QWidget
    card_layout: ExpandLayout
    config: AdvancedConfig
    cards: List[Any]  # 更具体的类型可以根据实际卡片类型定义

    def __init__(self, parent: QWidget | None = None, config: AdvancedConfig | None = None) -> None:
        """初始化高级配置控件

        Args:
            config: 高级配置数据模型，默认为 None
            parent: 父控件，默认为 None
        """
        super().__init__(parent=parent)
        self.config = config
        self.cards = []

        self._setup_ui()
        self._setup_layout()

        # 填充配置值
        self.fill_value()

    def _setup_ui(self) -> None:
        """初始化界面控件"""
        # 设置 ScrollArea
        self.setObjectName("AdvanceWidget")
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        # 创建视图和布局
        self.view = QWidget()
        self.view.setObjectName("AdvanceWidgetView")
        self.setWidget(self.view)
        self.setWidgetResizable(True)

        self.card_layout = ExpandLayout(self.view)

        # 创建配置卡片
        self._create_config_cards()

    def _create_config_cards(self) -> None:
        """创建所有配置卡片控件"""
        self.auto_start_card = SwitchConfigCard(
            icon=FluentIcon.PLAY,
            title=self.tr("自动启动"),
            content=self.tr("是否在启动时自动启动 bot"),
            parent=self.view,
        )
        self.offline_notice = SwitchConfigCard(
            icon=FluentIcon.MEGAPHONE,
            title=self.tr("掉线通知"),
            content=self.tr("当Bot状态为 离线 时, 发送通知"),
            parent=self.view,
        )
        self.parse_mult_message = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("解析合并转发消息"),
            content=self.tr("是否解析合并转发消息"),
            parent=self.view,
        )
        self.packet_server_card = LineEditConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("Packet Server"),
            content=self.tr("设置 Packet Server 地址, 为空则使用默认值"),
            parent=self.view,
        )
        self.packet_backend_card = LineEditConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("Packet Backend"),
            content=self.tr("设置 Packet Backend, 为空则使用默认值"),
            parent=self.view,
        )
        self.local_file_to_url_card = SwitchConfigCard(
            icon=FluentIcon.SHARE,
            title=self.tr("LocalFile2Url"),
            content=self.tr("是否将本地文件转换为URL, 如果获取不到url则使用base64字段返回文件内容"),
            value=True,
            parent=self.view,
        )
        self.file_log_card = SwitchConfigCard(
            icon=FluentIcon.SAVE_AS,
            title=self.tr("文件日志"),
            content=self.tr("是否要将日志记录到文件"),
            parent=self.view,
        )
        self.console_log_card = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("控制台日志"),
            content=self.tr("是否启用控制台日志"),
            value=True,
            parent=self.view,
        )
        self.file_log_level_card = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("文件日志等级"),
            content=self.tr("设置文件日志输出等级"),
            texts=["debug", "info", "error"],
            parent=self.view,
        )
        self.console_level_card = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("控制台日志等级"),
            content=self.tr("设置控制台日志输出等级"),
            texts=["info", "debug", "error"],
            parent=self.view,
        )
        self.o3_hook_mode_card = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("O3 Hook 模式"),
            content=self.tr("设置 O3 Hook 模式"),
            texts=["0", "1"],
            parent=self.view,
        )

        # 收集所有卡片
        self.cards = [
            self.auto_start_card,
            self.offline_notice,
            self.parse_mult_message,
            self.packet_server_card,
            self.packet_backend_card,
            self.local_file_to_url_card,
            self.file_log_card,
            self.console_log_card,
            self.file_log_level_card,
            self.console_level_card,
            self.o3_hook_mode_card,
        ]

    def _setup_layout(self) -> None:
        """设置布局"""
        self.card_layout.setContentsMargins(0, 0, 0, 0)
        self.card_layout.setSpacing(2)

        for card in self.cards:
            self.card_layout.addWidget(card)
            self.adjustSize()

        self.view.setLayout(self.card_layout)

    # ==================== 公共方法 ====================
    def fill_value(self) -> None:
        """使用配置数据填充所有卡片的值"""
        if not self.config:
            return

        self.auto_start_card.fill_value(self.config.autoStart)
        self.offline_notice.fill_value(self.config.offlineNotice)
        self.parse_mult_message.fill_value(self.config.parseMultMsg)
        self.packet_server_card.fill_value(self.config.packetServer)
        self.packet_backend_card.fill_value(self.config.packetBackend)
        self.local_file_to_url_card.fill_value(self.config.enableLocalFile2Url)
        self.file_log_card.fill_value(self.config.fileLog)
        self.console_log_card.fill_value(self.config.consoleLog)
        self.file_log_level_card.fill_value(self.config.fileLogLevel)
        self.console_level_card.fill_value(self.config.consoleLogLevel)
        self.o3_hook_mode_card.fill_value(str(self.config.o3HookMode))

    def get_value(self) -> Dict[str, Any]:
        """获取所有配置项的当前值

        Returns:
            Dict[str, Any]: 包含所有配置项的字典
        """
        return {
            "autoStart": self.auto_start_card.get_value(),
            "offlineNotice": self.offline_notice.get_value(),
            "parseMultMsg": self.parse_mult_message.get_value(),
            "packetServer": self.packet_server_card.get_value(),
            "packetBackend": self.packet_backend_card.get_value(),
            "enableLocalFile2Url": self.local_file_to_url_card.get_value(),
            "fileLog": self.file_log_card.get_value(),
            "consoleLog": self.console_log_card.get_value(),
            "fileLogLevel": self.file_log_level_card.get_value(),
            "consoleLogLevel": self.console_level_card.get_value(),
            "o3HookMode": int(self.o3_hook_mode_card.get_value()),
        }

    def clear_values(self) -> None:
        """清空所有配置项的值"""
        for card in self.cards:
            card.clear()

    # ==================== 辅助方法 ====================
    def adjustSize(self) -> None:
        """调整控件大小以适应内容

        重写此方法以实现自定义大小调整逻辑
        """
        h = self.card_layout.heightForWidth(self.width()) + 46
        return self.resize(self.width(), h)

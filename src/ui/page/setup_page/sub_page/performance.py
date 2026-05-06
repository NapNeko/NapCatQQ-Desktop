# -*- coding: utf-8 -*-
"""性能设置页面"""

# 第三方库导入
from qfluentwidgets import ExpandLayout, FluentIcon, RangeSettingCard, ScrollArea, SettingCardGroup, SwitchSettingCard
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config import cfg


class Performance(ScrollArea):
    """性能设置页面"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        # 创建控件
        self.view = QWidget()
        self.expand_layout = ExpandLayout(self.view)

        # 设置 ScrollArea 和控件
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.view.setObjectName("SetupView")
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setObjectName("SetupPerformanceWidget")

        # 调用方法
        self._create_config_cards()
        self._set_layout()

    def _create_config_cards(self) -> None:
        """创建配置项卡片"""
        # 创建组 - 主页性能监控
        self.home_monitor_group = SettingCardGroup(title=self.tr("主页性能监控"), parent=self.view)
        
        # 性能监控开关
        self.monitor_enabled_card = SwitchSettingCard(
            configItem=cfg.performance_monitor_enabled,
            icon=FluentIcon.SPEED_HIGH,
            title=self.tr("启用性能监控"),
            content=self.tr("关闭后主页将不再显示 CPU 和内存占用数据"),
            parent=self.home_monitor_group,
        )
        
        # 性能监控采样频率
        self.monitor_interval_card = RangeSettingCard(
            configItem=cfg.performance_monitor_interval,
            icon=FluentIcon.HISTORY,
            title=self.tr("采样频率"),
            content=self.tr("主页 CPU 和内存监控的数据采样间隔（毫秒）"),
            parent=self.home_monitor_group,
        )

        # 创建组 - Bot 监控
        self.bot_monitor_group = SettingCardGroup(title=self.tr("Bot 监控"), parent=self.view)
        
        # Bot 登录状态检查间隔
        self.login_check_interval_card = RangeSettingCard(
            configItem=cfg.bot_login_check_interval,
            icon=FluentIcon.SYNC,
            title=self.tr("登录状态检查间隔"),
            content=self.tr("Bot 登录状态和在线状态的检查间隔（毫秒），未登录时强制1秒检查"),
            parent=self.bot_monitor_group,
        )
        
        # Bot 内存监控采样频率
        self.memory_monitor_interval_card = RangeSettingCard(
            configItem=cfg.bot_memory_monitor_interval,
            icon=FluentIcon.DATE_TIME,
            title=self.tr("内存监控采样频率"),
            content=self.tr("Bot 卡片中内存占用的更新间隔（毫秒），运行时长固定1秒更新"),
            parent=self.bot_monitor_group,
        )

    def _set_layout(self) -> None:
        """设置布局"""
        # 添加主页监控组
        self.home_monitor_group.addSettingCard(self.monitor_enabled_card)
        self.home_monitor_group.addSettingCard(self.monitor_interval_card)
        
        # 添加 Bot 监控组
        self.bot_monitor_group.addSettingCard(self.login_check_interval_card)
        self.bot_monitor_group.addSettingCard(self.memory_monitor_interval_card)

        # 添加到主布局
        self.expand_layout.setSpacing(28)
        self.expand_layout.setContentsMargins(24, 10, 24, 0)
        self.expand_layout.addWidget(self.home_monitor_group)
        self.expand_layout.addWidget(self.bot_monitor_group)

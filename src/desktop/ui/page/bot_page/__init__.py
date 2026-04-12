# -*- coding: utf-8 -*-
"""
此模块用于展示软件内 Bot 页面, 这是软件的核心页面, 包含很多子页面
"""
from __future__ import annotations

# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self

# 第三方库导入
from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module, it
from PySide6.QtWidgets import QVBoxLayout, QWidget

from src.desktop.core.config.operate_config import read_config
from src.desktop.core.logging import LogSource, logger
from src.desktop.core.logging.crash_bundle import mask_qqid
from src.desktop.core.runtime.napcat import ManagerNapCatQQProcess
from src.desktop.ui.components.stacked_widget import TransparentStackedWidget
from .sub_page import BotListPage, BotLogPage, ConfigPage
from .widget import HeaderWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.desktop.ui.window.main_window import MainWindow


class BotPage(QWidget):
    """Bot 页面"""

    header: HeaderWidget
    view: TransparentStackedWidget
    bot_list_page: BotListPage
    bot_config_page: ConfigPage
    add_config_page: ConfigPage
    log_page: BotLogPage

    def __init__(self) -> None:
        super().__init__()

    def initialize(self, parent: "MainWindow") -> Self:
        """初始化"""
        # 创建控件
        self.header = HeaderWidget(self)
        self.view = TransparentStackedWidget(self)
        self.vBoxLayout = QVBoxLayout(self)

        # 设置属性
        self.setObjectName("bot_page")
        self.setParent(parent)

        # 设置布局
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.vBoxLayout.setSpacing(16)

        self.vBoxLayout.addWidget(self.header, 1)
        self.vBoxLayout.addWidget(self.view, 9)

        # 调用方法
        self.conncet_signal()
        self.setup_view()
        self._auto_start_bots()

        return self

    def _auto_start_bots(self) -> None:
        """自动启动配置了 autoStart 的 Bot"""
        configs = read_config()
        process_manager = it(ManagerNapCatQQProcess)

        for config in configs:
            if config.advanced.autoStart:
                qq_id = str(config.bot.QQID)
                # 检查是否已经在运行
                if process_manager.get_process(qq_id) is not None:
                    logger.trace(f"自动启动跳过: Bot 已在运行(QQID: {mask_qqid(qq_id)})", log_source=LogSource.UI)
                    continue

                logger.info(f"自动启动 Bot(QQID: {mask_qqid(qq_id)})", log_source=LogSource.UI)
                process_manager.create_napcat_process(config)

    def setup_view(self) -> None:
        """设置 view 界面"""
        # 创建 sub page
        self.bot_list_page = BotListPage(self)
        self.bot_config_page = ConfigPage(self)
        self.add_config_page = self.bot_config_page
        self.log_page = BotLogPage(self)

        # 添加到 view
        self.view.addWidget(self.bot_list_page)
        self.view.addWidget(self.bot_config_page)
        self.view.addWidget(self.add_config_page)
        self.view.addWidget(self.log_page)

        # 设置初始页面
        self.view.setCurrentWidget(self.bot_list_page)

    def conncet_signal(self) -> None:
        """链接信号"""
        self.view.currentChanged.connect(self.header.setup_breadcrumb_bar)


class BotPageCreator(AbstractCreator, ABC):
    """Bot 页面创建器"""

    targets = (
        CreateTargetInfo(
            module="src.desktop.ui.page.bot_page",
            identify="BotPage",
            humanized_name="Bot 页面",
            description="NapCatQQ Desktop Bot 页面",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断 Bot 页面模块是否可用"""
        return exists_module("src.ui.page.bot_page")

    @staticmethod
    def create(create_type):
        """创建 Bot 页面实例"""
        return create_type()


add_creator(BotPageCreator)

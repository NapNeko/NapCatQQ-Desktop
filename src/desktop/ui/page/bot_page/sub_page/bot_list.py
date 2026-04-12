# -*- coding: utf-8 -*-
"""这是 Bot 列表子页面模块"""

# 第三方库导入
from creart import it
from qfluentwidgets import FlowLayout, FluentIcon, PrimaryToolButton, ScrollArea, ToolButton
from PySide6.QtCore import QSize, Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.desktop.core.config.config_model import Config
from src.desktop.core.config.operate_config import delete_config, read_config
from src.desktop.core.logging.crash_bundle import mask_qqid
from src.desktop.core.logging import LogSource, logger
from src.desktop.ui.components.info_bar import error_bar

from ..widget.card import BotCard


class BotListPage(ScrollArea):
    """Bot 列表子页面"""

    def __init__(self, parent: QWidget | None = None) -> None:
        """构造函数"""
        super().__init__(parent)
        # 设置属性
        self._bot_config_list: list[Config] = []
        self._bot_card_list: list[BotCard] = []

        # 创建视图和布局
        self.view = QWidget(self)
        self.view_layout = FlowLayout(self.view)
        self.add_button = PrimaryToolButton(FluentIcon.ADD, self)
        self.update_button = ToolButton(FluentIcon.UPDATE, self)

        # 设置控件
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.view_layout.setContentsMargins(0, 0, 0, 0)
        self.view_layout.setSpacing(4)

        self.add_button.setFixedSize(40, 40)
        self.add_button.setIconSize(QSize(20, 20))
        self.update_button.setFixedSize(40, 40)
        self.update_button.setIconSize(QSize(20, 20))

        # 连接信号
        self.add_button.clicked.connect(self.slot_add_button)
        self.update_button.clicked.connect(self.update_bot_list)

        # 调用方法
        self.update_bot_list()

    # ==================== 重写方法 ===================
    def resizeEvent(self, event):
        super().resizeEvent(event)
        width = self.width() - self.add_button.width()
        height = self.height() - self.add_button.height()
        self.add_button.move(width - 16, height - 32)
        self.update_button.move(width - 16, height - 82)

    # ==================== 公共方法 ====================
    def update_bot_list(self) -> None:
        """刷新 Bot 列表

        用于刷新 view 中的 Bot Card
        """
        # 判断原有 bot config list 是否为空, 不为空则清空
        if self._bot_card_list:
            self.remove_all_bot()

        # 读取配置文件
        if (configs := read_config()) == self._bot_config_list:
            # 如果读取的配置文件与现有配置文件一致, 则跳过
            logger.trace("Bot 列表刷新跳过: 配置未发生变化", log_source=LogSource.UI)
            return
        else:
            # 不一致则赋值给属性
            self._bot_config_list = configs.copy()
            logger.trace(f"Bot 列表已刷新: count={len(self._bot_config_list)}", log_source=LogSource.UI)

        # 创建 Bot Card 并添加到布局
        for config in self._bot_config_list:
            card = BotCard(config)
            card.remove_signal.connect(self.remove_bot_by_qqid)
            card.update_info_card()
            self._bot_card_list.append(card)
            self.view_layout.addWidget(card)

    def _is_card_alive(self, card: BotCard) -> bool:
        """判断 Bot Card 是否仍然有效。"""
        try:
            card.parent()
        except RuntimeError:
            logger.warning("检测到已失效的 Bot 卡片引用，已跳过处理", log_source=LogSource.UI)
            return False

        return True

    def _dispose_card(self, card: BotCard) -> None:
        """安全移除单个 Bot Card。"""
        if not self._is_card_alive(card):
            return

        try:
            self.view_layout.removeWidget(card)
            card.setParent(None)
            card.deleteLater()
        except RuntimeError:
            logger.warning("移除 Bot 卡片时检测到对象已失效，已跳过剩余清理", log_source=LogSource.UI)

    def remove_bot_by_qqid(self, qqid: str) -> None:
        """通过 QQID 移除 Bot Card

        用于移除 view 中指定 QQID 的 Bot Card
        """
        target_config = next((config for config in self._bot_config_list if str(config.bot.QQID) == qqid), None)
        if target_config is None:
            logger.warning(f"尝试移除不存在的 Bot 配置(QQID: {mask_qqid(qqid)})", log_source=LogSource.UI)
            error_bar(self.tr("未找到待移除的 Bot 配置"))
            return

        logger.info(f"准备移除 Bot 配置(QQID: {mask_qqid(qqid)})", log_source=LogSource.UI)
        if not delete_config(target_config):
            logger.error(f"移除 Bot 配置失败(QQID: {mask_qqid(qqid)})", log_source=LogSource.UI)
            error_bar(self.tr("移除 Bot 配置失败"))
            return

        self._bot_config_list = [config for config in self._bot_config_list if str(config.bot.QQID) != qqid]

        target_card: BotCard | None = None
        remaining_cards: list[BotCard] = []

        for card in self._bot_card_list:
            if not self._is_card_alive(card):
                continue

            if target_card is None and str(card._config.bot.QQID) == qqid:
                target_card = card
                continue

            remaining_cards.append(card)

        self._bot_card_list = remaining_cards

        if target_card is None:
            logger.warning(f"Bot 卡片不存在或已失效(QQID: {mask_qqid(qqid)})", log_source=LogSource.UI)
            return

        self._dispose_card(target_card)
        logger.info(f"Bot 卡片已从列表移除(QQID: {mask_qqid(qqid)})", log_source=LogSource.UI)

    def remove_all_bot(self) -> None:
        """移除所有 Bot Card

        用于移除 view 中的所有 Bot Card
        """
        for card in self._bot_card_list:
            self._dispose_card(card)

        self._bot_card_list.clear()
        self._bot_config_list.clear()

    # =================== 槽函数 =======================
    def slot_add_button(self) -> None:
        """添加按钮槽函数"""
        # 判断有没有安装 NapCatQQ
        from src.desktop.core.versioning import LocalVersionTask

        if LocalVersionTask().get_napcat_version():
            # 项目内模块导入
            from src.desktop.ui.page.bot_page import BotPage

            logger.trace("进入新增 Bot 配置流程", log_source=LogSource.UI)
            page = it(BotPage)
            page.view.setCurrentWidget(page.add_config_page)
            page.add_config_page.clear_config()
            page.header.setup_breadcrumb_bar(999)

        else:
            from src.desktop.ui.components.info_bar import warning_bar

            logger.warning("新增 Bot 配置被拒绝: 未检测到 NapCatQQ 安装", log_source=LogSource.UI)
            warning_bar("请先安装 NapCatQQ 后再添加 Bot 配置！")


# -*- coding: utf-8 -*-

# 标准库导入
import math
from enum import Enum

# 第三方库导入
from qfluentwidgets import BreadcrumbBar, setFont
from PySide6.QtGui import QFont
from PySide6.QtWidgets import QVBoxLayout, QWidget


class HeaderWidget(QWidget):
    """页面顶部的信息标头"""

    class PageEnum(Enum):

        BOT_LIST = 0
        BOT_CONFIG = 1
        LOG_PAGE = 2

        # 因为此处与 BOT_CONFIG 为同一个页面, 需要区分
        ADD_CONFIG = 999

    def __init__(self, parent: QWidget | None = None) -> None:
        """构造函数

        Args:
            parent (QWidget | None, optional): 父控件. Defaults to None.
        """
        super().__init__(parent)

        # 设置属性
        # 在我们以编程方式更新 breadcrumb_bar 时会阻塞其信号
        self._suppress_breadcrumb_signals = False
        # 当我们正在通过面包屑触发页面切换时，防止重复或重入
        self._changing_view = False

        # 创建控件
        self.breadcrumb_bar = BreadcrumbBar(self)
        self.v_box_layout = QVBoxLayout(self)

        # 设置控件
        setFont(self.breadcrumb_bar, 28, QFont.Weight.Bold)
        self.breadcrumb_bar.setSpacing(16)
        self.setFixedHeight(48)

        # 连接信号
        self.breadcrumb_bar.currentItemChanged.connect(self.slot_current_item_changed)

        # 设置布局
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(6)
        self.v_box_layout.addWidget(self.breadcrumb_bar)

    def setup_breadcrumb_bar(self, index: int):
        """通过index设置对应的breadcrumb_bar"""
        # 阻塞面包屑信号，避免在程序化更新时触发槽函数
        self._suppress_breadcrumb_signals = True
        self.breadcrumb_bar.blockSignals(True)
        try:
            self.breadcrumb_bar.clear()

            # 兼容传入 int 或已构造的 PageEnum；对无效值回退到 BOT_LIST
            try:
                page = index if isinstance(index, self.PageEnum) else self.PageEnum(index)
            except Exception:
                page = self.PageEnum.BOT_LIST

            # 基础项（大多数页面都会包含）
            items: list[tuple[str, str]] = [("bot_list", self.tr("List"))]

            # 根据页面追加特定项
            if page is self.PageEnum.BOT_CONFIG:
                items.append(("bot_config", self.tr("Config")))
            elif page is self.PageEnum.ADD_CONFIG:
                # 与 BOT_CONFIG 共用 key，但文本不同以表示添加操作
                items.append(("bot_config", self.tr("Add")))
            elif page is self.PageEnum.LOG_PAGE:
                items.append(("bot_log", self.tr("Log")))

            for key, text in items:
                self.breadcrumb_bar.addItem(key, text)
        finally:
            self.breadcrumb_bar.blockSignals(False)
            self._suppress_breadcrumb_signals = False

    # ==================== 槽函数 ====================
    def slot_current_item_changed(self, key: str) -> None:
        """面包屑导航栏当前项改变时触发的槽函数

        Args:
            key (str): 当前项的key
        """

        # 如果当前正在程序化更新 breadcrumb 或正在切换 view，忽略该信号以防止循环
        if self._suppress_breadcrumb_signals or self._changing_view:
            return

        from src.ui.page.bot_page import BotPage
        from creart import it

        page = it(BotPage)

        if key == "bot_list":
            target = self.PageEnum.BOT_LIST.value
        elif key == "bot_config":
            target = self.PageEnum.BOT_CONFIG.value
        elif key == "bot_log":
            target = self.PageEnum.LOG_PAGE.value
        else:
            return

        # 如果目标页面已经是当前页面，则无需再次切换
        try:
            current = page.view.currentIndex()
        except Exception:
            current = None

        if current == target:
            return

        try:
            self._changing_view = True
            page.view.setCurrentIndex(target)
        finally:
            self._changing_view = False

# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING, Optional, Self

from PySide6.QtCore import Slot
from PySide6.QtWidgets import QVBoxLayout, QWidget

# 项目内模块导入
from src.core.utils.get_version import GetVersion
from src.core.utils.singleton import singleton
from src.ui.common.style_sheet import StyleSheet
from src.ui.components.stacked_widget import TransparentStackedWidget
from src.ui.page.unit_page.napcat_desktop_page import NCDPage
from src.ui.page.unit_page.napcat_page import NapCatPage
from src.ui.page.unit_page.qq_page import QQPage
from src.ui.page.unit_page.top import TopWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import MainWindow


@singleton
class UnitWidget(QWidget):
    """单元页面控件，包含 NapCat、QQ 和 Desktop 三个标签页的版本信息管理"""

    def __init__(self) -> None:
        """初始化 UnitWidget"""
        super().__init__()
        # 预定义控件
        self.view: TransparentStackedWidget | None = None
        self.top_card: TopWidget | None = None
        self.v_box_layout: QVBoxLayout | None = None
        self.get_version: GetVersion | None = None
        self.napcat_page: NapCatPage | None = None
        self.qq_page: QQPage | None = None
        self.ncd_page: NCDPage | None = None

    def _setup_ui(self, parent: "MainWindow") -> Self:
        """初始化界面控件和布局

        Args:
            parent: 父窗口实例

        Returns:
            Self: 返回自身实例用于链式调用
        """
        self.setParent(parent)
        self.setObjectName("UpdatePage")

        self._create_widgets()
        self._setup_layout()
        self._connect_signals()
        self._apply_styles()

        # 启用一次更新
        self.on_update()

        return self

    def _create_widgets(self) -> None:
        """创建所有子控件"""
        self.v_box_layout = QVBoxLayout(self)
        self.top_card = TopWidget(self)
        self.get_version = GetVersion(self)
        self._create_view()

    def _create_view(self) -> None:
        """创建并配置堆叠视图，包含三个版本信息页面"""
        self.view = TransparentStackedWidget()
        self.view.setObjectName("UpdateView")

        self.napcat_page = NapCatPage(self)
        self.qq_page = QQPage(self)
        self.ncd_page = NCDPage(self)

        self.view.addWidget(self.napcat_page)
        self.view.addWidget(self.qq_page)
        self.view.addWidget(self.ncd_page)

        # 添加标签页项
        self.top_card.pivot.addItem(
            routeKey=self.napcat_page.objectName(),
            text=self.tr("NapCat"),
            onClick=lambda: self.view.setCurrentWidget(self.napcat_page),
        )
        self.top_card.pivot.addItem(
            routeKey=self.qq_page.objectName(),
            text=self.tr("QQ"),
            onClick=lambda: self.view.setCurrentWidget(self.qq_page),
        )
        self.top_card.pivot.addItem(
            routeKey=self.ncd_page.objectName(),
            text=self.tr("Desktop"),
            onClick=lambda: self.view.setCurrentWidget(self.ncd_page),
        )

        # 设置默认页面
        self.view.setCurrentWidget(self.napcat_page)
        self.top_card.pivot.setCurrentItem(self.napcat_page.objectName())

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

        # 更新按钮信号
        self.top_card.update_button.clicked.connect(self.on_update)

        # 版本更新信号
        self.get_version.remote_finish_signal.connect(self.napcat_page.on_update_remote_version)
        self.get_version.remote_finish_signal.connect(self.qq_page.on_update_remote_version)
        self.get_version.remote_finish_signal.connect(self.ncd_page.on_update_remote_version)
        self.get_version.remote_finish_signal.connect(lambda: self.top_card.update_button.setEnabled(True))

        self.get_version.local_finish_signal.connect(self.napcat_page.on_update_local_version)
        self.get_version.local_finish_signal.connect(self.qq_page.on_update_local_version)
        self.get_version.local_finish_signal.connect(self.ncd_page.on_update_local_version)

    def _apply_styles(self) -> None:
        """应用样式表"""
        StyleSheet.UNIT_WIDGET.apply(self)

    # ==================== 槽函数 ====================
    @Slot()
    def on_current_index_changed(self, index: int) -> None:
        """视图切换时同步更新标签页选中状态

        Args:
            index: 当前视图索引
        """
        widget = self.view.widget(index)
        if widget:
            self.top_card.pivot.setCurrentItem(widget.objectName())

    @Slot()
    def on_update(self) -> None:
        """执行版本更新检查"""
        self.get_version.update()
        self.top_card.update_button.setEnabled(False)

# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING, Self
from abc import ABC

# 第三方库导入
from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtCore import Slot
from PySide6.QtWidgets import QVBoxLayout, QWidget

# 项目内模块导入
from src.core.versioning import VersionService
from src.core.logging import LogSource, logger
from src.ui.common.style_sheet import PageStyleSheet
from src.ui.components.stacked_widget import TransparentStackedWidget
from src.ui.page.component_page.desktop_page import DesktopPage
from src.ui.page.component_page.napcat_page import NapCatPage
from src.ui.page.component_page.qq_page import QQPage
from src.ui.page.component_page.top import TopWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import MainWindow


class ComponentPage(QWidget):
    """组件页面控件，负责协调安装与更新相关子页面的版本刷新。"""

    def __init__(self) -> None:
        """初始化 ComponentPage"""
        super().__init__()
        self.v_box_layout = QVBoxLayout(self)
        self.top_card = TopWidget(self)
        self.version_service = VersionService(self)
        self.view = TransparentStackedWidget()
        self.napcat_page = NapCatPage(self)
        self.qq_page = QQPage(self)
        self.desktop_page = DesktopPage(self)

    def initialize(self, parent: "MainWindow") -> Self:
        """初始化界面控件和布局。

        Args:
            parent: 父窗口实例

        Returns:
            Self: 返回自身实例用于链式调用
        """
        self.setParent(parent)
        self.setObjectName("ComponentPage")

        self._create_widgets()
        self._setup_layout()
        self._connect_signals()
        self._apply_styles()

        # 启用一次更新
        self.refresh_versions()

        return self

    def _create_widgets(self) -> None:
        """创建所有子控件"""
        self._create_view()

    def _create_view(self) -> None:
        """创建并配置堆叠视图，包含三个版本信息页面"""
        self.view.setObjectName("UpdateView")

        self.view.addWidget(self.napcat_page)
        self.view.addWidget(self.qq_page)
        self.view.addWidget(self.desktop_page)

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
            routeKey=self.desktop_page.objectName(),
            text=self.tr("Desktop"),
            onClick=lambda: self.view.setCurrentWidget(self.desktop_page),
        )

        # 设置默认页面
        self.view.setCurrentWidget(self.napcat_page)
        self.top_card.pivot.setCurrentItem(self.napcat_page.objectName())

    def _setup_layout(self) -> None:
        """设置控件布局"""
        self.v_box_layout.addWidget(self.top_card)
        self.v_box_layout.addWidget(self.view)
        self.v_box_layout.setContentsMargins(24, 20, 24, 20)
        self.setLayout(self.v_box_layout)

    def _connect_signals(self) -> None:
        """连接所有信号与槽"""
        # 视图切换信号
        self.view.currentChanged.connect(self.handle_current_index_changed)

        # 更新按钮信号
        self.top_card.update_button.clicked.connect(self.refresh_versions)

        # 版本更新信号
        self.version_service.remote_versions_loaded.connect(self.napcat_page.apply_remote_version_data)
        self.version_service.remote_versions_loaded.connect(self.qq_page.apply_remote_version_data)
        self.version_service.remote_versions_loaded.connect(self.desktop_page.apply_remote_version_data)
        self.version_service.remote_versions_loaded.connect(lambda: self.top_card.update_button.setEnabled(True))

        self.version_service.local_versions_loaded.connect(self.napcat_page.apply_local_version_data)
        self.version_service.local_versions_loaded.connect(self.qq_page.apply_local_version_data)
        self.version_service.local_versions_loaded.connect(self.desktop_page.apply_local_version_data)

    def _apply_styles(self) -> None:
        """应用样式表"""
        PageStyleSheet.UNIT.apply(self)

    # ==================== 槽函数 ====================
    @Slot()
    def handle_current_index_changed(self, index: int) -> None:
        """视图切换时同步更新标签页选中状态。

        Args:
            index: 当前视图索引
        """
        logger.trace(f"组件页标签切换: page={self.view.widget(index).objectName()}", log_source=LogSource.UI)
        self.top_card.pivot.setCurrentItem(self.view.widget(index).objectName())

    @Slot()
    def refresh_versions(self) -> None:
        """执行一次版本刷新。"""
        logger.info("开始执行版本更新检查", log_source=LogSource.UI)
        self.napcat_page.begin_version_refresh()
        self.qq_page.begin_version_refresh()
        self.desktop_page.begin_version_refresh()
        self.napcat_page.log_card.set_loading(True)
        self.desktop_page.log_card.set_loading(True)
        self.version_service.refresh()
        self.top_card.update_button.setEnabled(False)


class ComponentPageCreator(AbstractCreator, ABC):
    """组件页面创建器"""

    targets = (
        CreateTargetInfo(
            module="src.ui.page.component_page.page",
            identify="ComponentPage",
            humanized_name="组件页面",
            description="NapCatQQ Desktop 组件页面",
        ),
    )

    @staticmethod
    def available() -> bool:
        """检查所需模块是否存在以支持创建 ComponentPage"""
        return exists_module("src.ui.page.component_page.page")

    @staticmethod
    def create(create_type):
        """创建并返回 ComponentPage 实例"""
        return create_type()


add_creator(ComponentPageCreator)


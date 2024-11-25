# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING, Self, Optional

from PySide6.QtWidgets import QVBoxLayout

# 项目内模块导入
from src.Ui.common import CodeEditor
from src.Core.Config import cfg
from src.Ui.StyleSheet import StyleSheet
from src.Ui.common.widget import BackgroundWidget
from src.Core.Utils.singleton import singleton
from src.Ui.common.stacked_widget import TransparentStackedWidget
from src.Ui.SetupPage.SetupTopCard import SetupTopCard
from src.Ui.SetupPage.SetupScrollArea import SetupScrollArea

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


@singleton
class SetupWidget(BackgroundWidget):
    """设置页面"""

    view: Optional[TransparentStackedWidget]
    topCard: Optional[SetupTopCard]
    setupScrollArea: Optional[SetupScrollArea]
    vBoxLayout: Optional[QVBoxLayout]
    infoWidget: Optional[CodeEditor]

    def __init__(self, parent: "MainWindow"):
        super().__init__(parent=parent)
        # 传入配置
        self.bgEnabledConfig = cfg.bgSettingPage
        self.bgPixmapLightConfig = cfg.bgSettingPageLight
        self.bgPixmapDarkConfig = cfg.bgSettingPageDark
        self.bgOpacityConfig = cfg.bgSettingPageOpacity

        # 调用方法
        super().updateBgImage()

    def initialize(self) -> Self:
        """
        ## 初始化
        """
        # 创建控件
        self.topCard = SetupTopCard(self)
        self.vBoxLayout = QVBoxLayout()
        self._createView()

        # 调整控件
        self.setObjectName("SetupPage")
        self.view.setObjectName("SetupView")

        # 设置布局
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.view)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)

        # 应用样式表
        StyleSheet.SETUP_WIDGET.apply(self)
        return self

    def _createView(self) -> None:
        """
        ## 创建并配置 QStackedWidget
        """
        self.view = TransparentStackedWidget(self)
        self.setupScrollArea = SetupScrollArea(self)
        self.view.addWidget(self.setupScrollArea)

        # 连接信号并初始化当前标签页
        self.view.setCurrentWidget(self.setupScrollArea)

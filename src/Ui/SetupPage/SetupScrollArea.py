# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from qfluentwidgets import (
    FluentIcon,
    ScrollArea,
    ExpandLayout,
    SettingCardGroup,
    OptionsSettingCard,
    ComboBoxSettingCard,
    CustomColorSettingCard,
    setTheme,
    setThemeColor,
)
from PySide6.QtCore import Qt, QStandardPaths
from PySide6.QtWidgets import QWidget, QFileDialog

# 项目内模块导入
from src.Core.Config import cfg
from src.Ui.common.info_bar import success_bar

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


class SetupScrollArea(ScrollArea):

    def __init__(self, parent) -> None:
        """
        初始化
        """
        super().__init__(parent=parent)
        # 创建控件
        self.view = QWidget()
        self.expand_layout = ExpandLayout(self.view)

        # 设置 ScrollArea 和控件
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.view.setObjectName("SetupView")
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        # 调用方法
        self._createConfigCards()
        self._connect_signal()
        self._setLayout()

    def _createConfigCards(self) -> None:
        """
        创建配置项卡片
        """

        # 创建组 - 启动项
        self.startGroup = SettingCardGroup(title=self.tr("启动项"), parent=self.view)
        self.startOpenHomePageViewCard = OptionsSettingCard(
            configItem=cfg.StartOpenHomePageView,
            icon=FluentIcon.COPY,
            title=self.tr("切换主页页面"),
            content=self.tr("选择启动时主页展示的页面"),
            texts=[self.tr("展示页面"), self.tr("功能页面")],
            parent=self.startGroup,
        )

        # 创建组 - 个性化
        self.personalGroup = SettingCardGroup(title=self.tr("个性化"), parent=self.view)
        # 创建项
        self.themeCard = OptionsSettingCard(
            configItem=cfg.themeMode,
            icon=FluentIcon.BRUSH,
            title=self.tr("切换主题"),
            content=self.tr("切换程序的主题"),
            texts=[self.tr("Light"), self.tr("Dark"), self.tr("Auto")],
            parent=self.personalGroup,
        )
        self.themeColorCard = CustomColorSettingCard(
            configItem=cfg.themeColor,
            icon=FluentIcon.PALETTE,
            title=self.tr("主题颜色"),
            content=self.tr("选择主题色"),
            parent=self.personalGroup,
        )
        self.languageCard = ComboBoxSettingCard(
            configItem=cfg.language,
            icon=FluentIcon.LANGUAGE,
            title=self.tr("语言"),
            content=self.tr("设置程序的首选语言"),
            texts=["简体中文", "繁體中文", "English", self.tr("Use system setting")],
            parent=self.personalGroup,
        )

        # 创建组 - 路径
        # self.pathGroup = SettingCardGroup(title=self.tr("Path"), parent=self.view)

    def _setLayout(self) -> None:
        """
        控件布局
        """
        # 将卡片添加到组
        self.startGroup.addSettingCard(self.startOpenHomePageViewCard)

        self.personalGroup.addSettingCard(self.themeCard)
        self.personalGroup.addSettingCard(self.themeColorCard)
        self.personalGroup.addSettingCard(self.languageCard)

        # 添加到布局
        self.expand_layout.addWidget(self.startGroup)
        self.expand_layout.addWidget(self.personalGroup)
        self.expand_layout.setContentsMargins(0, 0, 0, 0)
        self.view.setLayout(self.expand_layout)

    def _connect_signal(self) -> None:
        """
        信号处理
        """
        # 连接重启提示
        cfg.appRestartSig.connect(lambda: success_bar(self.tr("配置在重启后生效")))

        # 连接启动相关
        self.startOpenHomePageViewCard.optionChanged.connect(
            lambda value: cfg.set(cfg.StartOpenHomePageView, value.value, True)
        )

        # 连接个性化相关
        self.themeCard.optionChanged.connect(self._themeModeChanged)
        self.themeColorCard.colorChanged.connect(lambda color: setThemeColor(color, save=True, lazy=True))

    def _selectFolder(self) -> str:
        """
        选择文件夹的槽函数
        """
        folder = QFileDialog.getExistingDirectory(
            parent=self,
            caption=self.tr("Choose folder"),
            dir=QStandardPaths.writableLocation(QStandardPaths.StandardLocation.DesktopLocation),
        )
        return folder

    @staticmethod
    def _themeModeChanged(theme) -> None:
        """
        主题切换槽函数
        """
        # 最好还是重启下吧，不然有些地方不生效，修也不好修，就很烦
        cfg.appRestartSig.emit()
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow

        setTheme(cfg.get(theme), save=True)
        it(MainWindow).home_widget.updateBgImage()

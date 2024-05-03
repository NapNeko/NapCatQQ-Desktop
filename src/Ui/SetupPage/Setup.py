# -*- coding: utf-8 -*-
from abc import ABC
from typing import TYPE_CHECKING, Self

from PySide6.QtCore import Qt, QStandardPaths
from PySide6.QtWidgets import QWidget, QFileDialog
from creart import add_creator, exists_module, it
from creart.creator import AbstractCreator, CreateTargetInfo
from qfluentwidgets import ScrollArea
from qfluentwidgets.common import FluentIcon, setTheme, setThemeColor
from qfluentwidgets.components import (
    InfoBar,
    ExpandLayout,
    SettingCardGroup,
    OptionsSettingCard,
    CustomColorSettingCard,
    ComboBoxSettingCard,
    PushSettingCard,
)

from src.Core.Config import cfg
from src.Core.PathFunc import PathFunc
from src.Ui.Icon import NapCatDesktopIcon
from src.Ui.StyleSheet import StyleSheet

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class SetupWidget(ScrollArea):

    def __init__(self) -> None:
        """
        初始化
        """
        super().__init__()
        self.expand_layout = None
        self.view = None

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        # 创建控件
        self.view = QWidget()
        self.expand_layout = ExpandLayout(self.view)

        # 设置 ScrollArea 和控件
        self.setParent(parent)
        self.setObjectName("SetupPage")
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.view.setObjectName("SetupView")

        # 调用方法
        self.__createConfigCards()
        self._connect_signal()
        self.__setLayout()

        # 应用样式表
        StyleSheet.SETUP_WIDGET.apply(self)

        return self

    def __createConfigCards(self) -> None:
        """
        创建配置项卡片
        """

        # 创建组 - 启动项
        self.startGroup = SettingCardGroup(title=self.tr("Startup Item"), parent=self.view)
        self.startOpenHomePageViewCard = OptionsSettingCard(
            configItem=cfg.StartOpenHomePageView,
            icon=FluentIcon.COPY,
            title=self.tr("Switch HomePage View"),
            content=self.tr("Select the page on your homepage when you start"),
            texts=[self.tr("A useless display page"), self.tr("Function page")],
            parent=self.startGroup,
        )

        # 创建组 - 个性化
        self.personalGroup = SettingCardGroup(title=self.tr("Personalize"), parent=self.view)
        # 创建项
        self.themeCard = OptionsSettingCard(
            configItem=cfg.themeMode,
            icon=FluentIcon.BRUSH,
            title=self.tr("Switch themes"),
            content=self.tr("Switch the theme of the app"),
            texts=[self.tr("Light"), self.tr("Dark"), self.tr("Auto")],
            parent=self.personalGroup,
        )
        self.themeColorCard = CustomColorSettingCard(
            configItem=cfg.themeColor,
            icon=FluentIcon.PALETTE,
            title=self.tr("Theme Color"),
            content=self.tr("Choose a theme color"),
            parent=self.personalGroup,
        )
        self.languageCard = ComboBoxSettingCard(
            configItem=cfg.language,
            icon=FluentIcon.LANGUAGE,
            title=self.tr("Language"),
            content=self.tr("Set your preferred language for UI"),
            texts=["简体中文", "繁體中文", "English", self.tr("Use system setting")],
            parent=self.personalGroup,
        )

        # 创建组 - 路径
        self.pathGroup = SettingCardGroup(title=self.tr("Path"), parent=self.view)
        self.QQPathCard = PushSettingCard(
            icon=NapCatDesktopIcon.QQ,
            title=self.tr("QQ installation path"),
            content=str(it(PathFunc).getQQPath()),
            text=self.tr("Choose folder"),
            parent=self.pathGroup,
        )
        self.NapCatPathCard = PushSettingCard(
            icon=FluentIcon.GITHUB,
            title=self.tr("NapCat path"),
            content=str(it(PathFunc).getNapCatPath()),
            text=self.tr("Choose folder"),
            parent=self.pathGroup,
        )

    def __setLayout(self) -> None:
        """
        控件布局
        """
        # 将卡片添加到组
        self.startGroup.addSettingCard(self.startOpenHomePageViewCard)

        self.personalGroup.addSettingCard(self.themeCard)
        self.personalGroup.addSettingCard(self.themeColorCard)
        self.personalGroup.addSettingCard(self.languageCard)

        self.pathGroup.addSettingCard(self.QQPathCard)
        self.pathGroup.addSettingCard(self.NapCatPathCard)

        # 添加到布局
        self.expand_layout.addWidget(self.startGroup)
        self.expand_layout.addWidget(self.personalGroup)
        self.expand_layout.addWidget(self.pathGroup)
        self.expand_layout.setContentsMargins(20, 10, 30, 10)
        self.view.setLayout(self.expand_layout)

    def _connect_signal(self) -> None:
        """
        信号处理
        """
        # 连接重启提示
        cfg.appRestartSig.connect(self._showRestartTooltip)

        # 连接启动相关
        self.startOpenHomePageViewCard.optionChanged.connect(
            lambda value: cfg.set(cfg.StartOpenHomePageView, value.value, True)
        )

        # 连接个性化相关
        self.themeCard.optionChanged.connect(self._themeModeChanged)
        self.themeColorCard.colorChanged.connect(lambda color: setThemeColor(color, save=True, lazy=True))

        # 连接路径相关
        self.QQPathCard.clicked.connect(self._onQQFolderCardClicked)
        self.NapCatPathCard.clicked.connect(self._onNapCatFolderCardClicked)

    def _onQQFolderCardClicked(self) -> None:
        """
        选择 QQ 路径的设置卡槽函数
        """
        folder = self._selectFolder()
        if folder:
            cfg.set(cfg.QQPath, folder, save=True)
            self.QQPathCard.setContent(folder)

    def _onNapCatFolderCardClicked(self) -> None:
        """
        选择 NapCat 路径的设置卡槽函数
        """
        folder = self._selectFolder()
        if folder:
            cfg.set(cfg.NapCatPath, folder, save=True)
            self.NapCatPathCard.setContent(folder)

    def _selectFolder(self) -> str:
        """
        选择文件夹的槽函数
        """
        folder = QFileDialog.getExistingDirectory(
            parent=self,
            caption=self.tr("Chosse folder"),
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
        from src.Ui.MainWindow import MainWindow

        setTheme(cfg.get(theme), save=True)
        it(MainWindow).home_widget.updateBgImage()

    def _showRestartTooltip(self) -> None:
        """
        显示重启提示
        """
        InfoBar.success(
            self.tr("Updated successfully"),
            self.tr("Configuration takes effect after restart"),
            orient=Qt.Orientation.Vertical,
            duration=3000,
            parent=self,
        )


class SetupWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.SetupPage.Setup", "SetupWidget"),)

    # 静态方法available()，用于检查模块"Setup"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.SetupPage.Setup")

    # 静态方法create()，用于创建SetupWidget类的实例，返回值为SetupWidget对象。
    @staticmethod
    def create(create_type: [SetupWidget]) -> SetupWidget:
        return SetupWidget()


add_creator(SetupWidgetClassCreator)

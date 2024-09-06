# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from creart import it
from PySide6.QtCore import Qt, QStandardPaths
from qfluentwidgets import (
    InfoBar,
    FluentIcon,
    ScrollArea,
    ExpandLayout,
    PushSettingCard,
    SettingCardGroup,
    OptionsSettingCard,
    ComboBoxSettingCard,
    CustomColorSettingCard,
    setTheme,
    setThemeColor,
)
from PySide6.QtWidgets import QWidget, QFileDialog

from src.Ui.Icon import NapCatDesktopIcon
from src.Core.Config import cfg
from src.Core.Utils.PathFunc import PathFunc

if TYPE_CHECKING:
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

    def _setLayout(self) -> None:
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
        self.expand_layout.setContentsMargins(0, 0, 0, 0)
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
        from src.Ui.MainWindow import MainWindow

        setTheme(cfg.get(theme), save=True)
        it(MainWindow).home_widget.updateBgImage()

    def _showRestartTooltip(self) -> None:
        """
        显示重启提示
        """
        from src.Ui.SetupPage.Setup import SetupWidget

        InfoBar.success(
            self.tr("Updated successfully"),
            self.tr("Configuration takes effect after restart"),
            orient=Qt.Orientation.Vertical,
            duration=3000,
            parent=it(SetupWidget),
        )

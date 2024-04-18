# -*- coding: utf-8 -*-
from abc import ABC
from typing import TYPE_CHECKING, Self

from PySide6.QtCore import Qt
from PySide6.QtGui import QPixmap, QPainter
from PySide6.QtWidgets import QWidget
from creart import add_creator, exists_module, it
from creart.creator import AbstractCreator, CreateTargetInfo
from qfluentwidgets.common import FluentIcon, setTheme, setThemeColor, Theme, isDarkTheme
from qfluentwidgets.components import (
    InfoBar,
    ScrollArea,
    ExpandLayout,
    SettingCardGroup,
    OptionsSettingCard,
    CustomColorSettingCard,
    ComboBoxSettingCard,
    SwitchSettingCard,
    PushSettingCard,
)

from src.core.config import cfg
from src.core.path_func import PathFunc
from src.ui.style_sheet import StyleSheet
from src.ui.icon import NapCatDesktopIcon

if TYPE_CHECKING:
    from src.ui.main_window import MainWindow


class SetupWidget(ScrollArea):

    def __init__(self) -> None:
        """
        初始化
        """
        super().__init__()

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
        self.update_bg_image()
        self.create_config_cards()
        self.connect_signal()
        self.set_layout()

        # 应用样式表
        StyleSheet.SETUP_WIDGET.apply(self)

        return self

    def create_config_cards(self) -> None:
        """
        创建配置项卡片
        """

        # 创建组 - 个性化
        self.personal_group = SettingCardGroup(
            title=self.tr("Personalize"), parent=self.view
        )
        # 创建项
        self.mica_card = SwitchSettingCard(
            configItem=cfg.micaEnabled,
            icon=FluentIcon.TRANSPARENT,
            title=self.tr('Mica effect'),
            content=self.tr('Apply semi transparent to windows and surfaces'),
            parent=self.personal_group
        )
        self.theme_card = OptionsSettingCard(
            configItem=cfg.themeMode,
            icon=FluentIcon.BRUSH,
            title=self.tr("Switch themes"),
            content=self.tr("Switch the theme of the app"),
            texts=[self.tr("Light"), self.tr("Dark"), self.tr("Auto")],
            parent=self.personal_group
        )
        self.theme_color_card = CustomColorSettingCard(
            configItem=cfg.themeColor,
            icon=FluentIcon.PALETTE,
            title=self.tr("Theme Color"),
            content=self.tr("Choose a theme color"),
            parent=self.personal_group
        )
        self.language_card = ComboBoxSettingCard(
            configItem=cfg.language,
            icon=FluentIcon.LANGUAGE,
            title=self.tr("Language"),
            content=self.tr("Set your preferred language for UI"),
            texts=["简体中文", "繁體中文", "English", self.tr("Use system setting")],
            parent=self.personal_group
        )

        # 创建组 - 路径
        self.path_group = SettingCardGroup(
            title=self.tr("Path"), parent=self.view
        )
        self.qq_path_card = PushSettingCard(
            icon=NapCatDesktopIcon.QQ,
            title=self.tr("QQ installation path"),
            content=str(it(PathFunc).get_qq_path()),
            text=self.tr("Choose folder"),
            parent=self.path_group
        )

        self.napcat_path_card = PushSettingCard(
            icon=FluentIcon.GITHUB,
            title=self.tr("NapCat path"),
            content=str(it(PathFunc).get_napcat_path()),
            text=self.tr("Choose folder"),
            parent=self.path_group
        )

    def set_layout(self) -> None:
        """
        控件布局
        """
        # 将卡片添加到组
        self.personal_group.addSettingCard(self.mica_card)
        self.personal_group.addSettingCard(self.theme_card)
        self.personal_group.addSettingCard(self.theme_color_card)
        self.personal_group.addSettingCard(self.language_card)

        self.path_group.addSettingCard(self.qq_path_card)
        self.path_group.addSettingCard(self.napcat_path_card)

        # 添加到布局
        self.expand_layout.addWidget(self.personal_group)
        self.expand_layout.addWidget(self.path_group)
        self.expand_layout.setContentsMargins(15, 5, 15, 5)
        self.view.setLayout(self.expand_layout)

    def connect_signal(self) -> None:
        """
        信号处理
        """
        # 连接重启提示
        cfg.appRestartSig.connect(self.show_restart_tooltip)

        # 连接个性化相关
        self.theme_card.optionChanged.connect(self.theme_mode_changed)
        self.theme_color_card.colorChanged.connect(
            lambda color: setThemeColor(color, save=True, lazy=True)
        )

    @staticmethod
    def theme_mode_changed(theme) -> None:
        """
        主题切换槽函数
        """
        from src.ui.main_window import MainWindow
        setTheme(cfg.get(theme), save=True, lazy=True)
        it(MainWindow).home_widget.update_bg_image()
        it(MainWindow).setup_widget.update_bg_image()

    def show_restart_tooltip(self) -> None:
        """
        显示重启提示
        """
        InfoBar.success(
            self.tr('Updated successfully'),
            self.tr('Configuration takes effect after restart'),
            orient=Qt.Orientation.Vertical,
            duration=1500,
            parent=self
        )

    def update_bg_image(self) -> None:
        """
        用于更新图片大小
        """
        # 重新加载图片保证缩放后清晰
        if not isDarkTheme():
            self.bg_pixmap = QPixmap(":Global/image/Global/page_bg_light.png")
        else:
            self.bg_pixmap = QPixmap(":Global/image/Global/page_bg_dark.png")

        self.bg_pixmap = self.bg_pixmap.scaled(
            self.size(),
            aspectMode=Qt.AspectRatioMode.KeepAspectRatioByExpanding,  # 等比缩放
            mode=Qt.TransformationMode.SmoothTransformation  # 平滑效果
        )
        self.update()

    def paintEvent(self, event) -> None:
        """
        重写绘制事件绘制背景图片
        """
        painter = QPainter(self.viewport())
        painter.drawPixmap(self.rect(), self.bg_pixmap)
        super().paintEvent(event)

    def resizeEvent(self, event) -> None:
        """
        重写缩放事件
        """
        self.update_bg_image()
        super().resizeEvent(event)


class SetupWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.ui.setup_page.setup_widget", "SetupWidget"),)

    # 静态方法available()，用于检查模块"SetupWidget"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.ui.setup_page.setup_widget")

    # 静态方法create()，用于创建SetupWidget类的实例，返回值为SetupWidget对象。
    @staticmethod
    def create(create_type: [SetupWidget]) -> SetupWidget:
        return SetupWidget()


add_creator(SetupWidgetClassCreator)

# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from qfluentwidgets import (
    SpinBox,
    ComboBox,
    BodyLabel,
    FluentIcon,
    ScrollArea,
    ExpandLayout,
    SwitchButton,
    SettingCardGroup,
    OptionsSettingCard,
    CustomColorSettingCard,
    ExpandGroupSettingCard,
    setTheme,
    setThemeColor,
)
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QHBoxLayout

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
        # self.startGroup = SettingCardGroup(title=self.tr("启动项"), parent=self.view)

        # 创建组 - 个性化
        self.personalGroup = SettingCardGroup(title=self.tr("个性化"), parent=self.view)
        # 创建项
        self.themeCard = OptionsSettingCard(
            configItem=cfg.themeMode,
            icon=FluentIcon.BRUSH,
            title=self.tr("切换主题"),
            content=self.tr("切换程序的主题"),
            texts=[self.tr("日间主题"), self.tr("暗夜主题"), self.tr("跟随系统")],
            parent=self.personalGroup,
        )
        self.themeColorCard = CustomColorSettingCard(
            configItem=cfg.themeColor,
            icon=FluentIcon.PALETTE,
            title=self.tr("主题颜色"),
            content=self.tr("选择主题色"),
            parent=self.personalGroup,
        )
        # self.languageCard = ComboBoxSettingCard(
        #     configItem=cfg.Language,
        #     icon=FluentIcon.LANGUAGE,
        #     title=self.tr("语言"),
        #     content=self.tr("设置程序的首选语言"),
        #     texts=["简体中文", "繁體中文", self.tr("Use system setting")],
        #     parent=self.personalGroup,
        # )
        self.titleTabBarSettingCard = TitleTabBarSettingCard(self)

        # 创建组 - 路径
        # self.pathGroup = SettingCardGroup(title=self.tr("Path"), parent=self.view)

    def _setLayout(self) -> None:
        """
        控件布局
        """
        # 将卡片添加到组

        self.personalGroup.addSettingCard(self.themeCard)
        self.personalGroup.addSettingCard(self.themeColorCard)
        # self.personalGroup.addSettingCard(self.languageCard)
        self.personalGroup.addSettingCard(self.titleTabBarSettingCard)

        # 添加到布局
        # self.expand_layout.addWidget(self.startGroup)
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

        # 连接个性化相关
        self.themeCard.optionChanged.connect(self._themeModeChanged)
        self.themeColorCard.colorChanged.connect(lambda color: setThemeColor(color, save=True, lazy=True))

    @staticmethod
    def _themeModeChanged(theme) -> None:
        """
        主题切换槽函数
        """
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow

        setTheme(cfg.get(theme), save=True)
        it(MainWindow).home_widget.updateBgImage()


class TitleTabBarSettingCard(ExpandGroupSettingCard):
    """
    ## 标题选项卡配置项
    """

    def __init__(self, parent) -> None:
        super().__init__(
            icon=FluentIcon.TAG,
            title=self.tr("标题选项卡配置"),
            content=self.tr("设置标题选项卡的开关和行为"),
            parent=parent
        )

        # 第一组
        self.switchLabel = BodyLabel(self.tr("启用标题选项卡"))
        self.switchButton = SwitchButton()
        self.switchButton.setFixedWidth(135)

        # 第二组
        self.setMovableLabel = BodyLabel(self.tr("启用拖动标签页"))
        self.setMovableButton = SwitchButton()
        self.setMovableButton.setFixedWidth(135)

        # 第三组
        self.setScrollableLabel = BodyLabel(self.tr("启用标签页范围可滚动"))
        self.setScrollableButton = SwitchButton()
        self.setScrollableButton.setFixedWidth(135)

        # 第四组
        self.setTabShadowEnabledLabel = BodyLabel(self.tr("启用标签页阴影"))
        self.setTabShadowEnabledButton = SwitchButton()
        self.setTabShadowEnabledButton.setFixedWidth(135)

        # 第五组
        self.setCloseButtonLabel = BodyLabel(self.tr("关闭按钮显示方式"))
        self.setCloseButtonComboBox = ComboBox()
        self.setCloseButtonComboBox.addItems([self.tr("始终显示"), self.tr("悬停显示"), self.tr("从不显示")])
        self.setCloseButtonComboBox.setFixedWidth(135)

        # 第六组
        self.setTabMaximumWidthLabel = BodyLabel(self.tr("标签最大宽度"))
        self.setTabMaximumWidthSpinBox = SpinBox()
        self.setTabMaximumWidthSpinBox.setRange(64, 200)
        self.setTabMaximumWidthSpinBox.setFixedWidth(135)

        # 第七组
        self.setTabMinimumWidthLabel = BodyLabel(self.tr("标签最小宽度"))
        self.setTabMinimumWidthSpinBox = SpinBox()
        self.setTabMinimumWidthSpinBox.setRange(32, 64)
        self.setTabMinimumWidthSpinBox.setFixedWidth(135)

        # 调整内部布局
        self.viewLayout.setContentsMargins(0, 0, 0, 0)
        self.viewLayout.setSpacing(0)

        self.add(self.switchLabel, self.switchButton)
        self.add(self.setMovableLabel, self.setMovableButton)
        self.add(self.setScrollableLabel, self.setScrollableButton)
        self.add(self.setTabShadowEnabledLabel, self.setTabShadowEnabledButton)
        self.add(self.setCloseButtonLabel, self.setCloseButtonComboBox)
        self.add(self.setTabMaximumWidthLabel, self.setTabMaximumWidthSpinBox)
        self.add(self.setTabMinimumWidthLabel, self.setTabMinimumWidthSpinBox)

    def add(self, label, widget) -> None:
        """
        ## 添加组件到内部
        """
        # 创建布局
        layout = QHBoxLayout(QWidget(self))
        layout.setContentsMargins(48, 12, 48, 12)
        layout.parent().setFixedHeight(60)

        # 添加到布局中
        layout.addWidget(label)
        layout.addStretch(1)
        layout.addWidget(widget)

        # 添加组件到选项卡
        self.addGroupWidget(layout.parent())

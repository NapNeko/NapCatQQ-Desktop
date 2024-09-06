# -*- coding: utf-8 -*-
import json
from typing import TYPE_CHECKING, List

from creart import it
from PySide6.QtCore import Qt, Slot
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout
from qfluentwidgets.common import Action, FluentIcon
from qfluentwidgets.components import (
    RoundMenu,
    MessageBox,
    PushButton,
    TitleLabel,
    CaptionLabel,
    SegmentedWidget,
    PrimaryPushButton,
)

from src.Ui.common.info_bar import error_bar, success_bar, warning_bar

if TYPE_CHECKING:
    from src.Ui.AddPage import AddWidget


class AddTopCard(QWidget):
    """
    ## AddWidget 顶部展示的信息, 操作面板
        用于展示切换 view 的 SegmentedWidget
        包括清空配置项的按钮, 添加到列表的按钮, 创建脚本的按钮

    """

    def __init__(self, parent: "AddWidget") -> None:
        super().__init__(parent=parent)
        # 创建所需控件
        self.pivot = SegmentedWidget()
        self.hBoxLayout = QHBoxLayout()
        self.labelLayout = QVBoxLayout()
        self.buttonLayout = QHBoxLayout()

        # 调用方法
        self._createLabel()
        self._createButton()
        self._setLayout()
        self._connectSignal()

    def _createLabel(self) -> None:
        """构建 Label 并配置"""
        self.titleLabel = TitleLabel(self.tr("Add bot"), self)
        self.subtitleLabel = CaptionLabel(self.tr("Before adding a robot, you need to do some configuration"), self)

    def _createButton(self) -> None:
        """构建 Button 并配置"""
        self.clearConfigButton = PushButton(icon=FluentIcon.DELETE, text=self.tr("Clear config"))
        self.psPushButton = PrimaryPushButton(icon=FluentIcon.ADD, text=self.tr("Add to bot list"))

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        # 对 Label 区域进行布局
        self.labelLayout.setSpacing(0)
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.addWidget(self.titleLabel)
        self.labelLayout.addSpacing(5)
        self.labelLayout.addWidget(self.subtitleLabel)
        self.labelLayout.addSpacing(4)
        self.labelLayout.addWidget(self.pivot)

        # 对 Button 区域进行布局
        self.buttonLayout.setSpacing(4)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)
        self.buttonLayout.addWidget(self.clearConfigButton),
        self.buttonLayout.addSpacing(2)
        self.buttonLayout.addWidget(self.psPushButton)
        self.buttonLayout.setAlignment(Qt.AlignmentFlag.AlignBottom | Qt.AlignmentFlag.AlignLeft)

        # 添加到总布局
        self.hBoxLayout.addLayout(self.labelLayout)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addLayout(self.buttonLayout)
        self.hBoxLayout.setContentsMargins(1, 0, 1, 5)

        # 设置页面布局
        self.setLayout(self.hBoxLayout)

    def _connectSignal(self) -> None:
        """
        ## 链接所需信号
        """
        self.clearConfigButton.clicked.connect(self._clearBtnSlot)
        self.psPushButton.clicked.connect(self._addBotListBtnSlot)

    @Slot()
    def _addBotListBtnSlot(self) -> None:
        """
        ## 添加到机器人列表
        先保存到配置文件，然后执行 update 进行刷新
        """
        from src.Core.Utils.PathFunc import PathFunc
        from src.Ui.AddPage.AddWidget import AddWidget
        from src.Core.Config.ConfigModel import Config
        from src.Ui.BotListPage.BotListWidget import BotListWidget

        bot_config_path = it(PathFunc).bot_config_path
        try:
            # 读取配置文件并追加, 判断是否存在相同的 QQID
            config = Config(**it(AddWidget).getConfig())
            with open(str(bot_config_path), "r", encoding="utf-8") as f:
                bot_configs: List[dict] = json.load(f)

            for bot_config in bot_configs:
                # 遍历验证是否存在相同的机器人
                if config.bot.QQID == Config(**bot_config).bot.QQID:
                    error_bar(
                        self.tr("Bots can't be added"),
                        self.tr(f"{config.bot.QQID} it already exists, please do not add it repeatedly"),
                    )
                    return
            # 追加到配置文件, 使用json方法将内部转为json对象, 再用loads方法转为dict对象, 以确保列表内数据一致性
            # 不可以直接使用 dict方法 转为 dict对象, 内部 WebsocketUrl 和 HttpUrl 不会自动转为 str
            bot_configs.append(json.loads(config.json()))

            with open(str(bot_config_path), "w", encoding="utf-8") as f:
                json.dump(bot_configs, f, indent=4)

            # 执行刷新
            it(BotListWidget).botList.updateList()

            success_bar(
                self.tr("Bot addition success!"),
                self.tr(f"Bot({config.bot.QQID}) it has been successfully added, you can view it in BotList"),
            )

        except FileNotFoundError:
            # 如果 json 文件没有被创建则创建一个并写入
            config = Config(**it(AddWidget).getConfig())
            config = [json.loads(config.json())]
            with open(str(bot_config_path), "w", encoding="utf-8") as f:
                json.dump(config, f, indent=4)

        except ValueError as e:
            # 如果用户没有输入必须值，则提示
            error_bar(self.tr("Bots can't be added"), str(e))

    @Slot()
    def _clearBtnSlot(self) -> None:
        """
        ## 清理按钮的槽函数
            用于提示用户是否确认清空(还原)所有已有配置项
        """
        from src.Ui.AddPage import AddWidget

        box = MessageBox(
            title=self.tr("Confirm clearing configuration"),
            content=self.tr(
                "After clearing, all configuration items on this page "
                "will be cleared, and this operation cannot be undone"
            ),
            parent=it(AddWidget),
        )

        if box.exec():
            it(AddWidget).botWidget.clearValues()
            it(AddWidget).connectWidget.clearValues()
            it(AddWidget).advancedWidget.clearValues()
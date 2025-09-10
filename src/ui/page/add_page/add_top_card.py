# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets.common import FluentIcon
from qfluentwidgets.components import (
    CaptionLabel,
    MessageBox,
    PrimaryPushButton,
    SegmentedWidget,
    TitleLabel,
    ToolButton,
)
from PySide6.QtCore import Qt, Slot
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config.operate_config import check_duplicate_bot, update_config
from src.ui.components.info_bar import error_bar, success_bar
from src.ui.components.separator import Separator
from src.ui.page.add_page.signal_bus import add_page_singal_bus

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.page.add_page import AddWidget


class AddTopCard(QWidget):
    """AddWidget 顶部展示的信息, 操作面板

        用于展示切换 view 的 SegmentedWidget
        包括清空配置项的按钮, 添加到列表的按钮

    Attributes:
        pivot (SegmentedWidget): 用于切换 view 的 SegmentedWidget
        h_box_layout (QHBoxLayout): 总布局
        label_layout (QVBoxLayout): Label 区域布局
        button_layout (QHBoxLayout): Button 区域布局
        title_label (TitleLabel): 主标题
        subtitle_label (CaptionLabel): 副标题
        clear_config_button (ToolButton): 清空配置按钮
        add_push_button (PrimaryPushButton): 添加到列表按钮
        separator (Separator): 分割线
        add_connect_config_button (PrimaryPushButton): 添加连接配置按钮
    """

    def __init__(self, parent: "AddWidget") -> None:
        super().__init__(parent=parent)
        # 创建所需控件
        self.pivot = SegmentedWidget()
        self.h_box_layout = QHBoxLayout()
        self.label_layout = QVBoxLayout()
        self.button_layout = QHBoxLayout()

        # 调用方法
        self._create_label()
        self._create_button()
        self._set_layout()
        self._connect_signal()

    def _create_label(self) -> None:
        """构建 Label 并配置"""
        self.title_label = TitleLabel(self.tr("添加机器人"), self)
        self.subtitle_label = CaptionLabel(self.tr("在添加机器人之前，您需要做一些配置"), self)

    def _create_button(self) -> None:
        """构建 Button相关 并配置"""
        self.clear_config_button = ToolButton(FluentIcon.DELETE)
        self.add_push_button = PrimaryPushButton(FluentIcon.ADD, self.tr("添加"))
        self.separator = Separator(self)
        self.add_connect_config_button = PrimaryPushButton(FluentIcon.ADD, self.tr("添加连接配置"), self)

        # 设置一下默认隐藏
        self.separator.hide()
        self.add_connect_config_button.hide()

    def _set_layout(self) -> None:
        """对内部进行布局"""
        # 对 Label 区域进行布局
        self.label_layout.setSpacing(0)
        self.label_layout.setContentsMargins(0, 0, 0, 0)
        self.label_layout.addWidget(self.title_label)
        self.label_layout.addSpacing(5)
        self.label_layout.addWidget(self.subtitle_label)
        self.label_layout.addSpacing(4)
        self.label_layout.addWidget(self.pivot)

        # 对 Button 区域进行布局
        self.button_layout.setSpacing(4)
        self.button_layout.setContentsMargins(0, 0, 0, 0)
        self.button_layout.addWidget(self.clear_config_button),
        self.button_layout.addSpacing(2)
        self.button_layout.addWidget(self.add_push_button)
        self.button_layout.addSpacing(2)
        self.button_layout.addWidget(self.separator)
        self.button_layout.addSpacing(2)
        self.button_layout.addWidget(self.add_connect_config_button)
        self.button_layout.setAlignment(Qt.AlignmentFlag.AlignBottom | Qt.AlignmentFlag.AlignLeft)

        # 添加到总布局
        self.h_box_layout.addLayout(self.label_layout)
        self.h_box_layout.addStretch(1)
        self.h_box_layout.addLayout(self.button_layout)
        self.h_box_layout.setContentsMargins(1, 0, 1, 5)

        # 设置页面布局
        self.setLayout(self.h_box_layout)

    def _connect_signal(self) -> None:
        """链接所需信号"""
        self.clear_config_button.clicked.connect(self._on_clear_button)
        self.add_push_button.clicked.connect(self._on_add_bot_list_button)
        add_page_singal_bus.add_widget_view_change_signal.connect(self._on_button_visiable)
        self.add_connect_config_button.clicked.connect(
            add_page_singal_bus.add_connect_config_button_clicked_signal.emit
        )

    @Slot()
    def _on_add_bot_list_button(self) -> None:
        """添加到机器人列表"""
        # 项目内模块导入
        from src.core.config.config_model import Config
        from src.ui.page.add_page.add_widget import AddWidget
        from src.ui.page.bot_list_page.bot_list_widget import BotListWidget

        # 读取配置文件并追加, 判断是否存在相同的 QQID
        config = Config(**AddWidget().get_config())

        if check_duplicate_bot(config):
            # 检查是否已存在相同的机器人配置
            error_bar(
                self.tr(f"{config.bot.qq_id} 已存在, 请重新输入"),
            )
            return

        if update_config(config):
            # 更新配置文件, 如果返回为 True 则代表更新成功
            # 执行刷新
            BotListWidget().botList.updateList()
            success_bar(self.tr(f"Bot({config.bot.qq_id}) 已经添加成功，你可以在 机器人列表 中查看😼"))
        else:
            # 更新失败则提示查看日志
            error_bar(self.tr("更新配置文件时引发错误, 请前往 设置 > log 中查看详细错误"))

    @Slot()
    def _on_clear_button(self) -> None:
        """清理按钮的槽函数, 用于提示用户是否确认清空(还原)所有已有配置项"""
        # 项目内模块导入
        from src.ui.page.add_page import AddWidget
        from src.ui.window.main_window import MainWindow

        box = MessageBox(
            title=self.tr("确认清除配置"),
            content=self.tr("清空后，该页面的所有配置项都会被清空，且该操作无法撤销"),
            parent=MainWindow(),
        )

        if box.exec():
            AddWidget().bot_widget.clear_values()
            AddWidget().connect_widget.clear_values()
            AddWidget().advanced_widget.clearValues()

    @Slot(int)
    def _on_button_visiable(self, index) -> None:
        """设置按钮的激活状态"""

        # 判断当前再哪一页,然后决定按钮是否显示
        match index:
            case 0 | 2:
                self.add_push_button.setVisible(True)
                self.clear_config_button.setVisible(True)
                self.separator.setVisible(False)
                self.add_connect_config_button.setVisible(False)
            case 1:
                self.add_push_button.setVisible(True)
                self.clear_config_button.setVisible(True)
                self.separator.setVisible(True)
                self.add_connect_config_button.setVisible(True)
            case _:
                pass

# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING, Optional

# 第三方库导入
from qfluentwidgets import BodyLabel, CardWidget, ImageLabel, ToolTipFilter, setFont
from PySide6.QtCore import Qt, QUrl, QUrlQuery, Slot
from PySide6.QtGui import QIcon, QPixmap
from PySide6.QtNetwork import QNetworkAccessManager, QNetworkReply, QNetworkRequest
from PySide6.QtWidgets import QVBoxLayout

# 项目内模块导入
from src.core.config.config_model import Config
from src.core.network.urls import Urls
from src.ui.components.info_bar import error_bar

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.page.bot_list_page.bot_list import BotList
    from src.ui.page.bot_list_page.bot_widget import BotWidget


class BotCard(CardWidget):
    """机器人配置卡片控件，用于在 BotList 中展示单个机器人的配置信息

    Args:
        config: 机器人配置数据
        parent: 父控件，默认为 None
    """

    def __init__(self, config: Config, parent: "BotList" = None) -> None:
        """初始化机器人卡片

        Args:
            config: 机器人配置数据
            parent: 父控件，默认为 None
        """
        super().__init__(parent=parent)
        self.config = config
        self.bot_widget: Optional[BotWidget] = None

        self._setup_ui()
        self._qq_avatar()
        self._info_label()
        self._set_layout()

    def _setup_ui(self) -> None:
        """初始化卡片UI并设置基本参数"""
        self.setFixedSize(190, 230)
        self.vBoxLayout = QVBoxLayout(self)

        self.clicked.connect(self._on_click)

        # 如果配置为自动启动，则创建并启动机器人控件
        if self.config.advanced.autoStart:
            # 项目内模块导入
            from src.ui.page.bot_list_page.bot_list_widget import BotListWidget
            from src.ui.page.bot_list_page.bot_widget import BotWidget

            # 创建页面
            self.bot_widget = BotWidget(self.config)
            BotListWidget().view.addWidget(self.bot_widget)

            # 启动机器人
            self.bot_widget.on_run_button()

    def _qq_avatar(self) -> None:
        """创建并加载QQ头像的ImageLabel"""
        self.QQAvatarLabel = ImageLabel(":Global/logo.png", self)
        self.QQAvatarLabel.scaledToHeight(115)
        self.QQAvatarLabel.setBorderRadius(5, 5, 5, 5)

        # 处理QQ头像的URL
        avatar_url: QUrl = Urls.QQ_AVATAR.value
        query = QUrlQuery()
        query.addQueryItem("spec", "640")
        query.addQueryItem("dst_uin", str(self.config.bot.QQID))
        avatar_url.setQuery(query)

        # 创建网络请求获取头像
        request = QNetworkRequest(avatar_url)
        manager = QNetworkAccessManager(self)
        reply = manager.get(request)
        reply.finished.connect(lambda: self._set_avatar(reply))

    def _info_label(self) -> None:
        """创建并设置展示机器人信息的Label"""
        self.idLabel = BodyLabel(f"{self.config.bot.name}", self)
        self.idLabel.setToolTip(self.idLabel.text())
        self.idLabel.setToolTipDuration(1000)
        self.idLabel.installEventFilter(ToolTipFilter(self.idLabel))
        setFont(self.idLabel, 16)

    def _set_layout(self) -> None:
        """设置卡片的布局"""
        self.vBoxLayout.addSpacing(20)
        self.vBoxLayout.addWidget(self.QQAvatarLabel, alignment=Qt.AlignmentFlag.AlignCenter)
        self.vBoxLayout.addSpacing(25)
        self.vBoxLayout.addWidget(self.idLabel, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addStretch(1)
        self.vBoxLayout.setContentsMargins(10, 10, 10, 10)
        self.setLayout(self.vBoxLayout)

    @Slot()
    def _set_avatar(self, replay: QNetworkReply) -> None:
        """设置QQ头像的网络请求完成槽函数

        Args:
            replay: 网络回复对象
        """
        if replay.error() == QNetworkReply.NetworkError.NoError:
            # 如果请求成功则设置头像
            avatar = QPixmap()
            avatar.loadFromData(replay.readAll())
            self.QQAvatarLabel.setImage(avatar)
            self.QQAvatarLabel.scaledToHeight(115)
            self.QQAvatarLabel.setBorderRadius(6, 6, 6, 6)
        else:
            error_bar(self.tr("获取 QQ 头像时引发错误, 请前往 设置 > log 查看错误原因"))

    @Slot()
    def _on_click(self) -> None:
        """卡片点击事件槽函数，处理卡片被点击时的逻辑"""
        # 项目内模块导入
        from src.ui.page.bot_list_page.bot_list_widget import BotListWidget
        from src.ui.page.bot_list_page.bot_widget import BotWidget
        from src.ui.window.main_window.window import MainWindow

        # 添加标签页项
        BotListWidget().top_card.add_item(f"{self.config.bot.name} ({self.config.bot.QQID})")
        BotListWidget().top_card.update_list_button.hide()

        # 如果机器人控件不存在则创建
        if self.bot_widget is None:
            self.bot_widget = BotWidget(self.config)
            BotListWidget().view.addWidget(self.bot_widget)
            BotListWidget().view.setCurrentWidget(self.bot_widget)

            # 在主窗口标题栏添加标签页
            MainWindow().title_bar.tab_bar.addTab(
                f"{self.config.bot.QQID}",
                f"{self.config.bot.name} ({self.config.bot.QQID})",
                QIcon(self.QQAvatarLabel.pixmap()),
                lambda: (MainWindow().switchTo(BotListWidget()), self._on_click()),
            )
        else:
            BotListWidget().view.setCurrentWidget(self.bot_widget)

        # 设置当前选中的标签页
        MainWindow().title_bar.tab_bar.setCurrentTab(f"{self.config.bot.QQID}")

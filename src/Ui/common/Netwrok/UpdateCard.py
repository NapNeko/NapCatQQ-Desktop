# -*- coding: utf-8 -*-
import random
from typing import Optional
from pathlib import Path

from creart import it
from PySide6.QtGui import QFont, QColor
from PySide6.QtCore import Qt, Slot, QSize
from qfluentwidgets import (
    Flyout,
    BodyLabel,
    FluentIcon,
    ImageLabel,
    MessageBox,
    TitleLabel,
    CaptionLabel,
    FlyoutViewBase,
    HyperlinkLabel,
    SimpleCardWidget,
    VerticalSeparator,
    FlyoutAnimationType,
    TransparentToolButton,
    setFont,
)
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout

from src.Core import timer
from src.Core.Config import cfg
from src.Core.NetworkFunc import Urls, NapCatDownloader
from src.Ui.common.info_bar import error_bar
from src.Core.Utils.PathFunc import PathFunc
from src.Core.Utils.GetVersion import GetVersion
from src.Ui.common.Netwrok.DownloadCard import NapCatInstallWorker
from src.Ui.common.InfoCard.UpdateLogCard import UpdateLogCard
from src.Ui.common.Netwrok.DownloadButton import ProgressBarButton


class UpdateCardBase(SimpleCardWidget):
    """
    ## 用于实现软件中的下载/安装任务
        - 此类为基类, 仅仅实现 Ui 具体功能请继承实现
    """

    def __init__(self, parent=None) -> None:
        """
        ## 初始化控件
        """
        super().__init__(parent=parent)
        # 创建属性
        self.zipFilePath: Optional[Path] = None
        self.ncInstallPath = it(PathFunc).getNapCatPath()

        # 创建控件
        self.iconLabel = ImageLabel(":Global/logo.png", self)
        self.nameLabel = TitleLabel(self)
        self.latestVersionLabel = BodyLabel(self)
        self.updateButton = ProgressBarButton(self.tr("Update"), self)
        self.updateLogButton = TransparentToolButton(FluentIcon.DICTIONARY, self)
        self.companyLabel = HyperlinkLabel(self)

        # 设置控件样式
        self.setFixedHeight(185)
        self.companyLabel.setFixedWidth(100)
        self.iconLabel.scaledToWidth(120)
        self.latestVersionLabel.setFixedSize(180, 40)
        self.latestVersionLabel.hide()

        setFont(self.nameLabel, 24, QFont.Weight.DemiBold)
        setFont(self.companyLabel, 12)

        # 创建布局
        self.viewLayout = QHBoxLayout()  # 总布局
        self.leftLayout = QVBoxLayout()  # 左侧内容总布局

        self.nameLayout = QVBoxLayout()  # 标题和超链接标签的布局
        self.buttonLayout = QHBoxLayout()  # 按钮的布局
        self.topLayout = QHBoxLayout()  # titleLayout 和 buttonLayout 的布局
        self.infoLayout = QHBoxLayout()  # 信息控件的布局

    def _setLayout(self) -> None:
        """
        ## 设置内部控件的布局
        """
        # nameLayout 以及 buttonLayout
        self.nameLayout.setContentsMargins(0, 0, 0, 0)
        self.nameLayout.setSpacing(4)
        self.nameLayout.addWidget(self.nameLabel)
        self.nameLayout.addWidget(self.companyLabel)

        self.buttonLayout.setContentsMargins(0, 0, 0, 0)
        self.buttonLayout.addWidget(self.updateLogButton)
        self.buttonLayout.addWidget(self.updateButton)
        self.buttonLayout.addWidget(self.latestVersionLabel)

        self.topLayout.addLayout(self.nameLayout)
        self.topLayout.addStretch(1)
        self.topLayout.addLayout(self.buttonLayout)

        # leftLayout
        self.leftLayout.setContentsMargins(0, 0, 0, 0)
        self.leftLayout.setSpacing(10)
        self.leftLayout.addLayout(self.topLayout)
        self.leftLayout.addLayout(self.infoLayout)

        # viewLayout
        self.viewLayout.addSpacing(30)
        self.viewLayout.addWidget(self.iconLabel)
        self.viewLayout.addSpacing(35)
        self.viewLayout.addLayout(self.leftLayout)
        self.viewLayout.setContentsMargins(24, 24, 24, 24)

        self.setLayout(self.viewLayout)


class InfoWidget(QWidget):
    """
    ## 用于显示 UpdateCardBase 上的信息
    """

    def __init__(self, title: str, value: str, parent=None) -> None:
        super().__init__(parent=parent)
        self.titleLabel = CaptionLabel(title, self)
        self.valueLabel = BodyLabel(value, self)
        self.vBoxLayout = QVBoxLayout(self)

        self.vBoxLayout.setContentsMargins(8, 0, 8, 0)
        self.vBoxLayout.addWidget(self.valueLabel, 0, Qt.AlignmentFlag.AlignTop)
        self.vBoxLayout.addWidget(self.titleLabel, 0, Qt.AlignmentFlag.AlignBottom)

        setFont(self.valueLabel, 16, QFont.Weight.DemiBold)
        self.setFixedHeight(35)
        self.titleLabel.setTextColor(QColor(96, 96, 96), QColor(206, 206, 206))

    def setText(self, text: str):
        self.valueLabel.setText(text)


class NapCatUpdateCard(UpdateCardBase):
    """
    ## 实现 NapCat 的更新卡片
    """

    def __init__(self, parent=None) -> None:
        """
        ## 初始化控件
        """
        super().__init__(parent=parent)
        self.isInstall = False

        self._log = "Unknown"
        self.zipFilePath: Optional[Path] = None
        self.ncInstallPath = it(PathFunc).getNapCatPath()

        # 创建控件
        self.downloader = NapCatDownloader(Urls.NAPCAT_DOWNLOAD.value, it(PathFunc).tmp_path)
        self.versionWidget = InfoWidget(self.tr("Version"), self.tr("Unknown"), self)
        self.platformWidget = InfoWidget(self.tr("Platform"), cfg.get(cfg.PlatformType), self)
        self.systemWidget = InfoWidget(self.tr("System"), cfg.get(cfg.SystemType), self)

        # 调整控件
        self.updateButton.clicked.connect(self._updateButtonSlot)
        self.downloader.progressBarToggle.connect(self.switchProgressBar)
        self.downloader.downloadProgress.connect(self.updateButton.setValue)
        self.downloader.errorFinsh.connect(self.showErrorTips)
        self.downloader.downloadFinish.connect(self._downloadFinishSlot)
        self.nameLabel.setText("NapCatQQ")
        self.companyLabel.setUrl(Urls.NAPCATQQ_REPO.value)
        self.companyLabel.setText(self.tr("Project repositories"))
        self.updateLogButton.clicked.connect(self._updateLogButtonSlot)

        # 设置布局
        self.infoLayout.addWidget(self.versionWidget)
        self.infoLayout.addWidget(VerticalSeparator(self))
        self.infoLayout.addWidget(self.platformWidget)
        self.infoLayout.addWidget(VerticalSeparator(self))
        self.infoLayout.addWidget(self.systemWidget)
        self.infoLayout.addStretch(1)
        self.versionWidget.vBoxLayout.setContentsMargins(0, 0, 8, 0)

        # 调用方法
        self.checkForUpdates()
        self._setLayout()

    @Slot()
    def _updateButtonSlot(self):
        """
        ## 更新按钮槽函数
        """
        from src.Ui.HomePage.Home import HomeWidget
        from src.Ui.BotListPage.BotListWidget import BotListWidget

        # 检查是否有 bot 正在运行, 如果有则提示
        if it(BotListWidget).getBotIsRun():
            box = MessageBox(
                self.tr("Stop NapCat"),
                self.tr(
                    "A robot sample has been detected to be running, "
                    "and you will need to stop all robots to update NapCat"
                ),
                it(HomeWidget),
            )
            if not box.exec():
                return
            it(BotListWidget).stopAllBot()

        # 开始下载操作
        self.downloader.start()

    @Slot(bool)
    def _downloadFinishSlot(self):
        """
        ## 下载完成后的安装操作
        """
        self.installWorker = NapCatInstallWorker(it(PathFunc).tmp_path / self.downloader.url.fileName())
        self.installWorker.installFinished.connect(self._installationFinished)
        self.installWorker.errorFinished.connect(self.showErrorTips)
        self.installWorker.progressBarToggle.connect(self.switchProgressBar)
        self.installWorker.start()

    @Slot(bool)
    def _installationFinished(self) -> None:
        """
        ## 下载完成后的安装操作
        """
        self.updateButton.hide()
        self.updateLogButton.hide()
        self.latestVersionLabel.show()

    @Slot()
    def _updateLogButtonSlot(self):
        """
        ## 显示更新日志按钮的槽函数
        """
        Flyout.make(UpdateFlyoutView(self._log), self.updateLogButton, self, aniType=FlyoutAnimationType.DROP_DOWN)

    @timer(86_400_000)
    def checkForUpdates(self):
        """
        ## 检查是否有更新
        """
        local_version = it(GetVersion).getLocalNapCatVersion()
        remote_version = it(GetVersion).getRemoteNapCatVersion()

        if not local_version or not remote_version:
            # 确保两个都获取到的都不是 None
            self.latestVersionLabel.setText(self.tr("The check update failed"))
            self._log = self.tr("Failed to get the changelog, please check the network settings")
            self.versionWidget.setText(self.tr("Unknown"))
            self.updateButton.hide()
            self.latestVersionLabel.show()
            return

        self.versionWidget.setText(remote_version)
        self._log = ""

        if local_version == remote_version:
            self.latestVersionLabel.setText(self.tr("It's already the latest version"))
            self.updateButton.hide()
            self.updateLogButton.hide()
            self.latestVersionLabel.show()

        else:
            self.updateButton.show()
            self.updateLogButton.show()
            self.latestVersionLabel.hide()

    @Slot(int)
    def switchProgressBar(self, mode: int):
        """
        ## 切换进度条样式
            - 0: 进度模式
            - 1: 未知进度模式
            - 2: 文字模式
            - 3: 禁用按钮
            - 4: 解除禁用
        """
        match mode:
            case 0:
                self.updateButton.setProgressBarState(False)
            case 1:
                self.updateButton.setProgressBarState(True)
            case 2:
                self.updateButton.setTestVisible(True)
            case 3:
                self.updateButton.setEnabled(False)
            case 4:
                self.updateButton.setEnabled(True)

    @Slot()
    def showErrorTips(self):
        """
        ## 下载时发生了错误, 提示用户查看 log 以寻求帮助或者解决问题
        """
        error_bar(self.tr("下载 NapCat 发生了错误, 请前往 设置 > log 查看错误原因"))


class UpdateFlyoutView(FlyoutViewBase):

    def __init__(self, log: str, parent=None):
        super().__init__(parent)

        # 创建属性
        self.logTest = log
        self.images = [f":1920_540/image/1920_540/image_{index}.png" for index in range(1, 7)]

        # 创建布局和控件
        self.vBoxLayout = QVBoxLayout(self)
        self.viewLayout = QHBoxLayout()
        self.widgetLayout = QVBoxLayout()

        self.logCard = UpdateLogCard(self)
        self.imageLabel = ImageLabel(self)
        self.closeButton = TransparentToolButton(FluentIcon.CLOSE, self)

        # 调用方法
        self._initWidget()
        self._setLayout()

    def _initWidget(self):
        """
        ## 初始化内部控件
        """
        self.setFixedWidth(300)

        self.imageLabel.setImage(random.choice(self.images))

        self.closeButton.setFixedSize(32, 32)
        self.closeButton.setIconSize(QSize(12, 12))
        self.logCard.setText("unknown")
        self.logCard.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        self.closeButton.clicked.connect(self.hide)

    def _setLayout(self):
        """
        ## 对内部控件进行布局
        """
        self.vBoxLayout.setContentsMargins(1, 1, 1, 1)
        self.widgetLayout.setContentsMargins(0, 8, 0, 8)
        self.viewLayout.setSpacing(4)
        self.widgetLayout.setSpacing(0)
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.addLayout(self.viewLayout)

        # log
        self._adjustText()
        self.widgetLayout.addWidget(self.logCard)
        self.viewLayout.addLayout(self.widgetLayout)

        # close button
        self.viewLayout.addWidget(self.closeButton, 0, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignTop)

        # content margins
        self.viewLayout.setContentsMargins(6, 5, 6, 5)

        # image
        self._adjustImage()
        self._addImageToLayout()

    def _adjustText(self):
        """
        ## 调整文本
        """

        # 调整 log
        self.logCard.setMarkdown(self.logTest)

    def _adjustImage(self):
        """
        ## 调整图像
        """
        w = self.vBoxLayout.sizeHint().width() - 8
        self.imageLabel.scaledToWidth(w)

    def _addImageToLayout(self):
        self.imageLabel.setBorderRadius(8, 8, 0, 0)
        self.vBoxLayout.insertWidget(0, self.imageLabel)

    def showEvent(self, e):
        super().showEvent(e)
        self._adjustImage()
        self.adjustSize()

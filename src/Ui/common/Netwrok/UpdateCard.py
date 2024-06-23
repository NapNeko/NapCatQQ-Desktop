# -*- coding: utf-8 -*-
import random

from pathlib import Path
from creart import it
from typing import Optional

from PySide6.QtCore import Qt, QSize, QUrl, Slot, QThread, Signal, QProcess, QCoreApplication
from PySide6.QtGui import QFont, QColor, QPixmap, QDesktopServices, QCursor
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QApplication, QTextEdit
from qfluentwidgets import (
    SimpleCardWidget, ImageLabel, TitleLabel, HyperlinkLabel, FluentIcon, CaptionLabel, BodyLabel, setFont,
    TransparentToolButton, FlyoutView, Flyout, VerticalSeparator, PushButton, MessageBoxBase, SubtitleLabel,
    FlyoutViewBase, PrimaryPushButton, TextWrap, FlyoutAnimationType, MessageBox
)

from src.Core import timer
from src.Core.NetworkFunc import Urls, Downloader
from src.Core.GetVersion import GetVersion
from src.Core.PathFunc import PathFunc
from src.Core.Config import cfg
from src.Ui.common.InfoCard.UpdateLogCard import UpdateLogCard
from src.Ui.common.Netwrok.DownloadButton import ProgressBarButton
from src.Ui.common.Netwrok.DownloadCard import NapCatInstallWorker


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
        self._timeout = False
        self.isInstall = False
        self.isRun = False

        self._log = "Unknown"
        self.zipFilePath: Optional[Path] = None
        self.ncInstallPath = it(PathFunc).getNapCatPath()

        # 创建控件
        self.downloader = Downloader(self._getNCDownloadUrl(), it(PathFunc).tmp_path)
        self.versionWidget = InfoWidget(self.tr("Version"), self.tr("Unknown"), self)
        self.platformWidget = InfoWidget(self.tr("Platform"), cfg.get(cfg.PlatformType), self)
        self.systemWidget = InfoWidget(self.tr("System"), cfg.get(cfg.SystemType), self)

        # 调整控件
        self.updateButton.clicked.connect(self._updateButtonSlot)
        self.downloader.downloadProgress.connect(self.updateButton.setValue)
        self.downloader.finished.connect(self._install)
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
        self._onTimer()
        self._setLayout()

    @Slot()
    def _updateButtonSlot(self):
        """
        ## 更新按钮槽函数
        """
        from src.Ui.BotListPage.BotListWidget import BotListWidget
        from src.Ui.HomePage.Home import HomeWidget
        # 检查是否有 bot 正在运行, 如果有则提示
        if it(BotListWidget).getBotIsRun():
            box = MessageBox(
                self.tr("Stop NapCat"),
                self.tr(
                    "A robot sample has been detected to be running, "
                    "and you will need to stop all robots to update NapCat"
                ),
                it(HomeWidget)
            )
            if not box.exec():
                return
            it(BotListWidget).stopAllBot()

        # 检查是否正在下载
        if self.isRun:
            # 如果正在下载/安装再点击则是取消操作
            self.downloader.stop()  # 先停止下载
            self.updateButton.setValue(0)
            self.updateButton.setProgressBarState(False)
            self.updateButton.setTestVisible(True)
            self.isRun = False
        else:
            # 反之则开始下载等操作
            self.downloader.start()
            self.zipFilePath = it(PathFunc).tmp_path / self.downloader.url.fileName()
            self.updateButton.setProgressBarState(False)
            self.updateButton.setTestVisible(False)
            self.isRun = True

    @Slot(bool)
    def _install(self, value):
        """
        ## 下载完成后的安装操作
            - value 用于判断是否下载成功
        """
        if value:
            self.isRun = False
            self.updateButton.setProgressBarState(True)
            self.installWorker = NapCatInstallWorker(self.ncInstallPath, self.zipFilePath)
            self.installWorker.finished.connect(self._installationFinished)
            self.installWorker.start()

    @Slot(bool)
    def _installationFinished(self, value: bool) -> None:
        """
        ## 下载完成后的安装操作
            - value 用于判断是否下载成功
        """
        if value:
            self.updateButton.setProgressBarState(False)
            self.updateButton.hide()
            self.updateLogButton.hide()
            self.latestVersionLabel.show()

    @Slot()
    def _updateLogButtonSlot(self):
        """
        ## 显示更新日志按钮的槽函数
        """
        Flyout.make(UpdateFlyoutView(self._log), self.updateLogButton, self, aniType=FlyoutAnimationType.DROP_DOWN)

    @timer(10_000, True)
    def _onTimer(self):
        """
        ## 延时启动计时器, 等待用于等待网络请求
        """
        if not self._timeout:
            self._timeout = True
            return
        self.checkForUpdates()

    @timer(10_000)
    def checkForUpdates(self):
        """
        ## 检查是否有更新
        """
        local_version = it(GetVersion).napcatLocalVersion
        remote_version = it(GetVersion).napcatRemoteVersion

        if not local_version or not remote_version:
            # 确保两个都获取到的都不是 None
            self.latestVersionLabel.setText(self.tr("The check update failed"))
            self._log = self.tr("Failed to get the changelog, please check the network settings")
            self.versionWidget.setText(self.tr("Unknown"))
            self.updateButton.hide()
            self.latestVersionLabel.show()
            return

        self.versionWidget.setText(remote_version)
        self._log = it(GetVersion).napcatUpdateLog

        if local_version == remote_version:
            self.latestVersionLabel.setText(self.tr("It's already the latest version"))
            self.updateButton.hide()
            self.updateLogButton.hide()
            self.latestVersionLabel.show()

        else:
            self.updateButton.show()
            self.updateLogButton.show()
            self.latestVersionLabel.hide()

    @staticmethod
    def _getNCDownloadUrl() -> QUrl:
        """
        ## 通过系统,平台,匹配下载连接并返回
        """
        download_links = {
            ("Linux", "aarch64"): Urls.NAPCAT_ARM64_LINUX.value,
            ("Linux", "x86_64"): Urls.NAPCAT_64_LINUX.value,
            ("Linux", "AMD64"): Urls.NAPCAT_64_LINUX.value,
            ("Windows", "x86_64"): Urls.NAPCAT_WIN.value,
            ("Windows", "AMD64"): Urls.NAPCAT_WIN.value
        }
        return download_links.get((cfg.get(cfg.SystemType), cfg.get(cfg.PlatformType)))


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
        self.viewLayout.addWidget(
            self.closeButton, 0, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignTop
        )

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

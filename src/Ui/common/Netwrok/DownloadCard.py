# -*- coding: utf-8 -*-
import shutil
import zipfile
from pathlib import Path

from creart import it
from typing import Optional

from PySide6.QtCore import Qt, QSize, QUrl, Slot, QThread, Signal
from PySide6.QtGui import QFont, QColor, QPixmap, QDesktopServices
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout
from qfluentwidgets import (
    SimpleCardWidget, ImageLabel, TitleLabel, HyperlinkLabel, FluentIcon, CaptionLabel,
    BodyLabel, setFont, TransparentToolButton, FlyoutView, Flyout, VerticalSeparator, PushButton
)

from src.Core.NetworkFunc import Urls, GetNewVersion, Downloader
from src.Core.GetVersion import GetVersion
from src.Core.PathFunc import PathFunc
from src.Core.Config import cfg
from src.Ui.common.Netwrok.DownloadButton import ProgressBarButton
from src.Ui.Icon import NapCatDesktopIcon as NCDIcon


class DownloadCardBase(SimpleCardWidget):
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
        self.isInstall = False
        self.isRun = False

        # 创建控件
        self.iconLabel = ImageLabel(":Global/logo.png", self)
        self.nameLabel = TitleLabel(self)
        self.openInstallPathButton = PushButton(self.tr("Open file path"), self)
        self.installButton = ProgressBarButton(self)
        self.companyLabel = HyperlinkLabel(self)
        self.descriptionLabel = BodyLabel(self)
        self.shareButton = TransparentToolButton(FluentIcon.SHARE, self)

        # 设置控件样式
        self.setFixedHeight(225)
        self.iconLabel.scaledToWidth(100),
        self.openInstallPathButton.setFixedWidth(140)
        self.descriptionLabel.setWordWrap(True)
        self.shareButton.setFixedSize(32, 32)
        self.shareButton.setIconSize(QSize(14, 14))
        self.openInstallPathButton.hide()
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

        self.buttonLayout.addWidget(self.installButton)
        self.buttonLayout.addWidget(self.openInstallPathButton)
        self.buttonLayout.addWidget(self.shareButton)

        self.topLayout.addLayout(self.nameLayout)
        self.topLayout.addLayout(self.buttonLayout)

        # leftLayout
        self.leftLayout.setContentsMargins(0, 0, 0, 0)
        self.leftLayout.setSpacing(10)
        self.leftLayout.addLayout(self.topLayout)
        self.leftLayout.addLayout(self.infoLayout)
        self.leftLayout.addWidget(self.descriptionLabel)

        # viewLayout
        self.viewLayout.addSpacing(30)
        self.viewLayout.addWidget(self.iconLabel)
        self.viewLayout.addSpacing(35)
        self.viewLayout.addLayout(self.leftLayout)
        self.viewLayout.setContentsMargins(24, 24, 24, 24)

        self.setLayout(self.viewLayout)


class InfoWidget(QWidget):
    """
    ## 用于显示 DownloadCardBase 上的信息
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


class NapCatDownloadCard(DownloadCardBase):
    """
    ## 实现 NapCat 的下载卡片
    """

    def __init__(self, parent=None) -> None:
        """
        ## 初始化控件
        """
        super().__init__(parent=parent)
        self.zipFilePath: Optional[Path] = None
        self.isInstall = False
        self.ncInstallPath = it(PathFunc).getNapCatPath()

        # 创建控件
        self.downloader = Downloader(self._getNCDownloadUrl(), it(PathFunc).tmp_path)
        if (version := it(GetNewVersion).getNapCatVersion()) is None:
            version = self.tr("Unknown")
        self.versionWidget = InfoWidget(self.tr("Version"), version, self)
        self.platformWidget = InfoWidget(self.tr("Platform"), cfg.get(cfg.PlatformType), self)
        self.systemWidget = InfoWidget(self.tr("System"), cfg.get(cfg.SystemType), self)

        # 调整控件
        self.installButton.clicked.connect(self._installButtonSlot)
        self.downloader.downloadProgress.connect(self.installButton.setValue)
        self.downloader.finished.connect(self._install)
        self.nameLabel.setText("NapCatQQ")
        self.companyLabel.setUrl(Urls.NAPCATQQ_REPO.value)
        self.companyLabel.setText(self.tr("Project repositories"))
        self.descriptionLabel.setText(self.tr(
            "NapCatQQ is a headless bot framework based on the PC NTQQ core. "
            "The name implies 'Sleepy Cat QQ', running in the background with low "
            "resource usage, like it's asleep, and without the need for a GUI "
            "interface for NTQQ."
        ))
        self.shareButton.clicked.connect(self._shareButtonSlot)
        self.openInstallPathButton.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(str(it(PathFunc).getNapCatPath())))
        )

        # 设置布局
        self.infoLayout.addWidget(self.versionWidget)
        self.infoLayout.addWidget(VerticalSeparator(self))
        self.infoLayout.addWidget(self.platformWidget)
        self.infoLayout.addWidget(VerticalSeparator(self))
        self.infoLayout.addWidget(self.systemWidget)
        self.infoLayout.addStretch(1)
        self.versionWidget.vBoxLayout.setContentsMargins(0, 0, 8, 0)

        self._checkInstall()
        self._setLayout()

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

    @Slot()
    def _installButtonSlot(self) -> None:
        """
        ## 安装按钮槽函数
        """
        if self.isRun:
            # 如果正在下载/安装再点击则是取消操作
            self.downloader.stop()  # 先停止下载
            self.installButton.setValue(0)
            self.installButton.setProgressBarState(False)
            self.installButton.setTestVisible(True)
            self.isRun = False
        else:
            # 反之则开始下载等操作
            self.downloader.start()
            self.zipFilePath = it(PathFunc).tmp_path / self.downloader.url.fileName()
            self.installButton.setProgressBarState(False)
            self.installButton.setTestVisible(False)
            self.isRun = True

    @Slot(bool)
    def _install(self, value: bool) -> None:
        """
        ## 下载完成后的安装操作
            - value 用于判断是否下载成功
        """
        if value:
            self.isRun = False
            self.installButton.setProgressBarState(True)
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
            self.installButton.setProgressBarState(False)
            self.installButton.hide()
            self.openInstallPathButton.show()

    def _checkInstall(self) -> None:
        """
        ## 检查是否安装
        """
        if it(GetVersion).getNapCatVersion():
            self.installButton.hide()
            self.openInstallPathButton.show()
            self.isInstall = True

    @Slot()
    def _shareButtonSlot(self) -> None:
        """
        ## 分享按钮的槽函数
        """
        shareView = FlyoutView(
            title=self.tr("What are you doing ?"),
            content=self.tr(
                "Please do not promote NapCatQQ in irrelevant places,\n"
                "this project is only for learning node-related knowledge,\n"
                "and should not be used for illegal purposes"
            ),
            isClosable=True,
            image=":Global/image/Global/image_1.jpg"
        )
        view = Flyout.make(shareView, self.shareButton, self)
        shareView.closed.connect(view.close)


class NapCatInstallWorker(QThread):
    finished = Signal(bool)

    def __init__(self, ncInstallPath, zipFilePath, parent=None):
        super().__init__(parent)
        self.ncInstallPath = ncInstallPath
        self.zipFilePath = zipFilePath

    def run(self) -> None:
        try:
            self._rmOldFile()
            self._unzipFile()
            self.finished.emit(True)
        except Exception as e:
            print(f'Error: {e}')
            self.finished.emit(False)

    def _rmOldFile(self) -> None:
        """
        ## 移除老文件(如果有)
        """
        if not self.ncInstallPath.exists():
            # 检查路径是否存在, 不存在则创建
            self.ncInstallPath.mkdir(parents=True, exist_ok=True)

        # 遍历目录中的所有项
        for item in self.ncInstallPath.iterdir():
            # 如果是config目录，则跳过
            if item.is_dir() and item.name == 'config':
                continue
            # 如果是文件或其他目录，则删除
            shutil.rmtree(item) if item.is_dir() else item.unlink()

    def _unzipFile(self) -> None:
        """
        ## 解压到临时目录并移动到安装目录
        """
        # 解压缩文件到临时目录
        with zipfile.ZipFile(str(self.zipFilePath), 'r') as zip_ref:
            zip_ref.extractall(str(it(PathFunc).tmp_path))

        # 记录没有被移动的文件和文件夹
        skipped_items = []

        # 获取临时目录中所有文件和文件夹
        for item in (self.zipFilePath.parent / self.zipFilePath.stem).iterdir():
            target_path = self.ncInstallPath / item.name

            if target_path.exists():
                # 跳过同名文件或文件夹
                skipped_items.append(item)
                continue

            # 移动文件或文件夹
            shutil.move(str(item), str(self.ncInstallPath))

        # 删除未移动的文件和文件夹
        for item in skipped_items:
            if item.is_dir():
                shutil.rmtree(item)
            else:
                item.unlink()

        # 删除下载文件和解压出来的文件夹
        shutil.rmtree(self.zipFilePath.parent / self.zipFilePath.stem)
        self.zipFilePath.unlink()


class QQDownloadCard(DownloadCardBase):
    """
    ## 实现 QQ 的下载卡片
    """

    def __init__(self, parent=None) -> None:
        """
        ## 初始化控件
        """
        super().__init__(parent=parent)

        # 调整控件
        self.nameLabel.setText("NTQQ")
        self.iconLabel.setImage(QPixmap(NCDIcon.QQ.path()))
        self.iconLabel.scaledToWidth(100)
        self.companyLabel.setUrl(Urls.QQ_OFFICIAL_WEBSITE.value)
        self.companyLabel.setText(self.tr("QQ official website"))
        self.descriptionLabel.setText(self.tr(
            "NapCatQQ is a headless bot framework based on the PC NTQQ core, "
            "so you will need to install it."
        ))
        self.shareButton.clicked.connect(self._shareButtonSlot)

        # 创建控件
        if (version := it(GetNewVersion).getQQVersion()) is None:
            version = self.tr("Unknown")
        self.versionWidget = InfoWidget(self.tr("Version"), version, self)
        self.platformWidget = InfoWidget(self.tr("Platform"), cfg.get(cfg.PlatformType), self)
        self.systemWidget = InfoWidget(self.tr("System"), cfg.get(cfg.SystemType), self)

        # 设置布局
        self.infoLayout.addWidget(self.versionWidget)
        self.infoLayout.addWidget(VerticalSeparator(self))
        self.infoLayout.addWidget(self.platformWidget)
        self.infoLayout.addWidget(VerticalSeparator(self))
        self.infoLayout.addWidget(self.systemWidget)
        self.infoLayout.addStretch(1)
        self.versionWidget.vBoxLayout.setContentsMargins(0, 0, 8, 0)

        self._setLayout()

    @Slot()
    def _shareButtonSlot(self) -> None:
        """
        ## 分享按钮的槽函数
        """
        shareView = FlyoutView(
            title=self.tr("What are you doing ?"),
            content=self.tr(
                "Please do not promote NapCatQQ in irrelevant places,\n"
                "this project is only for learning node-related knowledge,\n"
                "and should not be used for illegal purposes"
            ),
            isClosable=True,
            image=":Global/image/Global/image_1.jpg"
        )
        view = Flyout.make(shareView, self.shareButton, self)
        shareView.closed.connect(view.close)

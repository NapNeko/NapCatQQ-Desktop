# -*- coding: utf-8 -*-
import shutil
import zipfile
from pathlib import Path
from typing import Optional

from PySide6.QtCore import Qt, QSize, QUrl, Slot, QThread, Signal, QProcess, QCoreApplication
from PySide6.QtGui import QFont, QColor, QPixmap, QDesktopServices
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QApplication
from creart import it
from loguru import logger
from qfluentwidgets import (
    SimpleCardWidget, ImageLabel, TitleLabel, HyperlinkLabel, FluentIcon, CaptionLabel, BodyLabel, setFont,
    TransparentToolButton, FlyoutView, Flyout, VerticalSeparator, PushButton, MessageBoxBase, SubtitleLabel
)

from src.Core import timer
from src.Core.Config import cfg
from src.Core.GetVersion import GetVersion
from src.Core.NetworkFunc import Urls, NapCatDownloader, QQDownloader
from src.Core.PathFunc import PathFunc
from src.Ui.Icon import NapCatDesktopIcon as NCDIcon
from src.Ui.common.Netwrok.DownloadButton import ProgressBarButton


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
        self.installButton = ProgressBarButton(self.tr("Install"), self)
        self.companyLabel = HyperlinkLabel(self)
        self.descriptionLabel = BodyLabel(self)
        self.shareButton = TransparentToolButton(FluentIcon.SHARE, self)

        # 设置控件样式
        self.setFixedHeight(225)
        self.iconLabel.scaledToWidth(100)
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

    def setValue(self, text: str):
        self.valueLabel.setText(text)


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

        # 创建控件
        self.downloader = NapCatDownloader(Urls.NAPCAT_DOWNLOAD.value, it(PathFunc).tmp_path)
        self.versionWidget = InfoWidget(self.tr("Version"), self.tr("Unknown"), self)
        self.platformWidget = InfoWidget(self.tr("Platform"), cfg.get(cfg.PlatformType), self)
        self.systemWidget = InfoWidget(self.tr("System"), cfg.get(cfg.SystemType), self)

        # 调整控件
        self.installButton.clicked.connect(self.downloader.start)
        self.downloader.progressBarToggle.connect(self.switchProgressBar)
        self.downloader.downloadProgress.connect(self.installButton.setValue)
        self.downloader.errorFinsh.connect(self.showErrorTips)
        self.downloader.downloadFinish.connect(self._downloadFinishSlot)
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

        self.checkInstall()
        self._setLayout()
        self._onTimer()

    @timer(10_000, True)
    def _onTimer(self):
        """
        ## 延时启动计时器, 等待用于等待网络请求
        """
        self.updateVersion()

    @timer(86_400_000)
    def updateVersion(self):
        """
        ## 更新显示版本和下载器所下载的版本url
        """
        self.versionWidget.setValue(it(GetVersion).napcatRemoteVersion)

    @timer(3000)
    def checkInstall(self) -> None:
        """
        ## 检查是否安装
        """
        if not it(GetVersion).napcatLocalVersion is None:
            self.installButton.hide()
            self.openInstallPathButton.show()
            self.isInstall = True
        else:
            self.installButton.show()
            self.openInstallPathButton.hide()
            self.isInstall = False

    @Slot()
    def _downloadFinishSlot(self) -> None:
        """
        ## 下载完成后的安装操作
        """
        self.installWorker = NapCatInstallWorker(it(PathFunc).tmp_path / self.downloader.url.fileName())
        self.installWorker.installFinished.connect(self._installationFinished)
        self.installWorker.errorFinished.connect(self.showErrorTips)
        self.installWorker.progressBarToggle.connect(self.switchProgressBar)
        self.installWorker.start()

    @Slot()
    def _installationFinished(self) -> None:
        """
        ## 安装完成后的操作
        """
        from src.Ui.HomePage.Home import HomeWidget
        # 下载完成后, 显示打开安装路径按钮
        self.installButton.hide()
        self.openInstallPathButton.show()

        # 提示用户应该修补后使用
        it(HomeWidget).showInfo(
            self.tr("Installation successful!"),
            self.tr(
                "Congratulations on the successful installation,\n"
                "you still need to patch before using it~"
            )
        )

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
                self.installButton.setProgressBarState(False)
            case 1:
                self.installButton.setProgressBarState(True)
            case 2:
                self.installButton.setTestVisible(True)
            case 3:
                self.installButton.setEnabled(False)
            case 4:
                self.installButton.setEnabled(True)

    @Slot()
    def showErrorTips(self):
        """
        ## 下载时发生了错误, 提示用户查看 log 以寻求帮助或者解决问题
        """
        from src.Ui.HomePage.Home import HomeWidget
        it(HomeWidget).showError(
            self.tr("Failed"),
            self.tr("Error sent while downloading/installing NapCat,\nplease go to Setup > log for details")
        )

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
    """
    ## NapCat 安装任务
    """
    # 进度条模式切换 (进度模式: 0 \ 未知进度模式: 1 \ 文字模式: 2)
    progressBarToggle = Signal(int)
    # 安装结束信号
    installFinished = Signal()
    # 发送错误退出信号
    errorFinished = Signal()

    def __init__(self, zipFilePath, parent=None):
        super().__init__(parent)
        self.ncInstallPath = it(PathFunc).getNapCatPath()
        self.zipFilePath = zipFilePath  # it(PathFunc).tmp_path / self.downloader.url.fileName()

    def run(self) -> None:
        try:
            logger.info(f"{'-' * 10} 开始移除旧版 NapCat {'-' * 10}")
            self.progressBarToggle.emit(1)
            self.progressBarToggle.emit(3)
            if not self.ncInstallPath.exists():
                # 检查路径是否存在, 不存在则创建
                self.ncInstallPath.mkdir(parents=True, exist_ok=True)
                logger.warning(f"路径 {self.ncInstallPath} 不存在, 已创建")

            # 遍历 NapCat 文件夹中旧版文件并删除
            for item in self.ncInstallPath.iterdir():
                # 跳过 config 目录保证配置文件不丢失
                if item.is_dir() and item.name == 'config':
                    continue

                # 移除旧版文件
                shutil.rmtree(item) if item.is_dir() else item.unlink()
                logger.info(f"删除文件 {item}")
            logger.info(f"{'-' * 10} 成功移除旧版 NapCat {'-' * 10}")
            logger.info(f"{'-' * 10} 开始解压新版 NapCat {'-' * 10}")

            # 直接解包到 NapCat 目录
            with zipfile.ZipFile(str(self.zipFilePath), 'r') as zip_ref:
                zip_ref.extractall(str(self.ncInstallPath))

            # 修改 NapCat 的 loadNapCat.js 文件
            with open(str(self.ncInstallPath / 'loadNapCat.js'), 'w', encoding='utf-8') as f:
                f.write(f'(async () => {{await import("file:///{(self.ncInstallPath / "napcat.mjs").as_posix()}")}})()')

            # 删除包释放空间
            self.zipFilePath.unlink()
            logger.info(f"{'-' * 10} 成功解压新版 NapCat {'-' * 10}")

        except (zipfile.BadZipFile, PermissionError, FileNotFoundError, Exception) as e:
            logger.error(f"安装 NapCat 时引发 {type(e).__name__}: {e}")
            self.errorFinished.emit()
        else:
            # 没有引发异常
            self.installFinished.emit()
        finally:
            # 无论是否出错,都会重置进度条
            self.progressBarToggle.emit(2)
            self.progressBarToggle.emit(4)


class QQDownloadCard(DownloadCardBase):
    """
    ## 实现 QQ 的下载卡片
    """

    def __init__(self, parent=None) -> None:
        """
        ## 初始化控件
        """
        super().__init__(parent=parent)
        self.installExePath: Path = it(PathFunc).tmp_path / Urls.QQ_WIN_DOWNLOAD.value.fileName()
        self.isInstall = False

        # 创建控件
        self.downloader = QQDownloader(Urls.QQ_WIN_DOWNLOAD.value, it(PathFunc).tmp_path)
        self.versionWidget = InfoWidget(self.tr("Version"), self.tr("9.9.15-27597"), self)
        self.platformWidget = InfoWidget(self.tr("Platform"), cfg.get(cfg.PlatformType), self)
        self.systemWidget = InfoWidget(self.tr("System"), cfg.get(cfg.SystemType), self)

        # 调整控件
        self.installButton.clicked.connect(self.downloader.start)
        self.downloader.progressBarToggle.connect(self.switchProgressBar)
        self.downloader.downloadProgress.connect(self.installButton.setValue)
        self.downloader.errorFinsh.connect(self.showErrorTips)
        self.downloader.downloadFinish.connect(self._downloadFinishSlot)
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

        # 设置布局
        self.infoLayout.addWidget(self.versionWidget)
        self.infoLayout.addWidget(VerticalSeparator(self))
        self.infoLayout.addWidget(self.platformWidget)
        self.infoLayout.addWidget(VerticalSeparator(self))
        self.infoLayout.addWidget(self.systemWidget)
        self.infoLayout.addStretch(1)
        self.versionWidget.vBoxLayout.setContentsMargins(0, 0, 8, 0)

        self.checkInstall()
        self._setLayout()
        # self._onTimer()

    @timer(10_000, True)
    # def _onTimer(self):
    #     """
    #     ## 延时启动计时器, 等待用于等待网络请求
    #     """
    #     self.updateVersion()
    #
    # @timer(86_400_000)
    # def updateVersion(self):
    #     """
    #     ## 更新显示版本和下载器所下载的版本url
    #     """
    #     self.versionWidget.setValue(it(GetVersion).QQRemoteVersion)

    @timer(3000)
    def checkInstall(self) -> None:
        """
        ## 检查是否安装
        """
        if not it(GetVersion).QQLocalVersion is None:
            # 如果获取得到版本则表示已安装
            self.openInstallPathButton.clicked.connect(
                lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(str(it(PathFunc).getQQPath())))
            )
            self.installButton.hide()
            self.openInstallPathButton.show()
            self.isInstall = True
        else:
            self.installButton.show()
            self.openInstallPathButton.hide()
            self.isInstall = False

    @Slot(bool)
    def _downloadFinishSlot(self) -> None:
        """
        ## 下载完成后的安装操作
        """
        # 创建 QProcess 进程
        self.process = QProcess(self)
        self.process.finished.connect(self._installationFinished)

        # 询问用户应该如何安装(手动/静默)
        from src.Ui.HomePage.Home import HomeWidget
        if (box := InstallationMessageBox(it(HomeWidget))).exec():
            # True 为手动安装, 反之为静默安装
            self.process.setProgram(str(self.installExePath))
        else:
            self.process.setProgram(str(self.installExePath))
            self.process.setArguments(["/s"])


        # QProcess 的启动应该是非阻塞的,我不知道为什么到这里变成了阻塞,只能通过processEvents缓解卡顿
        QApplication.processEvents()
        self.process.start()

    @Slot(int, QProcess.ExitStatus)
    def _installationFinished(self, exit_code: int, exit_status: QProcess.ExitStatus) -> None:
        """
        ## 安装完成后的安装操作
        """
        self.installButton.setEnabled(True)
        self.installButton.setProgressBarState(False)
        if exit_status == QProcess.ExitStatus.NormalExit and not (QQPath := it(PathFunc).getQQPath()) is None:
            # 如果进程正常退出, 则检查一次路径是否存在QQ, 存在则发送成功, 否则失败
            self.installButton.hide()
            self.openInstallPathButton.show()
            self.openInstallPathButton.clicked.connect(
                lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(str(QQPath)))
            )
        else:
            logger.error(f"QQ installation failed, exit code: {exit_code}, exit status: {exit_status}")
            self.installButton.setTestVisible(True)

        self.installExePath.unlink()

        # 处理事件循环中的事件
        QCoreApplication.processEvents()

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
                self.installButton.setProgressBarState(False)
            case 1:
                self.installButton.setProgressBarState(True)
            case 2:
                self.installButton.setTestVisible(True)
            case 3:
                self.installButton.setEnabled(False)
            case 4:
                self.installButton.setEnabled(True)

    @Slot()
    def showErrorTips(self):
        """
        ## 下载时发生了错误, 提示用户查看 log 以寻求帮助或者解决问题
        """
        from src.Ui.HomePage.Home import HomeWidget
        it(HomeWidget).showError(
            self.tr("Failed"),
            self.tr("Error sent while downloading/installing QQ,\nplease go to Setup > log for details")
        )

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


class InstallationMessageBox(MessageBoxBase):
    """
    ## 创建对话框询问用户是手动安装还是静默安装
    """

    def __init__(self, parent):
        super().__init__(parent=parent)
        self.titleLabel = SubtitleLabel(self.tr('Manual or automatic installation'), self)
        self.contentsLabel = BodyLabel(
            self.tr(
                "Please click the button below to select the installation method\n\n"
                "If it is a manual installation, the program will open the QQ installation package path\n"
                "If it is an automatic installer, it will perform a silent installation (the installation "
                "path cannot be specified, and the installation GUI will not be displayed, so please wait "
                "patiently for the installation to complete)"
            ),
            self
        )
        self.contentsLabel.setWordWrap(True)

        # 将组件添加到布局中
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.contentsLabel)

        # 设置对话框
        self.widget.setMinimumWidth(400)
        self.yesButton.setText(self.tr("Manual installation"))
        self.cancelButton.setText(self.tr("Silent installation"))

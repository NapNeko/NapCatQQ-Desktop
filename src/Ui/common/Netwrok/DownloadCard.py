# -*- coding: utf-8 -*-
import shutil
import textwrap
import zipfile
from pathlib import Path
from string import Template
from typing import Optional
from urllib import request

from PySide6.QtCore import Qt, QSize, QUrl, Slot, QThread, Signal, QProcess, QCoreApplication
from PySide6.QtGui import QFont, QColor, QPixmap, QDesktopServices
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QApplication
from creart import it
from loguru import logger
from qfluentwidgets import (
    SimpleCardWidget, ImageLabel, TitleLabel, HyperlinkLabel, FluentIcon, CaptionLabel, BodyLabel, setFont,
    TransparentToolButton, FlyoutView, Flyout, VerticalSeparator, PushButton, MessageBoxBase, SubtitleLabel,
    MessageBox
)

from src.Core import timer
from src.Core.Config import cfg
from src.Core.GetVersion import GetVersion
from src.Core.NetworkFunc import Urls, Downloader
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
        self._timeout = False  # 计时器启动标记
        self.zipFilePath: Optional[Path] = None
        self.isInstall = False
        self.ncInstallPath = it(PathFunc).getNapCatPath()

        # 创建控件
        self.downloader = Downloader(self._getNCDownloadUrl(), it(PathFunc).tmp_path)
        self.versionWidget = InfoWidget(self.tr("Version"), self.tr("Unknown"), self)
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

        self.checkInstall()
        self._setLayout()
        self._onTimer()

    @timer(10000, True)
    def _onTimer(self):
        """
        ## 延时启动计时器, 等待用于等待网络请求
        """
        if not self._timeout:
            self._timeout = True
            return
        self.updateVersion()

    @timer(65000)
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

        if self._qqIsInstall():
            # 反之则开始下载等操作
            self.downloader.start()
            self.zipFilePath = it(PathFunc).tmp_path / self.downloader.url.fileName()
            self.installButton.setProgressBarState(False)
            self.installButton.setTestVisible(False)
            self.isRun = True

    def _qqIsInstall(self) -> bool:
        """
        ## 检查 QQ 是否安装, 没安装则提示先安装QQ
            - 9.9.12 修改临时方案需要修改 QQ 代码, 故需要检查 QQ 是否安装
        """
        if not it(GetVersion).QQLocalVersion is None:
            return True

        # 获取为 None 则表示没安装
        msg = MessageBox(self.tr("Installation failed"), self.tr("Please install QQ first"), self.parent().parent())
        msg.show()
        return False

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
        # try:
        self._rmOldFile()
        self._unzipFile()
        self._editQQCode()
        self._fixQQ()
        self.finished.emit(True)
        # except Exception as e:
        #     logger.error(e)
        #     self.finished.emit(False)

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

    @staticmethod
    def _editQQCode() -> None:
        """
        ## 修改 QQ 代码
            - 9.9.12 版本需要
        """
        js_code_template = (
            "const hasNapcatParam = process.argv.includes('--enable-logging');\n"
            "if (hasNapcatParam) {\n"
            "    (async () => {\n"
            "        await import('$module_name');\n"
            "    })();\n"
            "} else {\n"
            "    require('./launcher.node').load('external_index', module);\n"
            "}\n"
        )
        template = Template(js_code_template)
        index_path = it(PathFunc).getQQPath() / r"resources/app/app_launcher/index.js"
        nc_path = "file://{}".format(str(it(PathFunc).napcat_path / 'napcat.mjs').replace('\\', '//'))
        with open(str(index_path), "w", encoding="utf-8") as f:
            f.write(textwrap.dedent(template.substitute(module_name=nc_path).strip()))

    @staticmethod
    def _fixQQ():
        """
        ## 下载修补 QQ 文件
            - 一样是临时方法啦~
            - DLLHijackMethod 仓库地址: https://github.com/LiteLoaderQQNT/QQNTFileVerifyPatch/tree/DLLHijackMethod
        """
        with request.urlopen(Urls.QQ_FIX_64.value.url()) as response:
            data = response.read()

        with open(str(it(PathFunc).getQQPath() / "dbghelp.dll"), "wb") as f:
            f.write(data)


class QQDownloadCard(DownloadCardBase):
    """
    ## 实现 QQ 的下载卡片
    """

    def __init__(self, parent=None) -> None:
        """
        ## 初始化控件
        """
        super().__init__(parent=parent)
        self._timeout = False  # 计时器启动标记
        self.installExePath: Optional[Path] = None
        self.isInstall = False
        self.installMode = False

        # 创建控件
        self.downloader = Downloader(path=it(PathFunc).tmp_path)
        self.versionWidget = InfoWidget(self.tr("Version"), self.tr("Unknown"), self)
        self.platformWidget = InfoWidget(self.tr("Platform"), cfg.get(cfg.PlatformType), self)
        self.systemWidget = InfoWidget(self.tr("System"), cfg.get(cfg.SystemType), self)

        # 调整控件
        self.installButton.clicked.connect(self._installButtonSlot)
        self.downloader.downloadProgress.connect(self.installButton.setValue)
        self.downloader.finished.connect(self._install)
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
        self._onTimer()

    @timer(10000, True)
    def _onTimer(self):
        """
        ## 延时启动计时器, 等待用于等待网络请求
        """
        if not self._timeout:
            self._timeout = True
            return
        self.updateVersion()

    @timer(65000)
    def updateVersion(self):
        """
        ## 更新显示版本和下载器所下载的版本url
        """
        try:
            self.versionWidget.setValue(it(GetVersion).QQRemoteVersion)
            self.downloader.setUrl(it(GetVersion).QQRemoteDownloadUrls[cfg.get(cfg.PlatformType)])
        except TypeError:
            # 解析失败跳过本次解析
            return

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
            self.installMode = InstallationMessageBox(self.parent().parent()).exec()
            self.downloader.start()
            self.installExePath = it(PathFunc).tmp_path / self.downloader.url.fileName()
            self.installButton.setProgressBarState(False)
            self.installButton.setTestVisible(False)
            self.isRun = True

    @Slot(bool)
    def _install(self, value: bool) -> None:
        """
        ## 下载完成后的安装操作
            - value 用于判断是否下载成功
        """
        if not value:
            # 如果下载失败则重置按钮
            self.installButton.setProgressBarState(False)
            self.installButton.setValue(0)
            self.installButton.setTestVisible(True)
            return

        self.isRun = False
        self.installButton.setEnabled(False)
        self.installButton.setProgressBarState(True)

        self.process = QProcess(self)
        self.process.finished.connect(self._installationFinished)

        if self.installMode:
            # 执行静默安装
            self.process.setProgram(str(self.installExePath))
            self.process.setArguments(["/s"])
        else:
            self.process.setProgram(str(self.installExePath))

        # QProcess 的启动应该是非阻塞的,我不知道为什么到这里变成了阻塞,只能通过processEvents缓解卡顿
        QApplication.processEvents()
        self.process.start()

    @Slot(int, QProcess.ExitStatus)
    def _installationFinished(self, exit_code: int, exit_status: QProcess.ExitStatus) -> None:
        """
        ## 下载完成后的安装操作
            - value 用于判断是否下载成功
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

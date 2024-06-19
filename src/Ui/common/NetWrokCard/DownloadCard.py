# -*- coding: utf-8 -*-
import platform

from creart import it

from PySide6.QtCore import Qt, QSize, QUrl
from PySide6.QtGui import QFont, QColor, QPixmap, QDesktopServices
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout
from qfluentwidgets import (
    SimpleCardWidget, ImageLabel, TitleLabel, PrimaryPushButton, HyperlinkLabel,
    FluentIcon, FluentIconBase, CaptionLabel, BodyLabel, setFont, TransparentToolButton,
    FlyoutView, Flyout, VerticalSeparator, PushButton
)

from src.Core.NetworkFunc import Urls, GetNewVersion
from src.Core.GetVersion import GetVersion
from src.Core.PathFunc import PathFunc
from src.Ui.Icon import NapCatDesktopIcon as NCDIcon


class DownloadCardBase(SimpleCardWidget):
    """
    ## 用于实现软件中的下载/安装任务
        - 此类为基类, 仅仅实现 Ui 具体功能请继承实现
    """

    def __init__(self, parent=None):
        """
        ## 初始化控件
        """
        super().__init__(parent=parent)
        # 创建控件
        self.iconLabel = ImageLabel(":Global/logo.png", self)
        self.nameLabel = TitleLabel(self)
        self.openInstallPathButton = PushButton(self.tr("Open file path"), self)
        self.installButton = PrimaryPushButton(self.tr("Install"), self)
        self.companyLabel = HyperlinkLabel(self)
        self.descriptionLabel = BodyLabel(self)
        self.shareButton = TransparentToolButton(FluentIcon.SHARE, self)

        # 设置控件样式
        self.setFixedHeight(225)
        self.iconLabel.scaledToWidth(100),
        self.installButton.setFixedWidth(140)
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

    def _setLayout(self):
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

    def __init__(self, title: str, value: str, parent=None):
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

    def __init__(self, parent=None):
        """
        ## 初始化控件
        """
        super().__init__(parent=parent)
        self.isInstall = False

        # 调整控件
        self.nameLabel.setText("NapCatQQ")
        self.companyLabel.setUrl(Urls.NAPCATQQ_REPO.value)
        self.companyLabel.setText(self.tr("Project repositories"))
        self.descriptionLabel.setText(self.tr(
            "NapCatQQ is a headless bot framework based on the PC NTQQ core. "
            "The name implies 'Sleepy Cat QQ', running in the background with low "
            "resource usage, like it's asleep, and without the need for a GUI "
            "interface for NTQQ."
        ))
        self.shareButton.clicked.connect(self._shareButtonSolt)
        self.openInstallPathButton.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(str(it(PathFunc).getNapCatPath())))
        )

        # 创建控件
        if (version := it(GetNewVersion).getNapCatVersion()) is None:
            version = self.tr("Unknown")
        self.versionWidget = InfoWidget(self.tr("Version"), version, self)
        self.commentWidget = InfoWidget(self.tr("Platform"), platform.machine(), self)
        self.systemWidget = InfoWidget(self.tr("System"), platform.system(), self)

        # 设置布局
        self.infoLayout.addWidget(self.versionWidget)
        self.infoLayout.addWidget(VerticalSeparator(self))
        self.infoLayout.addWidget(self.commentWidget)
        self.infoLayout.addWidget(VerticalSeparator(self))
        self.infoLayout.addWidget(self.systemWidget)
        self.infoLayout.addStretch(1)
        self.versionWidget.vBoxLayout.setContentsMargins(0, 0, 8, 0)

        self._checkInstall()
        self._setLayout()

    def _checkInstall(self):
        """
        ## 检查是否安装
        """
        if it(GetVersion).getNapCatVersion():
            self.installButton.hide()
            self.openInstallPathButton.show()
            self.isInstall = True

    def _shareButtonSolt(self):
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


class QQDownloadCard(DownloadCardBase):
    """
    ## 实现 QQ 的下载卡片
    """

    def __init__(self, parent=None):
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
        self.shareButton.clicked.connect(self._shareButtonSolt)

        # 创建控件
        if (version := it(GetNewVersion).getQQVersion()["windows_version"]) is None:
            version = self.tr("Unknown")
        self.versionWidget = InfoWidget(self.tr("Version"), version, self)
        self.commentWidget = InfoWidget(self.tr("Platform"), platform.machine(), self)
        self.systemWidget = InfoWidget(self.tr("System"), platform.system(), self)

        # 设置布局
        self.infoLayout.addWidget(self.versionWidget)
        self.infoLayout.addWidget(VerticalSeparator(self))
        self.infoLayout.addWidget(self.commentWidget)
        self.infoLayout.addWidget(VerticalSeparator(self))
        self.infoLayout.addWidget(self.systemWidget)
        self.infoLayout.addStretch(1)
        self.versionWidget.vBoxLayout.setContentsMargins(0, 0, 8, 0)

        self._setLayout()

    def _shareButtonSolt(self):
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


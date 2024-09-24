# -*- coding: utf-8 -*-

from creart import it
from PySide6.QtCore import Qt, QPoint
from qfluentwidgets import (
    BodyLabel,
    FluentIcon,
    IconWidget,
    IconInfoBadge,
    ToolTipFilter,
    FluentIconBase,
    StrongBodyLabel,
    InfoBadgeManager,
    SimpleCardWidget,
)
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout

from src.Core import timer
from src.Ui.Icon import NapCatDesktopIcon as NCIcon
from src.Core.Utils.GetVersion import GetVersion


class VersionCardBase(QWidget):
    """
    ## 用于显示版本信息和比对最新版本
    """

    def __init__(self, icon: FluentIconBase, name: str, contents: str, parent=None) -> None:
        """
        ## 初始化控件
        """
        super().__init__(parent=parent)

        # 创建要显示的标签和布局
        self.view = SimpleCardWidget(self)
        self.iconLabel = IconWidget(icon, self.view)
        self.nameLabel = StrongBodyLabel(name, self.view)
        self.contentsLabel = BodyLabel(contents, self.view)
        self.errorBadge = IconInfoBadge.error(FluentIcon.CANCEL_MEDIUM, self, self.view, "Version")
        self.warningBadge = IconInfoBadge.warning(FluentIcon.UP, self, self.view, "Version")

        self.hBoxLayout = QHBoxLayout()
        self.vBoxLayout = QVBoxLayout()

        # 设置 标签 以及 SimpleCardWidget 的一些属性
        self.view.setFixedSize(310, 95)
        self.setFixedSize(self.view.width() + 10, self.view.height() + 10)
        self.view.move(0, self.height() - self.view.height())
        self.iconLabel.setFixedSize(48, 48)
        self.installEventFilter(ToolTipFilter(self))
        self.errorBadge.hide()
        self.warningBadge.hide()

        # 调用方法
        self._setLayout()

    def _setLayout(self) -> None:
        """
        ## 将控件添加到布局
        """

        self.vBoxLayout.setSpacing(2)
        self.vBoxLayout.setContentsMargins(0, 5, 0, 5)
        self.vBoxLayout.addWidget(self.nameLabel, 0, Qt.AlignmentFlag.AlignLeft)
        self.vBoxLayout.addWidget(self.contentsLabel, 0, Qt.AlignmentFlag.AlignLeft)

        self.hBoxLayout.setSpacing(15)
        self.hBoxLayout.setContentsMargins(30, 5, 15, 5)
        self.hBoxLayout.addWidget(self.iconLabel, 0, Qt.AlignmentFlag.AlignLeft)
        self.hBoxLayout.addLayout(self.vBoxLayout, 0)
        self.hBoxLayout.setAlignment(Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft)

        self.view.setLayout(self.hBoxLayout)


class NapCatVersionCard(VersionCardBase):
    """
    ## 实现 NapCat 的版本小卡片
    """

    def __init__(self, parent=None) -> None:
        super().__init__(NCIcon.LOGO, "NapCat Version", "Unknown Version", parent)
        self.updateSate = False  # 是否有更新标记
        self.isInstall = False  # 检查是否有安装 NapCat 的标记, False 表示没有安装
        # 启动时触发一次检查更新
        self.getLocalVersion()
        self.checkUpdates()

    @timer(900_000)
    def checkUpdates(self) -> None:
        """
        ## 检查更新逻辑
        """
        if not self.isInstall:
            # 如果本地没有安装 NapCat 则跳过本次检查
            self.updateSate = False
            return

        if (result := it(GetVersion).checkUpdate()) is None:
            # 如果返回了 None 则代表获取不到远程版本, 添加错误提示
            self.setToolTip(self.tr("无法检查更新, 请检查您的网络连接"))
            self.errorBadge.show()
            self.updateSate = False
            return
        else:
            self.errorBadge.hide()

        if result["result"]:
            # 如果结果为 True 则代表有更新, 添加更新提示
            self.setToolTip(self.tr(f"发现新版本({result['remoteVersion']}), 请及时更新"))
            self.warningBadge.show()
            self.updateSate = True
        else:
            self.warningBadge.hide()

    @timer(3000)
    def getLocalVersion(self) -> None:
        """
        ## 获取本地版本
        """
        if (version := it(GetVersion).getLocalNapCatVersion()) is None:
            self.isInstall = False
            self.contentsLabel.setText("Unknown version")
            self.setToolTip(self.tr("您还没有安装 NapCat，点击这里进入安装页面"))
            self.errorBadge.show()
        else:
            self.isInstall = True
            self.setToolTip("")
            self.errorBadge.hide()
            self.contentsLabel.setText(version)

    def mousePressEvent(self, event):
        """
        ## 重写事件以控制自身点击事件
        """
        super().mousePressEvent(event)

        if not self.isInstall:
            # 如果没有安装则跳转到下载页面
            from src.Ui.HomePage.Home import HomeWidget

            it(HomeWidget).setCurrentWidget(it(HomeWidget).downloadView)
            return

        if self.updateSate:
            # 如果有更新则跳转到更新页面
            from src.Ui.MainWindow.Window import MainWindow

            it(MainWindow).unit_widget_button.click()

    def enterEvent(self, event):
        """
        ## 重写事件以控制鼠标样式
        """
        super().enterEvent(event)
        if self.updateSate or not self.isInstall:
            self.setCursor(Qt.CursorShape.PointingHandCursor)
        else:
            self.setCursor(Qt.CursorShape.ArrowCursor)


class QQVersionCard(VersionCardBase):
    """
    ## 实现显示 QQ版本 小卡片
    """

    def __init__(self, parent=None) -> None:
        super().__init__(NCIcon.QQ, "QQ Version", "Unknown Version", parent)
        self.updateSate = False  # 是否有更新标记
        self.isInstall = False  # 检查是否有安装 QQ 的标记, False 表示没有安装
        self.contentsLabel.setText(self.getLocalVersion())

    @timer(3000)
    def getLocalVersion(self) -> str:
        """
        ## 获取本地版本
        """
        if (version := it(GetVersion).getLocalQQVersion()) is None:
            # 如果没有获取到文件就会返回None, 也就代表QQ没有安装
            self.setToolTip(self.tr("没有找到 QQ 路径，请安装"))
            self.errorBadge.show()
            self.isInstall = False
        else:
            self.isInstall = True

        return version if version else "Unknown version"

    def mousePressEvent(self, event):
        """
        ## 重写事件以控制自身点击事件
        """
        super().mousePressEvent(event)

        if not self.isInstall:
            from src.Ui.HomePage.Home import HomeWidget

            it(HomeWidget).setCurrentWidget(it(HomeWidget).downloadView)

    def enterEvent(self, event):
        """
        ## 重写事件以控制鼠标样式
        """
        super().enterEvent(event)
        if self.updateSate or not self.isInstall:
            self.setCursor(Qt.CursorShape.PointingHandCursor)
        else:
            self.setCursor(Qt.CursorShape.ArrowCursor)


@InfoBadgeManager.register("Version")
class NewVersionInfoBadgeManager(InfoBadgeManager):
    """
    ## 更新图标显示位置调整
    """

    def position(self) -> QPoint:
        pos = self.target.geometry().topRight()
        x = pos.x() - self.badge.width() // 2 - 5
        y = pos.y() - self.badge.height() // 2 + 5
        return QPoint(x, y)

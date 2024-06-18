# -*- coding: utf-8 -*-
from typing import Optional

from PySide6.QtCore import Qt, QPoint, QTimer
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from creart import it
from qfluentwidgets import (
    SimpleCardWidget, FluentIconBase, IconWidget, StrongBodyLabel, BodyLabel, FluentIcon, IconInfoBadge,
    InfoBadgeManager, ToolTipFilter
)

from src.Core.GetVersion import GetVersion
from src.Core.NetworkFunc import GetNewVersion
from src.Ui.Icon import NapCatDesktopIcon as NCIcon


class VersionCardBase(QWidget):
    """
    ## 用于显示版本信息和比对最新版本
    """

    def __init__(self, icon: FluentIconBase, name: str, contents: str, parent=None):
        """
        ## 初始化控件
        """
        super().__init__(parent=parent)
        self.timer: Optional[QTimer] = None

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

    def onCheckUpdates(self) -> None:
        """
        ## 启用检查更新, 这部分请继承实现
        """
        pass

    def _checkUpdates(self) -> None:
        """
        ## 检查更新逻辑函数, 这部分请继承实现
        """
        pass

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

    def __init__(self, parent=None):
        super().__init__(NCIcon.LOGO, "NapCat Version", "Unknown Version", parent)
        self.contentsLabel.setText(self.getLocalVersion())
        self.onCheckUpdates()
        # 启动时触发一次检查更新
        self._checkUpdates()

    def onCheckUpdates(self) -> None:
        """
        ## 实现检查更新
        """
        # 创建 QTimer 对象
        self.timer = QTimer()
        # 将计时器超时信号连接到槽函数
        self.timer.timeout.connect(self._checkUpdates)
        # 设置计时器每隔 300000 毫秒（即 5 分钟）超时一次
        self.timer.start(300000)

    def _checkUpdates(self) -> None:
        """
        ## 检查更新逻辑
        """
        self.warningBadge.hide(), self.errorBadge.hide()
        if self.contentsLabel.text() == "Unknown Version":
            # 如果本地没有安装 NapCat 则跳过本次检查
            return

        if (result := it(GetNewVersion).checkUpdate()) is None:
            # 如果返回了 None 则代表获取不到远程版本, 添加错误提示
            self.setToolTip(self.tr("Unable to check for updates, please check your network connection or feedback"))
            self.errorBadge.show()
            return

        if result["result"]:
            # 如果结果为 True 则代表有更新, 添加更新提示
            self.setToolTip(self.tr(f"Discover new versions({result['remoteVersion']}), please update them in time"))
            self.warningBadge.show()
            return

    def getLocalVersion(self) -> str:
        """
        ## 获取本地版本
        """
        version = it(GetVersion).getNapCatVersion()

        if version is None:
            # 如果没有获取到文件就会返回None, 也就代表NapCat没有安装
            self.setToolTip(self.tr("No NapCat path found, please install it"))
            self.errorBadge.show()

        return version if version else "Unknown version"


class QQVersionCard(VersionCardBase):
    """
    ## 实现显示 QQ版本 小卡片
    """

    def __init__(self, parent=None):
        super().__init__(NCIcon.QQ, "QQ Version", "Unknown Version", parent)
        self.contentsLabel.setText(self.getLocalVersion())

    def getLocalVersion(self) -> str:
        """
        ## 获取本地版本
        """
        version = it(GetVersion).getQQVersion()

        if version is None:
            # 如果没有获取到文件就会返回None, 也就代表QQ没有安装
            self.setToolTip(self.tr("No NapCat path found, please install it"))
            self.errorBadge.show()

        return version if version else "Unknown version"


@InfoBadgeManager.register('Version')
class NewVersionInfoBadgeManager(InfoBadgeManager):
    """
    ## 更新图标显示位置调整
    """

    def position(self) -> QPoint:
        pos = self.target.geometry().topRight()
        x = pos.x() - self.badge.width() // 2 - 5
        y = pos.y() - self.badge.height() // 2 + 5
        return QPoint(x, y)

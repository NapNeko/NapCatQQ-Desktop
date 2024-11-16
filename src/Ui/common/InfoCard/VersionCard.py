# -*- coding: utf-8 -*-
# 第三方库导入
from creart import it
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
    setFont,
)
from PySide6.QtGui import QFont
from PySide6.QtCore import Qt, QPoint
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout

# 项目内模块导入
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
        self.view.setFixedSize(310, 120)
        self.setFixedSize(self.view.width() + 10, self.view.height() + 10)
        self.view.move(0, self.height() - self.view.height())
        self.iconLabel.setFixedSize(64, 64)
        self.installEventFilter(ToolTipFilter(self))
        self.errorBadge.hide()
        self.warningBadge.hide()
        setFont(self.nameLabel, 18, QFont.Weight.Bold)
        setFont(self.contentsLabel, 14, QFont.Weight.Normal)

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
        # 启动时触发一次检查更新
        self.getLocalVersion()
        self.checkUpdates()

    @timer(900_000)
    def checkUpdates(self) -> None:
        """
        ## 检查更新逻辑
        """
        if (local_ver := it(GetVersion).getLocalNapCatVersion()) is None:
            # 如果本地没有安装 NapCat 则跳过本次检查
            return

        if (remote_ver := it(GetVersion).getRemoteNapCatVersion()) is None:
            # 如果返回了 None 则代表获取不到远程版本, 添加错误提示
            self.setToolTip(self.tr("无法检查更新, 请检查您的网络连接"))
            self.errorBadge.show()
            return
        else:
            self.errorBadge.hide()

        if local_ver != remote_ver:
            # 如果结果为 True 则代表有更新, 添加更新提示
            self.setToolTip(self.tr(f"发现新版本({remote_ver}), 请及时更新"))
            self.warningBadge.show()
        else:
            self.warningBadge.hide()

    @timer(10_000)
    def getLocalVersion(self) -> None:
        """
        ## 获取本地版本
        """
        if (version := it(GetVersion).getLocalNapCatVersion()) is None:
            self.contentsLabel.setText("Unknown version")
            self.setToolTip(self.tr("您还没有安装 NapCat，点击这里进入安装页面"))
            self.errorBadge.show()
        else:
            self.setToolTip("")
            self.errorBadge.hide()
            self.contentsLabel.setText(version)

    def mousePressEvent(self, event):
        """
        ## 重写事件以控制自身点击事件
        """
        super().mousePressEvent(event)
        # 跳转到组件界面
        # 项目内模块导入
        from src.Ui.UnitPage.view import UnitWidget
        from src.Ui.MainWindow.Window import MainWindow

        MainWindow().switchTo(UnitWidget())
        UnitWidget().view.setCurrentWidget(UnitWidget().napcatPage)

    def enterEvent(self, event):
        """
        ## 重写事件以控制鼠标样式
        """
        super().enterEvent(event)
        self.setCursor(Qt.CursorShape.PointingHandCursor)


class QQVersionCard(VersionCardBase):
    """
    ## 实现显示 QQ版本 小卡片
    """

    def __init__(self, parent=None) -> None:
        super().__init__(NCIcon.QQ, "QQ Version", "Unknown Version", parent)
        self.updateSate = False  # 是否有更新标记
        self.contentsLabel.setText(self.getLocalVersion())

    @timer(10_000)
    def getLocalVersion(self) -> str:
        """
        ## 获取本地版本
        """
        if (version := it(GetVersion).getLocalQQVersion()) is None:
            # 如果没有获取到文件就会返回None, 也就代表QQ没有安装
            self.setToolTip(self.tr("没有找到 QQ 路径，请安装"))
            self.errorBadge.show()

        return version if version else "Unknown version"

    def mousePressEvent(self, event):
        """
        ## 重写事件以控制自身点击事件
        """
        super().mousePressEvent(event)
        # 跳转到组件界面
        # 项目内模块导入
        from src.Ui.UnitPage.view import UnitWidget
        from src.Ui.MainWindow.Window import MainWindow

        MainWindow().switchTo(UnitWidget())
        UnitWidget().view.setCurrentWidget(UnitWidget().qqPage)

    def enterEvent(self, event):
        """
        ## 重写事件以控制鼠标样式
        """
        super().enterEvent(event)
        self.setCursor(Qt.CursorShape.PointingHandCursor)


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

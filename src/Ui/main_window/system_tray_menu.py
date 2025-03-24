# -*- coding: utf-8 -*-

# 系统托盘图标组件


# 第三方库导入
from qfluentwidgets import Action, BodyLabel, IconWidget, SystemTrayMenu, StrongBodyLabel
from qfluentwidgets.common.icon import toQIcon
from PySide6.QtWidgets import QWidget, QSystemTrayIcon

# 项目内模块导入
from src.ui.icon import NCDIcon


class SystemTray(QSystemTrayIcon):

    def __init__(self, parent=None):
        """构造函数"""
        super().__init__(parent=parent)
        self.setIcon(toQIcon(NCDIcon.LOGO))

        # 定义菜单及选项
        self.menu = SystemTrayMenu(parent)
        self.info_card = SystemTrayIconInfoWidget(parent)

        # 添加到菜单
        self.menu.addWidget(self.info_card, selectable=False)
        self.menu.addSeparator()
        self.menu.addActions(
            [
                Action("🎤   唱"),
                Action("🕺   跳"),
                Action("🤘🏼   RAP"),
                Action("🎶   Music"),
                Action("🏀   篮球"),
            ]
        )
        self.setContextMenu(self.menu)


class SystemTrayIconInfoWidget(QWidget):
    """一个没啥用的信息展示组件"""

    def __init__(self, parent=None):
        super().__init__(parent=parent)
        # 创建信息组件
        self.icon_widget = IconWidget(NCDIcon.LOGO, self)
        self.title_label = StrongBodyLabel("NapCatQQ Desktop", self)
        self.license_label = BodyLabel("GPLv3 License", self)

        # 调整属性
        self.icon_widget.setFixedSize(48, 48)
        self.icon_widget.move(10, 10)
        self.title_label.move(75, 15)
        self.license_label.move(75, 35)

        # 调整大小
        self.setFixedSize(240, 70)

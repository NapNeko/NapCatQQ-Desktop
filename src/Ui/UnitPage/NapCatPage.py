# -*- coding: utf-8 -*-
from tkinter.messagebox import RETRY

from PySide6.QtCore import Qt, Slot
from qfluentwidgets import FluentIcon, ScrollArea, ExpandLayout
from PySide6.QtWidgets import QWidget

from src.Core import timer
from src.Core.Utils.GetVersion import GetVersion
from src.Ui.Icon import NapCatDesktopIcon
from src.Ui.common.InputCard import SwitchConfigCard, ComboBoxConfigCard, LineEditConfigCard
from src.Ui.UnitPage.Base import PageBase, Status
from src.Core.NetworkFunc import Urls

from src.Ui.common.info_bar import error_bar

from creart import it


class NapCatPage(PageBase):
    """
    ## NapCat 更新页面
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        self.setObjectName("UpdateNapCatPage")
        self.appCard.setName("NapCatQQ")
        self.appCard.setHyperLabelName(self.tr("仓库地址"))
        self.appCard.setHyperLabelUrl(Urls.NAPCATQQ_REPO.value)

        # 启动计时器
        self.updatePage()

    @timer(900_000)
    def updatePage(self) -> None:
        """
        ## 调用方法更新页面内容
            - 自动更新频率: 15分钟更新一次
        """
        self.checkStatus()
        self.getNewUpdateLog()

    def checkStatus(self) -> None:
        """
        ## 检查是否有更新, 发现更新改变按钮状态
        """
        if (local_ver := it(GetVersion).getLocalNapCatVersion()) is None:
            # 如果没有本地版本则显示安装按钮
            self.appCard.switchButton(Status.UNINSTALLED)
            return

        if (remote_ver := it(GetVersion).getRemoteNapCatVersion()) is None:
            # 如果拉取不到远程版本
            error_bar(self.tr("拉取远程版本时发生错误, 详情查看 设置 > Log"))
            return

        if local_ver != remote_ver:
            # 判断版本是否相等, 否则设置为更新状态
            self.appCard.switchButton(Status.UPDATE)

    def getNewUpdateLog(self) -> None:
        """
        ## 拉取最新更新日志到卡片
        """
        if (log := it(GetVersion).getRemoteNapCatUpdateLog()) is None:
            error_bar(self.tr("拉取更新日志时发生错误, 详情查看 设置 > Log"))
            return

        self.logCard.setLog(log)

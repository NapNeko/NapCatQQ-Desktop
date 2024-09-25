# -*- coding: utf-8 -*-

from PySide6.QtCore import Qt, QSize
from qfluentwidgets import FluentIcon, ScrollArea, ExpandLayout
from PySide6.QtWidgets import QWidget

from src.Ui.Icon import NapCatDesktopIcon
from src.Ui.common.InputCard import SwitchConfigCard, ComboBoxConfigCard, LineEditConfigCard
from src.Ui.UnitPage.Base import PageBase
from src.Core.NetworkFunc import Urls


class QQPage(PageBase):
    """
    ## QQ 更新页面
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        self.setObjectName("UpDateQQPage")
        self.appCard.setIcon(":/Icon/image/Icon/black/QQ.svg")
        self.appCard.setName("QQ")
        self.appCard.setHyperLabelName(self.tr("官网地址"))
        self.appCard.setHyperLabelUrl(Urls.QQ_OFFICIAL_WEBSITE.value)

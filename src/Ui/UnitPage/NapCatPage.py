# -*- coding: utf-8 -*-

from PySide6.QtCore import Qt
from qfluentwidgets import FluentIcon, ScrollArea, ExpandLayout
from PySide6.QtWidgets import QWidget

from src.Ui.Icon import NapCatDesktopIcon
from src.Ui.common.InputCard import SwitchConfigCard, ComboBoxConfigCard, LineEditConfigCard
from src.Ui.UnitPage.Base import PageBase
from src.Core.NetworkFunc import Urls


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

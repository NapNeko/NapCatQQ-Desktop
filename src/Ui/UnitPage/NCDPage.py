# -*- coding: utf-8 -*-

# 项目内模块导入
from src.Ui.UnitPage.Base import PageBase
from src.Core.NetworkFunc.Urls import Urls


class NCDPage(PageBase):
    """
    ## NapCat Desktop 更新页面
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        self.setObjectName("UnitNCDPage")
        self.appCard.setName("NapCat Desktop")
        self.appCard.setHyperLabelName(self.tr("仓库地址"))
        self.appCard.setHyperLabelUrl(Urls.NCD_REPO.value)

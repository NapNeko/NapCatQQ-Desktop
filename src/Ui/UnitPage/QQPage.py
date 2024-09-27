# -*- coding: utf-8 -*-

# 项目内模块导入
from src.Core.NetworkFunc import Urls
from src.Ui.UnitPage.Base import PageBase


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

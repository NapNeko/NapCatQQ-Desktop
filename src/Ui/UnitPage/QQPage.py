# -*- coding: utf-8 -*-

# 项目内模块导入
from src.Ui.UnitPage.Base import PageBase
from src.Core.NetworkFunc.Urls import Urls


class QQPage(PageBase):
    """
    ## QQ 更新页面
    """

    DESCRIPTION_TEXT = """
    ## 为什么要 QQ?
    
    NapCat 通过魔法的手段获得了 QQ 的发送消息、接收消息等接口，为了方便使用，猫猫框架将通过一种名为 [OneBot](https://11.onebot.dev) 的约定将你的 HTTP / WebSocket 请求按照规范读取，再去调用猫猫框架所获得的QQ发送接口之类的接口。
    
    所以 NapCat 只是让 QQ 以一种更加优雅的方式运行在你的服务器上，而不是 QQ 本身, 但是本质上还是离不开 QQ 的。
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        self.setObjectName("UnitQQPage")

        # 设置 appCard
        self.appCard.setIcon(":/Icon/image/Icon/black/QQ.svg")
        self.appCard.setName("QQ")
        self.appCard.setHyperLabelName(self.tr("官网地址"))
        self.appCard.setHyperLabelUrl(Urls.QQ_OFFICIAL_WEBSITE.value)

        # 设置 logCard
        self.logCard.setUrl(Urls.QQ_OFFICIAL_WEBSITE.value.url())
        self.logCard.setTitle(self.tr("描述"))
        self.logCard.setLog(DESCRIPTION_TEXT)

# -*- coding: utf-8 -*-
# 第三方库导入
from creart import it
from PySide6.QtCore import QObject

# 项目内模块导入
from src.Ui.MainWindow.Window import MainWindow


class BeginnerGuidance(QObject):
    """
    ## 新手引导(首次启动时)
        - 引导用户安装 NapCat, QQ
    """

    def __init__(self, parent):
        super().__init__()
        # 禁用所有侧边栏

    def skip(self):
        """
        ## 询问是否要跳过引导
        """

    def toDownloadPage(self):
        """
        ## 跳转到 Download 页面并指引下载
        """

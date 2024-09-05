# -*- coding: utf-8 -*-
from creart import it
from PySide6.QtCore import QObject

from src.Ui.MainWindow.Window import MainWindow


class BeginnerGuidance(QObject):
    """
    ## 新手引导(首次启动时)
        - 引导用户安装 NapCat, QQ
    """

    def __init__(self, parent):
        super().__init__()
        # 禁用所有侧边栏
        it(MainWindow).home_widget_button.setEnabled(False)
        it(MainWindow).add_widget_button.setEnabled(False)
        it(MainWindow).bot_list_widget_button.setEnabled(False)
        it(MainWindow).setup_widget_button.setEnabled(False)

    def skip(self):
        """
        ## 询问是否要跳过引导
        """

    def toDownloadPage(self):
        """
        ## 跳转到 Download 页面并指引下载
        """

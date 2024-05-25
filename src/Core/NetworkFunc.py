# -*- coding: utf-8 -*-
from abc import ABC
from enum import Enum
from functools import partial

from PySide6.QtCore import QUrl, QUrlQuery
from PySide6.QtGui import QPixmap
from PySide6.QtWidgets import QApplication
from creart import exists_module, AbstractCreator, CreateTargetInfo, add_creator, it

from PySide6.QtNetwork import QNetworkAccessManager, QNetworkRequest, QNetworkReply


class Urls(Enum):
    """
    ## 软件内部可能用的到的 Url
    """
    # NapCat 仓库地址
    NAPCATQQ_REPO = QUrl("https://github.com/NapNeko/NapCatQQ")
    NAPCATQQ_REPO_API = QUrl("https://api.github.com/repos/NapNeko/NapCatQQ/releases/latest")

    # 直接写入下载地址, 不请求 API 获取, 期望达到节省时间的目的
    NAPCAT_ARM64_LINUX = QUrl("https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.linux.arm64.zip")
    NAPCAT_64_LINUX = QUrl("https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.linux.x64.zip")
    NAPCAT_WIN = QUrl("https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.win32.x64.zip")

    # QQ 相关的 API
    QQ_AVATAR = QUrl("https://q.qlogo.cn/headimg_dl")


class NetworkFunc:
    """
    ## 软件内部的所有网络请求均通过此类实现
    """

    def __init__(self):
        """
        ## 创建 QNetworkAccessManager
        """
        self.manager = QNetworkAccessManager()


class NetworkFuncClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Core.NetworkFunc", "NetworkFunc"),)

    # 静态方法available()，用于检查模块"PathFunc"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Core.NetworkFunc")

    # 静态方法create()，用于创建PathFunc类的实例，返回值为PathFunc对象。
    @staticmethod
    def create(create_type: [NetworkFunc]) -> NetworkFunc:
        return NetworkFunc()


add_creator(NetworkFuncClassCreator)


if __name__ == '__main__':

    def handle_response(reply: QNetworkReply):
        if reply.error() == QNetworkReply.NetworkError.NoError:
            data = reply.readAll()
            print("Received data:", data)
        else:
            print("Error:", reply.errorString())
        reply.deleteLater()
        QApplication.quit()

    app = QApplication([])
    net = NetworkFunc()
    net.get(Urls.NAPCATQQ_REPO.value, handle_response)
    app.exec()

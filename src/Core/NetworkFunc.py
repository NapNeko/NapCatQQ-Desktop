# -*- coding: utf-8 -*-
import json
import re
from abc import ABC
from enum import Enum

from PySide6.QtCore import QUrl, QEventLoop
from PySide6.QtNetwork import QNetworkAccessManager, QNetworkRequest, QNetworkReply
from creart import exists_module, AbstractCreator, CreateTargetInfo, add_creator, it

from src.Core.GetVersion import GetVersion


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


class GetNewVersion:
    """
    ## 获取最新版本信息
    """

    @staticmethod
    def fetchApiResponse(url):
        """
        ## 通用的API请求方法
        """
        # 创建事件循环以及请求
        loop = QEventLoop()
        request = QNetworkRequest(url)
        # 发送请求
        reply = it(NetworkFunc).manager.get(request)
        # 连接信号槽，响应完成时退出事件循环
        reply.finished.connect(loop.quit)
        # 进入事件循环，等待响应完成
        loop.exec()

        # 如果请求失败返回 None
        if reply.error() != QNetworkReply.NetworkError.NoError:
            return None

        # 解析响应数据为 JSON 对象
        return json.loads(reply.readAll().data().decode().strip())

    def getNapCatVersion(self):
        """
        ## 获取 NapCat 的版本信息
        """
        response_dict = self.fetchApiResponse(Urls.NAPCATQQ_REPO_API.value)
        if response_dict is None:
            return None

        # 返回版本信息
        return response_dict.get("tag_name", None)

    def getQQVersion(self):
        """
        ## 获取 NapCat 所适配的 最新QQ版本
        """
        response_dict = self.fetchApiResponse(Urls.NAPCATQQ_REPO_API.value)
        if response_dict is None:
            return None

        # 正则表达式解析 QQ Version 信息
        windows_match = re.search(r'Windows\s([\d.]+-\d+)', response_dict.get("body"))
        linux_match = re.search(r'Linux\s([\d.]+-\d+)', response_dict.get("body"))

        # 返回版本信息
        return {
            "windows_version": windows_match.group(1) if windows_match else None,
            "linux_version": linux_match.group(1) if linux_match else None
        }

    def checkUpdate(self):
        """
        ## 检查 NapCat 是否有新版本, QQ暂时不支持检测(没有合理的下载指定版本的 api, 直接无脑最新版完事儿了)
        """
        remoteVersion = self.getNapCatVersion()
        localVersion = it(GetVersion).getNapCatVersion()

        if remoteVersion is None:
            # 如果获取不到远程版本, 则返回 None, Gui那边做ui处理
            return None

        return remoteVersion != localVersion


class GetNewVersionClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Core.NetworkFunc", "GetNewVersion"),)

    # 静态方法available()，用于检查模块"PathFunc"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Core.NetworkFunc")

    # 静态方法create()，用于创建PathFunc类的实例，返回值为PathFunc对象。
    @staticmethod
    def create(create_type: [GetNewVersion]) -> GetNewVersion:
        return GetNewVersion()


add_creator(GetNewVersionClassCreator)

# -*- coding: utf-8 -*-
import json
from abc import ABC
from enum import Enum
from pathlib import Path
from typing import Optional, IO

from PySide6.QtCore import QUrl, QEventLoop, QRegularExpression, Signal, Slot, QObject
from PySide6.QtNetwork import QNetworkAccessManager, QNetworkRequest, QNetworkReply
from creart import exists_module, AbstractCreator, CreateTargetInfo, add_creator, it

from src.Core.GetVersion import GetVersion
from src.Core.Config import cfg


class Urls(Enum):
    """
    ## 软件内部可能用的到的 Url
    """
    # NCD相关地址
    NCD_REPO = QUrl("https://github.com/HeartfeltJoy/NapCatQQ-Desktop")
    NCD_ISSUES = QUrl("https://github.com/HeartfeltJoy/NapCatQQ-Desktop/issues")

    # NapCat 相关地址
    NAPCATQQ_REPO = QUrl("https://github.com/NapNeko/NapCatQQ")
    NAPCATQQ_ISSUES = QUrl("https://github.com/NapNeko/NapCatQQ/issues")
    NAPCATQQ_REPO_API = QUrl("https://api.github.com/repos/NapNeko/NapCatQQ/releases/latest")
    NAPCATQQ_DOCUMENT = QUrl("https://napneko.github.io/")

    # 直接写入下载地址, 不请求 API 获取, 期望达到节省时间的目的
    NAPCAT_ARM64_LINUX = QUrl("https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.linux.arm64.zip")
    NAPCAT_64_LINUX = QUrl("https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.linux.x64.zip")
    NAPCAT_WIN = QUrl("https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.win32.x64.zip")

    # QQ 相关
    QQ_OFFICIAL_WEBSITE = QUrl("https://im.qq.com/index/")
    QQ_WIN_DOWNLOAD = QUrl("https://cdn-go.cn/qq-web/im.qq.com_new/latest/rainbow/windowsDownloadUrl.js")
    QQ_AVATAR = QUrl("https://q.qlogo.cn/headimg_dl")


class NetworkFunc(QObject):
    """
    ## 软件内部的所有网络请求均通过此类实现
    """
    response_ready = Signal(QNetworkReply)

    def __init__(self):
        """
        ## 创建 QNetworkAccessManager
        """
        super().__init__()
        self.manager = QNetworkAccessManager()

    def fetchResponse(self, url: QUrl):
        """
        ## 发送请求, 请求成功则通过信号返回
        """
        request = QNetworkRequest(url)
        reply = self.manager.get(request)
        reply.finished.connect(lambda: self.response_ready.emit(reply))


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
        # 创建事件循环和请求, 等待数据返回
        loop = QEventLoop()
        reply = it(NetworkFunc).manager.get(QNetworkRequest(url))
        reply.finished.connect(loop.quit)
        loop.exec()

        # 如果请求失败返回 None
        if reply.error() != QNetworkReply.NetworkError.NoError:
            return None

        # 返回响应数据
        return reply.readAll().data().decode().strip()

    def getNapCatVersion(self):
        """
        ## 获取 NapCat 的版本信息
        """
        if (response_dict := self.fetchApiResponse(Urls.NAPCATQQ_REPO_API.value)) is None:
            return None
        # 返回版本信息
        return json.loads(self.fetchApiResponse(Urls.NAPCATQQ_REPO_API.value)).get("tag_name", None)

    def getNapCatUpdateLog(self):
        """
        ## 获取 NapCat 的更新日志
        """
        if (response_dict := self.fetchApiResponse(Urls.NAPCATQQ_REPO_API.value)) is None:
            return None
        # 返回版本信息
        return json.loads(self.fetchApiResponse(Urls.NAPCATQQ_REPO_API.value)).get("body", None)

    def getQQVersion(self):
        """
        ## 获取 QQ 最新版本版本号
        """
        response_data = self.fetchApiResponse(Urls.QQ_WIN_DOWNLOAD.value)
        if response_data is None:
            return None

        # 定义正则表达式并匹配
        version = QRegularExpression(r'"version":\s*"([^"]+)"').match(response_data).captured(1)
        return version if version else None

    def getQQNewVersionUrl(self):
        """
        ## 获取最新QQ版本下载连接
        """
        response_data = self.fetchApiResponse(Urls.QQ_WIN_DOWNLOAD.value)
        if response_data is None:
            return None

        # 定义正则表达式并匹配
        match_x64 = QRegularExpression(r'"ntDownloadX64Url":\s*"([^"]+)"').match(response_data)
        match_arm = QRegularExpression(r'"ntDownloadARMUrl":\s*"([^"]+)"').match(response_data)

        # 提取所需的下载链接
        url_dict = {
            "x86_64": QUrl(match_x64.captured(1)),
            "AMD64": QUrl(match_x64.captured(1)),
            "ARM64": QUrl(match_arm.captured(1)),
            "aarch64": QUrl(match_arm.captured(1))
        }

        # 根据系统架构返回对应的下载链接
        return url_dict.get(cfg.get(cfg.PlatformType), None)

    def checkUpdate(self):
        """
        ## 检查 NapCat 是否有新版本, QQ暂时不支持检测(没有合理的下载指定版本的 api, 直接无脑最新版完事儿了)
        """
        remoteVersion = self.getNapCatVersion()
        localVersion = it(GetVersion).getNapCatVersion()

        if remoteVersion is None:
            # 如果获取不到远程版本, 则返回 None, Gui那边做ui处理
            return None

        return {
            "result": remoteVersion != localVersion,
            "localVersion": localVersion,
            "remoteVersion": remoteVersion
        }


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


class Downloader(QObject):
    """
    ## 执行下载任务
    """
    downloadProgress = Signal(int)
    finished = Signal(bool)
    errorOccurred = Signal(str, str)

    def __init__(self, url: QUrl, path: Path):
        """
        ## 初始化下载器
            - url 下载连接
            - path 下载路径
        """
        super().__init__()
        self.url: QUrl = url
        self.path: Path = path

        self.request = QNetworkRequest(self.url)
        self.reply: Optional[QNetworkReply] = None
        self.file: Optional[IO[bytes]] = None

    def start(self):
        """
        ## 启动下载
        """
        # 打开文件以写入下载数据
        self.file = open(str(self.path / self.url.fileName()), 'wb')
        # 执行下载任务并连接信号
        self.reply = it(NetworkFunc).manager.get(self.request)
        self.reply.downloadProgress.connect(self._downloadProgressSlot)
        self.reply.readyRead.connect(self._read2File)
        self.reply.finished.connect(self._finished)
        self.reply.errorOccurred.connect(self._error)

    def stop(self):
        """
        ## 停止下载
        """
        self.reply.abort() if self.reply else None

    @Slot()
    def _downloadProgressSlot(self, bytes_received: int, bytes_total: int):
        """
        ## 下载进度槽函数
            - bytes_received 接收的字节数
            - bytes_total 总字节数
        """
        if bytes_total > 0:
            # 防止发生零除以零的情况
            self.downloadProgress.emit(int((bytes_received / bytes_total) * 100))

    @Slot()
    def _read2File(self):
        """
        ## 读取数据并写入文件
        """
        if self.file:
            self.file.write(self.reply.readAll())

    @Slot()
    def _finished(self):
        """
        ## 下载结束并发送信号
        """
        if self.file:
            # 防止文件中途丢失导致关闭一个没有打开的文件引发报错
            self.file.close()
            self.file = None
        if self.reply.error() != QNetworkReply.NetworkError.NoError:
            self.finished.emit(False)
            return

        self.finished.emit(True)

    @Slot()
    def _error(self, error_code):
        if self.file:
            self.file.close()
            self.file = None
        self.finished.emit(False)
        self.errorOccurred.emit(self.reply.errorString(), str(error_code))

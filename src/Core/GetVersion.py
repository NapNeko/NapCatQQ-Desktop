# -*- coding: utf-8 -*-
import json
from abc import ABC
from json import JSONDecodeError
from loguru import logger

from PySide6.QtCore import QObject, QEventLoop, QRegularExpression, QUrl
from creart import it, AbstractCreator, CreateTargetInfo, exists_module, add_creator

from src.Core import timer
from src.Core.NetworkFunc import NetworkFunc, Urls, async_request
from src.Core.PathFunc import PathFunc


class GetVersion(QObject):
    """
    ## 提供两个方法, 分别获取本地的 NapCat 和 QQ 的版本
    """

    def __init__(self) -> None:
        super().__init__()
        # 创建属性
        self.napcatLocalVersion: None | str = None
        self.QQLocalVersion: None | str = None
        self.napcatRemoteVersion: None | str = None
        self.QQRemoteVersion: None | str = None
        self.QQRemoteDownloadUrls: None | dict = None
        self.napcatUpdateLog: None | str = None

        # 调用方法
        self.getRemoteNapCatUpdate()
        self.getQQDownloadUrl()
        self.getRemoteQQVersion()

        self.getLocalNapCatVersion()
        self.getLocalQQVersion()

    def checkUpdate(self) -> dict | None:
        """
        ## 检查 NapCat 是否有新版本, QQ暂时不支持检测(没有合理的下载指定版本的 api, 直接无脑最新版完事儿了)
        """
        if self.napcatRemoteVersion is None:
            # 如果获取不到远程版本, 则返回 None, Gui那边做ui处理
            return None

        return {
            "result": self.napcatRemoteVersion != self.getLocalNapCatVersion,
            "localVersion": self.getLocalNapCatVersion,
            "remoteVersion": self.napcatRemoteVersion
        }

    @timer(60_000)
    @async_request(Urls.NAPCATQQ_REPO_API.value)
    def getRemoteNapCatUpdate(self, reply) -> None:
        """
        ## 获取远程 NapCat 的版本信息和更新日志
            - 每 1 分钟读取一次并保存到变量中
        """
        if reply is None:
            # 如果请求失败则放弃覆盖变量
            return
        try:
            reply_dict = json.loads(reply)
            self.napcatRemoteVersion = reply_dict.get("tag_name", None)
            self.napcatUpdateLog = reply_dict.get("body", None)
        except JSONDecodeError:
            logger.error(f"Parsing Json errors, Sending the wrong string:[{reply}]")
            return

    @timer(60_000)
    @async_request(Urls.QQ_WIN_DOWNLOAD.value)
    def getRemoteQQVersion(self, reply) -> None:
        """
        ## 获取远程 QQ 版本版本号
            - 每 1 分钟读取一次并保存到变量中
        """
        if reply is None:
            # 如果请求失败则放弃覆盖变量
            return

        # 定义正则表达式并匹配
        version = QRegularExpression(r'"version":\s*"([^"]+)"').match(reply).captured(1)
        self.QQRemoteVersion = version if version else None

    @timer(60_000)
    @async_request(Urls.QQ_WIN_DOWNLOAD.value)
    def getQQDownloadUrl(self, reply) -> None:
        """
        ## 获取最新QQ版本下载连接
            - 每 1 分钟读取一次并保存到变量中
        """
        if reply is None:
            # 如果请求失败则放弃覆盖变量
            return

        # 定义正则表达式并匹配
        match_x64 = QRegularExpression(r'"ntDownloadX64Url":\s*"([^"]+)"').match(reply).captured(1)
        match_arm = QRegularExpression(r'"ntDownloadARMUrl":\s*"([^"]+)"').match(reply).captured(1)

        # 提取所需的下载链接
        self.QQRemoteDownloadUrls = {
            "x86_64": QUrl(match_x64),
            "AMD64": QUrl(match_x64),
            "ARM64": QUrl(match_arm),
            "aarch64": QUrl(match_arm)
        }

    @timer(3000)
    def getLocalNapCatVersion(self) -> None:
        """
        ## 获取本地 NapCat 的版本信息
            - 每 3 秒读取一次并保存到变量中
        """
        try:
            # 获取 package.json 路径并读取
            package_file_path = it(PathFunc).getNapCatPath() / "package.json"
            with open(str(package_file_path), "r", encoding="utf-8") as f:
                # 读取到参数返回版本信息
                self.napcatLocalVersion = f"v{json.loads(f.read())['version']}"

        except FileNotFoundError:
            # 文件不存在则返回 None
            self.napcatLocalVersion = None

    @timer(3000)
    def getLocalQQVersion(self) -> None:
        """
        ## 获取本地 QQ 的版本信息
            - 每 3 秒读取一次并保存到变量中
        """
        try:
            # 检查 QQPath
            if (qqPath := it(PathFunc).getQQPath()) is None:
                return None
            # 获取 package.json 路径并读取
            package_file_path = qqPath / "resources/app/package.json"
            with open(str(package_file_path), "r", encoding="utf-8") as f:
                # 读取参数并返回版本信息
                package = json.loads(f.read())
            # 拼接字符串返回版本信息
            platform = "Windows" if package["platform"] == "win32" else "Linux"
            self.QQLocalVersion = f"{platform} {package['version']}"

        except FileNotFoundError:
            # 文件不存在则返回 None
            self.QQLocalVersion = None


class GetVersionClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Core.GetVersion", "GetVersion"),)

    # 静态方法available()，用于检查模块"PathFunc"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Core.GetVersion")

    # 静态方法create()，用于创建PathFunc类的实例，返回值为PathFunc对象。
    @staticmethod
    def create(create_type: [GetVersion]) -> GetVersion:
        return GetVersion()


add_creator(GetVersionClassCreator)

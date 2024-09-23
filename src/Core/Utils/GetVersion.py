# -*- coding: utf-8 -*-
import json
import httpx
from abc import ABC
from json import JSONDecodeError

from creart import AbstractCreator, CreateTargetInfo, it, add_creator, exists_module
from PySide6.QtCore import QObject
from loguru import logger

from src.Core.NetworkFunc import Urls, async_request
from src.Core.Utils.PathFunc import PathFunc


class GetVersion(QObject):
    """
    ## 提供两个方法, 分别获取本地的 NapCat 和 QQ 的版本
    """

    def __init__(self) -> None:
        super().__init__()

    def checkUpdate(self) -> dict | None:
        """
        ## 检查 NapCat 是否有新版本
        """
        remote_version = self.getRemoteNapCatVersion()
        local_version = self.getLocalNapCatVersion()

        if remote_version is None:
            # 如果获取不到远程版本, 则返回 None, Gui那边做ui处理
            return None

        return {
            "result": remote_version != local_version,
            "localVersion": local_version,
            "remoteVersion": remote_version,
        }

    def getRemoteNapCatVersion(self) -> str | None:
        """
        ## 获取远程 NapCat 的版本信息
        """
        try:
            logger.info("获取 NapCat 远程版本信息")
            response = httpx.get(Urls.NAPCATQQ_REPO_API.value.url())
            logger.info(f"响应码: {response.status_code}")
            logger.debug(f"响应头: {response.headers}")
            logger.debug(f"数据: {response.json()}")
            logger.info(f"耗时: {response.elapsed}")
            return response.json()["tag_name"]
        except (httpx.RequestError, FileNotFoundError, PermissionError, Exception) as e:
            from src.Ui.common.info_bar import error_bar
            error_bar(self.tr("获取远程 NapCat 信息时引发错误, 请查看日志"))
            logger.error(f"获取 NapCat 版本信息时引发 {type(e).__name__}: {e}")
            return None
        finally:
            logger.info("获取 NapCat 版本信息结束")

    def getRemoteNapCatUpdateLog(self) -> str | None:
        """
        ## 获取 NapCat 的更新日志
        """
        try:
            logger.info("获取 NapCat 远程更新日志")
            response = httpx.get(Urls.NAPCATQQ_REPO_API.value.url())
            logger.info(f"响应码: {response.status_code}")
            logger.debug(f"响应头: {response.headers}")
            logger.debug(f"数据: {response.json()}")
            logger.info(f"耗时: {response.elapsed}")
            return response.json()["body"]
        except (httpx.RequestError, FileNotFoundError, PermissionError, Exception) as e:
            from src.Ui.common.info_bar import error_bar
            error_bar(self.tr("获取远程 NapCat 信息时引发错误, 请查看日志"))
            logger.error(f"获取 NapCat 版本信息时引发 {type(e).__name__}: {e}")
            return None
        finally:
            logger.info("获取 NapCat 更新日志结束")

    @staticmethod
    def getLocalNapCatVersion() -> str | None:
        """
        ## 获取本地 NapCat 的版本信息
        """
        try:
            # 获取 package.json 路径并读取
            with open(str(it(PathFunc).getNapCatPath() / "package.json"), "r", encoding="utf-8") as f:
                # 读取到参数返回版本信息
                return f"v{json.loads(f.read())['version']}"
        except FileNotFoundError:
            # 文件不存在则返回 None
            return None

    @staticmethod
    def getLocalQQVersion() -> str | None:
        """
        ## 获取本地 QQ 的版本信息
        """
        try:
            if (qq_path := it(PathFunc).getQQPath()) is None:
                # 检查 QQ 目录是否存在
                return None

            with open(str(qq_path / "versions" / "config.json"), "r", encoding='utf-8') as file:
                # 读取 config.json 文件获取版本信息
                return json.load(file)["curVersion"]
        except FileNotFoundError:
            # 文件不存在则返回 None
            return None


class GetVersionClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Core.Utils.GetVersion", "GetVersion"),)

    # 静态方法available()，用于检查模块"PathFunc"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Core.Utils.GetVersion")

    # 静态方法create()，用于创建PathFunc类的实例，返回值为PathFunc对象。
    @staticmethod
    def create(create_type: list[GetVersion]) -> GetVersion:
        return GetVersion()


add_creator(GetVersionClassCreator)

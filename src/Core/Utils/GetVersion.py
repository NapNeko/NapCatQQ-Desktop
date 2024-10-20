# -*- coding: utf-8 -*-
# 标准库导入
import json

# 第三方库导入
import httpx
from creart import it
from loguru import logger
from PySide6.QtCore import QUrl, Slot, Signal, QObject, QThread

# 项目内模块导入
from src.Core.Config import cfg
from src.Core.Utils.PathFunc import PathFunc
from src.Core.NetworkFunc.Urls import Urls


class GetVersion(QObject):

    remoteUpdateFinish = Signal()
    localUpdateFinish = Signal()

    def __init__(self, parent=None) -> None:
        super().__init__(parent=parent)

        # 本地
        self.local_NapCat = None
        self.local_QQ = None
        self.local_NCD = None

        # 远程
        self.remote_NapCat = None
        self.remote_QQ = None
        self.remote_NCD = None
        self.updateLog_NapCat = None
        self.updateLog_NCD = None
        self.download_qq_url = None

        # 初始化线程
        self.remoteUpdateThread = None
        self.localUpdateThread = None

    def update(self) -> None:
        """
        ## 更新内容
        """
        # 更新远程
        self.remoteUpdateThread = GetRemoteVersionThread()
        self.remoteUpdateThread.remoteSingle.connect(self.updateRemote)
        self.remoteUpdateThread.updateFinish.connect(self.remoteUpdateFinish.emit)
        self.remoteUpdateThread.start()

        # 更新本地
        self.localUpdateThread = GetLocalVersionThread()
        self.localUpdateThread.localSingle.connect(self.updateLocal)
        self.localUpdateThread.updateFinish.connect(self.localUpdateFinish.emit)
        self.localUpdateThread.start()

    @Slot(dict)
    def updateRemote(self, data: dict) -> None:
        """
        ## 更新远程
        """
        self.remote_NapCat = data["NapCat"]["version"]
        self.remote_QQ = data["QQ"]["version"]
        self.remote_NCD = data["NCD"]["version"]

        self.updateLog_NapCat = data["NapCat"]["update_log"]
        self.updateLog_NCD = data["NCD"]["update_log"]

        self.download_qq_url = QUrl(data["QQ"]["download_url"])

    @Slot(dict)
    def updateLocal(self, data: dict) -> None:
        """
        ## 更新本地
        """
        self.local_NapCat = data["NapCat"]
        self.local_QQ = data["QQ"]
        self.local_NCD = data["NCD"]


class GetRemoteVersionThread(QThread):
    """
    ## 更新远程版本
    """

    remoteSingle = Signal(dict)
    updateFinish = Signal()

    def __init__(self) -> None:
        super().__init__()

    def run(self) -> None:
        """
        ## 获取远程内容
        """
        remote_dict = {"NapCat": self.getNapCat(), "QQ": self.getQQ(), "NCD": self.getNapCatDesktop()}

        # 发送信号
        self.remoteSingle.emit(remote_dict)
        self.updateFinish.emit()

    def getNapCat(self) -> dict:
        """
        ## 获取 NapCat 相关内容
        """
        if (response := self.request(Urls.NAPCATQQ_REPO_API.value, "NapCat")) is not None:
            return {"version": response["tag_name"], "update_log": response["body"]}

    def getQQ(self) -> None:
        """
        ## 获取 QQ 相关内容
        """
        if (response := self.request(Urls.QQ_Version.value, "QQ")) is not None:
            download_url = f"https://dldir1.qq.com/qqfile/qq/QQNT/{response['verHash']}/QQ{response['version']}_x64.exe"
            return {"version": response["version"], "download_url": download_url.replace("-", ".")}

    def getNapCatDesktop(self) -> dict:
        """
        ## 获取 NCD 相关内容
        """
        if (response := self.request(Urls.NCD_REPO_API.value, "NapCat Desktop")) is not None:
            return {"version": response["tag_name"], "update_log": response["body"]}

    @staticmethod
    def request(url: QUrl, name: str) -> dict:
        """
        ## 网络请求
        """
        try:
            logger.debug(f"{10 * '-'} 开始获取 <-> {name[:16]:^16} {10 * '-'}")
            response = httpx.get(url.url())
            logger.debug(f"响应码: {response.status_code}")
            logger.debug(f"响应头: {response.headers}")
            logger.debug(f"数据: {response.json()}")
            logger.debug(f"耗时: {response.elapsed}")
            return response.json()
        except (httpx.RequestError, FileNotFoundError, PermissionError, Exception) as e:
            logger.error(f"获取 {name} 时引发 {type(e).__name__}: {e}")
        finally:
            logger.debug(f"{10 * '-'} 结束获取 <-> {name[:16]:^16} {10 * '-'}")


class GetLocalVersionThread(QThread):
    """
    ## 更新本地版本
    """

    localSingle = Signal(dict)
    updateFinish = Signal()

    def __init__(self) -> None:
        super().__init__()

    def run(self) -> None:
        """
        ## 获取远程内容
        """
        local_dict = {"NapCat": self.getNapCat(), "QQ": self.getQQ(), "NCD": self.getNapCatDesktop()}

        # 发送信号
        self.localSingle.emit(local_dict)
        self.updateFinish.emit()

    @staticmethod
    def getNapCat() -> dict:
        """
        ## 获取 NapCat 相关内容
        """
        try:
            with open(str(it(PathFunc).getNapCatPath() / "package.json"), "r", encoding="utf-8") as f:
                # 读取到参数返回版本信息
                return f"v{json.loads(f.read())['version']}"
        except FileNotFoundError:
            logger.warning("未找到 NapCat 的 package.json 文件, 可能是未安装 NapCat")
            return None

    @staticmethod
    def getQQ() -> None:
        """
        ## 获取 QQ 相关内容
        """
        try:
            if (qq_path := it(PathFunc).getQQPath()) is None:
                # 检查 QQ 目录是否存在
                logger.warning("未找到 QQ 的安装目录, 可能是未安装 QQ")
                return

            with open(str(qq_path / "versions" / "config.json"), "r", encoding="utf-8") as file:
                # 读取 config.json 文件获取版本信息
                return json.load(file)["curVersion"]
        except FileNotFoundError:
            # 文件不存在则返回 None
            logger.warning("未找到 QQ 的 config.json 文件, 可能是未安装 QQ")
            return

    @staticmethod
    def getNapCatDesktop() -> dict:
        """
        ## 获取 NCD 相关内容
        """
        return cfg.get(cfg.NCDVersion)

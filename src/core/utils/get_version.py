# -*- coding: utf-8 -*-
# 标准库导入
import json

# 第三方库导入
import httpx
from PySide6.QtCore import QObject, QThread, QUrl, Signal, Slot

# 项目内模块导入
from src.core.config import cfg
from src.core.network.urls import Urls
from src.core.utils.logger import logger
from src.core.utils.path_func import PathFunc


class GetVersion(QObject):

    remote_update_finish_signal = Signal()
    local_update_finish_signal = Signal()

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
        self.remote_update_thread = None
        self.local_update_thread = None

    def update(self) -> None:
        """更新内容"""
        # 更新远程
        self.remote_update_thread = GetRemoteVersionThread()
        self.remote_update_thread.remote_info_signal.connect(self.update_remote)
        self.remote_update_thread.update_finish_signal.connect(self.remote_update_finish_signal.emit)
        self.remote_update_thread.start()

        # 更新本地
        self.local_update_thread = GetLocalVersionThread()
        self.local_update_thread.local_info_signal.connect(self.update_local)
        self.local_update_thread.update_finish_signal.connect(self.local_update_finish_signal.emit)
        self.local_update_thread.start()

    @Slot(dict)
    def update_remote(self, data: dict) -> None:
        """更新远程"""
        self.remote_NapCat = data["NapCat"]["version"]
        self.remote_QQ = data["QQ"]["version"]
        self.remote_NCD = data["NCD"]["version"]

        self.updateLog_NapCat = data["NapCat"]["update_log"]
        self.updateLog_NCD = data["NCD"]["update_log"]

        self.download_qq_url = QUrl(data["QQ"]["download_url"])

    @Slot(dict)
    def update_local(self, data: dict) -> None:
        """更新本地"""
        self.local_NapCat = data["NapCat"]
        self.local_QQ = data["QQ"]
        self.local_NCD = data["NCD"]


class GetRemoteVersionThread(QThread):
    """更新远程版本"""

    remote_info_signal = Signal(dict)
    update_finish_signal = Signal()

    def __init__(self) -> None:
        super().__init__()

    def run(self) -> None:
        """获取远程内容"""
        remote_dict = {"NapCat": self.get_napcat(), "QQ": self.get_qq(), "NCD": self.get_napcat_desktop()}

        # 发送信号
        self.remote_info_signal.emit(remote_dict)
        self.update_finish_signal.emit()

    def get_napcat(self) -> dict | None:
        """获取 NapCat 相关内容"""
        if (response := self.request(Urls.NAPCATQQ_REPO_API.value, "NapCat")) is not None:
            return {"version": response["tag_name"], "update_log": response["body"]}

    def get_qq(self) -> dict | None:
        """获取 QQ 相关内容"""
        if (response := self.request(Urls.QQ_Version.value, "QQ")) is not None:
            ver_hash = response["verHash"]
            version = response["version"].replace("-", ".")
            download_url = f"https://dldir1.qq.com/qqfile/qq/QQNT/{ver_hash}/QQ{version}_x64.exe"
            return {"version": version, "download_url": download_url}

    def get_napcat_desktop(self) -> dict | None:
        """获取 NCD 相关内容"""
        try:
            if (response := self.request(Urls.NCD_REPO_API.value, "NapCat Desktop")) is not None:
                return {"version": response["tag_name"], "update_log": response["body"]}
        except (httpx.RequestError, PermissionError, Exception) as e:
            logger.error(f"获取 NCD 版本信息失败: {e}")
            return None

    @staticmethod
    def request(url: QUrl, name: str) -> dict | None:
        """网络请求"""
        try:
            return httpx.get(url.url()).json()

        except (httpx.RequestError, PermissionError, Exception) as e:
            logger.error(f"获取{name}版本信息失败: {e}")
            return None


class GetLocalVersionThread(QThread):
    """更新本地版本"""

    local_info_signal = Signal(dict)
    update_finish_signal = Signal()

    def __init__(self) -> None:
        super().__init__()

    def run(self) -> None:
        """获取远程内容"""
        local_dict = {"NapCat": self.get_napcat(), "QQ": self.get_qq(), "NCD": self.get_napcat_desktop()}

        # 发送信号
        self.local_info_signal.emit(local_dict)
        self.update_finish_signal.emit()

    @staticmethod
    def get_napcat() -> str | None:
        """获取 NapCat 相关内容"""
        try:
            with open(str(PathFunc().napcat_path / "package.json"), "r", encoding="utf-8") as f:
                # 读取到参数返回版本信息
                return f"v{json.loads(f.read())['version']}"
        except FileNotFoundError:
            logger.error("获取 NapCat 版本信息失败: 文件不存在")
            return None

    @staticmethod
    def get_qq() -> None:
        """获取 QQ 相关内容"""
        try:
            if (qq_path := PathFunc().get_qq_path()) is None:
                # 检查 QQ 目录是否存在
                logger.error("获取 QQ 版本信息失败: 文件不存在")
                return

            with open(str(qq_path / "versions" / "config.json"), "r", encoding="utf-8") as file:
                # 读取 config.json 文件获取版本信息
                return json.load(file)["curVersion"].replace("-", ".")
        except FileNotFoundError:
            # 文件不存在则返回 None
            logger.error("获取 QQ 版本信息失败: 文件不存在")
            return

    @staticmethod
    def get_napcat_desktop() -> dict:
        """获取 NCD 相关内容"""
        return cfg.get(cfg.napcat_desktop_version)

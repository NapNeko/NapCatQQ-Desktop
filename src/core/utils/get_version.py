# -*- coding: utf-8 -*-
"""
此模块用于处理 NCD 中的版本获取问题
"""

# 标准库导入
import json

# 第三方库导入
import httpx
from pydantic import BaseModel
from PySide6.QtCore import QObject, QRunnable, QThreadPool, QUrl, Signal

# 项目内模块导入
from src.core.config import cfg
from src.core.network.urls import Urls
from src.core.utils.logger import logger
from src.core.utils.path_func import PathFunc


class VersionData(BaseModel):
    """版本信息数据模型"""

    # 版本号
    napcat_version: str | None
    qq_version: str | None
    ncd_version: str | None

    # 下载链接
    qq_download_url: str | None = None

    # 更新日志
    napcat_update_log: str | None = None
    ncd_update_log: str | None = None


class VersionRunnableBase(QObject, QRunnable):
    """版本获取基类"""

    version_signal = Signal(VersionData)
    finish_signal = Signal()
    error_signal = Signal(str)

    def __init__(self) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)

    def run(self) -> None:
        """运行任务"""
        self.version_signal.emit(self.execute())

    def execute(self) -> VersionData:
        """子类实现此方法以执行任务"""
        raise NotImplementedError("Subclasses must implement this method")


class GetRemoteVersionRunnable(VersionRunnableBase):
    """获取远程版本信息

    运行流程:
    1. 请求 NapCat 版本信息
    2. 请求 QQ 版本信息
    3. 请求 NapCatQQ Desktop 版本信息
    4. 返回 VersionData 实例
    5. 通过 version_signal 发射版本信息
    6. 通过 finish_signal 发射任务完成信号
    7. 如果发生错误, 通过 error_signal 发射错误信息

    """

    def __init__(self) -> None:
        super().__init__()

    def execute(self) -> VersionData:
        """执行获取远程版本信息的任务"""
        napcat_info = self._get_version(Urls.NAPCATQQ_REPO_API.value, "NapCat", self._parse_github_response)
        qq_version = self._get_version(Urls.QQ_Version.value, "QQ", self._parse_qq_response)
        ncd_version = self._get_version(Urls.NCD_REPO_API.value, "NapCatQQ Desktop", self._parse_github_response)

        return VersionData(
            napcat_version=napcat_info["version"],
            qq_version=qq_version["version"],
            ncd_version=ncd_version["version"],
            qq_download_url=qq_version["download_url"],
            napcat_update_log=napcat_info["update_log"],
            ncd_update_log=ncd_version["update_log"],
        )

    def _get_version(self, url: str, name: str, parser: callable) -> dict[str, str | None]:
        """获取指定服务的版本信息

        Args:
            url (str): 服务的 API URL
            name (str): 服务名称
            parser (callable): 用于解析响应的函数

        Returns:
            dict: 包含版本信息的字典
        """
        response = self.request(QUrl(url), name)

        if response is None:
            return self._get_error_value(name)

        try:
            return parser(response)
        except KeyError as e:
            logger.error(f"解析 {name} 版本信息失败: {e}")
            self.error_signal.emit(f"解析 {name} 版本信息失败: {e}")
            return self._get_error_value(name)

    def _get_error_value(self, name: str) -> dict[str, None]:
        """根据服务名称返回对应的错误值"""
        error_values = {
            "QQ": {"version": None, "download_url": None},
            "NapCat": {"version": None, "update_log": None},
            "NapCatQQ Desktop": {"version": None, "update_log": None},
        }
        return error_values.get(name, {"version": None})

    def _parse_github_response(self, response: dict) -> dict[str, str | None]:
        """解析 GitHub API 响应格式"""
        return {"version": response["tag_name"], "update_log": response["body"]}

    def _parse_qq_response(self, response: dict) -> dict[str, str | None]:
        """解析 QQ 版本响应格式"""
        ver_hash = response["verHash"]
        version = response["version"].replace("-", ".")
        download_url = f"https://dldir1.qq.com/qqfile/qq/QQNT/{ver_hash}/QQ{version}_x64.exe"
        return {"version": version, "download_url": download_url}

    def request(self, url: QUrl, name: str) -> dict[str, str] | None:
        """网络请求"""
        try:
            with httpx.Client(timeout=5) as client:
                response = client.get(url.url())
                response.raise_for_status()
                return response.json()
        except (httpx.RequestError, httpx.HTTPStatusError) as e:
            logger.error(f"获取 {name} 版本信息失败: {e}")
            self.error_signal.emit(f"获取 {name} 版本信息失败: {e}")
            return None


class GetLocalVersionRunnable(VersionRunnableBase):
    """获取本地版本信息

    运行流程:
    1. 读取本地 NapCat 版本信息
    2. 读取本地 QQ 版本信息
    3. 读取本地 NapCatQQ Desktop 版本信息
    4. 返回 VersionData 实例
    5. 通过 version_signal 发射版本信息
    6. 通过 finish_signal 发射任务完成信号
    7. 如果发生错误, 通过 error_signal 发射错误信息

    """

    def __init__(self) -> None:
        super().__init__()

    def execute(self) -> VersionData:
        """执行获取本地版本信息的任务"""

        return VersionData(
            napcat_version=self.get_napcat_version(),
            qq_version=self.get_qq_version(),
            ncd_version=self.get_ncd_version(),
        )

    def get_napcat_version(self) -> str | None:
        """获取本地 NapCat 版本信息"""
        try:
            with open(str(PathFunc().napcat_path / "package.json"), "r", encoding="utf-8") as f:
                # 读取到参数返回版本信息
                return f"v{json.loads(f.read())['version']}"
        except FileNotFoundError:
            logger.error("获取 NapCat 版本信息失败: 文件不存在")
            self.error_signal.emit("获取 NapCat 版本信息失败: 文件不存在")
            return None

    def get_qq_version(self) -> str | None:
        """获取本地 QQ 版本信息"""
        try:
            if (qq_path := PathFunc().get_qq_path()) is None:
                # 检查 QQ 目录是否存在
                logger.error("获取 QQ 版本信息失败: 文件不存在")
                return None

            with open(str(qq_path / "versions" / "config.json"), "r", encoding="utf-8") as file:
                # 读取 config.json 文件获取版本信息
                return json.load(file)["curVersion"].replace("-", ".")
        except FileNotFoundError:
            # 文件不存在则返回 None
            logger.error("获取 QQ 版本信息失败: 文件不存在")
            self.error_signal.emit("获取 QQ 版本信息失败: 文件不存在")
            return None

    def get_ncd_version(self) -> str | None:
        """获取本地 NapCatQQ Desktop 版本信息"""
        return cfg.get(cfg.napcat_desktop_version)


class GetVersion(QObject):

    # 获取版本结束信号
    remote_finish_signal = Signal(VersionData)
    local_finish_signal = Signal(VersionData)

    def __init__(self, parent=...) -> None:
        super().__init__(parent)

    def update(self) -> None:
        """开始更新版本信息"""
        # 本地版本检查
        local_runnable = GetLocalVersionRunnable()
        local_runnable.version_signal.connect(self.local_finish_signal.emit)
        QThreadPool.globalInstance().start(local_runnable)

        # 远程版本检查
        remote_runnable = GetRemoteVersionRunnable()
        remote_runnable.version_signal.connect(self.remote_finish_signal.emit)
        QThreadPool.globalInstance().start(remote_runnable)

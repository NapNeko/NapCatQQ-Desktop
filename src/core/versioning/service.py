# -*- coding: utf-8 -*-
"""版本信息读取与拉取服务。"""

import json
import re
from collections.abc import Callable

from creart import it
import httpx
from pydantic import BaseModel
from PySide6.QtCore import QObject, QRunnable, QThreadPool, QUrl, Signal

from src.core.config import cfg
from src.core.network.urls import Urls
from src.core.logging import logger
from src.core.runtime.paths import PathFunc


class VersionSnapshot(BaseModel):
    """NapCat、QQ、Desktop 的版本快照。"""

    napcat_version: str | None
    qq_version: str | None
    ncd_version: str | None
    qq_download_url: str | None = None
    napcat_update_log: str | None = None
    ncd_update_log: str | None = None


class VersionTaskBase(QObject, QRunnable):
    """版本任务基类。"""

    version_signal = Signal(VersionSnapshot)
    finish_signal = Signal()
    error_signal = Signal(str)

    def __init__(self) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)

    def run(self) -> None:
        """执行任务并发出结果。"""
        self.version_signal.emit(self.execute())

    def execute(self) -> VersionSnapshot:
        """由子类实现具体版本任务。"""
        raise NotImplementedError("Subclasses must implement this method")


class RemoteVersionTask(VersionTaskBase):
    """远端版本信息拉取任务。"""

    def execute(self) -> VersionSnapshot:
        napcat_info = self._get_version_with_fallback(
            Urls.NAPCATQQ_REPO_API.value,
            Urls.NAPCATQQ_REPO_API_FALLBACK.value,
            "NapCat",
            self._parse_github_response,
        )
        qq_version = self._get_version(Urls.QQ_Version.value, "QQ", self._parse_qq_response)
        ncd_version = self._get_version_with_fallback(
            Urls.NCD_REPO_API.value,
            Urls.NCD_REPO_API_FALLBACK.value,
            "NapCatQQ Desktop",
            self._parse_github_response,
        )

        return VersionSnapshot(
            napcat_version=napcat_info["version"],
            qq_version=qq_version["version"],
            ncd_version=ncd_version["version"],
            qq_download_url=qq_version["download_url"],
            napcat_update_log=napcat_info["update_log"],
            ncd_update_log=ncd_version["update_log"],
        )

    def _get_version(
        self, url: str | QUrl, name: str, parser: Callable[[dict], dict[str, str | None]]
    ) -> dict[str, str | None]:
        response = self.request(QUrl(url), name)

        if response is None:
            return self._get_error_value(name)

        try:
            return parser(response)
        except KeyError as exc:
            logger.error(f"解析 {name} 版本信息失败: {exc}")
            self.error_signal.emit(f"解析 {name} 版本信息失败: {exc}")
            return self._get_error_value(name)

    def _get_version_with_fallback(
        self,
        primary_url: str | QUrl,
        fallback_url: str | QUrl,
        name: str,
        parser: Callable[[dict], dict[str, str | None]],
    ) -> dict[str, str | None]:
        """获取版本信息，主 URL 失败时使用兜底 URL。"""
        # 先尝试主 URL（镜像站）
        response = self.request(QUrl(primary_url), name, emit_error=False)

        # 如果镜像站失败，尝试 GitHub 官方 API
        if response is None:
            logger.warning(f"{name} 镜像站请求失败，尝试 GitHub 官方 API...")
            response = self.request(QUrl(fallback_url), name, emit_error=True)

        if response is None:
            return self._get_error_value(name)

        try:
            return parser(response)
        except KeyError as exc:
            logger.error(f"解析 {name} 版本信息失败: {exc}")
            self.error_signal.emit(f"解析 {name} 版本信息失败: {exc}")
            return self._get_error_value(name)

    def _get_error_value(self, name: str) -> dict[str, str | None]:
        error_values: dict[str, dict[str, str | None]] = {
            "QQ": {"version": None, "download_url": None},
            "NapCat": {"version": None, "update_log": None},
            "NapCatQQ Desktop": {"version": None, "update_log": None},
        }
        return error_values.get(name, {"version": None})

    @staticmethod
    def _parse_github_response(response: dict) -> dict[str, str | None]:
        return {"version": response["tag_name"], "update_log": response["body"]}

    def _parse_qq_response(self, response: dict) -> dict[str, str | None]:
        if not response:
            return {"version": None, "download_url": None}

        try:
            result = response.get("Windows")
            if result is not None:
                return {"version": result.get("version", ""), "download_url": result.get("ntDownloadX64Url")}
            return {"version": None, "download_url": None}
        except Exception as exc:
            logger.error(f"解析 QQ 版本信息失败: {exc}")
            self.error_signal.emit(f"解析 QQ 版本信息失败: {exc}")
            return {"version": None, "download_url": None}

    def request(
        self,
        url: QUrl,
        name: str,
        use_mirrors: bool = False,
        emit_error: bool = True,
    ) -> dict[str, str] | None:
        request_urls = [url.url()]
        if use_mirrors:
            request_urls.extend(f"{mirror.toString().rstrip('/')}/{url.url()}" for mirror in Urls.MIRROR_SITE.value)

        last_error: Exception | None = None
        for candidate_url in request_urls:
            try:
                with httpx.Client(timeout=5, follow_redirects=True) as client:
                    response = client.get(candidate_url)
                    response.raise_for_status()
                    return response.json()
            except (httpx.RequestError, httpx.HTTPStatusError, ValueError) as exc:
                last_error = exc

        if emit_error:
            logger.error(f"获取 {name} 版本信息失败: {last_error}")
            self.error_signal.emit(f"获取 {name} 版本信息失败: {last_error}")
        return None


class LocalVersionTask(VersionTaskBase):
    """本地版本信息读取任务。"""

    def execute(self) -> VersionSnapshot:
        return VersionSnapshot(
            napcat_version=self.get_napcat_version(),
            qq_version=self.get_qq_version(),
            ncd_version=self.get_ncd_version(),
        )

    def get_napcat_version(self) -> str | None:
        napcat_path = it(PathFunc).napcat_path

        if version := self._get_napcat_version_from_mjs(napcat_path / "napcat.mjs"):
            return version

        try:
            with open(str(napcat_path / "package.json"), "r", encoding="utf-8") as file:
                return f"v{json.loads(file.read())['version']}"
        except FileNotFoundError:
            logger.error("获取 NapCat 版本信息失败: 文件不存在")
            self.error_signal.emit("获取 NapCat 版本信息失败: 文件不存在")
            return None

    @staticmethod
    def _get_napcat_version_from_mjs(mjs_path) -> str | None:
        try:
            content = mjs_path.read_text(encoding="utf-8")
        except FileNotFoundError:
            return None

        match = re.search(r'napCatVersion\s*=\s*.*?"(\d+\.\d+\.\d+(?:[-+][^"]+)?)"', content)
        if match is None:
            return None

        return f"v{match.group(1)}"

    def get_qq_version(self) -> str | None:
        try:
            if (qq_path := it(PathFunc).get_qq_path()) is None:
                logger.error("获取 QQ 版本信息失败: 文件不存在")
                return None

            with open(str(qq_path / "versions" / "config.json"), "r", encoding="utf-8") as file:
                return json.load(file)["curVersion"].split("-")[0]
        except FileNotFoundError:
            logger.error("获取 QQ 版本信息失败: 文件不存在")
            self.error_signal.emit("获取 QQ 版本信息失败: 文件不存在")
            return None

    @staticmethod
    def get_ncd_version() -> str | None:
        return cfg.get(cfg.napcat_desktop_version)


class VersionService(QObject):
    """统一协调本地和远端版本任务。"""

    remote_versions_loaded = Signal(VersionSnapshot)
    local_versions_loaded = Signal(VersionSnapshot)

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)

    def refresh(self) -> None:
        local_task = LocalVersionTask()
        local_task.version_signal.connect(self.local_versions_loaded.emit)
        QThreadPool.globalInstance().start(local_task)

        remote_task = RemoteVersionTask()
        remote_task.version_signal.connect(self.remote_versions_loaded.emit)
        QThreadPool.globalInstance().start(remote_task)


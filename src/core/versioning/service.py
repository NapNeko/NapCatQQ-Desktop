# -*- coding: utf-8 -*-
"""版本信息读取与拉取服务. """

import json
import re
import time
from abc import ABC
from collections.abc import Callable
from dataclasses import dataclass
from threading import RLock

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module, it
import httpx
from pydantic import BaseModel
from PySide6.QtCore import QObject, QRunnable, QThreadPool, QUrl, Signal

from src.core.config import cfg
from src.core.network.urls import Urls
from src.core.network.proxy_signer import ProxySigner
from src.core.logging import logger
from src.core.runtime.paths import PathFunc


@dataclass(frozen=True)
class _FetchError:
    """归一化的拉取失败原因, 用于生成用户友好的错误文案."""

    kind: str       # "rate_limit" | "proxy_banned" | "timeout" | "network" | "parse" | "http"
    detail: str     # 调试细节, 进日志不上 UI

    def user_message(self, name: str) -> str:
        """简短文案 (适合 InfoBar 单行展示). 详细诊断走 logger."""
        if self.kind == "rate_limit":
            return f"获取 {name} 版本失败: GitHub 限速, 建议设置 PAT"
        if self.kind == "bad_token":
            return f"获取 {name} 版本失败: GitHub Token 无效"
        if self.kind == "proxy_banned":
            return f"获取 {name} 版本失败: 中转与直连均不可达"
        if self.kind == "timeout":
            return f"获取 {name} 版本超时"
        if self.kind == "network":
            return f"获取 {name} 版本失败: 网络不可达"
        if self.kind == "parse":
            return f"获取 {name} 版本失败: 数据格式异常"
        return f"获取 {name} 版本失败 (HTTP {self.detail})"


def _classify_exception(exc: Exception, response: httpx.Response | None = None) -> _FetchError:
    """把 httpx 异常 / response 状态归类成 _FetchError."""
    if response is not None and response.status_code == 429:
        return _FetchError("proxy_banned", "429")
    if response is not None and response.status_code == 401:
        return _FetchError("bad_token", "401")
    if response is not None and response.status_code == 403:
        # GitHub 限速 vs 代理签名失败的区分: GitHub 头里有 X-RateLimit-Remaining: 0
        remaining = response.headers.get("X-RateLimit-Remaining")
        if remaining == "0":
            return _FetchError("rate_limit", "github 403 with X-RateLimit-Remaining=0")
        return _FetchError("http", "403")
    if response is not None and response.status_code >= 400:
        return _FetchError("http", str(response.status_code))
    if isinstance(exc, httpx.TimeoutException):
        return _FetchError("timeout", str(exc))
    if isinstance(exc, httpx.RequestError):
        return _FetchError("network", str(exc))
    if isinstance(exc, ValueError):
        return _FetchError("parse", str(exc))
    return _FetchError("network", f"{type(exc).__name__}: {exc}")


class VersionSnapshot(BaseModel):
    """NapCat, QQ, Desktop, SnowLuma 的版本快照. """

    napcat_version: str | None
    qq_version: str | None
    ncd_version: str | None
    snowluma_version: str | None = None
    qq_download_url: str | None = None
    napcat_update_log: str | None = None
    ncd_update_log: str | None = None
    snowluma_update_log: str | None = None


class VersionTaskBase(QObject, QRunnable):
    """版本任务基类. """

    version_signal = Signal(VersionSnapshot)
    finish_signal = Signal()
    error_signal = Signal(str)

    def __init__(self) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)

    def run(self) -> None:
        """执行任务并发出结果. """
        self.version_signal.emit(self.execute())

    def execute(self) -> VersionSnapshot:
        """由子类实现具体版本任务. """
        raise NotImplementedError("Subclasses must implement this method")


class RemoteVersionTask(VersionTaskBase):
    """远端版本信息拉取任务. """

    def execute(self) -> VersionSnapshot:
        napcat_info = self._get_version_with_fallback(
            Urls.NAPCATQQ_REPO_API.value,
            Urls.NAPCATQQ_REPO_API_FALLBACK.value,
            "NapCat",
            self._parse_github_response,
            proxy_path=Urls.NAPCATQQ_REPO_API_PATH.value,
        )
        qq_version = self._get_version(Urls.QQ_Version.value, "QQ", self._parse_qq_response)
        ncd_version = self._get_version_with_fallback(
            Urls.NCD_REPO_API.value,
            Urls.NCD_REPO_API_FALLBACK.value,
            "NapCatQQ Desktop",
            self._parse_github_response,
            proxy_path=Urls.NCD_REPO_API_PATH.value,
        )
        # P1 (SnowLuma 适配): 多拉一份 SnowLuma. 与 NapCat / NCD 完全对称,
        # 走同款 _get_version_with_fallback + _parse_github_response 链路.
        snowluma_info = self._get_version_with_fallback(
            Urls.SNOWLUMA_REPO_API.value,
            Urls.SNOWLUMA_REPO_API_FALLBACK.value,
            "SnowLuma",
            self._parse_github_response,
            proxy_path=Urls.SNOWLUMA_REPO_API_PATH.value,
        )

        return VersionSnapshot(
            napcat_version=napcat_info["version"],
            qq_version=qq_version["version"],
            ncd_version=ncd_version["version"],
            snowluma_version=snowluma_info["version"],
            qq_download_url=qq_version["download_url"],
            napcat_update_log=napcat_info["update_log"],
            ncd_update_log=ncd_version["update_log"],
            snowluma_update_log=snowluma_info["update_log"],
        )

    def _get_version(
        self, url: str | QUrl, name: str, parser: Callable[[dict], dict[str, str | None]]
    ) -> dict[str, str | None]:
        response, err = self.request(QUrl(url), name)

        if response is None:
            if err is not None:
                logger.error(f"获取 {name} 版本信息失败: {err.kind} ({err.detail})")
                self.error_signal.emit(err.user_message(name))
            return self._get_error_value(name)

        try:
            return parser(response)
        except KeyError as exc:
            logger.error(f"解析 {name} 版本信息失败: {exc}")
            self.error_signal.emit(_FetchError("parse", str(exc)).user_message(name))
            return self._get_error_value(name)

    def _get_version_with_fallback(
        self,
        primary_url: str | QUrl,
        fallback_url: str | QUrl,
        name: str,
        parser: Callable[[dict], dict[str, str | None]],
        proxy_path: str | None = None,
    ) -> dict[str, str | None]:
        """获取版本信息: 中转代理优先, 失败回退 GitHub 官方 API.

        中转走 HMAC 签名 + 时钟自愈; 兜底走直连, 若用户配置了 GitHub Personal
        Token 则带上 Authorization 头把限速从 60/h 拉到 5000/h.
        只有主备全失败才 emit error_signal, 中间过程的失败不打扰 UI.
        """
        response, primary_err = self.request(QUrl(primary_url), name, proxy_path=proxy_path)

        if response is None:
            err_summary = (
                f"{primary_err.kind} ({primary_err.detail})"
                if primary_err is not None
                else "unknown"
            )
            logger.warning(
                f"{name} 中转站请求失败 [{err_summary}], 尝试 GitHub 官方 API..."
            )
            response, fallback_err = self.request(QUrl(fallback_url), name, use_github_token=True)
            if response is None and fallback_err is not None:
                logger.error(f"获取 {name} 版本信息失败: {fallback_err.kind} ({fallback_err.detail})")
                self.error_signal.emit(fallback_err.user_message(name))
                return self._get_error_value(name)

        if response is None:
            return self._get_error_value(name)

        try:
            return parser(response)
        except KeyError as exc:
            logger.error(f"解析 {name} 版本信息失败: {exc}")
            self.error_signal.emit(_FetchError("parse", str(exc)).user_message(name))
            return self._get_error_value(name)

    def _get_error_value(self, name: str) -> dict[str, str | None]:
        error_values: dict[str, dict[str, str | None]] = {
            "QQ": {"version": None, "download_url": None},
            "NapCat": {"version": None, "update_log": None},
            "NapCatQQ Desktop": {"version": None, "update_log": None},
            "SnowLuma": {"version": None, "update_log": None},
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
        proxy_path: str | None = None,
        use_github_token: bool = False,
    ) -> tuple[dict | None, _FetchError | None]:
        """通用 GET 请求, 返回 (json_body, fetch_error).

        ``proxy_path`` 非空时走中转代理: 注入 HMAC 签名头, 第一次签名失败 (403)
        会读响应头 ``X-Server-Time`` 校正时钟后重试一次.

        ``use_github_token=True`` 时, 若用户在设置中配置了 GitHub Personal Token,
        带上 ``Authorization: Bearer <token>``, 把直连 GitHub API 的限速拉到 5000/h.
        """
        request_urls = [url.url()]
        if use_mirrors:
            request_urls.extend(f"{mirror.toString().rstrip('/')}/{url.url()}" for mirror in Urls.MIRROR_SITE.value)

        last_err: _FetchError | None = None
        for candidate_url in request_urls:
            attempts = 2 if proxy_path else 1
            for attempt in range(attempts):
                resp_for_class: httpx.Response | None = None
                try:
                    headers: dict[str, str] = {}
                    if proxy_path:
                        headers.update(ProxySigner.instance().sign_headers(proxy_path))
                    elif use_github_token:
                        token = (cfg.get(cfg.github_personal_token) or "").strip()
                        if token:
                            headers["Authorization"] = f"Bearer {token}"
                            headers["Accept"] = "application/vnd.github+json"

                    with httpx.Client(timeout=5, follow_redirects=True) as client:
                        response = client.get(candidate_url, headers=headers)
                        resp_for_class = response

                        if (
                            proxy_path
                            and response.status_code == 403
                            and attempt == 0
                        ):
                            updated = ProxySigner.instance().update_offset_from_response(
                                response.headers
                            )
                            if updated:
                                logger.info(f"{name} 代理 403, 已校正本地时钟, 重试")
                                continue

                        response.raise_for_status()
                        return response.json(), None
                except (httpx.RequestError, httpx.HTTPStatusError, ValueError) as exc:
                    last_err = _classify_exception(exc, resp_for_class)
                    break  # 同一 URL 不要因网络/解析错误反复重试

        return None, last_err


class LocalVersionTask(VersionTaskBase):
    """本地版本信息读取任务. """

    def execute(self) -> VersionSnapshot:
        return VersionSnapshot(
            napcat_version=self.get_napcat_version(),
            qq_version=self.get_qq_version(),
            ncd_version=self.get_ncd_version(),
            snowluma_version=self.get_snowluma_version(),
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

    def get_snowluma_version(self) -> str | None:
        """读 ``<snowluma_path>/.installed_tag`` 返回已安装的 SnowLuma release tag.

        不读 ``package.json.version`` 的原因:
        上游 ``package.json.version`` 是内部版本号 (如 ``"0.1.0"``), 与 release tag
        (如 ``"v1.7.5"``) 不同步; 直接读会让 "本地 vs 远端" 永远不相等,
        BotCard 会错误地一直提示 "需要更新".

        ``.installed_tag`` 文件由 :class:`SnowLumaInstall` 在解压成功后写入,
        内容为纯文本 tag (含 ``v`` 前缀, 与远端 ``tag_name`` 符号对齐).

        Returns:
            tag 字符串 (如 ``"v1.7.5"``); 未安装 / 文件损坏时返回 ``None``.
        """
        try:
            installed_tag = it(PathFunc).snowluma_path / ".installed_tag"
            if not installed_tag.exists():
                return None
            content = installed_tag.read_text(encoding="utf-8").strip()
            return content or None
        except (OSError, UnicodeDecodeError, AttributeError):
            # AttributeError 兜底: PathFunc 单例在测试 monkeypatch 场景可能被替换为
            # 不含 snowluma_path 的 SimpleNamespace; 视为 "未安装", 不破坏调用方.
            return None


class VersionService(QObject):
    """统一协调本地和远端版本任务 (creart 单例).

    所有需要"远端版本号"的调用方都应通过 ``it(VersionService).refresh()`` 触发,
    避免启动期同一组 endpoint 被并发重复拉取.

    缓存策略
    --------

    远端 snapshot 在内存里缓存 ``REMOTE_CACHE_TTL_SECONDS`` 秒. 在此期间:

    - 调 ``refresh(force=False)`` (默认): 直接 emit 缓存, 不发新请求
    - 调 ``refresh(force=True)``: 跳过缓存, 强制发新请求 (用户主动点"检查更新")
    - 已经有任务在跑: 后到的调用直接 "搭便车", 等同一个任务结束

    **失败不写缓存**: ``RemoteVersionTask`` 若全部 endpoint 失败 (snapshot 字段
    全 None), 视为本次拉取无效, 不更新缓存; 下次 refresh 会重新尝试网络.

    本地 snapshot 不缓存: ``LocalVersionTask`` 仅读文件, 每次 refresh 重跑,
    确保用户安装/卸载组件后立刻反映最新本地版本.
    """

    remote_versions_loaded = Signal(VersionSnapshot)
    local_versions_loaded = Signal(VersionSnapshot)

    #: 远端 snapshot 缓存 TTL; 5 分钟内重复 refresh 直接走缓存
    REMOTE_CACHE_TTL_SECONDS: float = 5 * 60

    def __init__(self) -> None:
        super().__init__()
        self._remote_snapshot: VersionSnapshot | None = None
        self._fetched_at: float | None = None
        self._in_flight: RemoteVersionTask | None = None
        self._lock = RLock()

    def refresh(self, *, force: bool = False) -> None:
        """触发本地 + 远端版本刷新.

        Args:
            force: ``True`` 时跳过远端缓存, 强制重新拉取 (用户主动点"检查更新").
                ``False`` (默认) 时若缓存新鲜则直接 emit, 不发网络请求.
        """
        # 本地任务每次都跑 (读文件成本极低, 用户体验上要"立刻反映安装状态")
        local_task = LocalVersionTask()
        local_task.version_signal.connect(self.local_versions_loaded.emit)
        QThreadPool.globalInstance().start(local_task)

        # 远端: 缓存命中 / in-flight 复用 / 启动新任务 三选一
        cached_to_emit: VersionSnapshot | None = None
        should_start_task = False
        with self._lock:
            if not force and self._is_cache_fresh() and self._remote_snapshot is not None:
                cached_to_emit = self._remote_snapshot
                logger.info(
                    f"VersionService: 远端缓存命中 (age={int(time.time() - (self._fetched_at or 0))}s), 跳过网络请求"
                )
            elif self._in_flight is not None:
                logger.info("VersionService: 已有 in-flight 任务, 复用结果")
            else:
                task = RemoteVersionTask()
                task.version_signal.connect(self._on_remote_versions_loaded)
                self._in_flight = task
                should_start_task = True

        # 出锁后再 emit / start, 避免 slot 阻塞其他调用方
        if cached_to_emit is not None:
            self.remote_versions_loaded.emit(cached_to_emit)
        elif should_start_task:
            QThreadPool.globalInstance().start(self._in_flight)

    def _on_remote_versions_loaded(self, snapshot: VersionSnapshot) -> None:
        """远端任务回调: 写缓存 (失败不写) + 转发信号 + 清 in-flight."""
        # 全字段 None 视为失败, 不写缓存
        is_failure = (
            snapshot.napcat_version is None
            and snapshot.qq_version is None
            and snapshot.ncd_version is None
            and snapshot.snowluma_version is None
        )
        with self._lock:
            self._in_flight = None
            if not is_failure:
                self._remote_snapshot = snapshot
                self._fetched_at = time.time()
            else:
                logger.warning("VersionService: 远端拉取全失败, 不写缓存 (下次 refresh 会重试)")

        self.remote_versions_loaded.emit(snapshot)

    def _is_cache_fresh(self) -> bool:
        """缓存存在且未过 TTL 时返回 True; 调用方需持锁."""
        if self._remote_snapshot is None or self._fetched_at is None:
            return False
        return (time.time() - self._fetched_at) < self.REMOTE_CACHE_TTL_SECONDS


class VersionServiceCreator(AbstractCreator, ABC):
    """[`VersionService`](src/core/versioning/service.py) 单例创建器."""

    targets = (CreateTargetInfo("src.core.versioning.service", "VersionService"),)

    @staticmethod
    def available() -> bool:
        return exists_module("src.core.versioning.service")

    @staticmethod
    def create(create_type):
        return create_type()


add_creator(VersionServiceCreator)


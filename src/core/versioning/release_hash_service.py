# -*- coding: utf-8 -*-
"""[`ReleaseHashService`](src/core/versioning/release_hash_service.py): 上游 NapCat
release 完整性指纹的拉取/缓存/查询服务.

设计目标
--------

GitHub Releases API 的 ``assets[*].digest`` 字段已直接提供每个发布资产的
SHA256 指纹 (``"sha256:<64-hex>"``). 本服务从中提取 ``NapCat.Shell.zip`` 与
``NapCat.Framework.zip`` 的指纹, 用于安装前的完整性校验, 防止 GitHub
Release CDN 被劫持或镜像源被投毒.

数据源
------

按优先级:

1. **中转代理** (`/v1/release/napcat`): 走 HMAC 签名 + 时钟自愈; 中转命中
   缓存时延迟最低 (5 分钟新鲜窗口).
2. **GitHub 官方 API**: 中转不可用时直连 ``api.github.com``;
   若用户在设置中配置了 GitHub Personal Token, 会自动带 ``Authorization``
   把限速从 60/h 拉到 5000/h.

网络降级矩阵
------------

- 网络正常: 写缓存, ``fetch()`` 返回 ``FETCHED``
- 网络失败但缓存命中: 用缓存, 返回 ``CACHED``
- 网络失败 + 无缓存: 返回 ``NONE``, 调用方应弹二次确认对话框
- Hash 不匹配: 校验环节抛错(本模块不参与)

线程安全
--------

``fetch`` / ``lookup`` 均通过 RLock 保护; UI 启动期 worker 线程拉取与主线程
查询可并行. 不支持子进程间共享 (无锁文件), 不必要.
"""
from __future__ import annotations

# 标准库导入
import enum
import json
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

# 项目内模块导入
from src.core.logging import LogSource, LogType, logger
from src.core.network._build_constants import PROXY_BASE_URL


# ==================== 常量 ====================
#: 中转代理路径; 与 [`Urls.NAPCATQQ_REPO_API_PATH`](src/core/network/urls.py) 对齐
DEFAULT_PROXY_PATH: str = "/v1/release/napcat"

#: 中转代理完整 URL (主源); 走 HMAC 签名, 国内访问最快
DEFAULT_PRIMARY_SOURCE: str = f"{PROXY_BASE_URL.rstrip('/')}{DEFAULT_PROXY_PATH}"

#: GitHub 官方 API (兜底源); 中转挂掉时直连, 可携带 PAT 解 60/h 限速
DEFAULT_FALLBACK_SOURCE: str = (
    "https://api.github.com/repos/NapNeko/NapCatQQ/releases/latest"
)

DEFAULT_SOURCES: tuple[str, ...] = (DEFAULT_PRIMARY_SOURCE, DEFAULT_FALLBACK_SOURCE)

#: 缓存 TTL: 7 天. 超过仍可用 (``fetch`` 返回 CACHED), 仅 ``is_cache_fresh`` 转 False.
DEFAULT_CACHE_TTL_SECONDS: int = 7 * 24 * 3600

#: 单源 HTTP 超时 (连接 + 读取总和), 大陆 -> github 通常 5-15s, 给 20s 上限.
DEFAULT_HTTP_TIMEOUT_SECONDS: float = 20.0

#: GitHub release JSON 常态 ~30KB / 5 个 asset; 1MB 已是 30 倍冗余, 视为异常.
MAX_PAYLOAD_BYTES: int = 1024 * 1024

#: Shell zip 的 asset 文件名 (上游约定固定)
_SHELL_ASSET_NAME: str = "NapCat.Shell.zip"

#: Framework zip 的 asset 文件名 (上游约定固定)
_FRAMEWORK_ASSET_NAME: str = "NapCat.Framework.zip"


# ==================== 数据模型 ====================
@dataclass(slots=True, frozen=True)
class ReleaseHashEntry:
    """单条 release-hash 条目 (规范化后).

    内部 ``version`` 不带 ``v`` 前缀, 与 NapCat ``napcat.mjs`` 中
    ``napCatVersion`` 字段对齐, 便于直接用作字典 key 与版本比较.
    """

    #: 版本号字符串, 不含 ``v`` 前缀 (例如 ``"4.18.2"``)
    version: str

    #: ``NapCat.Shell.zip`` 的 SHA256 (64 位十六进制小写字符串)
    shell_sha256: str

    #: ``NapCat.Framework.zip`` 的 SHA256; 当前 Desktop 不消费, 但保留以便未来扩展.
    #: 上游缺该 asset 时为空串.
    framework_sha256: str

    #: ISO 8601 时间戳, 仅展示用 (取自 release ``published_at`` / ``created_at``)
    updated_at: str = ""

    @classmethod
    def from_github_release(cls, payload: Any) -> "ReleaseHashEntry | None":
        """从 GitHub Releases API 单条 release dict 构造对象.

        必须命中 ``NapCat.Shell.zip`` 这条 asset 且 digest 合法; 缺失或非法
        ``framework`` digest 不阻塞 (仅置空), 因为当前主校验目标是 shell.
        """
        if not isinstance(payload, dict):
            return None

        tag = payload.get("tag_name")
        if not isinstance(tag, str) or not tag.strip():
            return None
        normalized_version = _normalize_version(tag)
        if not normalized_version:
            return None

        assets = payload.get("assets")
        if not isinstance(assets, list):
            return None

        shell_sha = _extract_asset_sha256(assets, _SHELL_ASSET_NAME)
        if shell_sha is None:
            return None

        framework_sha = _extract_asset_sha256(assets, _FRAMEWORK_ASSET_NAME) or ""

        updated_at = payload.get("published_at") or payload.get("created_at") or ""
        if not isinstance(updated_at, str):
            updated_at = ""

        return cls(
            version=normalized_version,
            shell_sha256=shell_sha.lower(),
            framework_sha256=framework_sha.lower(),
            updated_at=updated_at,
        )

    @classmethod
    def from_cache_dict(cls, data: Any) -> "ReleaseHashEntry | None":
        """从缓存文件单条 entry 还原对象; 字段缺失或非法 hex 时返回 ``None``."""
        if not isinstance(data, dict):
            return None

        raw_version = data.get("version")
        if not isinstance(raw_version, str) or not raw_version.strip():
            return None
        normalized_version = _normalize_version(raw_version)
        if not normalized_version:
            return None

        shell_sha = data.get("shell_sha256")
        if not _is_valid_sha256_hex(shell_sha):
            return None

        framework_raw = data.get("framework_sha256", "")
        framework_sha = (
            framework_raw.lower()
            if isinstance(framework_raw, str) and _is_valid_sha256_hex(framework_raw)
            else ""
        )

        updated_at = data.get("updated_at", "")
        if not isinstance(updated_at, str):
            updated_at = ""

        return cls(
            version=normalized_version,
            shell_sha256=str(shell_sha).lower(),
            framework_sha256=framework_sha,
            updated_at=updated_at,
        )


class ReleaseHashFetchOutcome(enum.Enum):
    """``ReleaseHashService.fetch`` 的结果分类."""

    #: 远端拉取成功, 缓存已更新
    FETCHED = "fetched"

    #: 远端失败但缓存命中, 当前查询仍可用
    CACHED = "cached"

    #: 远端失败 + 无缓存, 调用方应走"二次确认"降级路径
    NONE = "none"


@dataclass(slots=True, frozen=True)
class ReleaseHashFetchResult:
    """``fetch`` 返回值."""

    outcome: ReleaseHashFetchOutcome

    #: 当前内存中已加载的条目数 (含从缓存恢复的)
    loaded_entries: int = 0

    #: ``fetched_at`` 时间戳; 当前为 ``CACHED`` / ``NONE`` 时可能是历史值或 ``None``
    fetched_at: float | None = None


# ==================== 工具函数 ====================
def _normalize_version(value: str) -> str:
    """归一化版本字符串, 去掉 ``v`` / ``V`` 前缀, 去除前后空白."""
    text = value.strip()
    if text.startswith(("v", "V")):
        text = text[1:]
    return text


def _is_valid_sha256_hex(value: Any) -> bool:
    """SHA256 必须是 64 位 hex 小写/大写均接受."""
    if not isinstance(value, str):
        return False
    if len(value) != 64:
        return False
    try:
        bytes.fromhex(value)
    except ValueError:
        return False
    return True


def _extract_asset_sha256(assets: list, asset_name: str) -> str | None:
    """从 GitHub release ``assets`` 数组提取指定文件名的 SHA256 hex.

    ``digest`` 字段格式为 ``"sha256:<64-hex>"``; 算法前缀不匹配 / hex 非法
    时返回 ``None``.
    """
    for asset in assets:
        if not isinstance(asset, dict):
            continue
        if asset.get("name") != asset_name:
            continue
        digest = asset.get("digest")
        if not isinstance(digest, str):
            return None
        prefix = "sha256:"
        if not digest.startswith(prefix):
            return None
        hex_part = digest[len(prefix):]
        if not _is_valid_sha256_hex(hex_part):
            return None
        return hex_part
    return None


def _default_fetcher(url: str) -> str:
    """生产环境 fetcher: 中转走 HMAC 签名 + 时钟自愈; 直连可带 PAT.

    选 stdlib 而非 ``httpx`` 原因:
    - 该 service 在 Desktop 启动期被调用, ``httpx`` 首次 import 较重
    - 仅做一次 GET, 不需要 ``httpx`` 的连接池能力
    - 测试可直接注入 ``fetcher`` 替身, 不必 mock 复杂的 httpx 客户端
    """
    is_proxy = url.startswith(PROXY_BASE_URL.rstrip("/"))

    base_headers = {
        "User-Agent": "NapCatQQ-Desktop/release-hash-service",
        "Accept": "application/vnd.github+json",
    }

    def _do_request(extra_headers: dict[str, str]) -> str:
        request = urllib.request.Request(url, headers={**base_headers, **extra_headers})
        with urllib.request.urlopen(  # noqa: S310 - 仅限白名单源
            request, timeout=DEFAULT_HTTP_TIMEOUT_SECONDS
        ) as response:
            raw = response.read(MAX_PAYLOAD_BYTES + 1)
        if len(raw) > MAX_PAYLOAD_BYTES:
            raise RuntimeError(
                f"release payload 超过 {MAX_PAYLOAD_BYTES} 字节上限, 视为异常"
            )
        return raw.decode("utf-8", errors="replace")

    if is_proxy:
        # 延迟 import 避免 release_hash_service 被纯逻辑场景 (单测) 拖入 cfg / qt 链路
        from src.core.network.proxy_signer import ProxySigner

        signer = ProxySigner.instance()
        path = urllib.parse.urlparse(url).path or DEFAULT_PROXY_PATH
        try:
            return _do_request(signer.sign_headers(path))
        except urllib.error.HTTPError as exc:
            # 中转 403 + 响应头带 X-Server-Time 时, 校时一次再试
            if exc.code == 403 and exc.headers is not None:
                if signer.update_offset_from_response(exc.headers):
                    return _do_request(signer.sign_headers(path))
            raise

    # 直连分支: 若用户配置了 PAT 则带上, 把限速从 60/h 拉到 5000/h
    extra: dict[str, str] = {}
    try:
        from src.core.config import cfg

        token = (cfg.get(cfg.github_personal_token) or "").strip()
        if token:
            extra["Authorization"] = f"Bearer {token}"
    except Exception:  # noqa: BLE001 - cfg 不可用 (测试 / 启动早期) 退化为无 token 直连
        pass

    return _do_request(extra)


# ==================== 服务主体 ====================
@dataclass
class ReleaseHashService:
    """上游 release-hash 拉取与查询服务.

    使用方式::

        service = ReleaseHashService()
        service.fetch()                       # 启动期或安装前异步调用
        entry = service.lookup("v4.18.2")     # 查询单版本
        if entry is not None:
            assert downloaded_sha256 == entry.shell_sha256

    Args:
        cache_path: 缓存文件路径; 缺省 ``{data_path}/runtime/cache/napcat-release-hash.json``
        sources: 候选 URL 列表, 顺序尝试; 缺省走 ``DEFAULT_SOURCES``
        fetcher: 单源拉取闭包, 失败应抛异常; 测试可注入
        ttl_seconds: ``is_cache_fresh`` 判定 TTL; 缺省 7 天
    """

    cache_path: Path = field(default_factory=lambda: _default_cache_path())
    sources: tuple[str, ...] = DEFAULT_SOURCES
    fetcher: Callable[[str], str] = field(default=_default_fetcher)
    ttl_seconds: int = DEFAULT_CACHE_TTL_SECONDS

    _entries_by_version: dict[str, ReleaseHashEntry] = field(
        default_factory=dict, init=False, repr=False
    )
    _fetched_at: float | None = field(default=None, init=False, repr=False)
    _lock: threading.RLock = field(default_factory=threading.RLock, init=False, repr=False)
    _loaded_from_disk: bool = field(default=False, init=False, repr=False)

    # ==================== 公共 API ====================
    def fetch(self, *, force: bool = False) -> ReleaseHashFetchResult:
        """从所有 ``sources`` 顺序拉取最新 release.

        网络失败时回落到磁盘缓存; 全部失败且无缓存时返回 ``NONE``.

        Args:
            force: 当前未使用, 保留接口给未来 "强制忽略 TTL" 场景.
        """
        del force  # 占位

        with self._lock:
            for source in self.sources:
                try:
                    raw = self.fetcher(source)
                except Exception as exc:  # noqa: BLE001 - 任意网络错误都视为该源失败
                    logger.warning(
                        f"release_hash_service: 拉取失败, 切换下一源: source={source}, exc={type(exc).__name__}: {exc}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )
                    continue

                entries = self._parse_payload(raw)
                if entries is None:
                    logger.warning(
                        f"release_hash_service: 上游 payload 解析失败, 切换下一源: source={source}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )
                    continue

                # 拉取成功, 更新内存 + 落盘
                self._entries_by_version = {entry.version: entry for entry in entries}
                self._fetched_at = time.time()
                self._save_cache()
                logger.info(
                    f"release_hash_service: 已拉取并缓存 {len(entries)} 个版本: source={source}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                return ReleaseHashFetchResult(
                    outcome=ReleaseHashFetchOutcome.FETCHED,
                    loaded_entries=len(entries),
                    fetched_at=self._fetched_at,
                )

            # 所有源都失败, 尝试缓存
            if self._load_cache_if_needed():
                logger.warning(
                    f"release_hash_service: 全部源失败, 已回落到磁盘缓存 ({len(self._entries_by_version)} 个版本)",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                return ReleaseHashFetchResult(
                    outcome=ReleaseHashFetchOutcome.CACHED,
                    loaded_entries=len(self._entries_by_version),
                    fetched_at=self._fetched_at,
                )

            logger.warning(
                "release_hash_service: 全部源失败且无缓存, 返回 NONE",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return ReleaseHashFetchResult(
                outcome=ReleaseHashFetchOutcome.NONE,
                loaded_entries=0,
                fetched_at=None,
            )

    def lookup(self, version: str) -> ReleaseHashEntry | None:
        """按版本号查询 hash 条目.

        Args:
            version: 接受 ``"v4.18.2"`` / ``"4.18.2"`` / ``"V4.18.2"`` 三种形式;
                空串与未知版本返回 ``None``.

        Returns:
            匹配的 :class:`ReleaseHashEntry`, 未命中时 ``None``.
        """
        if not version:
            return None
        with self._lock:
            self._load_cache_if_needed()
            normalized = _normalize_version(version)
            if not normalized:
                return None
            return self._entries_by_version.get(normalized)

    def is_cache_fresh(self) -> bool:
        """缓存 ``fetched_at`` 距今 < ``ttl_seconds`` 时返回 True; 无缓存返回 False."""
        with self._lock:
            self._load_cache_if_needed()
            if self._fetched_at is None:
                return False
            return (time.time() - self._fetched_at) < self.ttl_seconds

    def loaded_entries(self) -> int:
        """当前内存中已加载条目数 (含从缓存恢复)."""
        with self._lock:
            self._load_cache_if_needed()
            return len(self._entries_by_version)

    # ==================== 内部 ====================
    def _parse_payload(self, raw: str) -> list[ReleaseHashEntry] | None:
        """解析上游 payload (单个 GitHub release dict); 非法时返回 ``None``."""
        try:
            data = json.loads(raw)
        except (json.JSONDecodeError, ValueError):
            return None
        if not isinstance(data, dict):
            return None
        entry = ReleaseHashEntry.from_github_release(data)
        if entry is None:
            return None
        return [entry]

    def _save_cache(self) -> None:
        """把当前内存条目落到磁盘 (附 ``fetched_at`` 时间戳).

        缓存 schema 自定义 (与上游 release JSON 解耦), 字段对应 :class:`ReleaseHashEntry`,
        外层包一层 ``{ "fetched_at": <unix>, "entries": [...] }``.

        失败时仅记 warning, 不抛 (网络拉取已成功, 没必要因为写盘问题失败).
        """
        try:
            self.cache_path.parent.mkdir(parents=True, exist_ok=True)
            payload = {
                "fetched_at": self._fetched_at or time.time(),
                "entries": [
                    {
                        "version": f"v{entry.version}",
                        "shell_sha256": entry.shell_sha256,
                        "framework_sha256": entry.framework_sha256,
                        "updated_at": entry.updated_at,
                    }
                    for entry in self._entries_by_version.values()
                ],
            }
            tmp = self.cache_path.with_name(self.cache_path.name + ".tmp")
            tmp.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
            tmp.replace(self.cache_path)
        except OSError as exc:
            logger.warning(
                f"release_hash_service: 缓存写盘失败 (忽略): {exc!r}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )

    def _load_cache_if_needed(self) -> bool:
        """首次访问时把磁盘缓存读入内存. 若内存已加载 / 文件不存在则跳过.

        Returns:
            True 若加载到了至少一条; False 否则.
        """
        if self._loaded_from_disk:
            return bool(self._entries_by_version)
        self._loaded_from_disk = True

        if not self.cache_path.exists():
            return False

        try:
            payload = json.loads(self.cache_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError, ValueError):
            return False

        if not isinstance(payload, dict):
            return False

        entries_raw = payload.get("entries")
        if not isinstance(entries_raw, list):
            return False

        loaded: dict[str, ReleaseHashEntry] = {}
        for item in entries_raw:
            entry = ReleaseHashEntry.from_cache_dict(item)
            if entry is not None:
                loaded[entry.version] = entry

        if not loaded:
            return False

        self._entries_by_version = loaded
        fetched_at = payload.get("fetched_at")
        if isinstance(fetched_at, (int, float)):
            self._fetched_at = float(fetched_at)
        return True


def _default_cache_path() -> Path:
    """缺省缓存路径: ``{data_path}/runtime/cache/napcat-release-hash.json``.

    延迟 import ``app_paths`` 避免循环依赖与 import 期副作用.
    """
    from src.core.platform.app_paths import resolve_app_data_path

    return resolve_app_data_path() / "runtime" / "cache" / "napcat-release-hash.json"


__all__: tuple[str, ...] = (
    "DEFAULT_PROXY_PATH",
    "DEFAULT_PRIMARY_SOURCE",
    "DEFAULT_FALLBACK_SOURCE",
    "DEFAULT_SOURCES",
    "DEFAULT_CACHE_TTL_SECONDS",
    "DEFAULT_HTTP_TIMEOUT_SECONDS",
    "ReleaseHashEntry",
    "ReleaseHashFetchOutcome",
    "ReleaseHashFetchResult",
    "ReleaseHashService",
)

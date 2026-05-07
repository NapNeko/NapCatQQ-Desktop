# -*- coding: utf-8 -*-
"""[`ReleaseHashService`](src/core/versioning/release_hash_service.py): 上游 NapCat
release-hash 仓库的拉取/缓存/查询服务 (P5 安全收尾 F1.1).

设计目标
--------

NapCat 官方维护
[`napcat-release-hash`](https://github.com/NapNeko/napcat-release-hash) 仓库,
按版本提供 ``shell.sha512`` 与 ``framework.sha512`` 两个 artifact 的指纹.
Desktop 在执行本地 / 远端 NapCat 安装前消费这些指纹做完整性校验, 防止
GitHub Release CDN 被劫持或镜像源被投毒.

网络降级矩阵
------------

按 [`docs/requirements/2026-05-07-remote-ssh-security-hardening.md`](../../../docs/requirements/2026-05-07-remote-ssh-security-hardening.md) §F1.2:

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
import urllib.request
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

# 项目内模块导入
from src.core.logging import LogSource, LogType, logger


# ==================== 常量 ====================
#: 上游主源 (GitHub raw 直链), 大陆网络可能慢但内容最新
DEFAULT_PRIMARY_SOURCE: str = (
    "https://raw.githubusercontent.com/NapNeko/napcat-release-hash/main/release.json"
)

#: 备选源 (jsdelivr CDN), 国内访问通常更快, 但有 ~10 分钟同步延迟
DEFAULT_FALLBACK_SOURCE: str = (
    "https://cdn.jsdelivr.net/gh/NapNeko/napcat-release-hash@main/release.json"
)

DEFAULT_SOURCES: tuple[str, ...] = (DEFAULT_PRIMARY_SOURCE, DEFAULT_FALLBACK_SOURCE)

#: 缓存 TTL: 7 天. 超过仍可用 (``fetch`` 返回 CACHED), 仅 ``is_cache_fresh`` 转 False.
DEFAULT_CACHE_TTL_SECONDS: int = 7 * 24 * 3600

#: 单源 HTTP 超时 (连接 + 读取总和), 大陆 -> github 通常 5-15s, 给 20s 上限.
DEFAULT_HTTP_TIMEOUT_SECONDS: float = 20.0

#: 上游 release.json 当前 ~10KB / 100 个版本; 1MB 已是 10 倍冗余, 超过即视为异常.
MAX_PAYLOAD_BYTES: int = 1024 * 1024


# ==================== 数据模型 ====================
@dataclass(slots=True, frozen=True)
class ReleaseHashEntry:
    """单条 release-hash 条目 (规范化后).

    内部 ``version`` 不带 ``v`` 前缀, 与 NapCat ``napcat.mjs`` 中
    ``napCatVersion`` 字段对齐, 便于直接用作字典 key 与版本比较.
    """

    #: 版本号字符串, 不含 ``v`` 前缀 (例如 ``"4.18.1"``)
    version: str

    #: ``NapCat.Shell.zip`` 的 SHA512 (128 位十六进制小写字符串)
    shell_sha512: str

    #: LinuxQQ 框架包的 SHA512; 当前 Desktop 不消费, 但保留以便未来扩展
    framework_sha512: str

    #: ISO 8601 时间戳, 仅展示用
    updated_at: str = ""

    @classmethod
    def from_payload(cls, payload: Any) -> "ReleaseHashEntry | None":
        """从上游单条 entry 构造对象; 字段缺失或非法 hex 时返回 ``None``."""
        if not isinstance(payload, dict):
            return None

        raw_version = payload.get("version")
        if not isinstance(raw_version, str) or not raw_version.strip():
            return None
        normalized_version = _normalize_version(raw_version)
        if not normalized_version:
            return None

        shell = payload.get("shell")
        framework = payload.get("framework")
        if not isinstance(shell, dict) or not isinstance(framework, dict):
            return None

        shell_sha = shell.get("sha512")
        framework_sha = framework.get("sha512")
        if not _is_valid_sha512_hex(shell_sha) or not _is_valid_sha512_hex(framework_sha):
            return None

        updated_at = payload.get("updatedAt", "")
        if not isinstance(updated_at, str):
            updated_at = ""

        return cls(
            version=normalized_version,
            shell_sha512=str(shell_sha).lower(),
            framework_sha512=str(framework_sha).lower(),
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


def _is_valid_sha512_hex(value: Any) -> bool:
    """SHA512 必须是 128 位 hex 小写/大写均接受."""
    if not isinstance(value, str):
        return False
    if len(value) != 128:
        return False
    try:
        bytes.fromhex(value)
    except ValueError:
        return False
    return True


def _default_fetcher(url: str) -> str:
    """生产环境 fetcher: 用 stdlib urllib 拉取, 限制 payload 大小.

    选 stdlib 而非 ``httpx`` 原因:
    - 该 service 在 Desktop 启动期被调用, ``httpx`` 首次 import 较重
    - 仅做一次 GET, 不需要 ``httpx`` 的连接池能力
    - 测试可直接注入 ``fetcher`` 替身, 不必 mock 复杂的 httpx 客户端
    """
    request = urllib.request.Request(
        url,
        headers={
            "User-Agent": "NapCatQQ-Desktop/release-hash-service",
            "Accept": "application/json",
        },
    )
    with urllib.request.urlopen(  # noqa: S310 - 仅限白名单源, 见 DEFAULT_SOURCES
        request, timeout=DEFAULT_HTTP_TIMEOUT_SECONDS
    ) as response:
        # 防御过大 payload (恶意源/被劫持时可能返回无意义大 blob)
        raw = response.read(MAX_PAYLOAD_BYTES + 1)
    if len(raw) > MAX_PAYLOAD_BYTES:
        raise RuntimeError(f"release.json 超过 {MAX_PAYLOAD_BYTES} 字节上限, 视为异常")
    return raw.decode("utf-8", errors="replace")


# ==================== 服务主体 ====================
@dataclass
class ReleaseHashService:
    """上游 release-hash 拉取与查询服务.

    使用方式::

        service = ReleaseHashService()
        service.fetch()                       # 启动期或安装前异步调用
        entry = service.lookup("v4.18.1")     # 查询单版本
        if entry is not None:
            assert downloaded_sha512 == entry.shell_sha512

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
        """从所有 ``sources`` 顺序拉取最新 release.json.

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
            version: 接受 ``"v4.18.1"`` / ``"4.18.1"`` / ``"V4.18.1"`` 三种形式;
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
        """解析上游 payload; 非法 / 无可用条目时返回 ``None``."""
        try:
            data = json.loads(raw)
        except (json.JSONDecodeError, ValueError):
            return None
        if not isinstance(data, list):
            return None
        entries: list[ReleaseHashEntry] = []
        for item in data:
            entry = ReleaseHashEntry.from_payload(item)
            if entry is not None:
                entries.append(entry)
        if not entries:
            return None
        return entries

    def _save_cache(self) -> None:
        """把当前内存条目落到磁盘 (附 ``fetched_at`` 时间戳).

        缓存文件 schema 与上游 release.json 保持兼容 (``version`` 带 ``v`` 前缀,
        ``shell.sha512`` / ``framework.sha512`` 嵌套), 仅在外层包一层
        ``{ "fetched_at": <unix>, "entries": [...] }``. 用户手工核对缓存内容时
        与上游 release.json 一致, 同时也兼容直接把上游原始 release.json 复制成
        缓存文件 (省去 fetched_at 时退化为 0).

        失败时仅记 warning, 不抛 (网络拉取已成功, 没必要因为写盘问题失败).
        """
        try:
            self.cache_path.parent.mkdir(parents=True, exist_ok=True)
            payload = {
                "fetched_at": self._fetched_at or time.time(),
                "entries": [
                    {
                        "version": f"v{entry.version}",
                        "shell": {"sha512": entry.shell_sha512},
                        "framework": {"sha512": entry.framework_sha512},
                        "updatedAt": entry.updated_at,
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

        # 缓存文件 schema 与上游 release.json 完全一致, 直接复用 from_payload.
        loaded: dict[str, ReleaseHashEntry] = {}
        for item in entries_raw:
            entry = ReleaseHashEntry.from_payload(item)
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

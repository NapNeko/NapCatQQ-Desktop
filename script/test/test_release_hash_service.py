# -*- coding: utf-8 -*-
"""[`ReleaseHashService`](src/core/versioning/release_hash_service.py) 单测.

覆盖目标 (P5 安全收尾 F1.1):

- 上游 release.json 解析正确 (含 `v` 前缀归一化)
- 缓存写入 / 读取
- 多源 fallback (主源失败时切到 jsdelivr)
- 损坏 JSON 不阻断, 返回空集
- TTL 判定
- ``lookup`` 容忍 ``v4.18.1`` 与 ``4.18.1`` 两种形式
"""
from __future__ import annotations

# 标准库导入
import json
import time
from pathlib import Path

# 第三方库导入
import pytest


# ==================== Fixtures ====================
SAMPLE_RELEASE_JSON: list[dict] = [
    {
        "version": "v4.17.31",
        "shell": {
            "sha512": "b388f2eb6944ce9df7c2f877777c01a92cfcbbd3e342dc964cfae8d6c4f1973b"
                       "0e2e51bc863ad6cbd2acab5f462af09385eac4e3ac6f184e60a722c312f0c4c3"
        },
        "framework": {
            "sha512": "8d17a09803aa1ad0ab02c0618911e44f5ce6209d05ccf9c3659904cd4448551c"
                       "c6a11303eb08a64bdd613432958ece6ee639b981298b5d654d8cb86511de4dec"
        },
        "updatedAt": "2026-03-04T13:57:07.396Z",
    },
    {
        "version": "v4.18.1",
        "shell": {
            "sha512": "51d3d40c5141440cd623d64d8034514d7a0d2ce8a3ccc49407327dde53af35c0"
                       "d1751be384e7ff0f8e35979fe2479332bd828f72bd566730bb004a5073ee2bf6"
        },
        "framework": {
            "sha512": "c6607afac8ba23e58bcec869c73772549c150cc701d751cdc2a7fee234ca24a5"
                       "11337d02d6587e1a086cc5217073f32cd6cc95e9d16d16d15f933d54bdcd4df0"
        },
        "updatedAt": "2026-04-26T10:15:05.272Z",
    },
]


@pytest.fixture
def release_hash_service_module():
    """延迟 import 待测模块, 避免 collection 期 ImportError 阻断其他测试."""
    from src.core.versioning import release_hash_service as module

    return module


@pytest.fixture
def cache_path(tmp_path: Path) -> Path:
    return tmp_path / "napcat-release-hash.json"


# ==================== 模型解析 ====================
def test_release_hash_entry_normalizes_version_prefix(release_hash_service_module) -> None:
    """``v4.18.1`` 与 ``4.18.1`` 应统一存为不带 ``v`` 的内部形式."""
    entry = release_hash_service_module.ReleaseHashEntry.from_payload(SAMPLE_RELEASE_JSON[1])

    assert entry is not None
    assert entry.version == "4.18.1"
    assert len(entry.shell_sha512) == 128
    assert entry.shell_sha512.startswith("51d3d40c")


def test_release_hash_entry_rejects_bad_hex(release_hash_service_module) -> None:
    """SHA512 非 128 位 / 含非法字符 / 缺字段 -> 返回 None."""
    bad_payloads = [
        {"version": "v4.0.0", "shell": {"sha512": "deadbeef"}, "framework": {"sha512": "x" * 128}},  # 长度不足
        {"version": "v4.0.0", "shell": {"sha512": "z" * 128}, "framework": {"sha512": "a" * 128}},  # 非 hex
        {"version": "v4.0.0", "shell": {}, "framework": {"sha512": "a" * 128}},  # 缺 shell.sha512
        {"version": "v4.0.0", "framework": {"sha512": "a" * 128}},  # 缺 shell
        {},  # 空对象
    ]
    for payload in bad_payloads:
        entry = release_hash_service_module.ReleaseHashEntry.from_payload(payload)
        assert entry is None, f"payload 应当被拒: {payload}"


# ==================== 缓存读写 ====================
def test_service_writes_and_loads_cache(release_hash_service_module, cache_path: Path) -> None:
    """fetch 成功后落盘, 第二次 lookup 直接命中缓存."""
    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://primary",),
        fetcher=lambda url: json.dumps(SAMPLE_RELEASE_JSON),
    )
    result = service.fetch()

    assert result.outcome == release_hash_service_module.ReleaseHashFetchOutcome.FETCHED
    assert cache_path.exists()
    payload = json.loads(cache_path.read_text(encoding="utf-8"))
    assert "fetched_at" in payload
    assert isinstance(payload["entries"], list)
    assert len(payload["entries"]) == len(SAMPLE_RELEASE_JSON)

    entry = service.lookup("v4.18.1")
    assert entry is not None
    assert entry.version == "4.18.1"


def test_service_falls_back_to_cache_when_all_sources_fail(
    release_hash_service_module, cache_path: Path
) -> None:
    """全部源拉取失败时, 应回落到磁盘缓存且 outcome=CACHED."""
    # 预先写入一份缓存
    cache_path.write_text(
        json.dumps(
            {
                "fetched_at": time.time() - 100,
                "entries": SAMPLE_RELEASE_JSON,
            },
            ensure_ascii=False,
        ),
        encoding="utf-8",
    )

    def always_fail(url: str) -> str:
        raise RuntimeError(f"网络异常: {url}")

    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://a", "inline://b"),
        fetcher=always_fail,
    )
    result = service.fetch()

    assert result.outcome == release_hash_service_module.ReleaseHashFetchOutcome.CACHED
    assert service.lookup("v4.18.1") is not None


def test_service_returns_none_when_no_cache_and_network_fails(
    release_hash_service_module, cache_path: Path
) -> None:
    """无缓存 + 全部源失败 -> outcome=NONE, lookup 返回 None."""
    def always_fail(url: str) -> str:
        raise RuntimeError("offline")

    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://a",),
        fetcher=always_fail,
    )
    result = service.fetch()

    assert result.outcome == release_hash_service_module.ReleaseHashFetchOutcome.NONE
    assert service.lookup("v4.18.1") is None


def test_service_multi_source_fallback(release_hash_service_module, cache_path: Path) -> None:
    """主源抛错时切到下一个源."""
    calls: list[str] = []

    def fetcher(url: str) -> str:
        calls.append(url)
        if url == "inline://primary":
            raise RuntimeError("primary down")
        return json.dumps(SAMPLE_RELEASE_JSON)

    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://primary", "inline://secondary"),
        fetcher=fetcher,
    )
    result = service.fetch()

    assert result.outcome == release_hash_service_module.ReleaseHashFetchOutcome.FETCHED
    assert calls == ["inline://primary", "inline://secondary"]
    assert service.lookup("4.17.31") is not None


def test_service_handles_corrupt_cache(release_hash_service_module, cache_path: Path) -> None:
    """缓存文件损坏 (非合法 JSON) 应静默退化为空集, 不抛."""
    cache_path.write_text("{not valid json", encoding="utf-8")

    def always_fail(url: str) -> str:
        raise RuntimeError("offline")

    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://a",),
        fetcher=always_fail,
    )
    result = service.fetch()

    assert result.outcome == release_hash_service_module.ReleaseHashFetchOutcome.NONE
    assert service.lookup("v4.18.1") is None


def test_service_handles_corrupt_remote_payload(
    release_hash_service_module, cache_path: Path
) -> None:
    """远端返回非 JSON / 不是 list -> 视为本次 fetch 失败, 不污染缓存."""
    def fetcher(url: str) -> str:
        return "<html>404</html>"

    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://a",),
        fetcher=fetcher,
    )
    result = service.fetch()

    assert result.outcome == release_hash_service_module.ReleaseHashFetchOutcome.NONE
    assert not cache_path.exists() or json.loads(cache_path.read_text(encoding="utf-8")).get("entries", []) == []


# ==================== 版本归一化 ====================
def test_lookup_accepts_both_with_and_without_v_prefix(
    release_hash_service_module, cache_path: Path
) -> None:
    """``lookup("v4.18.1")`` 与 ``lookup("4.18.1")`` 应等价."""
    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://a",),
        fetcher=lambda url: json.dumps(SAMPLE_RELEASE_JSON),
    )
    service.fetch()

    a = service.lookup("v4.18.1")
    b = service.lookup("4.18.1")
    c = service.lookup("V4.18.1")  # 大写 V 也应兼容
    assert a is not None and b is not None and c is not None
    assert a.version == b.version == c.version == "4.18.1"


def test_lookup_returns_none_for_unknown_version(
    release_hash_service_module, cache_path: Path
) -> None:
    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://a",),
        fetcher=lambda url: json.dumps(SAMPLE_RELEASE_JSON),
    )
    service.fetch()
    assert service.lookup("v9.99.99") is None
    assert service.lookup("") is None
    assert service.lookup("not-a-version") is None


# ==================== TTL ====================
def test_is_cache_fresh_respects_ttl(
    release_hash_service_module, cache_path: Path
) -> None:
    """``is_cache_fresh`` 仅在 ``fetched_at`` 距今 < TTL 时返回 True."""
    cache_path.write_text(
        json.dumps(
            {"fetched_at": time.time() - 100, "entries": SAMPLE_RELEASE_JSON},
            ensure_ascii=False,
        ),
        encoding="utf-8",
    )

    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://a",),
        fetcher=lambda url: "should not be called",
        ttl_seconds=200,
    )
    assert service.is_cache_fresh() is True

    # TTL 设为 50s, 100s 前的缓存应被视为过期
    service2 = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://a",),
        fetcher=lambda url: "should not be called",
        ttl_seconds=50,
    )
    assert service2.is_cache_fresh() is False


def test_is_cache_fresh_returns_false_when_no_cache(
    release_hash_service_module, cache_path: Path
) -> None:
    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://a",),
        fetcher=lambda url: "noop",
    )
    assert service.is_cache_fresh() is False

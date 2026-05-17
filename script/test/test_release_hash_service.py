# -*- coding: utf-8 -*-
"""[`ReleaseHashService`](src/core/versioning/release_hash_service.py) 单测.

覆盖目标 (P5 安全收尾 F1.1, 改造后):

- GitHub Releases API 单条 release dict 解析 (含 ``v`` 前缀归一化, 从
  ``assets[*].digest`` 中按文件名提取 SHA256)
- 缓存写入 / 读取 (新 schema: 自定义 entries 字段)
- 多源 fallback (中转主源失败时切到 GitHub 直连)
- 损坏 JSON / payload 不阻断, 返回空集
- TTL 判定
- ``lookup`` 容忍 ``v4.18.2`` 与 ``4.18.2`` 两种形式
"""
from __future__ import annotations

# 标准库导入
import json
import time
from pathlib import Path

# 第三方库导入
import pytest


# ==================== Fixtures ====================
SAMPLE_SHELL_SHA256 = "1345603985a24e7a48f7125916f0fb7116af2da0d2c3e21e380235b8fd580250"
SAMPLE_FRAMEWORK_SHA256 = "894237ae71bce1534adc16373ce1cbffe1420a02fc5650a5cd7020d35194104c"

#: GitHub Releases API 返回的单条 release 样本 (与 v4.18.2 实际响应同结构, 字段裁剪)
SAMPLE_GITHUB_RELEASE: dict = {
    "tag_name": "v4.18.2",
    "name": "NapCatQQ v4.18.2",
    "published_at": "2026-05-10T13:57:07Z",
    "created_at": "2026-05-10T13:50:00Z",
    "assets": [
        {
            "name": "NapCat.Framework.zip",
            "digest": f"sha256:{SAMPLE_FRAMEWORK_SHA256}",
        },
        {
            "name": "NapCat.Shell.Windows.Node.zip",
            "digest": "sha256:65d2813203a4c9769c01a75fe040f6c291bbe3bd117a31e06a2ac6112bccc48c",
        },
        {
            "name": "NapCat.Shell.Windows.OneKey.zip",
            "digest": "sha256:fa365537039e9ec29730166f3f624eb147074be18be64d1981a03f35ecb2a2af",
        },
        {
            "name": "NapCat.Shell.zip",
            "digest": f"sha256:{SAMPLE_SHELL_SHA256}",
        },
    ],
}


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
    """``v4.18.2`` 与 ``4.18.2`` 应统一存为不带 ``v`` 的内部形式."""
    entry = release_hash_service_module.ReleaseHashEntry.from_github_release(
        SAMPLE_GITHUB_RELEASE
    )

    assert entry is not None
    assert entry.version == "4.18.2"
    assert len(entry.shell_sha256) == 64
    assert entry.shell_sha256 == SAMPLE_SHELL_SHA256
    assert entry.framework_sha256 == SAMPLE_FRAMEWORK_SHA256


def test_release_hash_entry_extracts_correct_asset(release_hash_service_module) -> None:
    """assets 数组中有 4 个 zip, 必须严格按文件名匹配 NapCat.Shell.zip."""
    entry = release_hash_service_module.ReleaseHashEntry.from_github_release(
        SAMPLE_GITHUB_RELEASE
    )

    assert entry is not None
    # 不应误用 Windows.Node 或 Windows.OneKey 那两个变体
    assert entry.shell_sha256 != "65d2813203a4c9769c01a75fe040f6c291bbe3bd117a31e06a2ac6112bccc48c"
    assert entry.shell_sha256 != "fa365537039e9ec29730166f3f624eb147074be18be64d1981a03f35ecb2a2af"


def test_release_hash_entry_rejects_bad_payload(release_hash_service_module) -> None:
    """缺关键字段 / 非法 hex / sha256 前缀缺失 -> 返回 None."""
    bad_payloads = [
        # 缺 tag_name
        {"assets": [{"name": "NapCat.Shell.zip", "digest": "sha256:" + "a" * 64}]},
        # 缺 assets
        {"tag_name": "v4.18.2"},
        # assets 中无 NapCat.Shell.zip
        {
            "tag_name": "v4.18.2",
            "assets": [{"name": "Other.zip", "digest": "sha256:" + "a" * 64}],
        },
        # digest 算法前缀错
        {
            "tag_name": "v4.18.2",
            "assets": [{"name": "NapCat.Shell.zip", "digest": "md5:" + "a" * 32}],
        },
        # digest hex 长度错
        {
            "tag_name": "v4.18.2",
            "assets": [{"name": "NapCat.Shell.zip", "digest": "sha256:deadbeef"}],
        },
        # digest 非 hex
        {
            "tag_name": "v4.18.2",
            "assets": [{"name": "NapCat.Shell.zip", "digest": "sha256:" + "z" * 64}],
        },
        {},  # 空对象
    ]
    for payload in bad_payloads:
        entry = release_hash_service_module.ReleaseHashEntry.from_github_release(payload)
        assert entry is None, f"payload 应当被拒: {payload}"


def test_release_hash_entry_tolerates_missing_framework(release_hash_service_module) -> None:
    """framework asset 缺失不应阻塞 (主校验目标是 shell), framework_sha256 置空."""
    payload = {
        "tag_name": "v4.18.2",
        "assets": [
            {"name": "NapCat.Shell.zip", "digest": f"sha256:{SAMPLE_SHELL_SHA256}"},
        ],
    }
    entry = release_hash_service_module.ReleaseHashEntry.from_github_release(payload)
    assert entry is not None
    assert entry.shell_sha256 == SAMPLE_SHELL_SHA256
    assert entry.framework_sha256 == ""


# ==================== 缓存读写 ====================
def test_service_writes_and_loads_cache(release_hash_service_module, cache_path: Path) -> None:
    """fetch 成功后落盘, 第二次 lookup 直接命中缓存."""
    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://primary",),
        fetcher=lambda url: json.dumps(SAMPLE_GITHUB_RELEASE),
    )
    result = service.fetch()

    assert result.outcome == release_hash_service_module.ReleaseHashFetchOutcome.FETCHED
    assert cache_path.exists()
    payload = json.loads(cache_path.read_text(encoding="utf-8"))
    assert "fetched_at" in payload
    assert isinstance(payload["entries"], list)
    assert len(payload["entries"]) == 1
    cached_entry = payload["entries"][0]
    assert cached_entry["version"] == "v4.18.2"
    assert cached_entry["shell_sha256"] == SAMPLE_SHELL_SHA256

    entry = service.lookup("v4.18.2")
    assert entry is not None
    assert entry.version == "4.18.2"


def test_service_falls_back_to_cache_when_all_sources_fail(
    release_hash_service_module, cache_path: Path
) -> None:
    """全部源拉取失败时, 应回落到磁盘缓存且 outcome=CACHED."""
    # 预先写入一份缓存 (新 schema)
    cache_path.write_text(
        json.dumps(
            {
                "fetched_at": time.time() - 100,
                "entries": [
                    {
                        "version": "v4.18.2",
                        "shell_sha256": SAMPLE_SHELL_SHA256,
                        "framework_sha256": SAMPLE_FRAMEWORK_SHA256,
                        "updated_at": "",
                    }
                ],
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
    assert service.lookup("v4.18.2") is not None


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
    assert service.lookup("v4.18.2") is None


def test_service_multi_source_fallback(release_hash_service_module, cache_path: Path) -> None:
    """主源抛错时切到下一个源."""
    calls: list[str] = []

    def fetcher(url: str) -> str:
        calls.append(url)
        if url == "inline://primary":
            raise RuntimeError("primary down")
        return json.dumps(SAMPLE_GITHUB_RELEASE)

    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://primary", "inline://secondary"),
        fetcher=fetcher,
    )
    result = service.fetch()

    assert result.outcome == release_hash_service_module.ReleaseHashFetchOutcome.FETCHED
    assert calls == ["inline://primary", "inline://secondary"]
    assert service.lookup("4.18.2") is not None


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
    assert service.lookup("v4.18.2") is None


def test_service_handles_corrupt_remote_payload(
    release_hash_service_module, cache_path: Path
) -> None:
    """远端返回非 JSON / 不是 dict / 缺关键字段 -> 视为本次 fetch 失败, 不污染缓存."""
    def fetcher(url: str) -> str:
        return "<html>404</html>"

    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://a",),
        fetcher=fetcher,
    )
    result = service.fetch()

    assert result.outcome == release_hash_service_module.ReleaseHashFetchOutcome.NONE
    assert (
        not cache_path.exists()
        or json.loads(cache_path.read_text(encoding="utf-8")).get("entries", []) == []
    )


# ==================== 版本归一化 ====================
def test_lookup_accepts_both_with_and_without_v_prefix(
    release_hash_service_module, cache_path: Path
) -> None:
    """``lookup("v4.18.2")`` 与 ``lookup("4.18.2")`` 应等价."""
    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://a",),
        fetcher=lambda url: json.dumps(SAMPLE_GITHUB_RELEASE),
    )
    service.fetch()

    a = service.lookup("v4.18.2")
    b = service.lookup("4.18.2")
    c = service.lookup("V4.18.2")  # 大写 V 也应兼容
    assert a is not None and b is not None and c is not None
    assert a.version == b.version == c.version == "4.18.2"


def test_lookup_returns_none_for_unknown_version(
    release_hash_service_module, cache_path: Path
) -> None:
    service = release_hash_service_module.ReleaseHashService(
        cache_path=cache_path,
        sources=("inline://a",),
        fetcher=lambda url: json.dumps(SAMPLE_GITHUB_RELEASE),
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
            {
                "fetched_at": time.time() - 100,
                "entries": [
                    {
                        "version": "v4.18.2",
                        "shell_sha256": SAMPLE_SHELL_SHA256,
                        "framework_sha256": SAMPLE_FRAMEWORK_SHA256,
                        "updated_at": "",
                    }
                ],
            },
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

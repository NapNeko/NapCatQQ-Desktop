# -*- coding: utf-8 -*-
"""[`NapCatInstall.verify_archive`](src/core/installation/installers.py) 单测
(P5 安全收尾 F1.3).

覆盖:
- hash 一致 -> 不抛, archive 保留
- hash 不一致 -> 抛 ``NapCatHashMismatchError``, archive 被删除
- 服务返回 None (无 hash 数据) -> 不抛, 仅返回 ``False`` 表示未校验
"""
from __future__ import annotations

# 标准库导入
import hashlib
from pathlib import Path

# 第三方库导入
import pytest


@pytest.fixture
def archive_path(tmp_path: Path) -> Path:
    """生成一个内容固定的小文件, 充当 NapCat.Shell.zip."""
    target = tmp_path / "NapCat.Shell.zip"
    target.write_bytes(b"hello napcat shell zip content for testing")
    return target


@pytest.fixture
def expected_sha512(archive_path: Path) -> str:
    """与 ``archive_path`` 内容真实匹配的 SHA512."""
    return hashlib.sha512(archive_path.read_bytes()).hexdigest()


@pytest.fixture
def stub_release_hash_service():
    """返回一个轻量替身, 仅暴露 ``lookup`` 接口."""
    from src.core.versioning.release_hash_service import ReleaseHashEntry

    class _StubService:
        def __init__(self, entry: ReleaseHashEntry | None) -> None:
            self._entry = entry

        def lookup(self, version: str) -> ReleaseHashEntry | None:
            del version
            return self._entry

    return _StubService


def test_verify_archive_passes_when_hash_matches(
    archive_path: Path, expected_sha512: str, stub_release_hash_service
) -> None:
    from src.core.installation.installers import verify_napcat_archive
    from src.core.versioning.release_hash_service import ReleaseHashEntry

    entry = ReleaseHashEntry(
        version="4.18.1",
        shell_sha512=expected_sha512,
        framework_sha512="0" * 128,
    )
    service = stub_release_hash_service(entry)

    verified = verify_napcat_archive(
        version="v4.18.1",
        archive_path=archive_path,
        hash_service=service,
    )
    assert verified is True
    assert archive_path.exists(), "校验通过时 archive 必须保留"


def test_verify_archive_raises_and_deletes_on_mismatch(
    archive_path: Path, stub_release_hash_service
) -> None:
    from src.core.installation.errors import NapCatHashMismatchError
    from src.core.installation.installers import verify_napcat_archive
    from src.core.versioning.release_hash_service import ReleaseHashEntry

    wrong_hash = "1" * 128
    entry = ReleaseHashEntry(
        version="4.18.1",
        shell_sha512=wrong_hash,
        framework_sha512="0" * 128,
    )
    service = stub_release_hash_service(entry)

    with pytest.raises(NapCatHashMismatchError) as exc_info:
        verify_napcat_archive(
            version="v4.18.1",
            archive_path=archive_path,
            hash_service=service,
        )

    assert exc_info.value.version == "4.18.1"
    assert exc_info.value.expected == wrong_hash
    assert len(exc_info.value.actual) == 128
    assert not archive_path.exists(), "校验失败时 archive 必须被删除"


def test_verify_archive_returns_false_when_no_hash_available(
    archive_path: Path, stub_release_hash_service
) -> None:
    """上游 release.json 没有该版本时, ``verify_napcat_archive`` 不抛, 仅返回 False.

    UI 层应据此弹"二次确认"对话框, 是否在缺乏校验数据的情况下继续安装.
    """
    from src.core.installation.installers import verify_napcat_archive

    service = stub_release_hash_service(None)
    verified = verify_napcat_archive(
        version="v4.18.1",
        archive_path=archive_path,
        hash_service=service,
    )
    assert verified is False
    assert archive_path.exists(), "无 hash 数据时不应删除 archive"


def test_verify_archive_uses_streaming_for_large_files(
    tmp_path: Path, stub_release_hash_service
) -> None:
    """流式读取必须能处理 > 1 chunk 的文件 (默认 chunk 4MB), 校验结果与一次性读取一致."""
    from src.core.installation.installers import verify_napcat_archive
    from src.core.versioning.release_hash_service import ReleaseHashEntry

    big_path = tmp_path / "Big.zip"
    # 8MB 数据 (> 4MB chunk), 确保流式读取走多次 read
    big_data = b"napcat-fake-archive-block" * (8 * 1024 * 1024 // 25 + 1)
    big_path.write_bytes(big_data)
    expected = hashlib.sha512(big_data).hexdigest()

    entry = ReleaseHashEntry(
        version="4.18.1",
        shell_sha512=expected,
        framework_sha512="0" * 128,
    )
    service = stub_release_hash_service(entry)

    verified = verify_napcat_archive(
        version="v4.18.1",
        archive_path=big_path,
        hash_service=service,
    )
    assert verified is True

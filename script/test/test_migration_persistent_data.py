# -*- coding: utf-8 -*-
"""[`BotMigrationService`](src/core/operation/migration.py) 持久数据搬运单测 (P4 W3 F6).

设计要点
========

- 使用纯 Python 的 ``_StubBackend`` (in-memory dict) 模拟 ``OperationBackend`` 的字节级 IO,
  避开 LocalBackend / RemoteBackend 的真实文件 / SSH 副作用.
- 通过 ``monkeypatch`` 替换 ``BotMigrationService._persistent_data_roots`` 注入测试根目录,
  以测试 ``_collect_persistent_files`` / ``_copy_with_resume`` / ``_transfer_persistent_data``
  三个核心方法.
- 不调用 ``service.execute(plan)`` 整体路径, 因 ``execute`` 内部会触发 backend 解析 / connect /
  config 文件枚举等无关分支; 此处只测 F6 增量逻辑.
"""
from __future__ import annotations

# 标准库导入
from typing import Any

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.operation.backend import FileEntry
from src.core.operation.migration import (
    PERSISTENT_DATA_CHUNK_SIZE,
    PERSISTENT_PARTIAL_SUFFIX,
    BotMigrationError,
    BotMigrationService,
    MigrationPlan,
)


# ==================== Stub backend ====================
class _StubBackend:
    """In-memory stub 实现 ``OperationBackend`` 字节级 IO 必需子集."""

    def __init__(self) -> None:
        self.files: dict[str, bytes] = {}
        self.fail_on_append: tuple[str, int] | None = None  # (path_substring, after_n_bytes)
        self._appended_bytes: int = 0

    # ---------- connect (no-op for stub) ----------
    def connect(self) -> None:
        """Stub connect, 供 _try_reconnect 调用."""

    # ---------- bytes IO ----------
    def file_exists(self, path: str) -> bool:
        if path in self.files:
            return True
        prefix = path.rstrip("/") + "/"
        return any(k.startswith(prefix) for k in self.files)

    def file_size(self, path: str) -> int:
        if path not in self.files:
            raise FileNotFoundError(path)
        return len(self.files[path])

    def read_bytes(self, path: str, *, offset: int = 0, length: int | None = None) -> bytes:
        if path not in self.files:
            raise FileNotFoundError(path)
        data = self.files[path]
        if length is None:
            return bytes(data[offset:])
        return bytes(data[offset : offset + length])

    def append_bytes(self, path: str, data: bytes) -> None:
        # 模拟中途失败: 当指定子串命中 + 累计写入超过阈值时 raise
        if self.fail_on_append is not None:
            sub, threshold = self.fail_on_append
            if sub in path and self._appended_bytes + len(data) > threshold:
                self._appended_bytes += len(data)
                raise OSError("simulated append failure")
        existing = self.files.get(path, b"")
        self.files[path] = existing + bytes(data)
        self._appended_bytes += len(data)

    def rename(self, src: str, dst: str) -> None:
        if src not in self.files:
            raise FileNotFoundError(src)
        self.files[dst] = self.files.pop(src)

    def remove(self, path: str, *, recursive: bool = False) -> None:
        self.files.pop(path, None)
        if recursive:
            prefix = path.rstrip("/") + "/"
            for key in list(self.files):
                if key.startswith(prefix):
                    self.files.pop(key)

    # ---------- list / walk ----------
    def list_dir(self, path: str) -> list[FileEntry]:
        prefix = path.rstrip("/") + "/"
        seen_dirs: set[str] = set()
        results: list[FileEntry] = []
        for key, value in self.files.items():
            if not key.startswith(prefix):
                continue
            rest = key[len(prefix) :]
            head, _, tail = rest.partition("/")
            if not head:
                continue
            if tail:
                if head not in seen_dirs:
                    seen_dirs.add(head)
                    results.append(FileEntry(name=head, is_dir=True, size=0))
            else:
                results.append(FileEntry(name=head, is_dir=False, size=len(value)))
        return results

    def walk_files(self, root: str) -> list[tuple[str, int]]:
        prefix = root.rstrip("/") + "/"
        return [
            (key[len(prefix) :], len(value))
            for key, value in self.files.items()
            if key.startswith(prefix)
        ]


# ==================== 公共 fixture ====================
@pytest.fixture
def plan() -> MigrationPlan:
    return MigrationPlan(
        qq_id="114514",
        source_target="srv-A",
        dest_target="local",
        move_persistent_data=True,
    )


@pytest.fixture
def patch_roots(monkeypatch: pytest.MonkeyPatch) -> Any:
    """让 ``_persistent_data_roots`` 返回基于 backend 身份的固定路径列表."""

    src_roots = ["/src/data", "/src/qq"]
    dst_roots = ["/dst/data", "/dst/qq"]
    src_backend_holder: dict[str, Any] = {}

    def _fake_roots(backend: Any, backend_type: Any = None) -> list[str]:
        if "src" not in src_backend_holder:
            src_backend_holder["src"] = backend
            return src_roots
        # 第二次调用一定是 dest
        return dst_roots

    monkeypatch.setattr(
        BotMigrationService,
        "_persistent_data_roots",
        staticmethod(_fake_roots),
    )
    return src_roots, dst_roots


# ==================== 完整搬运路径 ====================
def test_full_transfer_renames_partials_to_targets(plan: MigrationPlan, patch_roots: Any) -> None:
    src_roots, dst_roots = patch_roots
    src = _StubBackend()
    dst = _StubBackend()
    src.files = {
        f"{src_roots[0]}/account.db": b"hello-db-payload",
        f"{src_roots[0]}/sub/cache.bin": b"cache-bin",
        f"{src_roots[1]}/QQ.profile": b"profile-data",
    }

    service = BotMigrationService()
    transferred, file_count = service._transfer_persistent_data(src, dst, plan)

    assert file_count == 3
    assert transferred == sum(len(v) for v in src.files.values())
    # 全部完成 -> 没有遗留 .partial
    assert not any(k.endswith(PERSISTENT_PARTIAL_SUFFIX) for k in dst.files)
    # 内容按目标根目录映射存在
    assert dst.files[f"{dst_roots[0]}/account.db"] == b"hello-db-payload"
    assert dst.files[f"{dst_roots[0]}/sub/cache.bin"] == b"cache-bin"
    assert dst.files[f"{dst_roots[1]}/QQ.profile"] == b"profile-data"


# ==================== 续传: .partial 已存在时从 offset 续读 ====================
def test_resume_continues_from_existing_partial(plan: MigrationPlan, patch_roots: Any) -> None:
    src_roots, dst_roots = patch_roots
    payload = b"a" * 600 + b"b" * 600  # 1200 字节, 两个 chunk 内
    src = _StubBackend()
    src.files = {f"{src_roots[0]}/big.bin": payload}

    dst = _StubBackend()
    # 模拟上次中断时已写入 700 字节
    dst.files = {f"{dst_roots[0]}/big.bin{PERSISTENT_PARTIAL_SUFFIX}": payload[:700]}

    service = BotMigrationService()
    transferred, file_count = service._transfer_persistent_data(src, dst, plan)

    assert file_count == 1
    # 仅再传 500 字节即可 (1200 - 700)
    assert transferred == len(payload)
    final_path = f"{dst_roots[0]}/big.bin"
    assert dst.files[final_path] == payload
    assert f"{final_path}{PERSISTENT_PARTIAL_SUFFIX}" not in dst.files


# ==================== 同 size 已存在 -> 跳过 ====================
def test_existing_same_size_target_is_skipped(plan: MigrationPlan, patch_roots: Any) -> None:
    src_roots, dst_roots = patch_roots
    payload = b"identical-payload"
    src = _StubBackend()
    src.files = {f"{src_roots[0]}/keep.bin": payload}

    dst = _StubBackend()
    dst.files = {f"{dst_roots[0]}/keep.bin": payload}  # 已存在 + size 一致

    service = BotMigrationService()
    transferred, file_count = service._transfer_persistent_data(src, dst, plan)

    # collect 阶段直接跳过, file_count = 0
    assert file_count == 0
    assert transferred == 0


# ==================== 失败保留 .partial ====================
def test_failure_keeps_partial_for_retry(plan: MigrationPlan, patch_roots: Any, monkeypatch: pytest.MonkeyPatch) -> None:
    src_roots, dst_roots = patch_roots
    payload = b"x" * (PERSISTENT_DATA_CHUNK_SIZE + 1024)  # 跨 2 chunk
    src = _StubBackend()
    src.files = {f"{src_roots[0]}/will-fail.bin": payload}

    dst = _StubBackend()
    # 第一个 chunk 写入后再写就 fail
    dst.fail_on_append = ("will-fail.bin", PERSISTENT_DATA_CHUNK_SIZE + 100)

    # 跳过 time.sleep 避免测试变慢
    monkeypatch.setattr("time.sleep", lambda _: None)

    service = BotMigrationService()
    with pytest.raises(BotMigrationError) as excinfo:
        service._transfer_persistent_data(src, dst, plan)

    assert excinfo.value.stage == "persistent_data"
    # .partial 仍在 dst 中, 已写入字节 >= 一个 chunk
    partial_path = f"{dst_roots[0]}/will-fail.bin{PERSISTENT_PARTIAL_SUFFIX}"
    assert partial_path in dst.files
    assert len(dst.files[partial_path]) >= PERSISTENT_DATA_CHUNK_SIZE
    # 真正目标文件**不**应该出现 (rename 没发生)
    assert f"{dst_roots[0]}/will-fail.bin" not in dst.files


# ==================== bytes_progress_signal 累计推进 ====================
def test_bytes_progress_signal_accumulates(plan: MigrationPlan, patch_roots: Any) -> None:
    src_roots, _ = patch_roots
    src = _StubBackend()
    src.files = {
        f"{src_roots[0]}/a.bin": b"A" * 100,
        f"{src_roots[0]}/b.bin": b"B" * 200,
    }
    dst = _StubBackend()

    service = BotMigrationService()
    samples: list[tuple[int, int]] = []
    service.bytes_progress_signal.connect(lambda done, total: samples.append((done, total)))

    service._transfer_persistent_data(src, dst, plan)

    assert samples, "bytes_progress_signal 未发射"
    # 最后一帧应为 (300, 300)
    assert samples[-1] == (300, 300)
    # transferred 单调不递减
    for prev, curr in zip(samples, samples[1:]):
        assert curr[0] >= prev[0]


# ==================== 空白名单 -> 仅 emit (0,0), 不抛错 ====================
def test_no_files_returns_zero(plan: MigrationPlan, patch_roots: Any) -> None:
    src = _StubBackend()  # 空文件树
    dst = _StubBackend()
    service = BotMigrationService()
    samples: list[tuple[int, int]] = []
    service.bytes_progress_signal.connect(lambda done, total: samples.append((done, total)))

    transferred, file_count = service._transfer_persistent_data(src, dst, plan)

    assert (transferred, file_count) == (0, 0)
    assert samples == [(0, 0)]


# ==================== _persistent_data_roots 类型守卫 ====================
def test_persistent_data_roots_rejects_unknown_backend() -> None:
    """非 LocalBackend / RemoteBackend 应直接抛 BotMigrationError."""

    class _NotABackend:
        pass

    with pytest.raises(BotMigrationError) as excinfo:
        BotMigrationService._persistent_data_roots(_NotABackend())  # type: ignore[arg-type]

    assert excinfo.value.stage == "persistent_data_roots"

# -*- coding: utf-8 -*-
"""[`local_napcat_fallback`](src/core/remote/local_napcat_fallback.py) +
[`LinuxCoreDeployment._maybe_prefetch_napcat_archive_via_local`]
(src/core/remote/deployment.py) 单元测试.

覆盖三层短路 + httpx 顺序回退 + SHA512 校验:

- ``backend_can_reach_github``: http_code 200 -> True; 000 / 4xx -> False
- ``prefetch_napcat_archive_locally`` 缓存复用 (有/无 SHA512)
- ``prefetch_napcat_archive_locally`` 镜像顺序回退 (前 N 失败, 后续成功)
- ``prefetch_napcat_archive_locally`` SHA512 不匹配 -> 删除产物 + ValueError
- ``_maybe_prefetch_napcat_archive_via_local`` 三个分支:
  ``远端已有归档 -> 跳过`` / ``GitHub 可达 -> 跳过`` / ``GitHub 不可达 -> 上传``
- 上传失败时不抛, 仅 emit ``[WARN]`` 让主流程继续
"""
from __future__ import annotations

import hashlib
from dataclasses import dataclass, field
from pathlib import Path

import httpx
import pytest

from src.core.remote.deployment import LinuxCoreDeployment
from src.core.remote.execution_backend import ExecutionBackend
from src.core.remote.local_napcat_fallback import (
    _build_candidate_urls,
    backend_can_reach_github,
    prefetch_napcat_archive_locally,
)
from src.core.remote.models import LinuxCorePaths, RemoteCommandResult


# ==================== 通用 fake backend ====================
@dataclass
class _Backend(ExecutionBackend):
    """记录调用 + 按命令前缀返回伪造结果的最小可测试 backend."""

    health_http_code: str = "200"  # backend_can_reach_github 用
    archive_exists: bool = False  # 远端 ${package_dir}/NapCat.Shell.zip 是否已存在
    archive_is_corrupted: bool = False  # 已存在的归档是否损坏 (unzip -t 失败)
    upload_should_raise: bool = False
    history: list[str] = field(default_factory=list)
    upload_calls: list[tuple[str, str]] = field(default_factory=list)
    ensure_dir_calls: list[str] = field(default_factory=list)

    def run(self, command: str, *, timeout: float | None = None, check: bool = False) -> RemoteCommandResult:
        self.history.append(command)
        # 远端 archive 存在性测试 (优先于通用 test -f 判断, 避免与 unzip -t 串名)
        if command.startswith("test -f "):
            return RemoteCommandResult(
                command=command,
                exit_status=0 if self.archive_exists else 1,
            )
        # zip 完整性校验 (deployment.py 短路 1)
        if command.startswith("unzip -t "):
            return RemoteCommandResult(
                command=command,
                exit_status=1 if self.archive_is_corrupted else 0,
            )
        # 删除残件 (短路 1 在 archive_is_corrupted 时触发)
        if command.startswith("rm -f "):
            self.archive_exists = False  # 模拟真实删除效果
            return RemoteCommandResult(command=command, exit_status=0)
        # GitHub 连通性探测
        if "https://github.com" in command:
            return RemoteCommandResult(
                command=command,
                exit_status=0,
                stdout=self.health_http_code,
            )
        return RemoteCommandResult(command=command, exit_status=0)

    def ensure_directory(self, path: str) -> RemoteCommandResult:
        self.ensure_dir_calls.append(path)
        return RemoteCommandResult(command=f"mkdir -p {path}", exit_status=0)

    def upload_file(self, local_path, target_path: str) -> None:
        if self.upload_should_raise:
            raise OSError("simulated SFTP upload failure")
        self.upload_calls.append((str(local_path), target_path))

    def download_file(self, source_path: str, local_path) -> None:  # pragma: no cover
        raise NotImplementedError

    def close(self) -> None:  # pragma: no cover
        pass


# ==================== backend_can_reach_github ====================
class TestBackendReachGithub:
    def test_http_200_means_reachable(self) -> None:
        assert backend_can_reach_github(_Backend(health_http_code="200"))

    def test_http_301_means_reachable(self) -> None:
        assert backend_can_reach_github(_Backend(health_http_code="301"))

    def test_http_000_means_unreachable(self) -> None:
        assert not backend_can_reach_github(_Backend(health_http_code="000"))

    def test_http_403_means_unreachable(self) -> None:
        assert not backend_can_reach_github(_Backend(health_http_code="403"))

    def test_log_callback_emits_diagnostic_line(self) -> None:
        lines: list[str] = []
        backend_can_reach_github(
            _Backend(health_http_code="200"),
            log_callback=lines.append,
        )
        assert any("GitHub 连通性探测" in line for line in lines)
        assert any("reachable=yes" in line for line in lines)

    def test_command_includes_max_time(self) -> None:
        backend = _Backend(health_http_code="200")
        backend_can_reach_github(backend, timeout=12)
        assert any("--max-time 12" in cmd for cmd in backend.history)


# ==================== _build_candidate_urls ====================
def test_candidate_urls_official_first_then_mirrors() -> None:
    urls = _build_candidate_urls()
    assert urls[0].startswith("https://github.com/NapNeko/NapCatQQ/releases/")
    # 至少 1 个镜像 (Urls.MIRROR_SITE 长度)
    assert len(urls) >= 2
    # 镜像应包含原 URL 作为路径后缀
    assert all(
        urls[0] in u or "github.com" in u for u in urls
    )


# ==================== prefetch_napcat_archive_locally ====================
class TestPrefetchLocally:
    def _write_zip(self, path: Path, content: bytes = b"fake-zip-bytes") -> str:
        path.write_bytes(content)
        return hashlib.sha512(content).hexdigest()

    def test_reuse_cache_without_sha512(self, tmp_path: Path) -> None:
        target = tmp_path / "NapCat.Shell.zip"
        self._write_zip(target)
        result = prefetch_napcat_archive_locally(target_path=target)
        assert result == target
        # 没动文件 (没有 .part 残留)
        assert not (tmp_path / "NapCat.Shell.zip.part").exists()

    def test_reuse_cache_with_matching_sha512(self, tmp_path: Path) -> None:
        target = tmp_path / "NapCat.Shell.zip"
        sha = self._write_zip(target)
        result = prefetch_napcat_archive_locally(
            target_path=target,
            expected_sha512=sha,
        )
        assert result == target

    def test_cache_sha512_mismatch_triggers_redownload(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        target = tmp_path / "NapCat.Shell.zip"
        self._write_zip(target, b"old-broken-bytes")

        new_content = b"fresh-good-bytes"
        new_sha = hashlib.sha512(new_content).hexdigest()

        # 注入伪造的 _download_via_httpx: 第一个候选源直接成功
        def _fake_download(url: str, target_path: Path, *, log_callback=None, should_cancel=None) -> bool:
            target_path.write_bytes(new_content)
            return True

        monkeypatch.setattr(
            "src.core.remote.local_napcat_fallback._download_via_httpx",
            _fake_download,
        )

        result = prefetch_napcat_archive_locally(
            target_path=target,
            expected_sha512=new_sha,
        )
        assert result == target
        assert target.read_bytes() == new_content

    def test_first_mirror_fails_second_succeeds(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        target = tmp_path / "NapCat.Shell.zip"
        good_content = b"final-good-bytes"
        attempts: list[str] = []

        def _fake_download(url: str, target_path: Path, *, log_callback=None, should_cancel=None) -> bool:
            attempts.append(url)
            if len(attempts) == 1:
                return False  # 第一个源失败
            target_path.write_bytes(good_content)
            return True

        monkeypatch.setattr(
            "src.core.remote.local_napcat_fallback._download_via_httpx",
            _fake_download,
        )

        result = prefetch_napcat_archive_locally(target_path=target)
        assert result.exists()
        # 至少尝试了 2 个源 (一失败一成功)
        assert len(attempts) >= 2

    def test_all_sources_fail_raises_runtimeerror(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        target = tmp_path / "NapCat.Shell.zip"

        monkeypatch.setattr(
            "src.core.remote.local_napcat_fallback._download_via_httpx",
            lambda url, target_path, *, log_callback=None, should_cancel=None: False,
        )

        with pytest.raises(RuntimeError, match="本机预下载失败"):
            prefetch_napcat_archive_locally(target_path=target)
        assert not target.exists()

    def test_should_cancel_before_first_source_aborts_immediately(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """should_cancel() 在第一个源迭代前就返回 True -> 立刻抛 RemoteDeploymentCancelledError;
        不会真正调用 _download_via_httpx (即使是 fake)."""
        from src.core.remote.errors import RemoteDeploymentCancelledError

        target = tmp_path / "NapCat.Shell.zip"
        called: list[str] = []

        def _fake_download(url, target_path, *, log_callback=None, should_cancel=None):  # noqa: ARG001
            called.append(url)
            return True

        monkeypatch.setattr(
            "src.core.remote.local_napcat_fallback._download_via_httpx",
            _fake_download,
        )

        with pytest.raises(RemoteDeploymentCancelledError):
            prefetch_napcat_archive_locally(
                target_path=target,
                should_cancel=lambda: True,  # 永远 True
            )
        # 第一个源前的检查点已经命中, _download_via_httpx 不应被调过
        assert called == []

    def test_should_cancel_in_chunk_loop_aborts_mid_download(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """should_cancel() 在第一次 chunk 之间转 True -> _download_via_httpx 内部抛取消异常;
        prefetch 顶层函数同样透出取消异常."""
        from src.core.remote.errors import RemoteDeploymentCancelledError

        target = tmp_path / "NapCat.Shell.zip"
        # 把真实 httpx.stream stub 成产生几个 chunk 的迭代器
        toggle = {"flipped": False}

        class _FakeResp:
            def raise_for_status(self) -> None:
                pass

            def iter_bytes(self):
                # 第 1 个 chunk 返回前用户已经点了"取消"
                yield b"chunk1"
                toggle["flipped"] = True
                yield b"chunk2"

            def __enter__(self):
                return self

            def __exit__(self, *_args):
                pass

        monkeypatch.setattr(
            "src.core.remote.local_napcat_fallback.httpx.stream",
            lambda *a, **kw: _FakeResp(),
        )

        # should_cancel 在 toggle 翻转后返回 True
        def _checker() -> bool:
            return toggle["flipped"]

        with pytest.raises(RemoteDeploymentCancelledError):
            prefetch_napcat_archive_locally(
                target_path=target,
                should_cancel=_checker,
            )
        # 取消时 .part 被清理
        assert not target.with_name(target.name + ".part").exists()

    def test_sha512_mismatch_after_download_deletes_file(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        target = tmp_path / "NapCat.Shell.zip"

        def _fake_download(url: str, target_path: Path, *, log_callback=None, should_cancel=None) -> bool:
            target_path.write_bytes(b"corrupted-bytes")
            return True

        monkeypatch.setattr(
            "src.core.remote.local_napcat_fallback._download_via_httpx",
            _fake_download,
        )

        wrong_sha = "0" * 128
        with pytest.raises(ValueError, match="SHA512 校验失败"):
            prefetch_napcat_archive_locally(
                target_path=target,
                expected_sha512=wrong_sha,
            )
        # 校验失败后产物必须被删除
        assert not target.exists()


# ==================== _maybe_prefetch_napcat_archive_via_local ====================
class TestMaybePrefetchOrchestration:
    """deployment.py 里的私有方法, 直接通过 LinuxCoreDeployment 实例调用."""

    def _make_deployment(self, backend: _Backend) -> LinuxCoreDeployment:
        return LinuxCoreDeployment(backend, paths=LinuxCorePaths())

    def test_skips_when_remote_archive_exists(self, tmp_path: Path) -> None:
        backend = _Backend(archive_exists=True, archive_is_corrupted=False)
        deployment = self._make_deployment(backend)
        cache = tmp_path / "NapCat.Shell.zip"

        lines: list[str] = []
        deployment._maybe_prefetch_napcat_archive_via_local(
            local_archive_cache=cache,
            expected_sha512=None,
            force_update=False,
            log_callback=lines.append,
        )

        # zip 完整性 OK, 没触发 GitHub 探测, 没触发上传
        assert any(cmd.startswith("unzip -t ") for cmd in backend.history)
        assert not any("github.com" in cmd for cmd in backend.history)
        assert backend.upload_calls == []
        assert any("zip 完整" in line for line in lines)

    def test_corrupted_remote_archive_is_deleted_and_falls_through(
        self, tmp_path: Path
    ) -> None:
        """远端有归档但 zip 损坏 -> 删除残件并走后续短路 (这里 GitHub 通则不上传)."""
        backend = _Backend(
            archive_exists=True,
            archive_is_corrupted=True,
            health_http_code="200",
        )
        deployment = self._make_deployment(backend)
        cache = tmp_path / "NapCat.Shell.zip"

        lines: list[str] = []
        deployment._maybe_prefetch_napcat_archive_via_local(
            local_archive_cache=cache,
            expected_sha512=None,
            force_update=False,
            log_callback=lines.append,
        )

        # 1. 触发了 unzip -t 完整性验证
        assert any(cmd.startswith("unzip -t ") for cmd in backend.history)
        # 2. 触发了 rm -f 删除残件
        assert any(cmd.startswith("rm -f ") for cmd in backend.history)
        # 3. 删除后落入短路 2 -> GitHub 探测 (这里返回 200 -> 不上传)
        assert any("https://github.com" in cmd for cmd in backend.history)
        assert backend.upload_calls == []
        # 用户看到"已删除并重新下载"的提示
        assert any("归档损坏" in line for line in lines)

    def test_corrupted_archive_with_github_unreachable_triggers_upload(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """损坏 + GitHub 不通的最坏场景: 删除残件 + 本机下载 + SFTP 重新上传."""
        backend = _Backend(
            archive_exists=True,
            archive_is_corrupted=True,
            health_http_code="000",
        )
        deployment = self._make_deployment(backend)
        cache = tmp_path / "NapCat.Shell.zip"

        monkeypatch.setattr(
            "src.core.remote.local_napcat_fallback._download_via_httpx",
            lambda url, target_path, *, log_callback=None, should_cancel=None: (
                target_path.write_bytes(b"fresh-bytes") or True
            ),
        )

        deployment._maybe_prefetch_napcat_archive_via_local(
            local_archive_cache=cache,
            expected_sha512=None,
            force_update=False,
            log_callback=None,
        )

        # 残件删除 + 本机重下 + SFTP 上传, 三件事齐全
        assert any(cmd.startswith("rm -f ") for cmd in backend.history)
        assert len(backend.upload_calls) == 1
        assert backend.upload_calls[0][1].endswith("/NapCat.Shell.zip")

    def test_force_update_skips_remote_cache_short_circuit(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """force_update=True 即使远端有归档也要触发后续短路 (这里 GitHub 也通则同样跳过)."""
        backend = _Backend(archive_exists=True, health_http_code="200")
        deployment = self._make_deployment(backend)
        cache = tmp_path / "NapCat.Shell.zip"

        deployment._maybe_prefetch_napcat_archive_via_local(
            local_archive_cache=cache,
            expected_sha512=None,
            force_update=True,
            log_callback=None,
        )

        # 跳过了短路 1, 走到了 GitHub 探测
        assert any("https://github.com" in cmd for cmd in backend.history)
        # GitHub 通 -> 不上传
        assert backend.upload_calls == []

    def test_skips_when_github_reachable(self, tmp_path: Path) -> None:
        backend = _Backend(archive_exists=False, health_http_code="200")
        deployment = self._make_deployment(backend)
        cache = tmp_path / "NapCat.Shell.zip"

        deployment._maybe_prefetch_napcat_archive_via_local(
            local_archive_cache=cache,
            expected_sha512=None,
            force_update=False,
            log_callback=None,
        )

        assert backend.upload_calls == []

    def test_uploads_when_github_unreachable(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        backend = _Backend(archive_exists=False, health_http_code="000")
        deployment = self._make_deployment(backend)
        cache = tmp_path / "NapCat.Shell.zip"

        # 注入伪造下载: 直接写本地缓存
        def _fake_download(url: str, target_path: Path, *, log_callback=None, should_cancel=None) -> bool:
            target_path.write_bytes(b"local-downloaded-zip")
            return True

        monkeypatch.setattr(
            "src.core.remote.local_napcat_fallback._download_via_httpx",
            _fake_download,
        )

        lines: list[str] = []
        deployment._maybe_prefetch_napcat_archive_via_local(
            local_archive_cache=cache,
            expected_sha512=None,
            force_update=False,
            log_callback=lines.append,
        )

        # 必然触发上传, 远端目标路径必须是 ${package_dir}/NapCat.Shell.zip
        assert len(backend.upload_calls) == 1
        local_path, remote_path = backend.upload_calls[0]
        assert remote_path.endswith("/NapCat.Shell.zip")
        assert "/Napcat/packages/" in remote_path or "Napcat/packages" in remote_path
        # ensure_directory 被调用过 ${package_dir}
        assert backend.ensure_dir_calls
        # 兜底切换 + 上传完成两条提示
        assert any("无法直连 GitHub" in line for line in lines)
        assert any("已上传到远端" in line for line in lines)

    def test_upload_failure_does_not_raise(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """SFTP 上传失败时不应让 install_napcat 整个流程挂掉, 而是 emit [WARN] 退回."""
        backend = _Backend(
            archive_exists=False,
            health_http_code="000",
            upload_should_raise=True,
        )
        deployment = self._make_deployment(backend)
        cache = tmp_path / "NapCat.Shell.zip"

        monkeypatch.setattr(
            "src.core.remote.local_napcat_fallback._download_via_httpx",
            lambda url, target_path, *, log_callback=None, should_cancel=None: (
                target_path.write_bytes(b"x") or True
            ),
        )

        lines: list[str] = []
        # 不应抛
        deployment._maybe_prefetch_napcat_archive_via_local(
            local_archive_cache=cache,
            expected_sha512=None,
            force_update=False,
            log_callback=lines.append,
        )

        assert any("本机兜底失败" in line for line in lines)


# ==================== install_napcat 集成 (确认参数透传) ====================
def test_install_napcat_calls_prefetch_when_local_archive_cache_set(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """install_napcat 收到 local_archive_cache 时必须调用 _maybe_prefetch...; None 时不调."""
    from script.test.test_remote_deploy_runner import _RecordingBackend

    backend = _RecordingBackend()
    deployment = LinuxCoreDeployment(backend)

    calls: list[dict] = []
    monkeypatch.setattr(
        LinuxCoreDeployment,
        "_maybe_prefetch_napcat_archive_via_local",
        lambda self, **kwargs: calls.append(kwargs),
    )

    cache = tmp_path / "NapCat.Shell.zip"
    deployment.install_napcat(local_archive_cache=cache)

    assert len(calls) == 1
    assert calls[0]["local_archive_cache"] == cache
    assert calls[0]["force_update"] is False


def test_install_napcat_skips_prefetch_when_local_archive_cache_none(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from script.test.test_remote_deploy_runner import _RecordingBackend

    backend = _RecordingBackend()
    deployment = LinuxCoreDeployment(backend)

    calls: list[dict] = []
    monkeypatch.setattr(
        LinuxCoreDeployment,
        "_maybe_prefetch_napcat_archive_via_local",
        lambda self, **kwargs: calls.append(kwargs),
    )

    deployment.install_napcat()  # local_archive_cache 默认 None

    assert calls == []


def test_install_napcat_skips_prefetch_when_custom_download_url(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """download_url 显式覆盖时, 尊重调用方意图, 不走本机兜底."""
    from script.test.test_remote_deploy_runner import _RecordingBackend

    backend = _RecordingBackend()
    deployment = LinuxCoreDeployment(backend)

    calls: list[dict] = []
    monkeypatch.setattr(
        LinuxCoreDeployment,
        "_maybe_prefetch_napcat_archive_via_local",
        lambda self, **kwargs: calls.append(kwargs),
    )

    deployment.install_napcat(
        local_archive_cache=tmp_path / "NapCat.Shell.zip",
        download_url="https://my-mirror.example.com/x.zip",
    )

    assert calls == []

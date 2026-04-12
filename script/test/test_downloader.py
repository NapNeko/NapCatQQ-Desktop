# -*- coding: utf-8 -*-

# 标准库导入
from pathlib import Path

# 第三方库导入
import httpx
import pytest
from PySide6.QtCore import QUrl

# 项目内模块导入
from src.desktop.core.network.downloader import DownloaderBase, GithubDownloader, QQDownloader


class DummyStreamResponse:
    """用于下载测试的最小 HTTP 响应桩。"""

    def __init__(self, payload: bytes = b"payload", status_code: int = 200, headers: dict[str, str] | None = None) -> None:
        self.payload = payload
        self.status_code = status_code
        self.headers = headers or {"content-length": str(len(payload))}
        self.raise_called = False

    def raise_for_status(self) -> None:
        self.raise_called = True

    def iter_bytes(self):
        yield self.payload

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> bool:
        return False


class ChunkedResponse(DummyStreamResponse):
    """返回多个分片的 HTTP 响应桩。"""

    def __init__(self, chunks: list[bytes], status_code: int = 200, headers: dict[str, str] | None = None) -> None:
        self.chunks = chunks
        payload = b"".join(chunks)
        super().__init__(payload=payload, status_code=status_code, headers=headers)

    def iter_bytes(self):
        for chunk in self.chunks:
            yield chunk


def test_github_downloader_emits_finish_signal_when_primary_download_succeeds(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """主源下载成功时必须发出完成信号。"""

    downloader = GithubDownloader(QUrl("https://example.com/NapCat.Shell.zip"))
    finished = []

    monkeypatch.setattr(downloader, "check_network", lambda: True)
    monkeypatch.setattr(downloader, "download", lambda: True)
    downloader.download_finish_signal.connect(lambda: finished.append(True))

    downloader.run()

    assert finished == [True]


def test_github_downloader_download_checks_http_status(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path,
) -> None:
    """GitHub 下载应先校验 HTTP 状态码。"""

    downloader = GithubDownloader(QUrl("https://example.com/NapCat.Shell.zip"))
    downloader.path = tmp_path
    response = DummyStreamResponse()

    monkeypatch.setattr("src.core.network.downloader.httpx.stream", lambda *args, **kwargs: response)

    assert downloader.download() is True
    assert response.raise_called is True
    assert (tmp_path / "NapCat.Shell.zip").read_bytes() == b"payload"


def test_github_downloader_run_falls_back_to_mirror_when_primary_download_fails(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """主源失败后应继续尝试镜像地址。"""
    downloader = GithubDownloader(QUrl("https://example.com/NapCat.Shell.zip"))
    visited_urls: list[str] = []
    finished: list[bool] = []

    monkeypatch.setattr(downloader, "check_network", lambda: True)

    def fake_download() -> bool:
        visited_urls.append(downloader.url.toString())
        return len(visited_urls) == 2

    monkeypatch.setattr(downloader, "download", fake_download)
    downloader.download_finish_signal.connect(lambda: finished.append(True))

    downloader.run()

    assert finished == [True]
    assert visited_urls[0] == "https://example.com/NapCat.Shell.zip"
    assert visited_urls[1] != visited_urls[0]


def test_github_downloader_download_returns_false_when_content_length_missing(
    monkeypatch: pytest.MonkeyPatch, tmp_path
) -> None:
    """无法获取文件大小时应返回 False 而不是写空文件。"""

    class ZeroLengthResponse(DummyStreamResponse):
        def __init__(self) -> None:
            super().__init__(payload=b"")
            self.headers = {}

    downloader = GithubDownloader(QUrl("https://example.com/NapCat.Shell.zip"))
    downloader.path = tmp_path
    response = ZeroLengthResponse()
    statuses: list[str] = []
    downloader.status_label_signal.connect(statuses.append)

    monkeypatch.setattr("src.core.network.downloader.httpx.stream", lambda *args, **kwargs: response)

    assert downloader.download() is False
    assert "无法获取文件大小" in statuses
    assert not (tmp_path / "NapCat.Shell.zip").exists()


def test_github_downloader_cleans_partial_file_when_stream_breaks(monkeypatch: pytest.MonkeyPatch, tmp_path) -> None:
    """下载中断后不应留下 `.part` 残留文件。"""

    class BrokenResponse(DummyStreamResponse):
        def iter_bytes(self):
            yield b"pa"
            raise httpx.RequestError("boom", request=httpx.Request("GET", "https://example.com/NapCat.Shell.zip"))

    downloader = GithubDownloader(QUrl("https://example.com/NapCat.Shell.zip"))
    downloader.path = tmp_path
    monkeypatch.setattr("src.core.network.downloader.httpx.stream", lambda *args, **kwargs: BrokenResponse())

    assert downloader.download() is False
    assert not (tmp_path / "NapCat.Shell.zip").exists()
    assert not (tmp_path / "NapCat.Shell.zip.part").exists()


def test_github_downloader_download_returns_false_when_partial_file_is_locked(
    monkeypatch: pytest.MonkeyPatch, tmp_path
) -> None:
    """已有 `.part` 文件被占用时不应因清理失败崩溃。"""

    downloader = GithubDownloader(QUrl("https://example.com/NapCat.Shell.zip"))
    downloader.path = tmp_path
    partial_path = tmp_path / "NapCat.Shell.zip.part"
    partial_path.write_bytes(b"busy")
    statuses: list[str] = []
    downloader.status_label_signal.connect(statuses.append)

    original_unlink = Path.unlink

    def fake_unlink(self: Path, *args, **kwargs):
        if self == partial_path:
            raise PermissionError("[WinError 32] 文件被占用")
        return original_unlink(self, *args, **kwargs)

    monkeypatch.setattr(Path, "unlink", fake_unlink)
    monkeypatch.setattr(
        "src.core.network.downloader.httpx.stream",
        lambda *args, **kwargs: DummyStreamResponse(),
    )

    assert downloader.download() is False
    assert any("检测到上一次下载仍未结束" in message for message in statuses)


def test_github_downloader_rejects_duplicate_target_download(monkeypatch: pytest.MonkeyPatch, tmp_path) -> None:
    """同一目标文件已有下载任务时应拒绝重复启动。"""

    downloader = GithubDownloader(QUrl("https://example.com/NapCat.Shell.zip"))
    downloader.path = tmp_path
    target_key = downloader._target_key(tmp_path / "NapCat.Shell.zip")
    DownloaderBase._active_target_paths.clear()
    DownloaderBase._active_target_paths.add(target_key)

    errors: list[bool] = []
    statuses: list[str] = []
    downloader.error_finsh_signal.connect(lambda: errors.append(True))
    downloader.status_label_signal.connect(statuses.append)

    called = {"check_network": False}

    def fake_check_network() -> bool:
        called["check_network"] = True
        return True

    monkeypatch.setattr(downloader, "check_network", fake_check_network)

    try:
        downloader.run()
    finally:
        DownloaderBase._active_target_paths.clear()

    assert errors == [True]
    assert called["check_network"] is False
    assert any("当前下载任务仍在进行" in message for message in statuses)


def test_github_downloader_run_emits_paused_signal_and_keeps_partial_file(
    monkeypatch: pytest.MonkeyPatch, tmp_path
) -> None:
    """暂停下载时应保留 `.part` 文件以便继续。"""

    downloader = GithubDownloader(QUrl("https://example.com/NapCat.Shell.zip"))
    downloader.path = tmp_path
    paused: list[bool] = []
    statuses: list[str] = []

    response = ChunkedResponse([b"pa", b"yl", b"oad"])
    monkeypatch.setattr(downloader, "check_network", lambda: True)
    monkeypatch.setattr("src.core.network.downloader.httpx.stream", lambda *args, **kwargs: response)

    first_progress = {"done": False}

    def pause_after_first_chunk(_value: int) -> None:
        if not first_progress["done"]:
            first_progress["done"] = True
            downloader.request_pause()

    downloader.download_progress_signal.connect(pause_after_first_chunk)
    downloader.download_paused_signal.connect(lambda: paused.append(True))
    downloader.status_label_signal.connect(statuses.append)

    downloader.run()

    assert paused == [True]
    assert any("下载已暂停" in message for message in statuses)
    assert not (tmp_path / "NapCat.Shell.zip").exists()
    assert (tmp_path / "NapCat.Shell.zip.part").read_bytes() == b"pa"


def test_github_downloader_download_resumes_from_partial_file(monkeypatch: pytest.MonkeyPatch, tmp_path) -> None:
    """继续下载时应从已有 `.part` 文件续传。"""

    downloader = GithubDownloader(QUrl("https://example.com/NapCat.Shell.zip"))
    downloader.path = tmp_path
    partial_path = tmp_path / "NapCat.Shell.zip.part"
    partial_path.write_bytes(b"pa")

    captured_headers: list[dict[str, str]] = []
    response = ChunkedResponse(
        [b"yl", b"oad"],
        status_code=206,
        headers={"content-length": "5", "content-range": "bytes 2-6/7"},
    )

    def fake_stream(*args, **kwargs):
        captured_headers.append(kwargs.get("headers", {}))
        return response

    monkeypatch.setattr("src.core.network.downloader.httpx.stream", fake_stream)

    assert downloader.download() is True
    assert captured_headers == [{"Range": "bytes=2-"}]
    assert (tmp_path / "NapCat.Shell.zip").read_bytes() == b"payload"
    assert not partial_path.exists()


def test_qq_downloader_run_emits_canceled_signal_and_removes_partial_file(
    monkeypatch: pytest.MonkeyPatch, tmp_path
) -> None:
    """取消下载时应移除 `.part` 文件。"""

    downloader = QQDownloader(QUrl("https://example.com/QQ.exe"))
    downloader.path = tmp_path
    canceled: list[bool] = []
    statuses: list[str] = []

    response = ChunkedResponse([b"pa", b"yl", b"oad"])
    monkeypatch.setattr("src.core.network.downloader.httpx.stream", lambda *args, **kwargs: response)

    first_progress = {"done": False}

    def cancel_after_first_chunk(_value: int) -> None:
        if not first_progress["done"]:
            first_progress["done"] = True
            downloader.request_cancel()

    downloader.download_progress_signal.connect(cancel_after_first_chunk)
    downloader.download_canceled_signal.connect(lambda: canceled.append(True))
    downloader.status_label_signal.connect(statuses.append)

    downloader.run()

    assert canceled == [True]
    assert any("下载已取消" in message for message in statuses)
    assert not (tmp_path / "QQ.exe").exists()
    assert not (tmp_path / "QQ.exe.part").exists()


def test_github_downloader_reraises_unknown_exception(monkeypatch: pytest.MonkeyPatch, tmp_path) -> None:
    """未知异常不应被静默吞掉。"""

    class WeirdResponse(DummyStreamResponse):
        def iter_bytes(self):
            raise RuntimeError("unexpected")

    downloader = GithubDownloader(QUrl("https://example.com/NapCat.Shell.zip"))
    downloader.path = tmp_path
    monkeypatch.setattr("src.core.network.downloader.httpx.stream", lambda *args, **kwargs: WeirdResponse())

    with pytest.raises(RuntimeError, match="unexpected"):
        downloader.download()


def test_github_downloader_check_network_returns_false_on_request_error(monkeypatch: pytest.MonkeyPatch) -> None:
    """网络探测异常时应安全返回 False。"""
    downloader = GithubDownloader(QUrl("https://example.com/NapCat.Shell.zip"))
    statuses: list[str] = []
    downloader.status_label_signal.connect(statuses.append)

    monkeypatch.setattr(
        "src.core.network.downloader.httpx.head",
        lambda *args, **kwargs: (_ for _ in ()).throw(
            httpx.RequestError("boom", request=httpx.Request("HEAD", "https://objects.githubusercontent.com"))
        ),
    )

    assert downloader.check_network() is False
    assert any("网络检查失败" in message for message in statuses)


def test_qq_downloader_run_checks_http_status(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path,
) -> None:
    """QQ 下载流程也应在写文件前校验 HTTP 状态码。"""

    downloader = QQDownloader(QUrl("https://example.com/QQ.exe"))
    downloader.path = tmp_path
    response = DummyStreamResponse()
    finished = []

    monkeypatch.setattr("src.core.network.downloader.httpx.stream", lambda *args, **kwargs: response)
    downloader.download_finish_signal.connect(lambda: finished.append(True))

    downloader.run()

    assert response.raise_called is True
    assert finished == [True]
    assert (tmp_path / "QQ.exe").read_bytes() == b"payload"


def test_qq_downloader_emits_error_when_request_fails(monkeypatch: pytest.MonkeyPatch, tmp_path) -> None:
    """QQ 下载失败时应发出错误结束信号。"""
    downloader = QQDownloader(QUrl("https://example.com/QQ.exe"))
    downloader.path = tmp_path
    errors: list[bool] = []
    statuses: list[str] = []
    downloader.error_finsh_signal.connect(lambda: errors.append(True))
    downloader.status_label_signal.connect(statuses.append)

    monkeypatch.setattr(
        "src.core.network.downloader.httpx.stream",
        lambda *args, **kwargs: (_ for _ in ()).throw(
            httpx.RequestError("boom", request=httpx.Request("GET", "https://example.com/QQ.exe"))
        ),
    )

    downloader.run()

    assert errors == [True]
    assert any("下载失败" in message for message in statuses)

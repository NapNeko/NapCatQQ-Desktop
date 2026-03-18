# -*- coding: utf-8 -*-

# 第三方库导入
import httpx
import pytest
from PySide6.QtCore import QUrl

# 项目内模块导入
from src.core.network.downloader import GithubDownloader, QQDownloader


class DummyStreamResponse:
    """用于下载测试的最小 HTTP 响应桩。"""

    def __init__(self, payload: bytes = b"payload") -> None:
        self.payload = payload
        self.headers = {"content-length": str(len(payload))}
        self.raise_called = False

    def raise_for_status(self) -> None:
        self.raise_called = True

    def iter_bytes(self):
        yield self.payload

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> bool:
        return False


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

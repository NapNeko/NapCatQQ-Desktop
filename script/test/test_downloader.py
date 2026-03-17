# -*- coding: utf-8 -*-

# 第三方库导入
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

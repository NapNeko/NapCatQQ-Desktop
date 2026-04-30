# -*- coding: utf-8 -*-

# 第三方库导入
import httpx
import pytest

# 项目内模块导入
from src.ui.page.bot_page.widget import card as card_module
from src.ui.page.bot_page.widget.card import BotAvatarWidget


def test_get_avatar_worker_emits_error_signal_instead_of_touching_ui(monkeypatch: pytest.MonkeyPatch) -> None:
    """头像下载失败时，worker 应通过信号回主线程，而不是直接操作 UI。"""
    emitted: list[tuple[str, str]] = []

    def fake_get(*args, **kwargs):
        raise httpx.RequestError("avatar boom", request=httpx.Request("GET", "https://example.com/avatar"))

    monkeypatch.setattr(card_module.httpx, "get", fake_get)
    monkeypatch.setattr(
        card_module,
        "error_bar",
        lambda *args, **kwargs: pytest.fail("worker 不应直接调用 error_bar"),
    )

    worker = BotAvatarWidget.GetAvatarWorker("123456")
    worker.avatar_error_signal.connect(lambda qq_id, message: emitted.append((qq_id, message)))
    worker.run()

    assert emitted
    assert emitted[0][0] == "123456"
    assert "RequestError" in emitted[0][1]
    assert "avatar boom" in emitted[0][1]


def test_get_avatar_worker_supports_large_qqid(monkeypatch: pytest.MonkeyPatch) -> None:
    """大于 32 位有符号整型上限的 QQ 号也不应在信号发射时溢出。"""
    emitted: list[tuple[str, bytes]] = []

    class FakeResponse:
        content = b"avatar-bytes"

        @staticmethod
        def raise_for_status() -> None:
            return None

    monkeypatch.setattr(card_module.httpx, "get", lambda *args, **kwargs: FakeResponse())

    worker = BotAvatarWidget.GetAvatarWorker("2477817352")
    worker.avatar_bytes_signal.connect(lambda qq_id, data: emitted.append((qq_id, data)))
    worker.run()

    assert emitted == [("2477817352", b"avatar-bytes")]

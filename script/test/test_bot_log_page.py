# -*- coding: utf-8 -*-

import os

from PySide6.QtWidgets import QApplication

from src.ui.page.bot_page.sub_page.bot_log import BotLogPage


def ensure_qapp() -> QApplication:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _build_log_lines(count: int) -> str:
    return "\n".join(f"line {index}" for index in range(count))


def test_append_log_keeps_reader_position_when_not_pinned_to_bottom() -> None:
    ensure_qapp()
    page = BotLogPage()
    page.resize(520, 280)
    page.show()
    page.slot_set_log_view(_build_log_lines(300))
    QApplication.processEvents()

    scroll_bar = page.log_view.verticalScrollBar()
    middle_value = max(0, scroll_bar.maximum() // 2)
    scroll_bar.setValue(middle_value)
    QApplication.processEvents()

    before_value = scroll_bar.value()
    page.slot_insert_log_view("\nnew tail line")
    QApplication.processEvents()

    assert scroll_bar.value() == before_value
    page.close()


def test_append_log_follows_tail_when_reader_is_at_bottom() -> None:
    ensure_qapp()
    page = BotLogPage()
    page.resize(520, 280)
    page.show()
    page.slot_set_log_view(_build_log_lines(300))
    QApplication.processEvents()

    scroll_bar = page.log_view.verticalScrollBar()
    scroll_bar.setValue(scroll_bar.maximum())
    QApplication.processEvents()

    page.slot_insert_log_view("\nnew tail line")
    QApplication.processEvents()

    assert scroll_bar.value() == scroll_bar.maximum()
    page.close()


# ==================== P3.W3.E: 标题后缀根据 runtime_target ====================
class _BotShim:
    def __init__(self, qqid: int, runtime_target: str) -> None:
        self.QQID = qqid
        self.runtime_target = runtime_target


class _ConfigShim:
    def __init__(self, qqid: int, runtime_target: str) -> None:
        self.bot = _BotShim(qqid, runtime_target)


def test_title_suffix_empty_for_local_target() -> None:
    """``runtime_target='local'`` 时不应在标题后缀加 '远端' 字样."""
    ensure_qapp()
    config = _ConfigShim(2477817352, "local")
    suffix = BotLogPage._compose_title_suffix(config)  # type: ignore[arg-type]
    assert suffix == "", f"本地 Bot 标题不应有后缀, 实际: {suffix!r}"


def test_title_suffix_falls_back_to_target_id_when_server_unknown(monkeypatch) -> None:
    """ServerManager 找不到对应 profile 时, 标题应回退到 ``server_id`` 字面值."""
    import src.ui.page.bot_page.sub_page.bot_log as bot_log_mod

    # 让 ``it(ServerManager)`` 返回一个 get_server -> None 的 fake
    class _FakeManager:
        def get_server(self, _server_id: str) -> None:
            return None

    monkeypatch.setattr(bot_log_mod, "it", lambda _cls: _FakeManager())

    ensure_qapp()
    config = _ConfigShim(2477817352, "srv-fake-id")
    suffix = BotLogPage._compose_title_suffix(config)  # type: ignore[arg-type]
    assert suffix == " · 远端 [srv-fake-id]"


def test_title_suffix_uses_server_name_when_resolved(monkeypatch) -> None:
    """能解析到 ``ServerProfile`` 时应用 ``profile.name`` 而非裸 server_id."""
    import src.ui.page.bot_page.sub_page.bot_log as bot_log_mod

    class _Profile:
        name = "我的服务器"

    class _FakeManager:
        def get_server(self, _server_id: str) -> _Profile:
            return _Profile()

    monkeypatch.setattr(bot_log_mod, "it", lambda _cls: _FakeManager())

    ensure_qapp()
    config = _ConfigShim(2477817352, "srv-1")
    suffix = BotLogPage._compose_title_suffix(config)  # type: ignore[arg-type]
    assert suffix == " · 远端 [我的服务器]"

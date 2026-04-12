# -*- coding: utf-8 -*-

from src.desktop.core.home.notice_service import HomeNoticeService


def test_summarize_release_notes_strips_markdown_prefixes() -> None:
    text = "# Release Notes\n- fixed crash on startup\n> extra context"

    assert HomeNoticeService._summarize_release_notes(text) == "Release Notes"


def test_summarize_release_notes_handles_empty_text() -> None:
    assert HomeNoticeService._summarize_release_notes("") == "发布了新的版本。"
    assert HomeNoticeService._summarize_release_notes(None) == "发布了新的版本。"

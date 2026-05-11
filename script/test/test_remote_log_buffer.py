# -*- coding: utf-8 -*-
"""[`RemoteNapCatQQLog`](src/core/runtime/napcat.py) 增量去重单元测试 (P3).

重点覆盖 ``_compute_new_chunk`` 的最长后缀-前缀重叠算法, 确保:
- 首次拉取整段都视为新增
- 第二次拉取与第一次有重叠时, 只发射真正新增段
- 完全无重叠 (例如日志被外部截断重写) 时退化为整段新增
- 空尾部 / 与已见完全相同时不发射
"""

from __future__ import annotations

import os

import pytest

# 不依赖任何 backend / SSH; 直接构造 RemoteNapCatQQLog 实例并喂数据
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtWidgets import QApplication

from src.core.runtime.bot_process_manager import RemoteNapCatQQLog


@pytest.fixture(scope="module")
def qapp() -> QApplication:
    """模块级 QApplication, RemoteNapCatQQLog 内部用了 QTimer 需要 QApplication."""
    app = QApplication.instance() or QApplication([])
    return app


@pytest.fixture
def remote_log(qapp: QApplication, config_factory) -> RemoteNapCatQQLog:
    """构造一个 RemoteNapCatQQLog, 立刻停掉它的轮询计时器以便单元测试.

    我们只测 ``_compute_new_chunk`` / ``_on_tail_arrived`` 这两个纯函数式逻辑,
    不希望背后真有 SSH 拉日志 (会失败).
    """
    config = config_factory(qqid=2707600964)
    config.bot.runtime_target = "test-server"
    log = RemoteNapCatQQLog(config)
    log._poll_timer.stop()  # 立即停, 不发任何 _enqueue_tail
    return log


class TestComputeNewChunk:
    def test_first_tail_is_fully_new(self, remote_log: RemoteNapCatQQLog) -> None:
        """首次拉取时 ``_seen_tail`` 为空, 整段都是新增."""
        out = remote_log._compute_new_chunk("line1\nline2\nline3\n")
        assert out == "line1\nline2\nline3\n"

    def test_overlap_emits_only_new_suffix(self, remote_log: RemoteNapCatQQLog) -> None:
        """第二次拉取与已见尾部完全重叠的部分必须被吃掉,
        只剩真正新增的尾巴.
        """
        first = "line1\nline2\nline3\n"
        # 模拟首次完整发射
        remote_log._on_tail_arrived(remote_log._qq_id, first)
        # 第二次 tail 包含上次的 line2/line3 + 新增 line4/line5
        second = "line2\nline3\nline4\nline5\n"
        out = remote_log._compute_new_chunk(second)
        assert out == "line4\nline5\n"

    def test_identical_tail_emits_nothing(self, remote_log: RemoteNapCatQQLog) -> None:
        first = "line1\nline2\nline3\n"
        remote_log._on_tail_arrived(remote_log._qq_id, first)
        # 第二次拉取与第一次完全一致 -> 没有新增
        out = remote_log._compute_new_chunk(first)
        assert out == ""

    def test_no_overlap_falls_back_to_full(self, remote_log: RemoteNapCatQQLog) -> None:
        """日志被截断重写后, 新拉取与已见完全不相干, 整段视为新增 (不丢日志).

        这种场景偶尔会让用户多看一截, 但绝对不会丢失日志, 是可接受的折中.
        """
        remote_log._on_tail_arrived(remote_log._qq_id, "old-content-that-was-rotated\n")
        out = remote_log._compute_new_chunk("brand-new-after-rotation\n")
        assert out == "brand-new-after-rotation\n"

    def test_overlap_window_does_not_cross_history_cap(self, remote_log: RemoteNapCatQQLog) -> None:
        """``_seen_tail`` 受 ``_HISTORY_BYTES`` 限制只保留尾段;
        新拉取与早期被裁掉的部分重叠时不应当成有效重叠.
        """
        # 灌一大段, 远超 _HISTORY_BYTES, 让早期内容被裁掉
        big_blob = ("X" * 1000 + "\n") * 250  # ~ 250KB > _HISTORY_BYTES (200KB)
        remote_log._on_tail_arrived(remote_log._qq_id, big_blob)
        # 此时 _seen_tail 只剩末尾 ~200KB, 早期 "X" 已经被丢
        # 新拉取使用早期被裁掉部分 -> 应当视为无重叠 (因为我们窗口里看不见)
        # 但因为内容是均匀的 X, 仍会找到重叠. 这条测试主要锁定不会崩溃.
        out = remote_log._compute_new_chunk("Y\nZ\n")
        assert out == "Y\nZ\n"


class TestEmitContract:
    def test_signal_emits_only_new_chunk(self, qapp: QApplication, remote_log: RemoteNapCatQQLog) -> None:
        """``_on_tail_arrived`` 经由 signal 发送的内容必须只是新增段, 不含重复行."""
        emitted: list[str] = []
        remote_log.output_log_signal.connect(emitted.append)

        remote_log._on_tail_arrived(remote_log._qq_id, "a\nb\nc\n")
        remote_log._on_tail_arrived(remote_log._qq_id, "b\nc\nd\n")
        remote_log._on_tail_arrived(remote_log._qq_id, "c\nd\ne\n")

        assert emitted == ["a\nb\nc\n", "d\n", "e\n"]
        # 累计拼起来应当是无重复的真实历史
        assert remote_log.get_log_content() == "a\nb\nc\nd\ne\n"

    def test_qq_id_mismatch_is_dropped(self, remote_log: RemoteNapCatQQLog) -> None:
        """qq_id 不匹配时丢弃, 防止 RemoteBotOperationRunnable 错配回调."""
        emitted: list[str] = []
        remote_log.output_log_signal.connect(emitted.append)

        remote_log._on_tail_arrived("wrong-qq-id", "should not show\n")

        assert emitted == []

    def test_clear_resets_state(self, remote_log: RemoteNapCatQQLog) -> None:
        """``clear`` 之后, 下次拉取的内容会作为全新历史发射."""
        remote_log._on_tail_arrived(remote_log._qq_id, "x\ny\nz\n")
        remote_log.clear()
        assert remote_log.get_log_content() == ""

        emitted: list[str] = []
        remote_log.output_log_signal.connect(emitted.append)
        remote_log._on_tail_arrived(remote_log._qq_id, "x\ny\nz\n")
        # 清空后这段又是 "新的", 整段发射
        assert emitted == ["x\ny\nz\n"]


class TestErrorBackoff:
    """P3.W3.E: 连续失败退避语义."""

    def test_single_error_does_not_stop_polling(
        self, remote_log: RemoteNapCatQQLog
    ) -> None:
        """单次失败不应停止轮询; 也不应在日志缓冲注入错误行."""
        # 重新启动 fixture 的 timer (fixture 默认 stop 了)
        remote_log._poll_timer.start()
        emitted: list[str] = []
        remote_log.output_log_signal.connect(emitted.append)

        remote_log._on_tail_error(remote_log._qq_id, "transient")

        assert remote_log._poll_timer.isActive() is True
        assert emitted == []
        assert remote_log._consecutive_errors == 1

    def test_consecutive_errors_stop_polling_and_inject_log_line(
        self, remote_log: RemoteNapCatQQLog
    ) -> None:
        """连续达到阈值时应停掉 timer, 同时往缓冲注入一行错误."""
        remote_log._poll_timer.start()
        emitted: list[str] = []
        remote_log.output_log_signal.connect(emitted.append)

        for _ in range(remote_log._MAX_CONSECUTIVE_ERRORS):
            remote_log._on_tail_error(remote_log._qq_id, "boom")

        assert remote_log._poll_timer.isActive() is False, "达阈值后 timer 应停止"
        assert len(emitted) == 1, "应注入恰好 1 行错误提示"
        assert "[ERROR]" in emitted[0]
        assert "已停止轮询" in emitted[0]
        # 最后一条错误的内容应当出现在提示里
        assert "boom" in emitted[0]
        # 历史也包含该错误
        assert "[ERROR]" in remote_log.get_log_content()

    def test_success_resets_consecutive_counter(
        self, remote_log: RemoteNapCatQQLog
    ) -> None:
        """任意一次成功 (含空字符串) 都应让连续失败计数归零."""
        remote_log._poll_timer.start()
        # 喂 2 次失败 (未达阈值)
        remote_log._on_tail_error(remote_log._qq_id, "e1")
        remote_log._on_tail_error(remote_log._qq_id, "e2")
        assert remote_log._consecutive_errors == 2

        # 一次成功 (空内容也算成功)
        remote_log._on_tail_arrived(remote_log._qq_id, "")
        assert remote_log._consecutive_errors == 0

        # 再来 2 次失败仍未触发停掉
        remote_log._on_tail_error(remote_log._qq_id, "e3")
        remote_log._on_tail_error(remote_log._qq_id, "e4")
        assert remote_log._poll_timer.isActive() is True

    def test_error_only_for_matching_qq_id(
        self, remote_log: RemoteNapCatQQLog
    ) -> None:
        """qq_id 不匹配的错误不应累加计数."""
        remote_log._poll_timer.start()
        remote_log._on_tail_error("wrong-qq-id", "x")
        remote_log._on_tail_error("wrong-qq-id", "x")
        remote_log._on_tail_error("wrong-qq-id", "x")
        assert remote_log._consecutive_errors == 0
        assert remote_log._poll_timer.isActive() is True


class TestAnsiSanitization:
    """远端 ``tail`` 拿回的 Linux NapCat 日志带 ANSI 颜色转义,
    必须在拼入 ``_seen_tail`` / 发射给 UI 之前清洗, 否则:
        - ESC 字节会以 tofu 形式残留到 ``QPlainTextEdit``;
        - ``LogHighlighter`` 的 ``[info]`` / ``[debug]`` 正则会失配.
    """

    def test_ansi_escape_stripped_from_emitted_chunk(
        self, remote_log: RemoteNapCatQQLog
    ) -> None:
        emitted: list[str] = []
        remote_log.output_log_signal.connect(emitted.append)

        # NapCat Linux 端实际输出形态: `[\x1b[32minfo\x1b[39m] [NapCat] ...`
        raw = "05-01 02:01:02 [\x1b[32minfo\x1b[39m] [NapCat] hello\n"
        remote_log._on_tail_arrived(remote_log._qq_id, raw)

        assert emitted, "首次拉取必定发射"
        assert "\x1b" not in emitted[0]
        # 清洗后 `[info]` 应当原样保留, 这样 LogHighlighter 才能上色
        assert "[info]" in emitted[0]

    def test_seen_tail_uses_sanitized_text_for_dedup(
        self, remote_log: RemoteNapCatQQLog
    ) -> None:
        """两次拉取若仅在 ANSI 转义这一字节维度不同, 应被识别为重复内容.

        否则用户每 5s 轮询一次, 同一行 ``[info]`` 日志会被反复 emit.
        """
        emitted: list[str] = []
        remote_log.output_log_signal.connect(emitted.append)

        first = "[\x1b[32minfo\x1b[39m] same-line\n"
        second = "[\x1b[32minfo\x1b[39m] same-line\n"
        remote_log._on_tail_arrived(remote_log._qq_id, first)
        remote_log._on_tail_arrived(remote_log._qq_id, second)

        # 第二次必须被识别为完全重复, 不再发射
        assert len(emitted) == 1
        assert remote_log.get_log_content() == "[info] same-line\n"

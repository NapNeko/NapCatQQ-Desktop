# -*- coding: utf-8 -*-
"""Bot 进程管理 (Tier I, P2 SnowLuma WebUI 编程客户端化重构).

本模块取代原 ``napcat.py``, 提供:

- :class:`BotProcessManager`: Bot 进程管理总入口 (取代 ``ManagerNapCatQQProcess``).
  按 ``config.bot.backend_type`` 与 ``runtime_target`` 把启动/停止 dispatch 到对应
  driver / 远端 SSH worker; 持四个原 signal 名 (``process_changed_signal`` /
  ``notification_signal`` / ``snowluma_login_state_signal``).
- 进程数据模型: :class:`NapCatProcessModel`, :class:`RemoteProcessRecord`.
- 日志缓冲: :class:`NapCatQQProcessLog`, :class:`RemoteNapCatQQLog`,
  :class:`ManagerNapCatQQLog`.
- 远端 SSH worker: :class:`_RemoteLogTailRunnable`,
  :class:`RemoteBotOperationRunnable`.
- WebUI 探测: :class:`GetAuthStatusRunnable`, :class:`GetLoginStatusRunnable`.
- 登录态: :class:`NapCatQQLoginState`, :class:`ManagerNapCatQQLoginState`.
- 自动重启: :class:`ManagerAutoRestartProcess`.
- 4 个 ``creart`` Creator 注册.

具体的 NapCat / SnowLuma QProcess 创建逻辑分别在
:mod:`src.core.runtime.napcat_driver` 与 :mod:`src.core.runtime.snowluma_driver`,
本管理类只 dispatch + 注册 signal + 跟踪生命周期.

参见: ``docs/requirements/2026-05-10-snowluma-bot-form-backend-aware.md`` §2.15.
"""
# 标准库导入
import hashlib
import re
from abc import ABC
from collections import deque
from dataclasses import dataclass
from time import monotonic
from typing import cast

# 第三方库导入
import psutil
from creart import add_creator, exists_module, it
from creart.creator import AbstractCreator, CreateTargetInfo
from httpx import Client, post
from PySide6.QtCore import Qt, QObject, QProcess, QRunnable, QThreadPool, QTimer, Signal, Slot

# 项目内模块导入
from src.core.config import cfg
from src.core.config.config_enum import TimeUnitEnum
from src.core.config.config_model import Config
from src.core.network.email import Email, create_offline_email_task
from src.core.network.webhook import WebHook, create_offline_webhook_task
from src.core.logging import LogSource, LogType, logger
from src.core.remote.thread_pool import remote_ssh_pool
from src.core.runtime.backend_type import BackendType
from src.core.runtime.bot_backend_driver import BotBackendDriver, ProcessHandle
from src.core.runtime.napcat_driver import NapCatDriver
from src.core.runtime.paths import PathFunc
from src.core.runtime.snowluma_driver import (
    SnowLumaDriver,
    SnowLumaProcessModel,
    SnowLumaStartMode,
    terminate_async,
)
from src.core.runtime.snowluma_status_poller import SnowLumaStatusPoller

# ==================== 数据模型 ====================
NotificationTask = Email | WebHook


# ==================== 工具函数 ====================
_ANSI_ESCAPE_RE = re.compile(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])")


def _sanitize_log_text(data: str) -> str:
    """清洗日志文本中的 ANSI 转义、异常换行和多余空行.

    Linux 端 NapCat 通过 ``logger`` 输出的日志会带有 ``\\x1b[32m`` 之类的颜色
    转义序列, 直接写进 ``QPlainTextEdit`` 后 ESC 字节会以 tofu 形式残留,
    并破坏 [`LogHighlighter`](src/ui/components/code_editor/highlight.py)
    对 ``[info]`` 等级别标签的正则匹配, 从而让远端日志既漏字符又上不了色.
    本地 ``QProcess`` 与远端 ``tail`` 两条数据通路都需要走一次清洗.
    """
    if not data:
        return ""

    data = _ANSI_ESCAPE_RE.sub("", data)

    # 将 `\r\n`、`\r\r\n`、孤立的 `\r` 统一折叠成单个 `\n`
    data = re.sub(r"\r+\n", "\n", data)
    data = re.sub(r"\r+", "\n", data)

    # 压缩由异常换行导致的多余空白行
    data = re.sub(r"\n{2,}", "\n", data)
    return data


def _compute_tail_new_chunk(
    prev_seen_tail: str,
    sanitized_tail: str,
    *,
    overlap_window: int,
) -> str:
    """P3 perf W4: 将日志增量去重算法抽为纯函数, 供 worker 线程调用.

    逻辑与 [`RemoteNapCatQQLog._compute_new_chunk`](src/core/runtime/bot_process_manager.py) 一致:
    寻找 ``prev_seen_tail`` 的末尾在 ``sanitized_tail`` 开头重叠的最长长度 ``k``,
    返回 ``sanitized_tail[k:]`` 作为新增段; 无任何重叠 (首次 / 日志被截断) 时
    整段视为新增.

    移到 worker 的动机: 最坏情况下对 16KB overlap window 有 O(window²) 扫描,
    过去在主线程每 5s 一次跳比较直观, 几个 BotLogPage 同时打开时会感知到
    UI 卡顿. 搬到 worker 后主线程仅需 append + emit.
    """
    if not sanitized_tail:
        return ""
    if not prev_seen_tail:
        return sanitized_tail

    seen_window = prev_seen_tail[-overlap_window:]
    max_k = min(len(seen_window), len(sanitized_tail))
    for k in range(max_k, 0, -1):
        if seen_window.endswith(sanitized_tail[:k]):
            return sanitized_tail[k:]
    return sanitized_tail


@dataclass
class NapCatProcessModel:
    """NapCat 进程数据模型"""

    qq_id: str
    process: QProcess
    state: QProcess.ProcessState = QProcess.ProcessState.NotRunning
    started_at: float = 0.0


@dataclass
class RemoteProcessRecord:
    """远端 NapCat Bot 进程的运行时记录(P2.6).

    为远端 Bot 的进程管理提供与 [`NapCatProcessModel`](src/core/runtime/bot_process_manager.py)
    形状对齐的最小字段集 (``qq_id`` / ``state`` / ``started_at``), 让 UI 层
    (`BotCard` / `BotInfoWidget`) 可以无差别地读取运行时信息.

    Attributes:
        qq_id: Bot 的 QQ 号
        config: Bot 完整配置, 用于重启 / 查询 backend
        state: 当前运行状态 (复用 [`QProcess.ProcessState`](https://doc.qt.io/qt-6/qprocess.html#ProcessState-enum))
        started_at: 启动时间戳 (``time.monotonic()``)
        last_memory_rss_bytes: 上一次轮询拿到的远端 RSS, 单位字节; 未知时为 None
        polling_timer: 状态轮询计时器; 在 ``state==Running`` 期间存活
        login_state_published: 是否已经为该 Bot 创建过
            [`NapCatQQLoginState`](src/core/runtime/bot_process_manager.py); 防止重复
        login_state_port: 已发布的本地隧道端口, 用于探测端口变化触发重发
    """

    qq_id: str
    config: Config
    state: QProcess.ProcessState = QProcess.ProcessState.NotRunning
    started_at: float = 0.0
    last_memory_rss_bytes: int | None = None
    # 远端服务器物理总内存 (字节). 由 backend 在 ``ProcessStatus`` 上每次都带回来,
    # session 内不变. UI ``BotCard`` 走 ``BotProcessManager.get_total_memory_mb``
    # 拿到这个值并显示在 "X MB / Y MB" 的 Y 处, 让远端 Bot 不再误显 Desktop 本机 RAM.
    server_total_memory_bytes: int | None = None
    polling_timer: QTimer | None = None
    login_state_published: bool = False
    login_state_port: int | None = None
    # P3 perf W4: 单飞标记, 防止上一次 poll 还没回就排队下一次
    poll_in_flight: bool = False


# ==================== 工具类 ====================
class NapCatQQProcessLog(QObject):
    """进程的日志功能"""

    output_log_signal = Signal(str)

    # 内部分发 log 信号
    _log_dispatcher_signal = Signal(str)

    def __init__(self, config: Config, process: QProcess) -> None:
        super().__init__()
        # 设置属性
        self._config = config
        self._process = process

        # 日志存储
        self._log_storage = deque(maxlen=10000)

        # 连接信号
        self._process.readyReadStandardOutput.connect(self.handle_output)
        self._log_dispatcher_signal.connect(self.slot_get_web_ui_port)

    # ==================== 公共函数===================
    def get_log_content(self) -> str:
        """返回所有 log"""
        return self._sanitize_log_text("".join(self._log_storage))

    def clear(self) -> None:
        """清理所有 log"""
        self._log_storage.clear()

    @staticmethod
    def _sanitize_log_text(data: str) -> str:
        """清洗日志文本; 委托给模块级 [`_sanitize_log_text`](src/core/runtime/bot_process_manager.py)."""
        return _sanitize_log_text(data)

    # ==================== 响应函数===================
    def handle_output(self):
        """处理日志数据"""
        # 拿到解码后的数据
        data = bytes(self._process.readAllStandardOutput().data()).decode()
        data = _sanitize_log_text(data)
        if not data:
            return
        self._log_storage.append(data)

        # 信号发射
        self.output_log_signal.emit(data)
        self._log_dispatcher_signal.emit(data)

    def slot_get_web_ui_port(self, data: str) -> None:
        """从日志数据中提取 WebUI 端口 和 token

        检测到以下类似的日志
        [info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:xxx/webui?token=xxx

        Args:
            data (str): 日志数据
        """
        if (
            match := re.compile(
                r"\[info\] \[NapCat\] \[WebUi\] WebUi User Panel Url: http://127\.0\.0\.1:(\d+)/webui\?token=(\S+)"
            ).search(data)
        ) is not None:
            # 通过 ManagerNapCatQQLoginState 创建登录状态管理器
            it(ManagerNapCatQQLoginState).create_login_state(
                config=self._config, port=int(match.group(1)), token=match.group(2)
            )


class _RemoteLogTailRunnable(QObject, QRunnable):
    """单次 SSH ``tail`` 拉取的 [`QRunnable`](https://doc.qt.io/qt-6/qrunnable.html).

    在 [`remote_ssh_pool`](src/core/remote/thread_pool.py) 后台线程上调用
    [`RemoteBackend.tail_log`](src/core/operation/remote_backend.py),
    **在 worker 内部**完成 sanitize + overlap 去重, 仅把"真正新增段 + 更新后的
    _seen_tail 快照"通过 ``tail_chunk_signal`` 带回主线程, 主线程只负责 append + emit.

    P3 perf W4 改动背景: 过去 worker 返回整段 full_tail, 主线程做 sanitize
    (O(n) regex) + overlap dedup (O(window²) 扫描) + QPlainTextEdit append,
    在多 BotLogPage 同时 5s 轮询时, 主线程这一圈处理会感知到 UI 卡顿. 现在 worker
    独吞重计算, 主线程只读写简单字段 + 发信号.

    SSH 异常 / 资源错误一律走 ``error_signal`` 让上层 trace 一行了事,
    单次失败不影响下一次轮询.
    """

    # (qq_id, new_chunk, new_seen_tail)
    # new_chunk 已 sanitize + overlap 去重; new_seen_tail 已按 history_bytes 截尾,
    # 主线程直接 ``self._seen_tail = new_seen_tail`` 即可, 不再需要任何计算.
    tail_chunk_signal = Signal(str, str, str)
    error_signal = Signal(str, str)  # (qq_id, error_message)

    def __init__(
        self,
        qq_id: str,
        config: Config,
        *,
        lines: int,
        prev_seen_tail: str,
        overlap_window: int,
        history_bytes: int,
    ) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self._qq_id = qq_id
        self._config = config
        self._lines = lines
        # 快照: 单飞保护后这条 worker 是唯一消费者, 不存在对 _seen_tail 的竞态读.
        self._prev_seen_tail = prev_seen_tail
        self._overlap_window = overlap_window
        self._history_bytes = history_bytes

    def run(self) -> None:  # noqa: D401 - QRunnable 协议
        try:
            from src.core.operation.resolver import resolve_backend_for_bot

            backend = resolve_backend_for_bot(self._config)
            backend.connect()
            content = backend.tail_log(self._qq_id, lines=self._lines) or ""

            # worker 侧 sanitize: ANSI 清洗 + 换行归一, 输出即 _seen_tail 储存形态
            sanitized = _sanitize_log_text(content)
            new_chunk = _compute_tail_new_chunk(
                self._prev_seen_tail,
                sanitized,
                overlap_window=self._overlap_window,
            )
            if not new_chunk:
                # 空增量也算一次成功 (重置 _consecutive_errors); seen_tail 保持不变
                self.tail_chunk_signal.emit(self._qq_id, "", self._prev_seen_tail)
                return
            new_seen_tail = (self._prev_seen_tail + new_chunk)[-self._history_bytes:]
            self.tail_chunk_signal.emit(self._qq_id, new_chunk, new_seen_tail)
        except Exception as exc:  # noqa: BLE001 - 单次拉取失败不应影响后续轮询
            self.error_signal.emit(self._qq_id, f"{type(exc).__name__}: {exc}")


class RemoteNapCatQQLog(QObject):
    """远端 NapCat Bot 日志缓冲 (P3 实现).

    与 [`NapCatQQProcessLog`](src/core/runtime/bot_process_manager.py) 暴露完全一致的对外接口
    (``output_log_signal`` / ``get_log_content`` / ``clear``), 让
    [`BotLogPage`](src/ui/page/bot_page/sub_page/bot_log.py) 不需要做任何区分.

    数据来源不再是本地 ``QProcess`` 的 stdout, 而是周期性 SSH ``tail``:
    - 每 ``_POLL_INTERVAL_MS`` 毫秒派发一个 [`_RemoteLogTailRunnable`](src/core/runtime/bot_process_manager.py)
      到 [`remote_ssh_pool`](src/core/remote/thread_pool.py) 专用池 (P3 perf W4)
    - runnable 在后台线程调用
      [`RemoteBackend.tail_log`](src/core/operation/remote_backend.py) 拉取最近 N 行,
      **worker 内部直接做 ANSI sanitize + 最长后缀-前缀重叠去重**,
      只把"真正新增段 + 更新后的 _seen_tail"通过 ``tail_chunk_signal`` 带回主线程
    - 主线程拿到增量后仅执行: 更新 ``_seen_tail`` 快照 + append 到 ``_log_storage`` +
      emit ``output_log_signal``, 不再做任何重活

    设计要点:
    - 第一次拉取直接全量插入, 让用户开页时立刻有上下文 (而不是等 5 秒).
    - 后续每次拉取都做去重, 避免日志页上出现 N 倍的重复行.
    - P3 perf W4: 单飞保护 ``_tail_in_flight`` 防止 SSH 抖动时 runner 在池里堆积.
    - SSH 异常只 trace 一行, 不打断轮询; 用户停止 Bot 时调用 ``stop()`` 释放计时器.

    向后兼容说明:
    - 单元测试 ([`test_remote_log_buffer.py`](script/test/test_remote_log_buffer.py))
      直接调用 ``_on_tail_arrived(qq_id, full_tail)`` 来构造 "整段来自 SSH 的尾部"
      场景; 该槽保留, 继续在**主线程同步**做 sanitize + 去重, 仅用于测试路径.
      生产的 worker 不再连接此槽, 改走 ``_on_tail_chunk_arrived``.
    """

    output_log_signal = Signal(str)
    error_signal = Signal(str)

    # 5 秒一次轮询. NapCat 输出节奏并不密集, 太短会把 SSH 通道挤满,
    # 太长会让用户感觉 "日志卡了"; 5s 是经验值.
    _POLL_INTERVAL_MS = 5 * 1000
    # 每次 tail 拉取的行数. 1000 行覆盖 ~5s 内的输出绰绰有余,
    # 即使丢一两次轮询也不至于丢日志.
    _TAIL_LINES = 1000
    # 去重窗口大小. 取尾部最多 ``_OVERLAP_WINDOW`` 字节做与新拉取的前缀比对,
    # 避免对长字符串做 O(N²) 全量扫描.
    _OVERLAP_WINDOW = 16 * 1024
    # 已展示历史的最大字节数 (UI 端 ``deque(maxlen=10000)`` 是按 chunk 计数,
    # 这里按 bytes 限制内存占用上限 ~ 200KB).
    _HISTORY_BYTES = 200 * 1024
    # P3.W3.E: 连续失败阈值. 超过后停止轮询 + 在日志缓冲区注入一行错误,
    # 避免在 SSH 不可达 / 远端日志丢失场景下被仓赌的重试迫到靠谱.
    # 3 次 × 5s = 15s 才会放弃; 用户手动重启 Bot 会重新 create_remote_log 从而恢复.
    _MAX_CONSECUTIVE_ERRORS = 3

    def __init__(self, config: Config) -> None:
        super().__init__()
        self._config = config
        self._qq_id = str(config.bot.QQID)

        self._log_storage: deque[str] = deque(maxlen=10000)
        # 维护 "已展示给 UI 的累积文本" 的尾部, 用于增量去重.
        # 不与 ``_log_storage`` 复用 (那是 chunk 列表, 拼接代价高).
        self._seen_tail: str = ""
        # P3.W3.E: 连续失败计数; 成功拉取一次即重置
        self._consecutive_errors: int = 0
        # P3 perf W4: 单飞标记. _enqueue_tail 检查此旗后置位, 由
        # _on_tail_chunk_arrived / _on_tail_error 复位; SSH 抖动 / 大包
        # 拉取时上一次 runner 还没回来, 下一个 5s tick 会直接跳过, 防止池堆积.
        self._tail_in_flight: bool = False

        self._poll_timer = QTimer(self)
        self._poll_timer.setInterval(self._POLL_INTERVAL_MS)
        self._poll_timer.timeout.connect(self._enqueue_tail)
        self._poll_timer.start()

        # 立刻触发一次, 让用户打开 Bot 后很快就能看到日志, 不用等 5 秒.
        QTimer.singleShot(0, self._enqueue_tail)

    # ==================== 公共接口 (与 NapCatQQProcessLog 对齐) ====================
    def get_log_content(self) -> str:
        """返回截至目前已经累积的全部日志."""
        return "".join(self._log_storage)

    def clear(self) -> None:
        """清空缓冲. UI 侧 "清屏" 按钮的预留入口."""
        self._log_storage.clear()
        self._seen_tail = ""

    def stop(self) -> None:
        """停止轮询并释放计时器, 由 [`ManagerNapCatQQLog.remove_log`] 调用."""
        if self._poll_timer.isActive():
            self._poll_timer.stop()
        self._poll_timer.deleteLater()

    # ==================== 内部 ====================
    def _enqueue_tail(self) -> None:
        # P3 perf W4: 单飞保护. SSH 网络抖动时 5s tick 可能早于 runner 返回,
        # 直接跳过, 避免在 remote_ssh_pool 里堆积若干条陈旧 tail 请求.
        if self._tail_in_flight:
            return
        self._tail_in_flight = True
        runnable = _RemoteLogTailRunnable(
            self._qq_id,
            self._config,
            lines=self._TAIL_LINES,
            # 主线程快照: 在 worker 运行期间 _seen_tail 不会被其他 worker 改写
            # (因为单飞保护), 所以 worker 返回时主线程直接覆盖即可.
            prev_seen_tail=self._seen_tail,
            overlap_window=self._OVERLAP_WINDOW,
            history_bytes=self._HISTORY_BYTES,
        )
        runnable.tail_chunk_signal.connect(self._on_tail_chunk_arrived)
        runnable.error_signal.connect(self._on_tail_error)
        remote_ssh_pool().start(cast(QRunnable, runnable))

    def _on_tail_chunk_arrived(self, qq_id: str, new_chunk: str, new_seen_tail: str) -> None:
        """P3 perf W4: 接收 worker 侧已完成 sanitize + 去重的增量, 主线程仅 append + emit."""
        if qq_id != self._qq_id:
            self._tail_in_flight = False
            return
        self._tail_in_flight = False
        # 任何一次成功拉取 (包含空增量) 都重置连续失败计数
        self._consecutive_errors = 0
        # worker 已经按 _HISTORY_BYTES 截过尾, 主线程直接覆盖即可
        self._seen_tail = new_seen_tail
        if not new_chunk:
            return
        self._log_storage.append(new_chunk)
        self.output_log_signal.emit(new_chunk)

    def _on_tail_arrived(self, qq_id: str, full_tail: str) -> None:
        """**兼容入口** — 仅保留给单元测试
        ([`test_remote_log_buffer.py`](script/test/test_remote_log_buffer.py)) 使用.

        生产路径走 ``_on_tail_chunk_arrived`` (worker 侧已 sanitize + 去重).
        该槽继续在**主线程**同步做 sanitize + 去重 + append, 行为与历史版本一致,
        让覆盖 ``_compute_new_chunk`` / ANSI sanitize / error backoff 语义的测试
        不需要改 fixture.
        """
        if qq_id != self._qq_id:
            return
        # P3.W3.E: 任何一次成功拉取 (哪怕是空字符串) 都重置连续失败计数
        self._consecutive_errors = 0
        if not full_tail:
            return
        # Linux 端 NapCat 输出含 ANSI 颜色转义, SSH ``tail`` 会把这些转义原封不动
        # 带回来. 必须在拼入 ``_seen_tail`` / ``_log_storage`` 之前清洗一次, 否则:
        #   - 转义字节会以 tofu 形式渲染到 ``QPlainTextEdit``;
        #   - ``LogHighlighter`` 的 ``[info]`` / ``[debug]`` 正则会失配, 整页失色;
        #   - 去重比对的 ``_seen_tail`` 与新一轮 sanitize 后的内容也会偏移.
        full_tail = _sanitize_log_text(full_tail)
        if not full_tail:
            return
        new_chunk = self._compute_new_chunk(full_tail)
        if not new_chunk:
            return
        self._log_storage.append(new_chunk)
        self._seen_tail = (self._seen_tail + new_chunk)[-self._HISTORY_BYTES:]
        self.output_log_signal.emit(new_chunk)

    def _on_tail_error(self, qq_id: str, message: str) -> None:
        # P3 perf W4: 先放掉 in-flight 旗 (worker 失败不会再发 tail_chunk_signal),
        # 然后再做 qq_id 路由 — 避免因 qq_id mismatch 早返导致旗忘记复位.
        # error_signal 来自本实例 own 的 runnable, 实际上 qq_id 永远匹配, 但保留
        # 防御性 mismatch 路径并保证旗的清理.
        self._tail_in_flight = False
        if qq_id != self._qq_id:
            return
        self._consecutive_errors += 1
        logger.trace(
            f"远端 Bot 日志拉取失败(QQID: {qq_id}, consecutive={self._consecutive_errors}): {message}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        # P3.W3.E: 连续超阈值 → 停掉轮询 + 在日志缓冲区注入一行错误提示,
        # 让用户在 BotLogPage 看到为什么日志停了 (而不是静默看起来在跑).
        if (
            self._consecutive_errors >= self._MAX_CONSECUTIVE_ERRORS
            and self._poll_timer.isActive()
        ):
            self._poll_timer.stop()
            err_line = (
                f"\n[ERROR] 远端日志拉取连续失败 {self._consecutive_errors} 次, 已停止轮询; "
                f"请检查 SSH 连接 / 重启 Bot. 最后错误: {message}\n"
            )
            self._log_storage.append(err_line)
            self._seen_tail = (self._seen_tail + err_line)[-self._HISTORY_BYTES:]
            self.output_log_signal.emit(err_line)
            logger.warning(
                f"远端 Bot 日志轮询被退避停掉(QQID: {qq_id}): {message}",
                LogType.NETWORK,
                LogSource.CORE,
            )
        self.error_signal.emit(message)

    def _compute_new_chunk(self, full_tail: str) -> str:
        """从 ``full_tail`` 中切出本次相对上一次新增的部分.

        基本思路: ``full_tail`` 必然完整覆盖 NapCat 最近若干行,
        其中前面一段往往与 ``self._seen_tail`` 的尾部重叠. 找到最长的
        ``self._seen_tail`` 末尾段 = ``full_tail`` 起始段的 ``k``,
        则 ``full_tail[k:]`` 就是新增内容.

        若没有任何重叠 (例如首次拉取, 或日志被外部截断重写),
        把整段视为新增. 这种情况偶尔会让用户多看到一截重复行,
        但不会丢日志, 是可接受的.
        """
        if not self._seen_tail:
            return full_tail

        seen_window = self._seen_tail[-self._OVERLAP_WINDOW:]
        max_k = min(len(seen_window), len(full_tail))
        for k in range(max_k, 0, -1):
            if seen_window.endswith(full_tail[:k]):
                return full_tail[k:]
        return full_tail


class SnowLumaDaemonProcessLog(QObject):
    """SnowLuma Bot 日志桥接 (2026-05-11): 把 :class:`SnowLumaDaemon` 的 node.exe stdout
    包装成 ``NapCatQQProcessLog``-兼容接口, 让 :class:`BotLogPage` 直接复用现有 NapCat 日志页.

    设计背景:
    - SnowLuma Bot 的 ``QQ.exe`` 用 ``ForwardedChannels``, **stdout 不进 pipe**, 没法读;
    - daemon ``node.exe`` 用 ``MergedChannels``, 是 SnowLuma 业务日志 (登录/扫码/消息) 的
      唯一源头; 但 daemon 是**全局共享单例**, 多个 SnowLuma Bot 共用一个 node.exe;
    - 旧版 manager ``_start_local_snowluma`` 路径 HOT 模式不挂 log (primary_process is None),
      COLD 模式挂的是 QQ.exe 的空 log → 用户点【日志】按钮一片空白.

    现在所有 SnowLuma Bot 共享同一个 :class:`SnowLumaDaemonProcessLog` 实例 (manager
    持有, daemon 重启时重建): 每个 Bot 的 ``napcat_log_dict[qq_id]`` 都指向它. UI 拉
    ``get_log_content()`` 拿到全 daemon 节点输出, ``output_log_signal`` 实时增量推送.

    API 与 :class:`NapCatQQProcessLog` 对齐:
    - ``output_log_signal: Signal(str)`` — 增量日志
    - ``get_log_content() -> str`` — 全量日志快照
    """

    output_log_signal = Signal(str)

    def __init__(self, daemon: "SnowLumaDaemon", parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._daemon = daemon
        # 转发 daemon 的 node_log_output_signal 给本 bridge 的 output_log_signal,
        # BotLogPage 不感知底层 daemon, 与 NapCatQQProcessLog 一样订阅即可.
        self._daemon.node_log_output_signal.connect(self.output_log_signal.emit)

    def get_log_content(self) -> str:
        """返回 daemon node.exe stdout 全量缓冲 (与 NapCatQQProcessLog 同名 API)."""
        try:
            return self._daemon.get_node_log_content()
        except Exception:  # noqa: BLE001 - daemon 可能已 deleteLater (shutdown 期), 静默
            return ""

    def clear(self) -> None:
        """与 NapCatQQProcessLog API 对齐 (BotLogPage 不调, 但保留兼容)."""
        # daemon 的 _node_log_storage 是共享的, 单 Bot 不应清空; no-op.
        pass


class ManagerNapCatQQLog(QObject):
    """NapCatQQ 日志管理器 (含 SnowLuma 桥接, 2026-05-11)."""

    def __init__(self) -> None:
        super().__init__()
        self.napcat_log_dict: dict[
            str, NapCatQQProcessLog | RemoteNapCatQQLog | SnowLumaDaemonProcessLog
        ] = {}

    # ==================== 公共函数===================
    def create_log(self, config: Config, process: QProcess) -> None:
        """创建指定 QQ 号的本地日志缓冲区.

        Args:
            config (Config): 配置对象
            process (QProcess): NapCatQQ 进程对象
        """
        qq_id = str(config.bot.QQID)
        self.remove_log(qq_id)
        self.napcat_log_dict[qq_id] = NapCatQQProcessLog(config, process)

    def create_snowluma_log(self, config: Config, daemon: "SnowLumaDaemon") -> None:
        """创建 SnowLuma Bot 的日志桥接 (2026-05-11): 共享 daemon node.exe stdout.

        所有 SnowLuma Bot 共享一个 :class:`SnowLumaDaemonProcessLog` 实例 (per Bot 各创建
        一个, 但底层 daemon stdout 共用) — UI 不感知共享, 跟 NapCat 路径完全一样订阅 / 取内容.

        Args:
            config: Bot 配置.
            daemon: SnowLuma daemon 单例.
        """
        qq_id = str(config.bot.QQID)
        self.remove_log(qq_id)
        self.napcat_log_dict[qq_id] = SnowLumaDaemonProcessLog(daemon, parent=self)

    def create_remote_log(self, config: Config) -> None:
        """创建指定 QQ 号的远端日志缓冲区 (P3).

        与 [`create_log`](src/core/runtime/bot_process_manager.py) 对称, 但底层使用周期性 SSH
        ``tail`` 拉取远端 ``napcat_<qq_id>.log``, 而非本地 QProcess stdout.

        Args:
            config (Config): 配置对象, 必须 ``runtime_target != 'local'``.
        """
        qq_id = str(config.bot.QQID)
        self.remove_log(qq_id)
        self.napcat_log_dict[qq_id] = RemoteNapCatQQLog(config)

    def get_log(
        self, qq_id: str
    ) -> NapCatQQProcessLog | RemoteNapCatQQLog | SnowLumaDaemonProcessLog | None:
        """获取指定 QQ 号的日志缓冲区

        Args:
            qq_id (str): QQ 号

        Returns:
            日志缓冲对象 (NapCat / Remote / SnowLuma 三种之一), 如果不存在则返回 None.
        """
        return self.napcat_log_dict.get(qq_id, None)

    def remove_log(self, qq_id: str) -> None:
        """移除指定 QQ 号的日志缓冲区, 释放底层资源 (远端日志的 SSH 轮询计时器)."""
        log = self.napcat_log_dict.pop(qq_id, None)
        if log is None:
            return
        # 远端日志需要主动停掉计时器; 本地 QProcess 日志靠 process 自己生命周期回收.
        stop = getattr(log, "stop", None)
        if callable(stop):
            try:
                stop()
            except Exception:  # noqa: BLE001
                pass
        # SnowLumaDaemonProcessLog 是 QObject (parent=manager), 用 deleteLater 释放
        delete_later = getattr(log, "deleteLater", None)
        if callable(delete_later):
            try:
                delete_later()
            except Exception:  # noqa: BLE001
                pass


class RemoteBotOperationRunnable(QObject, QRunnable):
    """远端 Bot 操作的 [`QRunnable`](https://doc.qt.io/qt-6/qrunnable.html) (P2.6).

    在 [`QThreadPool`](https://doc.qt.io/qt-6/qthreadpool.html) 后台线程上执行 SSH 阻塞调用
    (``backend.start_napcat`` / ``stop_napcat`` / ``get_process_status`` / ``get_webui_endpoint``),
    以避免阻塞 Qt 主线程.

    动作语义:
    - ``"start"``: 调用 ``backend.start_napcat(qq_id, config)``;
      成功时 ``operation_finished_signal`` 携带 ``ProcessStatus``.
    - ``"stop"``: 调用 ``backend.stop_napcat(qq_id)``;
      成功时 ``operation_finished_signal`` 携带 None.
    - ``"poll"``: 调用 ``get_process_status`` + ``get_webui_endpoint``;
      成功时 ``operation_finished_signal`` 携带 ``(ProcessStatus, WebUIEndpoint | None)`` 元组.
    """

    # 信号: (qq_id, action, payload).  payload 类型按 action 区分.
    operation_finished_signal = Signal(str, str, object)
    # 信号: (qq_id, action, error_message)
    operation_failed_signal = Signal(str, str, str)

    def __init__(
        self,
        qq_id: str,
        config: Config,
        action: str,
    ) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self._qq_id = qq_id
        self._config = config
        self._action = action

    # P3 perf: ``start`` / ``stop`` 是用户可感知的、单次会话只触发一次的动作,
    # 上报给 BackgroundTaskCenter 以驱动 ProgressInfoBar; ``poll`` 是 5s 一次的
    # 静默轮询, 上报会让 InfoBar 频繁闪烁, 故不上报.
    _TASK_LABELS = {
        "start": "启动远端 Bot {qq_id}",
        "stop": "停止远端 Bot {qq_id}",
    }
    _TASK_CONTENTS = {
        "start": "正在通过 SSH 启动 NapCat 进程…",
        "stop": "正在通过 SSH 停止 NapCat 进程…",
    }
    _TASK_SUCCESS_MESSAGES = {
        "start": "Bot {qq_id} 启动成功",
        "stop": "Bot {qq_id} 已停止",
    }

    def _task_id(self) -> str | None:
        if self._action not in self._TASK_LABELS:
            return None
        return f"remote-bot-{self._action}-{self._qq_id}"

    def run(self) -> None:  # noqa: D401 - QRunnable 框架约定
        # 延迟导入避免循环依赖 (background_tasks 在测试环境下可能未被 creart 初始化)
        task_id = self._task_id()
        center = None
        if task_id is not None:
            try:
                from creart import it
                from src.core.runtime.background_tasks import BackgroundTaskCenter

                center = it(BackgroundTaskCenter)
                label = self._TASK_LABELS[self._action].format(qq_id=self._qq_id)
                content = self._TASK_CONTENTS.get(self._action, "")
                center.begin(task_id, label, content=content)
            except Exception:  # noqa: BLE001 - center 不可用时不应阻断 SSH 主流程
                center = None

        success = False
        failure_message = ""
        try:
            # 延迟导入避免循环依赖
            from src.core.operation.resolver import resolve_backend_for_bot

            backend = resolve_backend_for_bot(self._config)
            backend.connect()

            if self._action == "start":
                status = backend.start_napcat(self._qq_id, self._config)
                self.operation_finished_signal.emit(self._qq_id, self._action, status)
                success = True
                return

            if self._action == "stop":
                # 关闭 WebUI 隧道避免悬挂资源(若 backend 支持的话)
                close_tunnel = getattr(backend, "close_webui_tunnel", None)
                if close_tunnel is not None:
                    try:
                        close_tunnel(self._qq_id)
                    except Exception:  # noqa: BLE001
                        pass
                backend.stop_napcat(self._qq_id)
                self.operation_finished_signal.emit(self._qq_id, self._action, None)
                success = True
                return

            if self._action == "poll":
                status = backend.get_process_status(self._qq_id)
                endpoint = None
                if status.running:
                    try:
                        endpoint = backend.get_webui_endpoint(self._qq_id)
                    except Exception as exc:  # noqa: BLE001 - 探测失败不应影响 poll
                        logger.trace(
                            f"远端 WebUI 端点探测失败(QQID: {self._qq_id}): "
                            f"{type(exc).__name__}: {exc}",
                            LogType.NETWORK,
                            LogSource.CORE,
                        )
                self.operation_finished_signal.emit(
                    self._qq_id, self._action, (status, endpoint)
                )
                # poll 不上报 center, 这里 success 状态无意义
                return

            failure_message = f"未知远端操作: {self._action}"
            self.operation_failed_signal.emit(self._qq_id, self._action, failure_message)
        except Exception as exc:  # noqa: BLE001 - 边界处统一捕获, 把详细错误回到 UI 线程
            failure_message = f"{type(exc).__name__}: {exc}"
            logger.warning(
                f"远端 Bot {self._action} 操作失败(QQID: {self._qq_id}): {failure_message}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            self.operation_failed_signal.emit(self._qq_id, self._action, failure_message)
        finally:
            if center is not None and task_id is not None:
                try:
                    if success:
                        success_message = self._TASK_SUCCESS_MESSAGES.get(self._action, "").format(
                            qq_id=self._qq_id
                        )
                        center.end(task_id, success=True, message=success_message)
                    else:
                        center.fail(task_id, failure_message or f"{self._action} 失败")
                except Exception:  # noqa: BLE001
                    pass


class GetAuthStatusRunnable(QObject, QRunnable):
    """获取 NapCatQQ Auth 信息的任务类"""

    # 信号
    login_auth_signal = Signal(str)

    def __init__(self, port: int, token: str) -> None:
        """获取 NapCatQQ Auth 信息的任务类

        Args:
            port (int): WebUI 端口
            token (str): WebUI Token
        """
        QObject.__init__(self)
        QRunnable.__init__(self)
        # 设置属性
        self.port = port
        self.token = token

    def run(self) -> None:
        """执行获取认证信息的任务"""
        try:
            response = post(
                f"http://localhost:{self.port}/api/auth/login",
                json={"hash": hashlib.sha256((self.token + ".napcat").encode("utf-8")).hexdigest()},
                headers={"Content-Type": "application/json"},
                timeout=5,
            )
            if response.status_code == 200:
                self.login_auth_signal.emit(response.json().get("data", {}).get("Credential", ""))
        except Exception as e:
            logger.trace(
                f"获取认证信息失败: {type(e).__name__}: {e}",
                LogType.NETWORK,
                LogSource.CORE,
            )


class GetLoginStatusRunnable(QObject, QRunnable):
    """获取 NapCatQQ 登录状态的任务类"""

    # 信号
    login_status_signal = Signal(bool)
    login_qrcode_signal = Signal(str)
    online_status_signal = Signal(bool)
    auth_refresh_requested_signal = Signal()

    def __init__(self, port: int, token: str, auth: str | None) -> None:
        """获取 NapCatQQ Auth 信息的任务类

        Args:
            port (int): WebUI 端口
            token (str): WebUI Token
            auth (str | None): 认证信息
        """
        QObject.__init__(self)
        QRunnable.__init__(self)
        # 设置属性
        self.port = port
        self.token = token
        self.auth = auth
        self._auth_refresh_requested = False

    def _request_auth_refresh(self) -> None:
        """在鉴权失效时请求立即刷新 auth。"""
        if self._auth_refresh_requested:
            return

        self._auth_refresh_requested = True
        self.auth_refresh_requested_signal.emit()

    def run(self) -> None:
        """执行获取认证信息的任务"""
        if not self.auth:
            return

        # 创建 HTTP 客户端
        self.client = Client(base_url=f"http://localhost:{self.port}", timeout=5)
        self.client.headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {self.auth}",
        }

        try:
            # 获取登录状态
            self.get_login_status()
            # 获取在线状态
            self.get_online_status()
        except Exception as e:
            # 捕获所有异常，避免未处理异常导致崩溃
            logger.trace(
                f"获取 NapCat 登录状态失败: {type(e).__name__}: {e}",
                LogType.NETWORK,
                LogSource.CORE,
            )

    def get_login_status(self) -> None:
        """获取 NapCatQQ 登录状态"""
        try:
            response = self.client.post("/api/QQLogin/CheckLoginStatus")
            if response.status_code in (401, 403):
                logger.trace(
                    f"获取登录状态鉴权失效，准备刷新认证(QQ WebUI port={self.port}, status={response.status_code})",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                self._request_auth_refresh()
                return

            if response.status_code != 200:
                return

            # 解析结果
            result = response.json().get("data", {})
            is_login = result.get("isLogin", False)
            qr_code = result.get("qrcodeurl", "")

            # 发出信号
            self.login_status_signal.emit(is_login)

            if not is_login and qr_code:
                self.login_qrcode_signal.emit(qr_code)
        except Exception as e:
            logger.trace(
                f"获取登录状态失败: {type(e).__name__}: {e}",
                LogType.NETWORK,
                LogSource.CORE,
            )

    def get_online_status(self) -> None:
        """获取 NapCatQQ 在线状态"""
        try:
            response = self.client.post("/api/QQLogin/GetQQLoginInfo")
            if response.status_code in (401, 403):
                logger.trace(
                    f"获取在线状态鉴权失效，准备刷新认证(QQ WebUI port={self.port}, status={response.status_code})",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                self._request_auth_refresh()
                return

            if response.status_code == 200:
                result = response.json().get("data", {})
                self.online_status_signal.emit(result.get("online", False))
        except Exception as e:
            logger.trace(
                f"获取在线状态失败: {type(e).__name__}: {e}",
                LogType.NETWORK,
                LogSource.CORE,
            )


class NapCatQQLoginState(QObject):
    """NapCatQQ 登录状态类

    负责管理 NapCatQQ 的登录状态
    """

    qr_code_available_signal = Signal(str, str)
    qr_code_removed_signal = Signal(str)
    notification_signal = Signal(str, str)

    def __init__(self, config: Config, port: int, token: str) -> None:
        """初始化 NapCatQQ 登录状态"""
        super().__init__()
        # 设置属性
        self.config = config
        self.port = port
        self.token = token
        self.auth: str | None = None

        # 登录状态属性
        self._is_logged_in = False
        self._online_status = False
        self._offline_notice = False
        self._login_invalidated_while_online = False
        self._suppress_qrcode_until_online = False
        self._last_auth_refresh_attempt_at = 0.0
        self._login_in_flight = False
        self._auth_in_flight = False
        self._disposed = False

        # 启动定时器以定期获取授权状态
        self._auth_timer = QTimer(self)
        self._auth_timer.timeout.connect(self.slot_get_auth_status)
        self._auth_timer.start(30 * 60 * 1000)  # 30分钟

        # 启动定时器定期获取登录状态（使用配置的间隔）
        self._login_state_timer = QTimer(self)
        self._login_state_timer.timeout.connect(self.slot_get_login_state)
        login_check_interval = cfg.get(cfg.bot_login_check_interval)
        self._login_state_timer.start(login_check_interval)

        # 监听配置变化
        cfg.bot_login_check_interval.valueChanged.connect(self._on_login_check_interval_changed)

        # 立即执行一次（在事件循环中）
        QTimer.singleShot(0, self.slot_get_auth_status)
        # 未登录 bot 的首次登录态/二维码刷新应在 1 秒后触发
        # 不应被常规轮询配置间隔（可能被设置得很大）所延迟
        QTimer.singleShot(1000, self.slot_get_login_state)

    # ==================== 公共方法 ==================
    def get_login_state(self) -> bool:
        """获取登录状态"""
        return self._is_logged_in

    def get_online_status(self) -> bool:
        """获取在线状态"""
        return self._online_status

    def remove(self) -> None:
        """清理 Timer 和释放资源"""
        # P3 perf W4 (crash fix): 先置 _disposed 旗, 让任何后到的 slot 调用静默早返;
        # 否则 in-flight runner 结束后 emit 的信号在 deleteLater 之后命中 slot
        # 会触发 RuntimeError.
        self._disposed = True

        # 断开配置监听
        try:
            cfg.bot_login_check_interval.valueChanged.disconnect(self._on_login_check_interval_changed)
        except (RuntimeError, TypeError):
            pass

        self._auth_timer.stop()
        self._auth_timer.deleteLater()
        self._login_state_timer.stop()
        self._login_state_timer.deleteLater()
        self.qr_code_removed_signal.emit(str(self.config.bot.QQID))

    def _on_login_check_interval_changed(self, interval_ms: int) -> None:
        """登录检查间隔配置变化时更新定时器（仅在已登录时生效）"""
        if self._disposed:
            return
        if self._is_logged_in:
            self._login_state_timer.setInterval(interval_ms)
            logger.trace(
                f"NapCat 登录状态检查间隔已更新(QQID: {self.config.bot.QQID}, interval={interval_ms}ms)",
                LogType.NETWORK,
                LogSource.CORE,
            )
        else:
            logger.trace(
                f"NapCat 未登录状态，保持1秒检查间隔(QQID: {self.config.bot.QQID}, configured={interval_ms}ms)",
                LogType.NETWORK,
                LogSource.CORE,
            )

    def _emit_notification(self, level: str, message: str) -> None:
        """向 UI 层发布运行时通知"""
        self.notification_signal.emit(level, message)

    def _start_notification_task(self, task: NotificationTask, success_message: str) -> None:
        """启动通知任务并将结果转发给 UI 层"""
        task.success_signal.connect(lambda _: self._emit_notification("success", success_message))
        task.error_signal.connect(lambda msg: self._emit_notification("error", msg))
        QThreadPool.globalInstance().start(cast(QRunnable, task))

    # ==================== 槽函数 ====================
    def slot_get_login_state(self) -> None:
        """获取登录状态"""
        if self._disposed:
            return
        if not self.auth:
            self.slot_request_auth_refresh()
            return

        if self._login_in_flight:
            return
        self._login_in_flight = True

        runner = GetLoginStatusRunnable(port=self.port, token=self.token, auth=self.auth)
        runner.login_status_signal.connect(self.slot_update_login_state)
        runner.online_status_signal.connect(self.slot_update_online_status)
        runner.login_qrcode_signal.connect(self.slot_update_login_qrcode)
        runner.auth_refresh_requested_signal.connect(self.slot_request_auth_refresh)
        runner.login_status_signal.connect(lambda _ok: self._clear_login_in_flight())
        runner.auth_refresh_requested_signal.connect(self._clear_login_in_flight)
        QThreadPool.globalInstance().start(runner)

    def _clear_login_in_flight(self, *_args: object) -> None:
        """P3 perf W4: 统一复位 ``_login_in_flight``."""
        self._login_in_flight = False

    def slot_get_auth_status(self) -> None:
        """获取认证状态"""
        if self._disposed:
            return
        self._last_auth_refresh_attempt_at = monotonic()
        if self._auth_in_flight:
            return
        self._auth_in_flight = True
        runner = GetAuthStatusRunnable(port=self.port, token=self.token)
        runner.login_auth_signal.connect(self.slot_update_auth)
        runner.login_auth_signal.connect(lambda _auth: self._clear_auth_in_flight())
        QThreadPool.globalInstance().start(runner)

    def _clear_auth_in_flight(self, *_args: object) -> None:
        self._auth_in_flight = False

    def slot_request_auth_refresh(self) -> None:
        """在登录状态轮询鉴权失效时，立即刷新 auth。"""
        if self._disposed:
            return
        if monotonic() - self._last_auth_refresh_attempt_at < 5:
            return

        self.auth = None
        logger.trace(
            f"NapCat 请求立即刷新认证(QQID: {self.config.bot.QQID})",
            LogType.NETWORK,
            LogSource.CORE,
        )
        self.slot_get_auth_status()

    def slot_update_auth(self, auth: str) -> None:
        """更新认证信息"""
        if self._disposed:
            return
        self.auth = auth
        logger.trace(
            f"NapCat 登录认证信息已更新(QQID: {self.config.bot.QQID}, has_auth={bool(auth)})",
            LogType.NETWORK,
            LogSource.CORE,
        )

    def slot_update_login_state(self, is_login: bool) -> None:
        """更新登录状态"""
        if self._disposed:
            return
        prev_login = self._is_logged_in
        self._is_logged_in = is_login
        logger.trace(
            f"NapCat 登录状态更新(QQID: {self.config.bot.QQID}, is_login={is_login})",
            LogType.NETWORK,
            LogSource.CORE,
        )

        # 根据登录状态调整检查间隔
        if is_login:
            configured_interval = cfg.get(cfg.bot_login_check_interval)
            if self._login_state_timer.interval() != configured_interval:
                self._login_state_timer.setInterval(configured_interval)
                logger.trace(
                    f"NapCat 已登录，恢复配置的检查间隔(QQID: {self.config.bot.QQID}, interval={configured_interval}ms)",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
        else:
            if self._login_state_timer.interval() != 1000:
                self._login_state_timer.setInterval(1000)
                logger.trace(
                    f"NapCat 未登录，强制使用1秒检查间隔(QQID: {self.config.bot.QQID})",
                    LogType.NETWORK,
                    LogSource.CORE,
                )

        if is_login:
            self._login_invalidated_while_online = False
            self._suppress_qrcode_until_online = False
            self.qr_code_removed_signal.emit(str(self.config.bot.QQID))
            return

        if prev_login and self._online_status:
            self._login_invalidated_while_online = True
            logger.trace(
                f"NapCat 检测到登录状态在在线期间失效(QQID: {self.config.bot.QQID})，等待在线状态确认后处理二维码",
                LogType.NETWORK,
                LogSource.CORE,
            )

    def slot_update_online_status(self, online_status: bool) -> None:
        """更新在线状态"""
        if self._disposed:
            return
        prev_online = self._online_status
        login_invalidated_while_online = self._login_invalidated_while_online

        self._online_status = online_status
        logger.trace(
            (
                "NapCat 在线状态更新: "
                f"QQID={self.config.bot.QQID}, prev_online={prev_online}, online={online_status}, "
                f"is_logged_in={self._is_logged_in}, offline_notice_sent={self._offline_notice}, "
                f"offline_auto_restart={self.config.bot.offlineAutoRestart}, "
                f"login_invalidated_while_online={login_invalidated_while_online}, "
                f"suppress_qrcode_until_online={self._suppress_qrcode_until_online}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

        if online_status:
            self._offline_notice = False
            self._login_invalidated_while_online = False
            self._suppress_qrcode_until_online = False
            return

        if not prev_online:
            return

        if login_invalidated_while_online:
            self._login_invalidated_while_online = False
            self._suppress_qrcode_until_online = True
            self.qr_code_removed_signal.emit(str(self.config.bot.QQID))

        if not self._is_logged_in and not login_invalidated_while_online:
            return

        if self.config.bot.offlineAutoRestart:
            if not self._offline_notice and self.config.advanced.offlineNotice:
                if cfg.get(cfg.bot_offline_web_hook_notice):
                    self._start_notification_task(
                        create_offline_webhook_task(self.config), self.tr("已发送离线通知到配置的 WebHook 地址")
                    )

                if cfg.get(cfg.bot_offline_email_notice):
                    self._start_notification_task(
                        create_offline_email_task(self.config), self.tr("已发送离线通知到配置的邮箱地址")
                    )

                self._offline_notice = True

            # P2 (Tier I): ManagerNapCatQQProcess → BotProcessManager,
            # restart_process → restart_bot.
            it(BotProcessManager).restart_bot(self.config)
            return

        if self._offline_notice:
            return

        if not self.config.advanced.offlineNotice:
            return

        if cfg.get(cfg.bot_offline_web_hook_notice):
            self._start_notification_task(
                create_offline_webhook_task(self.config), self.tr("已发送离线通知到配置的 WebHook 地址")
            )

        if cfg.get(cfg.bot_offline_email_notice):
            self._start_notification_task(
                create_offline_email_task(self.config), self.tr("已发送离线通知到配置的邮箱地址")
            )

        self._offline_notice = True

    def slot_update_login_qrcode(self, qr_code: str) -> None:
        """更新登录二维码"""
        if self._disposed:
            return
        if self._login_invalidated_while_online or self._suppress_qrcode_until_online:
            logger.trace(
                f"NapCat 跳过展示已失效的登录二维码(QQID: {self.config.bot.QQID})",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return

        self.qr_code_available_signal.emit(str(self.config.bot.QQID), qr_code)


class ManagerNapCatQQLoginState(QObject):
    """NapCatQQ 登录状态管理类

    负责管理 NapCatQQ 的登录状态
    """

    qr_code_available_signal = Signal(str, str)
    qr_code_removed_signal = Signal(str)
    notification_signal = Signal(str, str)

    def __init__(self) -> None:
        """初始化 NapCatQQ 登录状态管理器"""
        super().__init__()
        self.napcat_login_state_dict: dict[str, NapCatQQLoginState] = {}

    def create_login_state(self, config: Config, port: int, token: str) -> None:
        """创建并添加登录状态对象"""
        qq_id = str(config.bot.QQID)
        self.remove_login_state(qq_id)
        logger.trace(
            f"创建 NapCat 登录状态管理器(QQID: {qq_id}, port={port}, has_token={bool(token)})",
            LogType.NETWORK,
            LogSource.CORE,
        )

        login_state = NapCatQQLoginState(config=config, port=port, token=token)
        login_state.qr_code_available_signal.connect(
            lambda emitted_qq_id, qr_code: self.qr_code_available_signal.emit(emitted_qq_id, qr_code)
        )
        login_state.qr_code_removed_signal.connect(
            lambda emitted_qq_id: self.qr_code_removed_signal.emit(emitted_qq_id)
        )
        login_state.notification_signal.connect(lambda level, message: self.notification_signal.emit(level, message))
        self.napcat_login_state_dict[qq_id] = login_state

    def get_login_state(self, qq_id: str) -> NapCatQQLoginState | None:
        """获取指定 QQ 号的登录状态对象"""
        return self.napcat_login_state_dict.get(qq_id, None)

    def remove_login_state(self, qq_id: str) -> None:
        """移除指定 QQ 号的登录状态对象"""
        if qq_id in self.napcat_login_state_dict:
            self.napcat_login_state_dict[qq_id].remove()
            self.napcat_login_state_dict.pop(qq_id)


class ManagerAutoRestartProcess(QObject):
    """NapCatQQ 自动重启管理类"""

    def __init__(self) -> None:
        super().__init__()
        self.auto_restart_process_dict: dict[str, QTimer] = {}

    def create_auto_restart_timer(self, config: Config) -> None:
        """创建自动重启定时器"""

        # 必要的前置检查
        if str(config.bot.QQID) in self.auto_restart_process_dict:
            return

        if not config.bot.autoRestartSchedule.enable:
            return

        # 计算时间间隔(毫秒)
        time_unit_multipliers = {
            TimeUnitEnum.MINUTE: 60,
            TimeUnitEnum.HOUR: 3600,
            TimeUnitEnum.DAY: 86400,
            TimeUnitEnum.MONTH: 2592000,
            TimeUnitEnum.YEAR: 31536000,
        }
        interval = (
            config.bot.autoRestartSchedule.duration * time_unit_multipliers[config.bot.autoRestartSchedule.time_unit]
        )
        interval_ms = interval * 1000
        logger.trace(
            (
                "创建自动重启定时器: "
                f"QQID={config.bot.QQID}, enable={config.bot.autoRestartSchedule.enable}, "
                f"duration={config.bot.autoRestartSchedule.duration}, "
                f"time_unit={config.bot.autoRestartSchedule.time_unit}, interval_ms={interval_ms}"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )

        # 创建定时器
        timer = QTimer(self)
        timer.setInterval(interval_ms)
        # P2 (Tier I): ManagerNapCatQQProcess → BotProcessManager,
        # restart_process → restart_bot.
        timer.timeout.connect(lambda: it(BotProcessManager).restart_bot(config))
        timer.start()

        # 添加到字典
        self.auto_restart_process_dict[str(config.bot.QQID)] = timer

    def remove_auto_restart_timer(self, qq_id: str) -> None:
        """移除自动重启定时器"""
        if qq_id in self.auto_restart_process_dict:
            self.auto_restart_process_dict[qq_id].stop()
            self.auto_restart_process_dict[qq_id].timeout.disconnect()
            self.auto_restart_process_dict[qq_id].deleteLater()
            self.auto_restart_process_dict.pop(qq_id)


# ==================== Bot 进程管理总入口 ====================
class BotProcessManager(QObject):
    """Bot 进程管理类 (Tier I 重构后, 取代原 ``ManagerNapCatQQProcess``).

    职责:
    - 持 :class:`NapCatDriver` + :class:`SnowLumaDriver` 两个 driver 实例,
      按 ``config.bot.backend_type`` dispatch 启动 / 停止
    - 持 ``napcat_process_dict`` (本地 NapCat 进程模型) 与 ``remote_process_dict``
      (远端 Bot 记录); SnowLuma 进程模型由 ``SnowLumaDriver`` 内部持有
    - 转发 driver / poller 信号统一对外暴露 ``process_changed_signal`` /
      ``notification_signal`` / ``snowluma_login_state_signal``
    - 远端 Bot SSH 调度 (P2.6 路径) 仍在本类中, 与 driver 体系正交
    - 单实例守护 (一期仅 1 SnowLuma Bot) 由 ``SnowLumaDriver._processes``
      自身字典检查触发 ``RuntimeError``, 这里捕获后 emit error.

    Attributes:
        process_changed_signal: ``(qq_id: str, state: QProcess.ProcessState)``
        notification_signal: ``(level: str, message: str)``
        snowluma_login_state_signal: ``(qq_id: str, state_name: str)`` —
            状态值取 ``"starting"`` / ``"waiting_for_qr_scan"`` / ``"logged_in"`` /
            ``"disconnected"`` (W5 重写后).
    """

    # 进程状态改变信号
    process_changed_signal = Signal(str, QProcess.ProcessState)
    notification_signal = Signal(str, str)
    # P1 (SnowLuma 适配): SnowLuma 登录态变更信号
    # 参数: (qq_id: str, state_name: str)
    # NapCat 路径不使用该信号 (仍由 ManagerNapCatQQLoginState 管理)
    snowluma_login_state_signal = Signal(str, str)

    # 远端 Bot 状态轮询周期 (毫秒).
    _REMOTE_POLLING_INTERVAL_MS = 5000

    def __init__(self) -> None:
        """初始化 Bot 进程管理器"""
        super().__init__()

        # 本地 NapCat 进程模型 (P1 字段名沿用, 不重命名避免连带影响)
        self.napcat_process_dict: dict[str, NapCatProcessModel] = {}
        # P2.6: 远端 Bot 字典 (走 SSH worker, 与本地 QProcess 完全分离)
        self.remote_process_dict: dict[str, RemoteProcessRecord] = {}

        # P2 Tier I: NapCat / SnowLuma driver 实例
        self._napcat_driver = NapCatDriver()
        self._snowluma_driver = SnowLumaDriver()

        # P2 Tier E (异步化修复): SnowLuma Phase C worker 引用字典, 防止 worker 在
        # QThreadPool 跑期间被 Python GC; 在 succeeded / failed 回调里 pop + deleteLater.
        # key = qq_id, value = _SnowLumaPhaseCWorker (W2 后只剩注入阶段, daemon 接管 wait_ready+login)
        self._snowluma_pending_workers: dict[str, object] = {}

        # W7 (2026-05-11): 接 SnowLuma daemon 的 crashed 信号, 触发全员 SnowLuma Bot
        # 清理路径. 不在这里做 ``it(SnowLumaDaemon)`` 的 hard 调用 (creart 在测试场景下
        # 可能未初始化), 改用 :meth:`_subscribe_daemon_crashed` 在第一次启 SnowLuma Bot
        # 前惰性 wire 一次. 用 ``_daemon_crashed_wired`` 守护重复 connect.
        self._daemon_crashed_wired: bool = False

        logger.info("Bot 进程管理器已初始化 (NapCat + SnowLuma drivers)")

    # ==================== W7: SnowLuma daemon 崩溃信号接线 ====================
    def _subscribe_daemon_crashed(self) -> None:
        """惰性接 SnowLuma daemon ``crashed`` 信号到 :meth:`_on_daemon_crashed`.

        触发时机: 每次启动 SnowLuma Bot 前调一次 (幂等). 选择惰性而非 ``__init__``:

        - 测试 / CI 环境 ``creart`` 可能尚未注册 ``SnowLumaDaemon`` (没启过任何
          SnowLuma Bot); ``__init__`` 时 ``it(SnowLumaDaemon)`` 会 raise.
        - 用户从未用过 SnowLuma 时, 不必构造 daemon 实例 (零开销).

        异常静默: 接线失败不应阻塞 Bot 启动; 仅 log warning. 主要风险是 ``Qt.QueuedConnection``
        因 daemon 已 ``deleteLater`` 而 connect 失败 (极端场景, 不阻塞主链路).
        """
        if self._daemon_crashed_wired:
            return
        try:
            daemon = self._snowluma_driver._get_daemon()
            # Qt.QueuedConnection: daemon.emit() 在工作线程 (node.exe finished 回调),
            # _on_daemon_crashed 必须主线程跑 (调 stop_bot → terminate_async).
            daemon.crashed.connect(self._on_daemon_crashed, Qt.ConnectionType.QueuedConnection)
            self._daemon_crashed_wired = True
            logger.info(
                "已接 SnowLuma daemon.crashed → BotProcessManager._on_daemon_crashed",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
        except Exception as exc:  # noqa: BLE001 - daemon 不可达不应阻塞 Bot 启动
            logger.warning(
                (
                    "接 SnowLuma daemon.crashed 信号失败 (将走兜底清理路径): "
                    f"{type(exc).__name__}: {exc}"
                ),
                LogType.FILE_FUNC,
                LogSource.CORE,
            )

    @Slot(str)
    def _on_daemon_crashed(self, message: str) -> None:
        """W7: SnowLuma daemon 崩溃信号槽 — 全员 SnowLuma Bot 清理 + 用户通知.

        触发场景: ``SnowLumaDaemon`` 检测到 ``node.exe`` **意外** finished (非
        ``release()`` 触发), state 切到 ``CRASHED`` 并 emit. daemon 已:

        - 清自己的 ``_ref_count = 0``, ``_webui_client = None``;
        - 标 ``_dead_event``, 唤醒任何在等 STARTING 的 caller (它们会 raise);
        - ``deleteLater()`` node.exe QProcess.

        manager 这里负责:

        1. 全员 SnowLuma Bot 走 :meth:`stop_bot` (异步 fire-and-forget):
           kill QQ.exe (COLD), 停 poller, 释放 driver 字典, emit ``NotRunning``.
        2. 一条 ``notification_signal("error", ...)`` 给 UI (info-bar / dialog).
        3. **不**自动重启 daemon: 本期决策 (D4) — 用户去组件页 SnowLuma tab 查日志,
           确认问题后停掉**所有** SnowLuma Bot 即可重启 daemon (
           ``_ref_count == 0`` 时 ``ensure_running`` 会重新 spawn node.exe).

        Args:
            message: daemon 拼好的崩溃描述 (含 ``exit_code`` + ``errorString``).
        """
        # 收集需要清的 SnowLuma Bot qq_id 列表 (复制一份, 因 stop_bot 会改 driver 字典).
        affected_qq_ids = [model.qq_id for model in self._snowluma_driver.list_processes()]

        logger.error(
            (
                "SnowLuma daemon 崩溃, 触发全员 SnowLuma Bot 清理: "
                f"affected_qq_ids={affected_qq_ids!r}, daemon_message={message!r}"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )

        # 给用户一条总提示 (info-bar 级别 error); 即使没有任何受影响 Bot 也发,
        # 让组件页可见 daemon 状态.
        self._safe_emit_notification(
            "error",
            (
                "SnowLuma daemon 已崩溃, 已停止所有 SnowLuma Bot. "
                "请到组件页 SnowLuma tab 查看 daemon 日志确认原因, 修复后重新启动 Bot. "
                f"(daemon: {message})"
            ),
        )

        # 全员清理: stop_bot 内部对 SnowLuma 路径会 detach poller + remove model + 释放 ref.
        for qq_id in affected_qq_ids:
            try:
                self.stop_bot(qq_id)
            except Exception as exc:  # noqa: BLE001 - 单个 Bot 清理失败不阻塞其他 Bot
                logger.warning(
                    (
                        "daemon crashed 清理路径 stop_bot 失败 (静默继续): "
                        f"qq_id={qq_id}, error={type(exc).__name__}: {exc}"
                    ),
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )

    # ==================== 私有: SnowLuma 轮询器接线 ====================
    def _connect_snowluma_poller_signal(self, qq_id: str) -> None:
        """W5: SnowLumaDriver 在 Phase D 内部已创建 + start 了 poller; manager 这里只需把
        poller.state_changed 连到 ``snowluma_login_state_signal`` 转发出去.

        Q2 (UIN 匹配): 同时接 poller.uin_detected → :meth:`_on_snowluma_uin_detected`,
        在主线程对比配置 QQID 与实际登录 UIN, 不一致时 emit error + stop bot.

        如果 poller 不存在 (driver.start 在 Phase D 之前失败), 静默忽略.
        """
        poller = self._snowluma_driver.get_status_poller(qq_id)
        if poller is None:
            return
        poller.state_changed.connect(
            lambda emitted_qq_id, state_name: self.snowluma_login_state_signal.emit(
                emitted_qq_id, state_name
            )
        )
        # Q2: UIN 匹配 — 主要用于热启动场景, 避免 inject 到错误账号的 QQ.exe.
        poller.uin_detected.connect(self._on_snowluma_uin_detected)
        # 2026-05-11 内存监控修复: poller 按 UIN 聚合的 hooked PID 集合写回 model.ancillary_pids,
        # ``get_memory_usage`` 据此累加 RSS 显示与 SnowLuma WebUI 一致的内存值.
        # 旧版 manager 没接此信号, ``ancillary_pids`` 永远空; 内存只走 ``qq_pid`` walk 树
        # (HOT 模式 qq_process is None → 总返回 0).
        poller.pid_set_changed.connect(self._on_snowluma_pid_set_changed)

    def _on_snowluma_pid_set_changed(self, qq_id: str, pid_list: list[int]) -> None:
        """``SnowLumaStatusPoller.pid_set_changed`` 回调: 写回 model.ancillary_pids.

        - poller 按本 Bot UIN 聚合 ``/api/processes`` 中的 hooked QQ.exe 进程, 集合变化时
          emit ``(qq_id, sorted(pids))``. 本 slot 把 list 转回 set 写入 model.
        - 后续 :meth:`get_memory_usage` 优先用 ``ancillary_pids`` 累加 RSS, 显示与
          SnowLuma WebUI 一致的内存 (hooked 进程集合; 不含 launcher Electron 父进程).
        - HOT 模式下 ``qq_process is None`` 旧版只能返回 0, 现在能拿到真实内存.
        """
        snow_model = self._snowluma_driver.get_process_model(qq_id)
        if snow_model is None:
            return
        snow_model.ancillary_pids = set(pid_list)
        logger.trace(
            (
                f"SnowLuma ancillary_pids 已更新(QQID: {qq_id}, "
                f"count={len(pid_list)}, pids={pid_list})"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )

    def _on_snowluma_uin_detected(self, qq_id: str, detected_uin: str) -> None:
        """Q2 (UIN 匹配): SnowLuma poller 首次拿到真实 UIN 时的回调.

        对比配置的 ``qq_id`` 与实际从 SnowLuma WebUI 拿到的 ``detected_uin``:

        - **一致** → log info 确认, 不做其他动作.
        - **不一致** → 主要发生在热启动场景: 用户附加到了登录其他账号的 QQ.exe.
          emit error notification 给 UI 弹错误条, 然后 :meth:`stop_bot` 把 Bot 干净停下来.
          (停止过程是异步的, 用户的 QQ 在热启动模式不会被杀, 只 unload SL hook.)
        """
        configured_qqid = qq_id  # qq_id 就是 config.bot.QQID 的字符串形式
        if detected_uin == configured_qqid:
            logger.info(
                (
                    f"SnowLuma UIN 匹配成功 (QQID: {configured_qqid}, "
                    f"detected_uin={detected_uin}); Bot 启动完成"
                ),
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            return

        # 不匹配 — 热启动场景常见 (用户附加到了错的 QQ.exe).
        msg = (
            f"SnowLuma UIN 不匹配! Bot 配置 QQID={configured_qqid}, 但注入的 QQ.exe 实际登录账号是 "
            f"{detected_uin}. 即将停止 Bot. 请确认热启动时选中了正确的 QQ.exe 进程 "
            "(其登录账号必须与 Bot 配置 QQID 一致), 或改用冷启动让 Desktop 自动启动新 QQ."
        )
        logger.warning(
            f"SnowLuma UIN 不匹配触发自动停止: configured={configured_qqid}, detected={detected_uin}",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        # Q2 (shutdown race): poller 信号 queued 到主线程时 manager 可能已销毁.
        self._safe_emit_notification("error", msg)
        # 异步 stop (driver.stop 本身已经是 fire-and-forget HTTP + 非阻塞 kill)
        self.stop_bot(qq_id)

    def _stop_snowluma_status_poller(self, qq_id: str) -> None:
        """停止并释放 SnowLuma 轮询器; 不存在时静默跳过 (NapCat 路径上是 no-op).

        W5: SnowLumaDriver.stop 内部已经 detach + stop + deleteLater poller, 这里仅供
        异常路径 (manager 主动清理时) 调用.
        """
        poller = self._snowluma_driver.detach_poller(qq_id)
        if poller is not None:
            try:
                poller.stop()
                poller.deleteLater()
            except Exception:  # noqa: BLE001
                pass

    # ==================== 私有: shutdown-race 友好 emit 辅助 ====================
    def _safe_emit_process_changed(self, qq_id: str, state: QProcess.ProcessState) -> None:
        """非阻塞 emit ``process_changed_signal``, 容忍 Qt 侧已销毁.

        **背景**: 用户在 stop bot 后立即关窗时, ``QApplication`` 自下而上销毁 ``QObject``
        树, ``QProcess`` C++ 端在被销毁时仍会 emit ``stateChanged(NotRunning)`` /
        ``finished(...)`` 作为收尾. 这些信号经 lambda 路由到 ``BotProcessManager`` 的槽,
        但 manager 的 Qt 侧此时可能已经被销毁 (Python 侧因 lambda 闭包持 ``self`` 引用而
        仍活着) — 访问 ``self.process_changed_signal.emit`` 会抛
        ``RuntimeError: Signal source has been deleted``, 进而触发未捕获异常
        → 崩溃诊断包.

        本方法把所有 emit 用 ``try/except RuntimeError`` 包起来, 凡是包含 "deleted" 的
        异常都静默忽略 (shutdown race 唯一可能原因, 不会掩盖真 bug — 其他 emit 失败仍 raise).
        """
        try:
            self.process_changed_signal.emit(qq_id, state)
        except RuntimeError as exc:
            if "deleted" in str(exc):
                return
            raise

    def _safe_emit_notification(self, level: str, message: str) -> None:
        """非阻塞 emit ``notification_signal``, 容忍 Qt 侧已销毁. 见 :meth:`_safe_emit_process_changed`."""
        try:
            self.notification_signal.emit(level, message)
        except RuntimeError as exc:
            if "deleted" in str(exc):
                return
            raise

    # ==================== 私有: QProcess 信号处理 ====================
    def _handle_process_state_changed(self, qq_id: str, state: QProcess.ProcessState) -> None:
        """同步底层 QProcess 状态，避免 UI 卡在旧状态。

        Q2 (shutdown race 修复): 两层防御:

        1. **逻辑层**: 两个 model 都找不到 → 说明 stop 已经走过, UI 已经知道 NotRunning,
           没必要再 emit. 直接 return.
        2. **Qt 层兜底**: emit 走 :meth:`_safe_emit_process_changed`, 自动吞 "deleted" 错误.
        """
        process_model = self.napcat_process_dict.get(qq_id)
        if process_model is not None:
            process_model.state = state
        # SnowLuma 模型也同步状态
        snow_model = self._snowluma_driver.get_process_model(qq_id)
        if snow_model is not None:
            snow_model.state = state

        # 防御层 1: model 全没了 → stop 已走完, 不再 emit (避免 shutdown race 时空 emit)
        if process_model is None and snow_model is None:
            return

        # 防御层 2: Qt 侧可能已销毁, 兜底
        self._safe_emit_process_changed(qq_id, state)

    def _handle_process_finished(
        self,
        qq_id: str,
        process: QProcess,
        exit_code: int,
        exit_status: QProcess.ExitStatus,
    ) -> None:
        """处理本地 Bot 进程异常或自然退出后的清理 (NapCat / SnowLuma 共用).

        通过比对 process 是否对应 NapCat 模型 / SnowLuma 模型, 自动路由清理路径.
        """
        # 1. NapCat 路径
        napcat_model = self.napcat_process_dict.get(qq_id)
        if napcat_model is not None and napcat_model.process is process:
            logger.warning(
                (
                    "NapCatQQ 进程已退出: "
                    f"QQID={qq_id}, exit_code={exit_code}, "
                    f"exit_status={getattr(exit_status, 'name', exit_status)}"
                ),
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            process.deleteLater()
            self.napcat_process_dict.pop(qq_id, None)
            it(ManagerNapCatQQLoginState).remove_login_state(qq_id)
            # Q2 (shutdown race): 用 safe emit, 容忍关窗时 QObject 已销毁.
            self._safe_emit_process_changed(qq_id, QProcess.ProcessState.NotRunning)
            return

        # 2. SnowLuma 路径 (W2 后: 监听的 QProcess 是 QQ.exe COLD 模式, 不再有 node.exe).
        #    daemon 的 node.exe finished 由 daemon 自己的 _on_node_finished 处理, emit
        #    ``crashed`` 信号 → W7 在 manager 接成全员清理. 这里只处理本 Bot 自己的 QQ.exe.
        snow_model = self._snowluma_driver.get_process_model(qq_id)
        if snow_model is not None and snow_model.qq_process is process:
            logger.warning(
                (
                    "SnowLuma QQ.exe 已退出 (意外终止): "
                    f"QQID={qq_id}, exit_code={exit_code}, "
                    f"exit_status={getattr(exit_status, 'name', exit_status)}"
                ),
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            # set dead_event 让仍在跑的 Phase C worker 快速失败 (协议沿用).
            snow_model.dead_event.set()
            # 停 poller, 释放 daemon ref (本 Bot 占的引用), 清 driver 字典.
            self._stop_snowluma_status_poller(qq_id)
            self._snowluma_driver.remove_process_model(qq_id)
            try:
                self._snowluma_driver._get_daemon().release()
            except Exception as exc:  # noqa: BLE001 - 释放路径吞所有异常
                logger.warning(
                    f"SnowLumaDaemon release (QQ.exe finished) 静默忽略: "
                    f"{type(exc).__name__}: {exc}",
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )
            process.deleteLater()
            # Q2 (shutdown race): 用 safe emit, 容忍关窗时 QObject 已销毁.
            self._safe_emit_process_changed(qq_id, QProcess.ProcessState.NotRunning)
            return

    def _handle_local_start_error(
        self,
        qq_id: str,
        process: QProcess,
        error: QProcess.ProcessError,
    ) -> None:
        """本地 Bot QProcess 启动 / 运行期错误回调.

        关键场景:
        - ``QProcess.ProcessError.FailedToStart``: 启动器找不到 / 权限不足 / 子进程立即崩溃,
          ``finished`` 信号不会被 emit, 必须由本回调自己负责清理 process_dict 并 emit
          ``NotRunning`` 让 UI 退出 ``Starting`` 态.
        - 其他错误 (Crashed / Timedout / WriteError / ReadError / UnknownError):
          进程已经成功启动过, ``finished`` 会负责清理, 这里仅记录 trace.
        """
        # 找出 process 对应的 model (NapCat 或 SnowLuma)
        napcat_model = self.napcat_process_dict.get(qq_id)
        snow_model = self._snowluma_driver.get_process_model(qq_id)

        is_napcat_process = napcat_model is not None and napcat_model.process is process
        # W2: SnowLuma per-Bot 只剩 qq_process; node.exe 已交给 daemon 全局管理.
        is_snow_process = snow_model is not None and snow_model.qq_process is process

        if not (is_napcat_process or is_snow_process):
            return

        if error == QProcess.ProcessError.FailedToStart:
            backend_label = "NapCatQQ" if is_napcat_process else "SnowLuma"
            logger.error(
                (
                    f"{backend_label} 进程启动失败(QQID: "
                    f"{qq_id}, error={getattr(error, 'name', error)}): "
                    f"{process.errorString()}"
                ),
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            if is_napcat_process:
                self.napcat_process_dict.pop(qq_id, None)
            else:
                # W2: SnowLuma QQ.exe FailedToStart → 释放 daemon ref + 清 driver state.
                # (旧版还会 kill node.exe; W2 后 daemon 管 node, 本路径只清自己的 QQ.exe.)
                if snow_model is not None:
                    snow_model.dead_event.set()
                self._stop_snowluma_status_poller(qq_id)
                self._snowluma_driver.remove_process_model(qq_id)
                try:
                    self._snowluma_driver._get_daemon().release()
                except Exception as exc:  # noqa: BLE001
                    logger.warning(
                        f"SnowLumaDaemon release (FailedToStart) 静默忽略: "
                        f"{type(exc).__name__}: {exc}",
                        LogType.FILE_FUNC,
                        LogSource.CORE,
                    )
            process.deleteLater()
            it(ManagerNapCatQQLoginState).remove_login_state(qq_id)
            it(ManagerAutoRestartProcess).remove_auto_restart_timer(qq_id)
            # Q2 (shutdown race): 用 safe emit, 容忍关窗时 QObject 已销毁.
            self._safe_emit_notification(
                "error", f"{backend_label} 进程启动失败: {process.errorString()}"
            )
            self._safe_emit_process_changed(qq_id, QProcess.ProcessState.NotRunning)
            return

        # 非 FailedToStart 的运行期错误: finished 会接管清理, 这里仅 trace
        logger.trace(
            (
                "本地 Bot 进程运行期错误(QQID: "
                f"{qq_id}, error={getattr(error, 'name', error)}): "
                f"{process.errorString()}"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )

    # ==================== 公共函数: 启动 / 停止 / 重启 ====================
    def start_bot(
        self,
        config: Config,
        *,
        snowluma_start_mode: SnowLumaStartMode = SnowLumaStartMode.COLD_START,
        snowluma_attach_pid: int = 0,
    ) -> None:
        """启动 Bot 进程, 按 backend_type / runtime_target dispatch.

        Args:
            config: 配置对象.
            snowluma_start_mode: Q2 — SnowLuma 启动模式 (冷启动 / 热启动). 仅 SnowLuma
                后端读取; NapCat / 远端忽略. 默认 COLD_START (历史行为).
            snowluma_attach_pid: Q2 — 热启动模式下用户选择的 QQ.exe PID. 仅 SnowLuma
                + HOT_START 时读取.
        """
        logger.trace(
            (
                "收到 Bot 启动请求("
                f"QQID: {config.bot.QQID}, "
                f"runtime_target={config.bot.runtime_target}, "
                f"backend_type={config.bot.backend_type.value}, "
                f"snowluma_start_mode={snowluma_start_mode.value}, "
                f"snowluma_attach_pid={snowluma_attach_pid}, "
                f"local_napcat_existing={len(self.napcat_process_dict)}, "
                f"local_snowluma_existing={len(self._snowluma_driver.list_processes())}, "
                f"remote_existing={len(self.remote_process_dict)})"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )

        # P2.6: 远端 Bot 走完全独立的启停路径 (SnowLuma 热启动不适用远端, 因为远端
        # QQ.exe 不在我们这台 Desktop 上, 无法 attach. 远端始终冷启动 — 由 SSH backend 拉起.)
        if config.bot.is_remote:
            self._create_remote_process(config)
            return

        # 本地路径: 按 backend_type 分流到对应 driver
        if config.bot.backend_type == BackendType.SNOWLUMA:
            self._start_local_snowluma(
                config,
                start_mode=snowluma_start_mode,
                attach_pid=snowluma_attach_pid,
            )
        else:
            self._start_local_napcat(config)

    # ==================== 本地 Bot 上限检查 ====================
    LOCAL_BOT_LIMIT: int = 4
    """本地 Bot (NapCat + SnowLuma) 合计上限.

    2026-05-11 (问题 3 修复): 上限来自 **NTQQ 多开真实限制** (整个系统最多 4 个 QQ.exe),
    与后端类型无关. P1 历史实现只在 NapCat 路径检查 ``len(napcat_process_dict) >= 4``,
    SnowLuma 路径完全无检查 — 用户能 4 NapCat + N SnowLuma 启起来, 第 5 个 QQ.exe
    被 NTQQ 拒绝, Desktop 这边却以为成功了, 后续 Phase C 注入失败时只看到模糊错误.

    Note:
        远端 Bot (``is_remote=True``) 的 QQ.exe 在远端机器上, 不占本地 4 个名额.
    """

    def _count_running_local_bots(self) -> int:
        """统计本地 NapCat + SnowLuma 在跑 / 启动中的 Bot 总数.

        包含 ``napcat_process_dict`` 与 ``SnowLumaDriver._processes`` 的 ``len`` 之和.
        Starting / Running 状态都算 (上限按"占用 QQ.exe 进程槽"语义, 不按运行就绪).
        """
        return (
            len(self.napcat_process_dict)
            + len(self._snowluma_driver.list_processes())
        )

    def _start_local_napcat(self, config: Config) -> None:
        """启动本地 NapCat Bot."""
        # 问题 3 修复 (合计上限): NapCat + SnowLuma 总数与 LOCAL_BOT_LIMIT 比较,
        # 拒掉超出上限的启动 (含 SnowLuma 已占用 4 个槽的场景).
        if self._count_running_local_bots() >= self.LOCAL_BOT_LIMIT:
            logger.warning(
                (
                    f"本地 Bot 数量已达上限 {self.LOCAL_BOT_LIMIT} (NapCat="
                    f"{len(self.napcat_process_dict)}, SnowLuma="
                    f"{len(self._snowluma_driver.list_processes())}), "
                    f"拒绝启动 NapCat Bot (QQID: {config.bot.QQID})"
                ),
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.notification_signal.emit(
                "error",
                (
                    f"本地 Bot 数量已达上限 ({self.LOCAL_BOT_LIMIT} 个, NTQQ 多开限制), "
                    "无法创建新进程!"
                ),
            )
            return

        try:
            handle = self._napcat_driver.start(config)
        except FileNotFoundError as exc:
            logger.error(
                f"未检测到 QQ 安装路径，无法启动 NapCatQQ 进程(QQID: {config.bot.QQID}): {exc}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.notification_signal.emit("error", str(exc))
            return

        process = handle.primary_process
        qq_id = handle.qq_id

        # 接 QProcess 信号 (用 lambda 闭包同时捕获 qq_id 与 process)
        process.stateChanged.connect(
            lambda state, emitted_qq_id=qq_id: self._handle_process_state_changed(emitted_qq_id, state)
        )
        process.errorOccurred.connect(
            lambda error, emitted_qq_id=qq_id, emitted_process=process: self._handle_local_start_error(
                emitted_qq_id, emitted_process, error
            )
        )
        process.finished.connect(
            lambda exit_code, exit_status, emitted_qq_id=qq_id, emitted_process=process: self._handle_process_finished(
                emitted_qq_id, emitted_process, exit_code, exit_status
            )
        )

        # 创建日志缓冲
        it(ManagerNapCatQQLog).create_log(config, process)

        # 在 process.start() 之前先把 model 注册到字典 (Starting 态)
        self.napcat_process_dict[qq_id] = NapCatProcessModel(
            qq_id=qq_id,
            process=process,
            state=QProcess.ProcessState.Starting,
            started_at=monotonic(),
        )
        self.process_changed_signal.emit(qq_id, QProcess.ProcessState.Starting)

        # 异步启动
        process.start()
        logger.info(f"NapCatQQ 进程已创建并发起启动(QQID: {config.bot.QQID})")

        # 自动重启 timer
        it(ManagerAutoRestartProcess).create_auto_restart_timer(config)

    def _start_local_snowluma(
        self,
        config: Config,
        *,
        start_mode: SnowLumaStartMode = SnowLumaStartMode.COLD_START,
        attach_pid: int = 0,
    ) -> None:
        """启动本地 SnowLuma Bot (W2 daemon 解耦重构 + Q2 冷热启动).

        异步序列 (W2):
        - Phase A 同步主线程: ``driver.start_async()`` 只构造 model + (COLD) 构造 QQ.exe QProcess + 渲染 onebot.json.
          **不再 spawn node** (daemon 接管).
        - manager 接完 QQ.exe (COLD) signal 后调 ``driver._start_phase_a_processes_async`` 启 QQ.exe
          (signal-driven, 不阻塞主线程; 2026-05-11 修复用户实测启动按钮点击瞬间 UI 卡顿).
          HOT 模式下整个 Phase A 阶段不动任何 QProcess.
        - Phase C 后台 worker: ``daemon.ensure_running()`` (首启 daemon 含 wait_ready+login, 后续 Bot 直接复用)
          → ``client.load_process(qq_pid)`` 注入. ≤55s 但不卡 UI.
        - Phase D 主线程回调: ``_on_snowluma_phase_c_succeeded`` 启动 Poller / emit Running.

        manager 在 Phase A 后立即:
        1. **COLD**: 接 QQ.exe 的 stateChanged/finished/errorOccurred 信号; **HOT**: 跳过 (Desktop 不拥有 QQ.exe)
        2. emit ``Starting`` 让 UI 立即进入启动期状态
        3. 提交 worker 到 :class:`QThreadPool`, 等 Phase C 完成

        - 异常分类:
          - Phase A ``FileNotFoundError``: QQ.exe / node.exe 缺失
          - Phase A ``RuntimeError``: HOT_START attach_pid 无效 / 同 QQID 重复启动
          - Phase C 失败: 走 :meth:`_on_snowluma_phase_c_failed` 回调清理

        Args:
            config: Bot 配置.
            start_mode: 冷启动 (spawn 新 QQ) / 热启动 (附加现有 QQ). 默认冷启动.
            attach_pid: 热启动 PID. 冷启动忽略.
        """
        # 问题 3 修复 (合计上限): NapCat + SnowLuma 总数与 LOCAL_BOT_LIMIT 比较,
        # 拒掉超出上限的启动. SnowLuma 路径在 P1/P2 时期完全没检查, 现在与 NapCat 对齐.
        # 注: 热启动模式也算占用名额 — attach_pid 指向的 QQ.exe 已经在 NTQQ 4 个上限内,
        # 我们这里"占名额"的语义是"Desktop 同时管理的 Bot 数量上限", 阻止用户起第 5 个 Bot model.
        if self._count_running_local_bots() >= self.LOCAL_BOT_LIMIT:
            logger.warning(
                (
                    f"本地 Bot 数量已达上限 {self.LOCAL_BOT_LIMIT} (NapCat="
                    f"{len(self.napcat_process_dict)}, SnowLuma="
                    f"{len(self._snowluma_driver.list_processes())}), "
                    f"拒绝启动 SnowLuma Bot (QQID: {config.bot.QQID}, "
                    f"start_mode={start_mode.value})"
                ),
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.notification_signal.emit(
                "error",
                (
                    f"本地 Bot 数量已达上限 ({self.LOCAL_BOT_LIMIT} 个, NTQQ 多开限制), "
                    "无法启动 SnowLuma Bot!"
                ),
            )
            return

        # W7 (2026-05-11): 第一次启 SnowLuma Bot 之前接 daemon.crashed (幂等, 后续启
        # 第二个 Bot 时直接 short-circuit). 放在 ``start_async`` 之前: ``start_async``
        # 会触发 daemon 单例的 ``_get_daemon()``, 此时 creart 已注册, 接信号最安全.
        self._subscribe_daemon_crashed()

        # W2: _do_phase_a 现在只**构造** model + 渲染 onebot.json + (COLD) 构造 QQ.exe QProcess (不 start).
        # node.exe 完全由 daemon 接管, manager 不再 spawn/wire node.
        try:
            handle, worker, _session = self._snowluma_driver.start_async(
                config, start_mode=start_mode, attach_pid=attach_pid
            )
        except FileNotFoundError as exc:
            logger.error(
                f"未检测到 SnowLuma 运行时，无法启动 SnowLuma 进程(QQID: {config.bot.QQID}): {exc}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.notification_signal.emit("error", str(exc))
            return
        except RuntimeError as exc:
            # 同 QQID 重复启动 / HOT_START attach_pid 无效
            logger.warning(
                f"SnowLuma 启动失败(QQID: {config.bot.QQID}): {exc}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.notification_signal.emit("error", str(exc))
            return

        # W2: primary_process = QQ.exe QProcess (COLD) 或 None (HOT).
        #     secondary_process 始终为 None (W2 后 daemon 持 node).
        primary_process = handle.primary_process
        qq_id = handle.qq_id

        # COLD 模式: 接 QQ.exe 的状态信号 (W2 后只有 QQ.exe 一个 per-Bot QProcess).
        # HOT 模式 (primary_process is None): manager 不拥有 QQ.exe, 跳过信号连线;
        # QQ 异常退出由 :class:`SnowLumaStatusPoller` 的 ``state_changed("disconnected")``
        # + W7 daemon.crashed 全员清理覆盖.
        if primary_process is not None:
            primary_process.stateChanged.connect(
                lambda state, emitted_qq_id=qq_id: self._handle_process_state_changed(
                    emitted_qq_id, state
                )
            )
            primary_process.errorOccurred.connect(
                lambda error, emitted_qq_id=qq_id, emitted_process=primary_process: (
                    self._handle_local_start_error(emitted_qq_id, emitted_process, error)
                )
            )
            primary_process.finished.connect(
                lambda exit_code, exit_status, emitted_qq_id=qq_id, emitted_process=primary_process: (
                    self._handle_process_finished(
                        emitted_qq_id, emitted_process, exit_code, exit_status
                    )
                )
            )

        # 2026-05-11 日志按钮修复 (用户反馈点【日志】没有任何输出):
        # SnowLuma Bot 的 ``QQ.exe`` 用 ``ForwardedChannels``, stdout 不进 pipe (转发到
        # Desktop 自身 stdout), 旧版本 ``create_log(config, primary_process)`` 监听 QQ
        # 进程的 ``readyReadStandardOutput`` 永远拿不到数据; HOT 模式 primary 是 None
        # 干脆不挂日志, 用户点【日志】按钮看到 "未找到对应的日志信息".
        # 现在统一用 :class:`SnowLumaDaemonProcessLog` 桥接 daemon node.exe stdout
        # (业务日志真正源头, 多 Bot 共享同一份). HOT/COLD 都走这条路径, BotLogPage
        # 完全复用 NapCat 日志页, 用户体验一致.
        it(ManagerNapCatQQLog).create_snowluma_log(
            config, self._snowluma_driver._get_daemon()
        )

        # ⬇ 信号 + log 缓冲全部就绪后, **现在**才 start QQ.exe (HOT 模式下 driver 内部直接走 on_started).
        # 2026-05-11 主线程卡顿修复: 旧版 ``_start_phase_a_processes`` 用
        # ``qq_process.waitForStarted(5000)`` 阻塞主线程, 用户实测【启动 Bot】点击瞬间 UI
        # 卡顿明显. 改成 signal-driven 异步: driver 内连 ``started`` 信号, 信号到时填
        # ``model.qq_pid`` 并调 ``_on_phase_a_started`` 推进 Phase C; 启动失败由
        # ``errorOccurred`` → :meth:`_handle_local_start_error` 接管, 本函数不再 raise.
        model = self._snowluma_driver.get_process_model(qq_id)
        assert model is not None  # _do_phase_a 刚注册过, 不可能 None

        def _on_phase_a_started(_model, emitted_qq_id=qq_id, w=worker, c=config) -> None:
            """driver ``started`` 信号回调: QQ.exe (或 HOT 模式直接) 就绪后推进 Phase C.

            主线程 slot, ``_start_phase_a_processes_async`` 在主线程同步调
            (COLD 模式经 ``QProcess.started`` 信号, HOT 模式直接同步调).
            """
            # 立即 emit Starting 让 UI 进入启动期状态 (Phase A 完成, Phase C 还在后台跑).
            # stateChanged(Starting) 信号链应也已自然 emit 过, 这里幂等再 emit 一次保兜底.
            self._safe_emit_process_changed(emitted_qq_id, QProcess.ProcessState.Starting)

            # 持有 worker 引用防止 GC; 后续 succeeded / failed 回调里 pop + deleteLater
            self._snowluma_pending_workers[emitted_qq_id] = w

            w.succeeded.connect(
                lambda client, eq=emitted_qq_id, conf=c: (
                    self._on_snowluma_phase_c_succeeded(eq, conf, client)
                )
            )
            w.failed.connect(
                lambda msg, eq=emitted_qq_id: (
                    self._on_snowluma_phase_c_failed(eq, msg)
                )
            )

            # 提交 worker 到 QThreadPool 跑 Phase C (daemon.ensure_running + load_process)
            QThreadPool.globalInstance().start(w)

            logger.info(
                f"SnowLuma Bot Phase A 已起 (QQID: {emitted_qq_id}); Phase C 后台执行中"
            )

        # 异步启动 QQ.exe: 不阻塞主线程; COLD 失败走 errorOccurred → _handle_local_start_error.
        self._snowluma_driver._start_phase_a_processes_async(model, _on_phase_a_started)

    def _on_snowluma_phase_c_succeeded(
        self, qq_id: str, config: Config, client
    ) -> None:
        """Phase C worker 在主线程成功回调.

        在主线程触发 Phase D (启动 Poller) + emit ``Running`` + 创建 auto-restart timer.
        """
        # Worker 已成功; 取消引用以便 Qt GC
        worker = self._snowluma_pending_workers.pop(qq_id, None)
        if worker is not None:
            worker.deleteLater()

        # 验证 model 仍存在 (用户可能在 Phase C 中点了 stop)
        model = self._snowluma_driver.get_process_model(qq_id)
        if model is None:
            logger.warning(
                f"SnowLuma Phase C 成功后发现 model 已消失 (用户停止了 Bot?), 忽略 Phase D",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            return

        # Phase D: 启动 Poller (主线程, QTimer 必须在 event loop 线程)
        try:
            self._snowluma_driver._do_phase_d_poller(model, client)
        except Exception as exc:  # noqa: BLE001
            logger.error(
                f"SnowLuma Phase D 失败 (QQID: {qq_id}): {type(exc).__name__}: {exc}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.notification_signal.emit("error", f"SnowLuma 状态轮询启动失败: {exc}")
            return

        # 转发 poller 信号到 manager 对外信号
        self._connect_snowluma_poller_signal(qq_id)

        # emit Running 让 UI 切到启动完成
        self.process_changed_signal.emit(qq_id, QProcess.ProcessState.Running)

        # 自动重启 timer
        it(ManagerAutoRestartProcess).create_auto_restart_timer(config)

        logger.info(
            f"SnowLuma Bot 启动完成 (QQID: {qq_id}, Phase A→D 全部就绪)"
        )

    def _on_snowluma_phase_c_failed(self, qq_id: str, message: str) -> None:
        """Phase C worker 在主线程失败回调.

        W2: worker 失败语义改为:

        - ``daemon.ensure_running`` 失败: daemon 内部已回滚 ref_count, daemon 自己处于 STOPPED.
          manager 这里只需 kill 本 Bot 的 QQ.exe (COLD) + 移除 model.
        - ``load_process`` 失败 (注入返回 error): daemon 仍 READY, 但本 Bot 没注入成功.
          manager kill QQ.exe (COLD) + daemon.release() + 移除 model.

        统一行为: 调 :meth:`SnowLumaDriver._abort_start` 等价路径 (kill QQ.exe COLD
        + release daemon + pop dict), 即 ``cleanup_failed_start``. 即使 _handle_process_finished
        已经先一步跑过 (QQ.exe 同时崩了), 这里调 release 也只是 ref_count 多扣一次
        (release 在 ref_count=0 时 no-op), 不会出问题.
        """
        worker = self._snowluma_pending_workers.pop(qq_id, None)
        if worker is not None:
            worker.deleteLater()

        logger.warning(
            f"SnowLuma Phase C 失败(QQID: {qq_id}): {message}",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        # Q2 (shutdown race): 用 safe emit, worker 信号 queued 到主线程时 manager 可能已销毁.
        self._safe_emit_notification("error", message)

        # 清理: 如果 model 仍在, 在主线程 kill QQ.exe (COLD) + release daemon ref.
        model = self._snowluma_driver.get_process_model(qq_id)
        if model is not None:
            if model.qq_process is not None:
                terminate_async(model.qq_process)
            try:
                self._snowluma_driver._get_daemon().release()
            except Exception as exc:  # noqa: BLE001
                logger.warning(
                    f"SnowLumaDaemon release (Phase C 失败) 静默忽略: "
                    f"{type(exc).__name__}: {exc}",
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )
            self._snowluma_driver.remove_process_model(qq_id)

        self._safe_emit_process_changed(qq_id, QProcess.ProcessState.NotRunning)

    def _forward_snowluma_stdout(self, qq_id: str, data: str) -> None:
        """**W2 后未连接 (历史遗留)**: SnowLuma node.exe 已由 daemon 全局管理, stdout 不再走
        per-Bot log 缓冲. 保留方法签名仅供回归兼容; 若未来需要 daemon 级日志转发, 可独立
        在 daemon 内拉通. 本函数被调时静默 no-op.
        """
        if not data:
            return
        for line in data.splitlines():
            stripped = line.strip()
            if not stripped:
                continue
            logger.info(
                f"[SnowLuma stdout QQID={qq_id}] {stripped}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )

    def _handle_snowluma_qq_finished(self, qq_id: str) -> None:
        """**W2 后未连接 (历史遗留)**: SnowLuma 模式下 QQ.exe 异常退出回调.

        W2 之前: 由 secondary_process (QQ.exe) 的 ``finished`` 信号触发, 与 primary
        (node.exe) finished 互为兜底.

        W2 之后: SnowLuma 路径下 QQ.exe 是 primary_process, 直接走
        :meth:`_handle_process_finished` 的 SnowLuma 分支统一处理; 不再需要专门兜底.
        本方法保留签名 (避免外部 import 断裂), 调用时静默走 stop_bot 兜底.
        """
        snow_model = self._snowluma_driver.get_process_model(qq_id)
        if snow_model is None:
            return
        logger.warning(
            f"SnowLuma QQ.exe 已退出 (legacy handler), 触发整个 Bot 停止 (QQID: {qq_id})",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        self.stop_bot(qq_id)

    # P2 Tier I: 旧名兼容 — 22 文件 rename 期间允许 create_napcat_process 调用,
    # rename 完后该 alias 一并清理. 实际计划是机械全量 rename, 不留 alias;
    # 但保留 alias 作为 W2 期间的临时安全网防止漏网调用点.
    # NOTE: 计划 §2.1 决议是不加 alias, 这里也不加, 留待 22 文件全量 rename.

    def get_process(self, qq_id: str) -> NapCatProcessModel | RemoteProcessRecord | SnowLumaProcessModel | None:
        """获取指定 QQ 号的进程记录.

        优先级: 远端记录 > 本地 SnowLuma 模型 > 本地 NapCat 模型.
        UI 层只读 ``qq_id`` / ``state`` / ``started_at`` 字段, 三类记录形状一致.
        """
        if (record := self.remote_process_dict.get(qq_id)) is not None:
            return record
        if (snow := self._snowluma_driver.get_process_model(qq_id)) is not None:
            return snow
        return self.napcat_process_dict.get(qq_id, None)

    def has_running_bot(self) -> bool:
        """检查是否有正在运行的 Bot (本地 NapCat / 本地 SnowLuma / 远端).

        Returns:
            bool: 任意一类有 Running 实例则返回 True.
        """
        if any(
            process_model.state == QProcess.ProcessState.Running
            for process_model in self.napcat_process_dict.values()
        ):
            return True
        if any(
            snow.state == QProcess.ProcessState.Running
            for snow in self._snowluma_driver.list_processes()
        ):
            return True
        return any(
            record.state == QProcess.ProcessState.Running
            for record in self.remote_process_dict.values()
        )

    def stop_bot(self, qq_id: str) -> None:
        """停止指定 QQ 号的进程 (本地 NapCat / 本地 SnowLuma / 远端, 自动路由).

        Args:
            qq_id (str): QQ 号
        """
        # P2.6: 远端 Bot 优先走异步停止路径
        if qq_id in self.remote_process_dict:
            self._stop_remote_process(qq_id)
            return

        # 本地 SnowLuma 路径
        snow_model = self._snowluma_driver.get_process_model(qq_id)
        if snow_model is not None:
            self._stop_snowluma_status_poller(qq_id)
            self._snowluma_driver.stop(qq_id)
            it(ManagerNapCatQQLoginState).remove_login_state(qq_id)
            logger.info(f"SnowLuma 进程已停止(QQID: {qq_id})")
            self.process_changed_signal.emit(qq_id, QProcess.ProcessState.NotRunning)
            return

        # 本地 NapCat 路径
        if (process_model := self.napcat_process_dict.get(qq_id)) is None:
            logger.warning(f"尝试停止不存在的 NapCatQQ 进程(QQID: {qq_id})", LogType.FILE_FUNC, LogSource.CORE)
            return

        process = process_model.process
        self._napcat_driver.stop(qq_id, process=process)

        process.deleteLater()
        self.napcat_process_dict.pop(qq_id, None)
        it(ManagerNapCatQQLoginState).remove_login_state(qq_id)

        logger.info(f"NapCatQQ 进程已停止(QQID: {qq_id})")
        self.process_changed_signal.emit(qq_id, QProcess.ProcessState.NotRunning)

    def stop_all_bots(self) -> None:
        """停止所有 Bot (本地 NapCat + 本地 SnowLuma + 远端)."""
        for qq_id in list(self.napcat_process_dict.keys()):
            self.stop_bot(qq_id)
        for snow in list(self._snowluma_driver.list_processes()):
            self.stop_bot(snow.qq_id)
        for qq_id in list(self.remote_process_dict.keys()):
            self.stop_bot(qq_id)

    def restart_bot(self, config: Config) -> None:
        """重启指定 QQ 号的进程 (本地或远端).

        Args:
            config (Config): 配置对象
        """
        qq_id = str(config.bot.QQID)

        # P2.6: 远端 Bot 重启路径
        if config.bot.is_remote and qq_id in self.remote_process_dict:
            logger.info(
                f"开始重启远端 NapCatQQ 进程(QQID: {qq_id}, target={config.bot.runtime_target})",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self._stop_remote_process(qq_id)
            self._create_remote_process(config)
            return

        # 本地路径: SnowLuma
        snow_model = self._snowluma_driver.get_process_model(qq_id)
        if snow_model is not None:
            logger.info(f"开始重启 SnowLuma 进程(QQID: {qq_id})", LogType.FILE_FUNC, LogSource.CORE)
            self.stop_bot(qq_id)
            self.start_bot(config)
            return

        # 本地路径: NapCat
        process_model = self.napcat_process_dict.get(qq_id)
        if process_model is None or not process_model.process:
            logger.warning(
                f"尝试重启不存在的 NapCatQQ 进程(QQID: {config.bot.QQID})",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            return

        logger.trace(
            (
                "收到 NapCatQQ 重启请求: "
                f"QQID={config.bot.QQID}, pid={process_model.process.processId()}, "
                f"state={getattr(process_model.process.state(), 'name', process_model.process.state())}"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        logger.info(f"开始重启 NapCatQQ 进程(QQID: {config.bot.QQID})", LogType.FILE_FUNC, LogSource.CORE)
        self.stop_bot(qq_id)
        self.start_bot(config)

    def get_memory_usage(self, qq_id: str) -> int:
        """获取指定 QQ 号的内存占用 (MB).

        - 本地 NapCat: 通过 :class:`NapCatDriver.get_memory_usage_for_pid` 累加进程树 RSS
        - 本地 SnowLuma:
          - 优先用 :attr:`SnowLumaProcessModel.ancillary_pids` (poller 按 UIN 聚合的
            hooked QQ.exe PIDs, 与 SnowLuma WebUI ``/api/processes`` 一致) 累加 RSS;
            **HOT 与 COLD 模式都走这条路径**.
          - 没拿到 ancillary_pids (poller 未启 / 未首次探测): fallback 到 ``qq_pid`` walk
            进程树.
        - 远端: 直接读取 :attr:`RemoteProcessRecord.last_memory_rss_bytes`

        2026-05-11 内存监控修复:
        旧版 SnowLuma 路径只走 ``qq_process.processId()`` walk 树, **HOT 模式 qq_process
        is None 永远返回 0**; COLD 模式累加 launcher 整棵 Electron 子进程树, 与 WebUI
        显示的 hooked 进程内存对不上. 现在优先 ancillary_pids 直接对齐 WebUI 数据.

        未运行 / 未知时返回 0.
        """
        # P2.6: 远端 Bot 走缓存
        if (record := self.remote_process_dict.get(qq_id)) is not None:
            if record.state != QProcess.ProcessState.Running or record.last_memory_rss_bytes is None:
                return 0
            return int(record.last_memory_rss_bytes / (1024 * 1024))

        # 本地 SnowLuma (W2 后: node.exe 由 daemon 全局共享, 不归本 Bot)
        snow_model = self._snowluma_driver.get_process_model(qq_id)
        if snow_model is not None:
            # 优先: poller 写入的 hooked PID 集合 (与 WebUI 显示一致)
            if snow_model.ancillary_pids:
                total_rss = 0
                for pid in snow_model.ancillary_pids:
                    if pid <= 0:
                        continue
                    try:
                        import psutil

                        total_rss += psutil.Process(pid).memory_info().rss
                    except Exception:  # noqa: BLE001 - 进程消失 / 权限 / psutil 任何异常静默
                        continue
                return int(total_rss / (1024 * 1024))

            # Fallback: ancillary_pids 还没写入 (poller 启动期 / UIN 未探到), 走 qq_pid.
            # COLD 模式 qq_pid = launcher PID, walk 整棵子进程树拿大致值;
            # HOT 模式 qq_pid = attach_pid (用户选的 main process), walk 也是树.
            if snow_model.qq_pid > 0:
                return NapCatDriver.get_memory_usage_for_pid(snow_model.qq_pid)
            return 0

        # 本地 NapCat
        if (process_model := self.napcat_process_dict.get(qq_id)) is None:
            return 0
        if not (process := process_model.process) or process.state() != QProcess.ProcessState.Running:
            return 0
        return NapCatDriver.get_memory_usage_for_pid(process.processId())

    def get_total_memory_mb(self, qq_id: str) -> int:
        """获取展示在 ``X MB / Y MB`` 中的 Y 值 (MB).

        - 远端 Bot: 取 ``RemoteProcessRecord.server_total_memory_bytes`` (backend 探到的
          服务器 RAM); 还没探到时 fallback 0, UI 显示 ``X MB / 0 MB`` 直到 backend 首轮
          poll 成功.
        - 本地 Bot (NC / SnowLuma): 走 ``psutil.virtual_memory().total``; UI 层模块级
          缓存了一次, 这里走快路径.

        修复 (2026-05-12): 早期版本远端 Bot 也读 ``psutil.virtual_memory().total``,
        显示的是 Desktop 本机 RAM (16/32 GB), 与服务器实际 RAM (常 1-2 GB) 不一致.
        """
        record = self.remote_process_dict.get(qq_id)
        if record is not None:
            if record.server_total_memory_bytes is None:
                return 0
            return int(record.server_total_memory_bytes / (1024 * 1024))

        import psutil

        try:
            return int(psutil.virtual_memory().total / (1024 * 1024))
        except Exception:  # noqa: BLE001 - psutil 异常 fallback 0
            return 0

    # ==================== P2.6: 远端 Bot 进程管理 ====================
    def _create_remote_process(self, config: Config) -> None:
        """在远端服务器上启动 NapCat Bot (异步).

        启动是异步的: 在 :class:`QThreadPool` 后台线程发起 SSH 调用, 成功后切回主线程
        更新 :class:`RemoteProcessRecord` 并启动状态轮询.
        """
        qq_id = str(config.bot.QQID)

        if qq_id in self.remote_process_dict:
            logger.warning(
                f"远端 NapCat Bot 已经在管理中(QQID: {qq_id}); 重启请使用 restart_bot",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            return

        if len(self.remote_process_dict) >= 4:
            logger.warning(
                f"远端 NapCat Bot 数量已达上限(QQID: {qq_id})",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.notification_signal.emit("error", "远端 NapCat Bot 数量已达上限!")
            return

        record = RemoteProcessRecord(
            qq_id=qq_id,
            config=config,
            state=QProcess.ProcessState.Starting,
            started_at=monotonic(),
        )
        self.remote_process_dict[qq_id] = record
        it(ManagerNapCatQQLog).create_remote_log(config)
        self.process_changed_signal.emit(qq_id, QProcess.ProcessState.Starting)

        runner = RemoteBotOperationRunnable(qq_id, config, "start")
        runner.operation_finished_signal.connect(self._on_remote_op_finished)
        runner.operation_failed_signal.connect(self._on_remote_op_failed)
        remote_ssh_pool().start(cast(QRunnable, runner))

    def _stop_remote_process(self, qq_id: str) -> None:
        """停止远端 Bot 进程; 不存在时静默返回."""
        record = self.remote_process_dict.get(qq_id)
        if record is None:
            logger.warning(
                f"尝试停止不存在的远端 NapCat Bot(QQID: {qq_id})",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            return

        # 立即在主线程把状态切换到 NotRunning, 避免轮询期间继续上报"在线"
        self._teardown_remote_polling(record)
        record.state = QProcess.ProcessState.NotRunning
        # P2.5 修复: 登录状态轮询(3s 间隔的 HTTP -> SSH 隧道) 必须在用户点 "停止"
        # 的瞬间立即停掉, 不能等到 SSH stop 命令返回.
        it(ManagerNapCatQQLoginState).remove_login_state(qq_id)
        record.login_state_published = False
        record.login_state_port = None

        runner = RemoteBotOperationRunnable(qq_id, record.config, "stop")
        runner.operation_finished_signal.connect(self._on_remote_op_finished)
        runner.operation_failed_signal.connect(self._on_remote_op_failed)
        remote_ssh_pool().start(cast(QRunnable, runner))

    def _on_remote_op_finished(self, qq_id: str, action: str, payload: object) -> None:
        """远端 SSH 操作成功的统一回调 (主线程)."""
        if action == "start":
            self._handle_remote_start_succeeded(qq_id, payload)
        elif action == "stop":
            self._handle_remote_stop_succeeded(qq_id)
        elif action == "poll":
            self._handle_remote_poll_result(qq_id, payload)

    def _on_remote_op_failed(self, qq_id: str, action: str, error: str) -> None:
        """远端 SSH 操作失败的统一回调 (主线程)."""
        if action == "start":
            record = self.remote_process_dict.pop(qq_id, None)
            if record is not None:
                self._teardown_remote_polling(record)
            it(ManagerNapCatQQLog).remove_log(qq_id)
            logger.error(
                f"远端 NapCat Bot 启动失败(QQID: {qq_id}): {error}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.process_changed_signal.emit(qq_id, QProcess.ProcessState.NotRunning)
            return

        if action == "stop":
            record = self.remote_process_dict.pop(qq_id, None)
            if record is not None:
                self._teardown_remote_polling(record)
            it(ManagerNapCatQQLog).remove_log(qq_id)
            logger.warning(
                f"远端 NapCat Bot 停止失败(QQID: {qq_id}): {error}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.process_changed_signal.emit(qq_id, QProcess.ProcessState.NotRunning)
            it(ManagerNapCatQQLoginState).remove_login_state(qq_id)
            return

        if action == "poll":
            logger.trace(
                f"远端 Bot 轮询单次失败(QQID: {qq_id}): {error}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            record = self.remote_process_dict.get(qq_id)
            if record is not None:
                record.poll_in_flight = False

    def _handle_remote_start_succeeded(self, qq_id: str, status: object) -> None:
        """远端启动成功后切到 Running + 启动轮询."""
        record = self.remote_process_dict.get(qq_id)
        if record is None:
            return

        record.state = QProcess.ProcessState.Running
        record.started_at = monotonic()
        from src.core.operation.backend import ProcessStatus as _ProcessStatus  # local alias to avoid global import cycle

        if isinstance(status, _ProcessStatus):
            record.last_memory_rss_bytes = status.memory_rss_bytes
            # session 内不变, 但 backend 每次都带回来; 拿到非空就缓存到 record
            if status.server_total_memory_bytes is not None:
                record.server_total_memory_bytes = status.server_total_memory_bytes

        self.process_changed_signal.emit(qq_id, QProcess.ProcessState.Running)
        logger.info(
            f"远端 NapCat Bot 启动成功(QQID: {qq_id}, target={record.config.bot.runtime_target})",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        it(ManagerAutoRestartProcess).create_auto_restart_timer(record.config)
        self._start_remote_polling(record)
        self._enqueue_remote_poll(record)

    def _handle_remote_stop_succeeded(self, qq_id: str) -> None:
        """远端停止确认后清理记录与登录状态."""
        record = self.remote_process_dict.pop(qq_id, None)
        if record is not None:
            self._teardown_remote_polling(record)
        it(ManagerAutoRestartProcess).remove_auto_restart_timer(qq_id)
        it(ManagerNapCatQQLoginState).remove_login_state(qq_id)
        it(ManagerNapCatQQLog).remove_log(qq_id)
        logger.info(
            f"远端 NapCat Bot 已停止(QQID: {qq_id})",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        self.process_changed_signal.emit(qq_id, QProcess.ProcessState.NotRunning)

    def _handle_remote_poll_result(self, qq_id: str, payload: object) -> None:
        """轮询结果到达后的处理: 更新内存 / 检测离线 / 发布 WebUI 登录状态."""
        record = self.remote_process_dict.get(qq_id)
        if record is None:
            return

        record.poll_in_flight = False

        if record.state == QProcess.ProcessState.NotRunning:
            return

        if not isinstance(payload, tuple) or len(payload) != 2:
            return
        status, endpoint = payload

        if not getattr(status, "running", False):
            logger.info(
                f"远端 NapCat Bot 已离线(QQID: {qq_id}); 同步状态到本地",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self._handle_remote_stop_succeeded(qq_id)
            return

        record.last_memory_rss_bytes = getattr(status, "memory_rss_bytes", None)
        # 与 _handle_remote_start_succeeded 同模式: 拿到非空就缓存; session 内不变
        sttm = getattr(status, "server_total_memory_bytes", None)
        if sttm is not None:
            record.server_total_memory_bytes = sttm

        if endpoint is not None:
            self._publish_remote_login_state(record, endpoint)

    def _publish_remote_login_state(self, record: RemoteProcessRecord, endpoint: object) -> None:
        """把远端 WebUI 端点 (经 SSH 隧道) 注册到 :class:`ManagerNapCatQQLoginState`."""
        base_url = getattr(endpoint, "base_url", "") or ""
        token = getattr(endpoint, "token", None)
        if not base_url or not token:
            logger.warning(
                (
                    f"远端 Bot 登录状态未发布: 字段空 "
                    f"(QQID={record.qq_id}, base_url={base_url!r}, has_token={bool(token)})"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            return

        match = re.match(r"http://(?:127\.0\.0\.1|localhost):(\d+)", base_url)
        if match is None:
            logger.warning(
                (
                    f"远端 Bot 登录状态未发布: base_url 格式不匹配 "
                    f"(QQID={record.qq_id}, base_url={base_url!r})"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            return
        local_port = int(match.group(1))

        if record.login_state_published and record.login_state_port == local_port:
            return

        logger.info(
            (
                "为远端 Bot 发布登录状态: "
                f"QQID={record.qq_id}, tunnel_port={local_port}, has_token={bool(token)}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )
        it(ManagerNapCatQQLoginState).create_login_state(
            config=record.config,
            port=local_port,
            token=str(token),
        )
        record.login_state_published = True
        record.login_state_port = local_port

    def _start_remote_polling(self, record: RemoteProcessRecord) -> None:
        """为远端 Bot 启动 :class:`QTimer` 周期性轮询."""
        if record.polling_timer is not None:
            return
        timer = QTimer(self)
        timer.setInterval(self._REMOTE_POLLING_INTERVAL_MS)
        timer.timeout.connect(lambda r=record: self._enqueue_remote_poll(r))
        timer.start()
        record.polling_timer = timer

    def _teardown_remote_polling(self, record: RemoteProcessRecord) -> None:
        """关闭远端 Bot 的轮询计时器."""
        timer = record.polling_timer
        if timer is None:
            return
        try:
            timer.stop()
            timer.deleteLater()
        except Exception:  # noqa: BLE001
            pass
        record.polling_timer = None

    def _enqueue_remote_poll(self, record: RemoteProcessRecord) -> None:
        """提交一次远端 Bot 轮询任务."""
        if record.qq_id not in self.remote_process_dict:
            return
        if record.poll_in_flight:
            logger.trace(
                f"远端 Bot 轮询跳过 (上一轮未返回, QQID: {record.qq_id})",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return
        record.poll_in_flight = True
        runner = RemoteBotOperationRunnable(record.qq_id, record.config, "poll")
        runner.operation_finished_signal.connect(self._on_remote_op_finished)
        runner.operation_failed_signal.connect(self._on_remote_op_failed)
        remote_ssh_pool().start(cast(QRunnable, runner))


# ==================== creart 创建器 ====================
class ManagerNapCatQQLogManagerCreator(AbstractCreator, ABC):
    """NapCatQQ 日志管理器创建器"""

    targets = (CreateTargetInfo("src.core.runtime.bot_process_manager", "ManagerNapCatQQLog"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.core.runtime.bot_process_manager")

    @staticmethod
    def create(create_type):
        """创建 ManagerNapCatQQLog 实例"""
        return create_type()


add_creator(ManagerNapCatQQLogManagerCreator)


class ManagerNapCatQQLoginStateCreator(AbstractCreator, ABC):
    """NapCatQQ 登录状态管理器创建器"""

    targets = (CreateTargetInfo("src.core.runtime.bot_process_manager", "ManagerNapCatQQLoginState"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.core.runtime.bot_process_manager")

    @staticmethod
    def create(create_type):
        """创建 ManagerNapCatQQLoginState 实例"""
        return create_type()


add_creator(ManagerNapCatQQLoginStateCreator)


class ManagerAutoRestartProcessCreator(AbstractCreator, ABC):
    """NapCatQQ 自动重启进程管理器创建器"""

    targets = (CreateTargetInfo("src.core.runtime.bot_process_manager", "ManagerAutoRestartProcess"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.core.runtime.bot_process_manager")

    @staticmethod
    def create(create_type):
        """创建 ManagerAutoRestartProcess 实例"""
        return create_type()


add_creator(ManagerAutoRestartProcessCreator)


class BotProcessManagerCreator(AbstractCreator, ABC):
    """Bot 进程管理器创建器 (Tier I 重命名后)."""

    targets = (CreateTargetInfo("src.core.runtime.bot_process_manager", "BotProcessManager"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.core.runtime.bot_process_manager")

    @staticmethod
    def create(create_type):
        """创建 BotProcessManager 实例"""
        return create_type()


add_creator(BotProcessManagerCreator)

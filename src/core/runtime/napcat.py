# -*- coding: utf-8 -*-
"""
## 运行 NapCat 流程
"""
# 标准库导入
import hashlib
import re
from abc import ABC
from collections import deque
from dataclasses import dataclass
from pathlib import Path
from time import monotonic
from typing import cast

# 第三方库导入
import psutil
from creart import add_creator, exists_module, it
from creart.creator import AbstractCreator, CreateTargetInfo
from httpx import Client, post
from PySide6.QtCore import QObject, QProcess, QRunnable, QThreadPool, QTimer, Signal

# 项目内模块导入
from src.core.config import cfg
from src.core.config.config_enum import TimeUnitEnum
from src.core.config.config_model import Config
from src.core.network.email import Email, create_offline_email_task
from src.core.network.webhook import WebHook, create_offline_webhook_task
from src.core.logging import LogSource, LogType, logger
from src.core.runtime.paths import PathFunc

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

    为远端 Bot 的进程管理提供与 [`NapCatProcessModel`](src/core/runtime/napcat.py)
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
            [`NapCatQQLoginState`](src/core/runtime/napcat.py); 防止重复
        login_state_port: 已发布的本地隧道端口, 用于探测端口变化触发重发
    """

    qq_id: str
    config: Config
    state: QProcess.ProcessState = QProcess.ProcessState.NotRunning
    started_at: float = 0.0
    last_memory_rss_bytes: int | None = None
    polling_timer: QTimer | None = None
    login_state_published: bool = False
    login_state_port: int | None = None


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
        """清洗日志文本; 委托给模块级 [`_sanitize_log_text`](src/core/runtime/napcat.py)."""
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

    在 [`QThreadPool`](https://doc.qt.io/qt-6/qthreadpool.html) 后台线程上调用
    [`RemoteBackend.tail_log`](src/core/operation/remote_backend.py),
    完成后通过 ``tail_signal`` 把整段尾部文本带回主线程,
    交由 [`RemoteNapCatQQLog`](src/core/runtime/napcat.py) 做去重 + 增量发射.

    SSH 异常 / 资源错误一律走 ``error_signal`` 让上层 trace 一行了事,
    单次失败不影响下一次轮询.
    """

    tail_signal = Signal(str, str)  # (qq_id, full_tail_text)
    error_signal = Signal(str, str)  # (qq_id, error_message)

    def __init__(self, qq_id: str, config: Config, *, lines: int) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self._qq_id = qq_id
        self._config = config
        self._lines = lines

    def run(self) -> None:  # noqa: D401 - QRunnable 协议
        try:
            from src.core.operation.resolver import resolve_backend_for_bot

            backend = resolve_backend_for_bot(self._config)
            backend.connect()
            content = backend.tail_log(self._qq_id, lines=self._lines)
            self.tail_signal.emit(self._qq_id, content or "")
        except Exception as exc:  # noqa: BLE001 - 单次拉取失败不应影响后续轮询
            self.error_signal.emit(self._qq_id, f"{type(exc).__name__}: {exc}")


class RemoteNapCatQQLog(QObject):
    """远端 NapCat Bot 日志缓冲 (P3 实现).

    与 [`NapCatQQProcessLog`](src/core/runtime/napcat.py) 暴露完全一致的对外接口
    (``output_log_signal`` / ``get_log_content`` / ``clear``), 让
    [`BotLogPage`](src/ui/page/bot_page/sub_page/bot_log.py) 不需要做任何区分.

    数据来源不再是本地 ``QProcess`` 的 stdout, 而是周期性 SSH ``tail``:
    - 每 ``_POLL_INTERVAL_MS`` 毫秒派发一个 [`_RemoteLogTailRunnable`](src/core/runtime/napcat.py)
      到 [`QThreadPool`](https://doc.qt.io/qt-6/qthreadpool.html)
    - runnable 在后台线程调用
      [`RemoteBackend.tail_log`](src/core/operation/remote_backend.py) 拉取最近 N 行
    - 主线程拿到完整尾部后, 通过最长后缀-前缀重叠算法计算"上次没见过的新增段",
      只把新增段写入 ``_log_storage`` 并 emit ``output_log_signal``,
      这样 UI 不会重复显示已经画过的内容.

    设计要点:
    - 第一次拉取直接全量插入, 让用户开页时立刻有上下文 (而不是等 5 秒).
    - 后续每次拉取都做去重, 避免日志页上出现 N 倍的重复行.
    - SSH 异常只 trace 一行, 不打断轮询; 用户停止 Bot 时调用 ``stop()`` 释放计时器.
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
        runnable = _RemoteLogTailRunnable(self._qq_id, self._config, lines=self._TAIL_LINES)
        runnable.tail_signal.connect(self._on_tail_arrived)
        runnable.error_signal.connect(self._on_tail_error)
        QThreadPool.globalInstance().start(cast(QRunnable, runnable))

    def _on_tail_arrived(self, qq_id: str, full_tail: str) -> None:
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


class ManagerNapCatQQLog(QObject):
    """NapCatQQ 日志管理器"""

    def __init__(self) -> None:
        super().__init__()
        self.napcat_log_dict: dict[str, NapCatQQProcessLog | RemoteNapCatQQLog] = {}

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

    def create_remote_log(self, config: Config) -> None:
        """创建指定 QQ 号的远端日志缓冲区 (P3).

        与 [`create_log`](src/core/runtime/napcat.py) 对称, 但底层使用周期性 SSH
        ``tail`` 拉取远端 ``napcat_<qq_id>.log``, 而非本地 QProcess stdout.

        Args:
            config (Config): 配置对象, 必须 ``runtime_target != 'local'``.
        """
        qq_id = str(config.bot.QQID)
        self.remove_log(qq_id)
        self.napcat_log_dict[qq_id] = RemoteNapCatQQLog(config)

    def get_log(self, qq_id: str) -> NapCatQQProcessLog | RemoteNapCatQQLog | None:
        """获取指定 QQ 号的日志缓冲区

        Args:
            qq_id (str): QQ 号

        Returns:
            NapCatQQProcessLog | RemoteNapCatQQLog | None: 对应的日志缓冲区对象,
            如果不存在则返回 None.
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

    def run(self) -> None:  # noqa: D401 - QRunnable 框架约定
        try:
            # 延迟导入避免循环依赖
            from src.core.operation.resolver import resolve_backend_for_bot

            backend = resolve_backend_for_bot(self._config)
            backend.connect()

            if self._action == "start":
                status = backend.start_napcat(self._qq_id, self._config)
                self.operation_finished_signal.emit(self._qq_id, self._action, status)
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
                return

            self.operation_failed_signal.emit(
                self._qq_id, self._action, f"未知远端操作: {self._action}"
            )
        except Exception as exc:  # noqa: BLE001 - 边界处统一捕获, 把详细错误回到 UI 线程
            logger.warning(
                f"远端 Bot {self._action} 操作失败(QQID: {self._qq_id}): "
                f"{type(exc).__name__}: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            self.operation_failed_signal.emit(
                self._qq_id, self._action, f"{type(exc).__name__}: {exc}"
            )


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

        # 启动定时器以定期获取授权状态
        self._auth_timer = QTimer(self)
        self._auth_timer.timeout.connect(self.slot_get_auth_status)
        self._auth_timer.start(30 * 60 * 1000)  # 30分钟

        # 启动定时器定期获取登录状态
        self._login_state_timer = QTimer(self)
        self._login_state_timer.timeout.connect(self.slot_get_login_state)
        self._login_state_timer.start(3 * 1000)  # 3秒

        # 立即执行一次（在事件循环中）
        QTimer.singleShot(0, self.slot_get_auth_status)
        QTimer.singleShot(3 * 1000, self.slot_get_login_state)

    # ==================== 公共方法 ==================
    def get_login_state(self) -> bool:
        """获取登录状态

        Returns:
            bool: 是否已登录
        """
        return self._is_logged_in

    def get_online_status(self) -> bool:
        """获取在线状态

        Returns:
            bool: 是否在线
        """
        return self._online_status

    def remove(self) -> None:
        """清理 Timer 和释放资源"""
        self._auth_timer.stop()
        self._auth_timer.deleteLater()
        self._login_state_timer.stop()
        self._login_state_timer.deleteLater()
        self.qr_code_removed_signal.emit(str(self.config.bot.QQID))

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
        if not self.auth:
            self.slot_request_auth_refresh()
            return

        runner = GetLoginStatusRunnable(port=self.port, token=self.token, auth=self.auth)
        runner.login_status_signal.connect(self.slot_update_login_state)
        runner.online_status_signal.connect(self.slot_update_online_status)
        runner.login_qrcode_signal.connect(self.slot_update_login_qrcode)
        runner.auth_refresh_requested_signal.connect(self.slot_request_auth_refresh)
        QThreadPool.globalInstance().start(runner)

    def slot_get_auth_status(self) -> None:
        """获取认证状态"""
        self._last_auth_refresh_attempt_at = monotonic()
        runner = GetAuthStatusRunnable(port=self.port, token=self.token)
        runner.login_auth_signal.connect(self.slot_update_auth)
        QThreadPool.globalInstance().start(runner)

    def slot_request_auth_refresh(self) -> None:
        """在登录状态轮询鉴权失效时，立即刷新 auth。"""
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
        """更新认证信息

        Args:
            auth (str): 认证信息
        """
        self.auth = auth
        logger.trace(
            f"NapCat 登录认证信息已更新(QQID: {self.config.bot.QQID}, has_auth={bool(auth)})",
            LogType.NETWORK,
            LogSource.CORE,
        )

    def slot_update_login_state(self, is_login: bool) -> None:
        """更新登录状态

        Args:
            is_login (bool): 是否已登录
        """
        prev_login = self._is_logged_in
        self._is_logged_in = is_login
        logger.trace(
            f"NapCat 登录状态更新(QQID: {self.config.bot.QQID}, is_login={is_login})",
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
        """更新在线状态

        Args:
            online_status (bool): 是否在线
        """
        # 记录之前的在线状态以判断是否发生了 状态从在线->离线 的转变
        prev_online = self._online_status
        login_invalidated_while_online = self._login_invalidated_while_online

        # 更新当前在线状态
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

        # 如果当前是在线，重置通知标志并直接返回
        if online_status:
            # 一旦恢复在线，允许之后再次发送离线通知
            self._offline_notice = False
            self._login_invalidated_while_online = False
            self._suppress_qrcode_until_online = False
            return

        # 只有当之前是在线并且当前已离线时，才触发离线逻辑
        if not prev_online:
            # 如果之前就已经离线，跳过（避免重复或启动时误判）
            return

        if login_invalidated_while_online:
            self._login_invalidated_while_online = False
            self._suppress_qrcode_until_online = True
            self.qr_code_removed_signal.emit(str(self.config.bot.QQID))

        # 如果未登录则不进行离线通知/重启处理
        if not self._is_logged_in and not login_invalidated_while_online:
            return

        # 如果配置为自动重启，优先发送通知（如果开启），然后再重启
        if self.config.bot.offlineAutoRestart:
            # 只有在未曾发送过离线通知且配置允许时才发送
            if not self._offline_notice and self.config.advanced.offlineNotice:
                if cfg.get(cfg.bot_offline_web_hook_notice):
                    self._start_notification_task(
                        create_offline_webhook_task(self.config), self.tr("已发送离线通知到配置的 WebHook 地址")
                    )

                if cfg.get(cfg.bot_offline_email_notice):
                    self._start_notification_task(
                        create_offline_email_task(self.config), self.tr("已发送离线通知到配置的邮箱地址")
                    )

                # 标记已发送，避免重复
                self._offline_notice = True

            # 执行重启（无论是否发送了通知）
            it(ManagerNapCatQQProcess).restart_process(self.config)
            return

        # 非自动重启场景：如果已经发送过通知则直接返回
        if self._offline_notice:
            return

        # 离线通知：由用户配置决定是否发送
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

        # 标记已发送，避免重复通知
        self._offline_notice = True

    def slot_update_login_qrcode(self, qr_code: str) -> None:
        """更新登录二维码

        Args:
            qr_code (str): 登录二维码
        """
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
        """创建并添加登录状态对象

        Args:
            config (Config): 配置对象
            port (int): WebUI 端口
            token (str): WebUI Token
        """
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
        """获取指定 QQ 号的登录状态对象

        Args:
            qq_id (str): QQ 号

        Returns:
            NapCatQQLoginState | None: 对应的登录状态对象, 如果不存在则返回 None
        """
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
        """创建自动重启定时器

        Args:
            config (Config): 配置对象
        """

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
        timer.timeout.connect(lambda: it(ManagerNapCatQQProcess).restart_process(config))
        timer.start()

        # 添加到字典
        self.auto_restart_process_dict[str(config.bot.QQID)] = timer

    def remove_auto_restart_timer(self, qq_id: str) -> None:
        """移除自动重启定时器

        Args:
            qq_id (str): QQ 号
        """
        if qq_id in self.auto_restart_process_dict:
            self.auto_restart_process_dict[qq_id].stop()
            self.auto_restart_process_dict[qq_id].timeout.disconnect()
            self.auto_restart_process_dict[qq_id].deleteLater()
            self.auto_restart_process_dict.pop(qq_id)


class ManagerNapCatQQProcess(QObject):
    """NapCatQQ 进程管理类

    负责创建和管理 NapCatQQ 的 QProcess 实例
    """

    # 进程状态改变信号
    process_changed_signal = Signal(str, QProcess.ProcessState)
    notification_signal = Signal(str, str)

    # 远端 Bot 状态轮询周期 (毫秒). 与 status.json 写入频率/SSH 往返相比,
    # 5s 是经验上的舒适值: 既能让 UI 在 Bot 崩溃 / 启动后较快感知, 又不至于
    # 让远端 sshd 频繁开 channel.
    _REMOTE_POLLING_INTERVAL_MS = 5000

    def __init__(self) -> None:
        """初始化 NapCatQQ 进程管理器

        Args:
            config (Config): 配置对象
        """
        super().__init__()
        self.napcat_process_dict: dict[str, NapCatProcessModel] = {}
        # P2.6: 远端 Bot 由 polling timer + SSH worker 驱动, 与本地 QProcess 完全分离.
        # 公开访问语义: ``get_process(qq_id)`` 同时检查这两个字典.
        self.remote_process_dict: dict[str, RemoteProcessRecord] = {}
        logger.info("NapCatQQ 进程管理器已初始化")

    # ==================== 私有函数===================
    def _get_env_variable(self) -> list[str]:
        """获取环境变量"""
        env = QProcess.systemEnvironment()
        env.append(f"NAPCAT_PATCH_PACKAGE={it(PathFunc).napcat_path / 'qqnt.json'}")
        env.append(f"NAPCAT_LOAD_PATH={it(PathFunc).napcat_path / 'loadNapCat.js'}")
        env.append(f"NAPCAT_INJECT_PATH={it(PathFunc).napcat_path / 'NapCatWinBootHook.dll'}")
        env.append(f"NAPCAT_LAUNCHER_PATH={it(PathFunc).napcat_path / 'NapCatWinBootMain.exe'}")
        env.append(f"NAPCAT_MAIN_PATH={it(PathFunc).napcat_path / 'napcat.mjs'}")

        return env

    def _write_load_script(self) -> None:
        """写入 loadNapCat.js 脚本文件"""
        with open(str(it(PathFunc).napcat_path / "loadNapCat.js"), "w") as file:
            file.write(
                "(async () => {await import(" f"'{ (it(PathFunc).napcat_path / 'napcat.mjs').as_uri() }'" ")})()"
            )
        logger.info("NapCatQQ 进程加载脚本已写入")

    def _create_napcat_process(self, config: Config, qq_path: Path) -> QProcess:
        """创建并配置 QProcess

        Args:
            config (Config): 配置对象
            qq_path (Path): QQ 安装目录

        Returns:
            QProcess: 配置好的 QProcess 对象
        """
        # 写入 loadNapCat.js 文件
        self._write_load_script()

        # 创建 QProcess 并配置
        process = QProcess()
        process.setEnvironment(self._get_env_variable())
        process.setProgram(str(it(PathFunc).napcat_path / "NapCatWinBootMain.exe"))
        process.setArguments(
            [
                str(qq_path / "QQ.exe"),
                str(it(PathFunc).napcat_path / "NapCatWinBootHook.dll"),
                str(config.bot.QQID),
            ]
        )
        process.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)

        return process

    def _handle_process_state_changed(self, qq_id: str, state: QProcess.ProcessState) -> None:
        """同步底层 QProcess 状态，避免 UI 卡在旧状态。"""
        if (process_model := self.napcat_process_dict.get(qq_id)) is not None:
            process_model.state = state

        self.process_changed_signal.emit(qq_id, state)

    def _handle_process_finished(
        self,
        qq_id: str,
        process: QProcess,
        exit_code: int,
        exit_status: QProcess.ExitStatus,
    ) -> None:
        """处理 NapCat 进程异常或自然退出后的清理。"""
        process_model = self.napcat_process_dict.get(qq_id)
        if process_model is None or process_model.process is not process:
            return

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
        self.process_changed_signal.emit(qq_id, QProcess.ProcessState.NotRunning)

    # ==================== 公共函数===================
    def create_napcat_process(self, config: Config) -> None:
        """创建并配置 QProcess

        Args:
            config (Config): 配置对象
            log (NapCatQQProcessLogger): 日志缓冲对象(需要实例)

        Returns:
            QProcess: 配置好的 QProcess 对象
        """
        logger.trace(
            (
                "收到 NapCatQQ 启动请求("
                f"QQID: {config.bot.QQID}, "
                f"runtime_target={config.bot.runtime_target}, "
                f"local_existing={len(self.napcat_process_dict)}, "
                f"remote_existing={len(self.remote_process_dict)})"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )

        # P2.6: 远端 Bot 走完全独立的启停路径
        if config.bot.is_remote:
            self._create_remote_process(config)
            return

        # 如果超过 4 个进程，则取消创建
        if len(self.napcat_process_dict) >= 4:
            logger.warning(
                f"NapCatQQ 进程数量已达上限，拒绝创建新进程(QQID: {config.bot.QQID})",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.notification_signal.emit("error", "NapCatQQ 进程数量已达上限，无法创建新进程!")
            return

        path_func = it(PathFunc)

        if (qq_path := path_func.get_qq_path()) is None:
            logger.error(
                f"未检测到 QQ 安装路径，无法启动 NapCatQQ 进程(QQID: {config.bot.QQID})",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.notification_signal.emit("error", "未检测到 QQ 安装路径，无法启动 NapCatQQ 进程!")
            return

        logger.trace(
            (
                "NapCatQQ 进程启动参数已解析: "
                f"QQID={config.bot.QQID}, qq_path={qq_path}, "
                f"launcher={getattr(path_func, 'napcat_path', '<unknown>')}"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )

        # 创建 QProcess
        process = self._create_napcat_process(config, qq_path)
        qq_id = str(config.bot.QQID)

        process.stateChanged.connect(lambda state, emitted_qq_id=qq_id: self._handle_process_state_changed(emitted_qq_id, state))
        process.finished.connect(
            lambda exit_code, exit_status, emitted_qq_id=qq_id, emitted_process=process: self._handle_process_finished(
                emitted_qq_id, emitted_process, exit_code, exit_status
            )
        )

        # 进行一些操作
        it(ManagerNapCatQQLog).create_log(config, process)

        # 启动进程
        process.start()
        logger.info(f"NapCatQQ 进程已创建并发起启动(QQID: {config.bot.QQID})")

        # 确保进程已启动
        if not process.waitForStarted(5000):
            logger.error(
                f"NapCatQQ 进程启动失败(QQID: {config.bot.QQID}): {process.errorString()}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.notification_signal.emit("error", "NapCatQQ 进程启动失败!")
            process.deleteLater()
            return

        logger.info(f"NapCatQQ 进程启动成功(QQID: {config.bot.QQID})")
        it(ManagerAutoRestartProcess).create_auto_restart_timer(config)

        # 添加到进程字典
        self.napcat_process_dict[qq_id] = NapCatProcessModel(
            qq_id=qq_id, process=process, state=QProcess.ProcessState.Running, started_at=monotonic()
        )

        # 发出新进程创建信号
        self.process_changed_signal.emit(qq_id, process.state())

    def get_process(self, qq_id: str) -> NapCatProcessModel | RemoteProcessRecord | None:
        """获取指定 QQ 号的进程记录.

        会先查找远端记录, 再退回本地记录;
        UI 层只读 ``qq_id`` / ``state`` / ``started_at`` 字段, 两类记录形状一致.

        Args:
            qq_id (str): QQ 号

        Returns:
            对应的 [`NapCatProcessModel`](src/core/runtime/napcat.py) 或
            [`RemoteProcessRecord`](src/core/runtime/napcat.py); 不存在时返回 None.
        """
        if (record := self.remote_process_dict.get(qq_id)) is not None:
            return record
        return self.napcat_process_dict.get(qq_id, None)

    def has_running_bot(self) -> bool:
        """检查是否有正在运行的 Bot (本地或远端).

        Returns:
            bool: 如果有正在运行的 Bot 则返回 True, 否则返回 False
        """
        if any(
            process_model.state == QProcess.ProcessState.Running
            for process_model in self.napcat_process_dict.values()
        ):
            return True
        return any(
            record.state == QProcess.ProcessState.Running
            for record in self.remote_process_dict.values()
        )

    def stop_process(self, qq_id: str) -> None:
        """停止指定 QQ 号的进程 (本地或远端, 自动路由).

        Args:
            qq_id (str): QQ 号
        """
        # P2.6: 远端 Bot 优先走异步停止路径
        if qq_id in self.remote_process_dict:
            self._stop_remote_process(qq_id)
            return

        if (process_model := self.napcat_process_dict.get(qq_id)) is None:
            logger.warning(f"尝试停止不存在的 NapCatQQ 进程(QQID: {qq_id})", LogType.FILE_FUNC, LogSource.CORE)
            return

        process = process_model.process
        logger.trace(
            (
                "开始停止 NapCatQQ 进程: "
                f"QQID={qq_id}, pid={process.processId()}, "
                f"state={getattr(process.state(), 'name', process.state())}"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )

        try:
            if (parent := psutil.Process(process.processId())).pid != 0:
                child_processes = parent.children(recursive=True)
                logger.trace(
                    f"检测到 NapCatQQ 子进程数量(QQID: {qq_id}, children={len(child_processes)})",
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )
                [child.kill() for child in child_processes]
                parent.kill()
                process.kill()
                process.waitForFinished()
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            process.kill()
            process.waitForFinished()

        process.deleteLater()
        self.napcat_process_dict.pop(qq_id, None)

        it(ManagerNapCatQQLoginState).remove_login_state(qq_id)

        logger.info(f"NapCatQQ 进程已停止(QQID: {qq_id})")
        self.process_changed_signal.emit(qq_id, QProcess.ProcessState.NotRunning)

    def stop_all_processes(self) -> None:
        """停止所有 NapCatQQ 进程 (本地 + 远端)."""
        for qq_id in list(self.napcat_process_dict.keys()):
            self.stop_process(qq_id)
        # 远端 Bot 与本地 dict 不重叠, 单独遍历
        for qq_id in list(self.remote_process_dict.keys()):
            self.stop_process(qq_id)

    def restart_process(self, config: Config) -> None:
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
            # 异步停止后立刻发起 start; 启动 worker 内部会再次调用 backend.start_napcat,
            # 远端 launcher 对"已经停止"的状态是幂等的.
            self._create_remote_process(config)
            return

        # 本地路径: 沿用历史行为
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
        self.stop_process(qq_id)
        self.create_napcat_process(config)

    def get_memory_usage(self, qq_id: str) -> int:
        """获取指定 QQ 号的内存占用 (MB).

        - 本地: 通过 [`psutil`](https://psutil.readthedocs.io/) 累加进程树 RSS
        - 远端: 直接读取 [`RemoteProcessRecord.last_memory_rss_bytes`](src/core/runtime/napcat.py),
          它由轮询 worker 从远端 ``ps -o rss=`` 写入

        未运行 / 未知时返回 0.
        """
        # P2.6: 远端 Bot 走缓存
        if (record := self.remote_process_dict.get(qq_id)) is not None:
            if record.state != QProcess.ProcessState.Running or record.last_memory_rss_bytes is None:
                return 0
            return int(record.last_memory_rss_bytes / (1024 * 1024))

        if (process_model := self.napcat_process_dict.get(qq_id)) is None:
            return 0

        if not (process := process_model.process) or process.state() != QProcess.ProcessState.Running:
            return 0

        if (main_pid := process.processId()) <= 0:
            return 0

        try:
            total_memory = 0
            processed_pids = set()
            queue = deque([main_pid])

            while queue:
                if (pid := queue.popleft()) in processed_pids:
                    continue

                total_memory += psutil.Process(pid).memory_info().rss

                for child in psutil.Process(pid).children():
                    if child.pid not in processed_pids:
                        queue.append(child.pid)
                processed_pids.add(pid)

            return int(total_memory / (1024 * 1024))

        except (psutil.NoSuchProcess, psutil.AccessDenied):
            return 0

    # ==================== P2.6: 远端 Bot 进程管理 ====================
    def _create_remote_process(self, config: Config) -> None:
        """在远端服务器上启动 NapCat Bot.

        启动是异步的: 在 [`QThreadPool`](https://doc.qt.io/qt-6/qthreadpool.html) 后台线程
        发起 SSH 调用, 成功后切回主线程更新 [`RemoteProcessRecord`](src/core/runtime/napcat.py)
        并启动状态轮询.
        """
        qq_id = str(config.bot.QQID)

        if qq_id in self.remote_process_dict:
            logger.warning(
                f"远端 NapCat Bot 已经在管理中(QQID: {qq_id}); 重启请使用 restart_process",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            return

        # 同时运行的远端 Bot 上限与本地一致, 防止 SSH 通道泛滥
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
        # P3: 远端 Bot 也要给 BotLogPage 一个日志缓冲, 否则用户点 "日志" 会看到
        # "未找到对应的日志信息". 这里使用 SSH ``tail`` 周期性拉取实现.
        # 立刻创建是为了让用户在 Bot 启动期间打开日志页就有内容,
        # 不必等 SSH stop 之后才补回来.
        it(ManagerNapCatQQLog).create_remote_log(config)
        # 立即发出 Starting 状态, 让 UI 切换到"启动中"
        self.process_changed_signal.emit(qq_id, QProcess.ProcessState.Starting)

        runner = RemoteBotOperationRunnable(qq_id, config, "start")
        runner.operation_finished_signal.connect(self._on_remote_op_finished)
        runner.operation_failed_signal.connect(self._on_remote_op_failed)
        QThreadPool.globalInstance().start(cast(QRunnable, runner))

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
        # 的瞬间立即停掉, 不能等到 SSH stop 命令返回 (那是 ~4s 之后).
        # 否则在 ``close_webui_tunnel`` 之后到 ``_handle_remote_stop_succeeded``
        # 之前的窗口里, 定时器还会继续 fire, 每次都打出
        # ``ConnectError: 由于目标计算机积极拒绝, 无法连接`` 噪音.
        # 同时清掉 record 的 login_state_published 标记, 防止万一有 in-flight
        # poll worker 在隧道还没关时拉到 endpoint 又尝试 republish.
        it(ManagerNapCatQQLoginState).remove_login_state(qq_id)
        record.login_state_published = False
        record.login_state_port = None
        # 不删除 record, 等 stop worker 完成后再清理 (确保 stop_all_processes 能正确等待)

        runner = RemoteBotOperationRunnable(qq_id, record.config, "stop")
        runner.operation_finished_signal.connect(self._on_remote_op_finished)
        runner.operation_failed_signal.connect(self._on_remote_op_failed)
        QThreadPool.globalInstance().start(cast(QRunnable, runner))

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
            # 启动失败: 移除记录, 通知 UI
            record = self.remote_process_dict.pop(qq_id, None)
            if record is not None:
                self._teardown_remote_polling(record)
            # P3: start 失败时同样回收日志缓冲, 避免 SSH tail 计时器悬挂
            it(ManagerNapCatQQLog).remove_log(qq_id)
            logger.error(
                f"远端 NapCat Bot 启动失败(QQID: {qq_id}): {error}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.notification_signal.emit("error", f"远端 NapCat Bot 启动失败: {error}")
            self.process_changed_signal.emit(qq_id, QProcess.ProcessState.NotRunning)
            return

        if action == "stop":
            # 停止失败: 仍按 NotRunning 处理 (record 已经在 _stop_remote_process 中标记)
            record = self.remote_process_dict.pop(qq_id, None)
            if record is not None:
                self._teardown_remote_polling(record)
            it(ManagerNapCatQQLog).remove_log(qq_id)
            logger.warning(
                f"远端 NapCat Bot 停止失败(QQID: {qq_id}): {error}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.notification_signal.emit("warning", f"远端 NapCat Bot 停止时报错: {error}")
            self.process_changed_signal.emit(qq_id, QProcess.ProcessState.NotRunning)
            it(ManagerNapCatQQLoginState).remove_login_state(qq_id)
            return

        if action == "poll":
            # 轮询单次失败不立即标 NotRunning, 仅记录; 多次连续失败的处理放给 P3
            logger.trace(
                f"远端 Bot 轮询单次失败(QQID: {qq_id}): {error}",
                LogType.NETWORK,
                LogSource.CORE,
            )

    def _handle_remote_start_succeeded(self, qq_id: str, status: object) -> None:
        """远端启动成功后切到 Running + 启动轮询."""
        record = self.remote_process_dict.get(qq_id)
        if record is None:
            return

        record.state = QProcess.ProcessState.Running
        record.started_at = monotonic()
        # 直接复用 ProcessStatus 中的内存数据
        from src.core.operation.backend import ProcessStatus as _ProcessStatus  # local alias to avoid global import cycle

        if isinstance(status, _ProcessStatus):
            record.last_memory_rss_bytes = status.memory_rss_bytes

        self.process_changed_signal.emit(qq_id, QProcess.ProcessState.Running)
        logger.info(
            f"远端 NapCat Bot 启动成功(QQID: {qq_id}, target={record.config.bot.runtime_target})",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        it(ManagerAutoRestartProcess).create_auto_restart_timer(record.config)
        self._start_remote_polling(record)
        # 立刻触发一次 poll, 尝试尽早拿到 WebUI 端点
        self._enqueue_remote_poll(record)

    def _handle_remote_stop_succeeded(self, qq_id: str) -> None:
        """远端停止确认后清理记录与登录状态."""
        record = self.remote_process_dict.pop(qq_id, None)
        if record is not None:
            self._teardown_remote_polling(record)
        it(ManagerAutoRestartProcess).remove_auto_restart_timer(qq_id)
        it(ManagerNapCatQQLoginState).remove_login_state(qq_id)
        # P3: 关闭远端日志的 SSH tail 轮询计时器
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

        # 用户已点 "停止" 但 SSH stop 还在路上时, record 仍然存在但 state=NotRunning,
        # 此时丢弃任何 in-flight 的 poll 结果, 避免重新发布登录状态 / 重建隧道.
        if record.state == QProcess.ProcessState.NotRunning:
            return

        if not isinstance(payload, tuple) or len(payload) != 2:
            return
        status, endpoint = payload

        # 状态变化: 远端进程不见了 -> 主动收尾
        if not getattr(status, "running", False):
            logger.info(
                f"远端 NapCat Bot 已离线(QQID: {qq_id}); 同步状态到本地",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            # 走 stop_succeeded 通用清理路径; 注意不发 stop SSH 命令(进程已不在)
            self._handle_remote_stop_succeeded(qq_id)
            return

        record.last_memory_rss_bytes = getattr(status, "memory_rss_bytes", None)

        # P2.5 集成: 一旦拿到 WebUI 端点, 把它发布给 ManagerNapCatQQLoginState,
        # 让 BotCard 的二维码 / WebUI 按钮链路自动激活.
        if endpoint is not None:
            self._publish_remote_login_state(record, endpoint)

    def _publish_remote_login_state(self, record: RemoteProcessRecord, endpoint: object) -> None:
        """把远端 WebUI 端点 (经 SSH 隧道) 注册到 [`ManagerNapCatQQLoginState`](src/core/runtime/napcat.py).

        通过解析 ``endpoint.base_url`` 中的本地隧道端口, 复用现有
        [`NapCatQQLoginState`](src/core/runtime/napcat.py) 的 HTTP 轮询机制.
        端口与 token 完全一致时跳过, 避免反复销毁/重建.
        """
        base_url = getattr(endpoint, "base_url", "") or ""
        token = getattr(endpoint, "token", None)
        if not base_url or not token:
            return

        match = re.match(r"http://(?:127\.0\.0\.1|localhost):(\d+)", base_url)
        if match is None:
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
        """为远端 Bot 启动 [`QTimer`](https://doc.qt.io/qt-6/qtimer.html) 周期性轮询."""
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
        runner = RemoteBotOperationRunnable(record.qq_id, record.config, "poll")
        runner.operation_finished_signal.connect(self._on_remote_op_finished)
        runner.operation_failed_signal.connect(self._on_remote_op_failed)
        QThreadPool.globalInstance().start(cast(QRunnable, runner))


# ==================== 创建器 ====================
class ManagerNapCatQQLogManagerCreator(AbstractCreator, ABC):
    """NapCatQQ 日志管理器创建器"""

    targets = (CreateTargetInfo("src.core.runtime.napcat", "ManagerNapCatQQLog"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.core.runtime.napcat")

    @staticmethod
    def create(create_type):
        """创建 ManagerNapCatQQLog 实例"""
        return create_type()


add_creator(ManagerNapCatQQLogManagerCreator)


class ManagerNapCatQQLoginStateCreator(AbstractCreator, ABC):
    """NapCatQQ 登录状态管理器创建器"""

    targets = (CreateTargetInfo("src.core.runtime.napcat", "ManagerNapCatQQLoginState"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.core.runtime.napcat")

    @staticmethod
    def create(create_type):
        """创建 ManagerNapCatQQLoginState 实例"""
        return create_type()


add_creator(ManagerNapCatQQLoginStateCreator)


class ManagerAutoRestartProcessCreator(AbstractCreator, ABC):
    """NapCatQQ 自动重启进程管理器创建器"""

    targets = (CreateTargetInfo("src.core.runtime.napcat", "ManagerAutoRestartProcess"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.core.runtime.napcat")

    @staticmethod
    def create(create_type):
        """创建 ManagerAutoRestartProcess 实例"""
        return create_type()


add_creator(ManagerAutoRestartProcessCreator)


class ManagerNapCatQQProcessCreator(AbstractCreator, ABC):
    """NapCatQQ 进程管理器创建器"""

    targets = (CreateTargetInfo("src.core.runtime.napcat", "ManagerNapCatQQProcess"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.core.runtime.napcat")

    @staticmethod
    def create(create_type):
        """创建 ManagerNapCatQQProcess 实例"""
        return create_type()


add_creator(ManagerNapCatQQProcessCreator)


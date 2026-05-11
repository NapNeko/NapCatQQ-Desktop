# -*- coding: utf-8 -*-
"""SnowLuma 全局 daemon: 共享 ``node.exe`` 子进程 + ``SnowLumaWebUIClient`` 单例.

把 SnowLuma 从"per-Bot node.exe"提升为"App 级 daemon", 对齐上游
``packages/core/src/index.ts`` 一份 ``HookManager`` / ``BridgeManager`` 控全部 QQ 的拓扑.

设计要点
========

- **生命周期**: 引用计数. 首个 SnowLuma Bot 启动时 :meth:`SnowLumaDaemon.ensure_running`
  拉起 (若未在跑), 最后一个 SnowLuma Bot 停止时 :meth:`SnowLumaDaemon.release` 回收.
- **线程安全**:

  - 所有状态机转移由 ``threading.Lock`` 保护
  - QProcess 创建 / start / waitForStarted **必须**在主线程, 由 :meth:`_spawn_and_start_node`
    保证 (若 :meth:`ensure_running` 在工作线程被调用, 通过 ``QTimer.singleShot(0, ...)`` +
    ``threading.Event`` 调度到主线程并阻塞等待结果).
  - ``SnowLumaWebUIClient.wait_ready`` / ``login`` 是纯 HTTP 调用, 任何线程都可跑;
    在调用线程 (一般是 driver 的 Phase C worker) 直接同步执行.

- **状态机**::

      STOPPED --ensure_running()--> STARTING
              <--startup-fail-----+
                                  ↓
                              READY --release(ref=0)--> STOPPING --> STOPPED
                                  ↓
                              CRASHED (node.exe 意外 finished)

- **崩溃语义**: ``node.exe`` 意外 finished → state ``CRASHED``, emit
  ``crashed(message)``. :class:`BotProcessManager` 接此信号后对所有依附 Bot 走清理路径
  (W7 落地). ``CRASHED`` 状态下 :meth:`ensure_running` 抛 ``RuntimeError``, 提示用户
  停掉所有 SnowLuma Bot 后再启动 (本期不自动重启).

- **配置渲染职责**: daemon 启动前**一次性**渲染 ``runtime.json`` / ``webui.json``;
  ``onebot_<uin>.json`` 仍由 driver 在 per-Bot 启动路径渲染 (一 Bot 一 UIN 一文件).

参见: ``docs/requirements/2026-05-11-snowluma-daemon-refactor.md`` §2.1.
"""
from __future__ import annotations

import enum
import threading
from abc import ABC
from collections import deque
from pathlib import Path
from typing import Optional

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module, it
from PySide6.QtCore import QCoreApplication, QObject, QProcess, QThread, QTimer, Signal

from src.core.logging import LogSource, LogType, logger
from src.core.runtime.paths import PathFunc
from src.core.runtime.snowluma_config_renderer import (
    render_runtime_json,
    render_webui_json,
)
from src.core.runtime.snowluma_session import (
    load_session,
    resolve_effective_password,
    update_last_rendered,
)
from src.core.runtime.snowluma_webui_client import (
    SnowLumaWebUIClient,
    SnowLumaWebUIError,
)


# SnowLuma WebUI 监听端口 (与 driver / renderer 对齐; 一期硬编码).
_SNOWLUMA_WEBUI_PORT: int = 5099
_SNOWLUMA_WEBUI_HOST: str = "127.0.0.1"

# QProcess.waitForStarted 超时 (毫秒); 5s 与 driver 一期一致.
_QPROCESS_START_TIMEOUT_MS: int = 5000
# QProcess.waitForFinished (terminate 兜底) 超时.
_QPROCESS_FINISH_TIMEOUT_MS: int = 5000

# Phase B WebUI ready 超时 (秒). SnowLuma node.exe 启动 hono server 一般 ≤5s, 30s 兜底.
_WAIT_READY_TIMEOUT_S: float = 30.0
# 工作线程调度 spawn 到主线程的超时. 主线程 event loop 卡住超过这个时间认为系统异常.
_MAIN_THREAD_SPAWN_TIMEOUT_S: float = 10.0


class DaemonState(enum.Enum):
    """SnowLuma daemon 状态机.

    - ``STOPPED``: 初始 / 已回收, 无 node.exe.
    - ``STARTING``: 首个 caller 正在 spawn + wait_ready + login. 后续 caller 会等
      ``_ready_event`` 触发后复检状态.
    - ``READY``: node.exe + WebUI client 完全就绪. ``ensure_running`` 返回 client.
    - ``STOPPING``: ``release()`` 触发的回收路径中, 仅供调试观察.
    - ``CRASHED``: node.exe 意外 finished. ``ensure_running`` 在此状态下抛错; 需用户
      明确停掉所有 SnowLuma Bot 后再启 (本期不自动恢复).
    """

    STOPPED = "stopped"
    STARTING = "starting"
    READY = "ready"
    STOPPING = "stopping"
    CRASHED = "crashed"


# ==================== 模块级辅助: 配置渲染 ====================
def render_daemon_globals(snowluma_path: Path, *, override: str = "") -> str:
    """渲染 daemon 全局配置文件并返回生效密码.

    渲染目标:

    - ``<snowluma_path>/config/runtime.json``: 仅 ``webuiPort`` 字段, 与 daemon 一对一.
    - ``<snowluma_path>/config/webui.json``: SnowLuma WebUI 登录密码 (scrypt hash + salt).

    密码解析委托 :func:`resolve_effective_password` (W5 已重写为不依赖 BotConfig):

    1. ``override`` 非空 → 直接用 ``override`` 作为生效密码.
    2. ``override`` 留空 → 读 ``snowluma-session.json``; 不存在则现场 :func:`create_session`
       生成强密码落盘.

    Args:
        snowluma_path: SnowLuma 安装根目录 (``PathFunc.snowluma_path``).
        override: App 级密码 override. 一般由 :class:`SnowLumaDaemon` 在调用前从
            ``cfg.get(cfg.snowluma_webui_password_override)`` 读出并传入.

    Returns:
        本次启动**生效的**明文密码. 同一字符串被写入 ``webui.json`` 的 scrypt hash, 也
        会作为 ``SnowLumaWebUIClient`` 的 ``password`` 参数用于后续 login.
    """
    render_runtime_json(snowluma_path, webui_port=_SNOWLUMA_WEBUI_PORT)

    effective_password = resolve_effective_password(override=override)
    password_source = "app override" if (override or "").strip() else "snowluma-session.json"

    # 单向覆盖 webui.json: daemon 启动前用 effective_password 重新生成 scrypt hash;
    # 用户在 SnowLuma WebUI 里手改的密码下次 daemon 重启即失效 (D2 决策延续).
    render_webui_json(
        snowluma_path,
        password=effective_password,
        must_change=False,
    )

    # 触发 last_rendered_at 时间戳同步 (override 模式下 session 仍可能存在, 仅作记录).
    session_after = load_session()
    if session_after is not None:
        update_last_rendered(session_after)

    logger.info(
        f"SnowLuma daemon 全局配置已渲染 (source={password_source}, snowluma_path={snowluma_path})",
        LogType.FILE_FUNC,
        LogSource.CORE,
    )
    return effective_password


def _read_cfg_snowluma_override() -> str:
    """读取 App 级 QConfig ``cfg.snowluma_webui_password_override`` (W5).

    延迟 import 与异常吞咽: 测试 / 极端 import 顺序下 ``src.core.config`` 可能未就绪
    (例如没构造 QApplication / PathFunc); 此时静默返回 ``""`` (等价无 override,
    走 session.json fallback).

    Returns:
        cfg 中存储的 override 字符串 (strip 后); 失败时返回 ``""``.
    """
    try:
        from src.core.config import cfg

        return (cfg.get(cfg.snowluma_webui_password_override) or "").strip()
    except Exception as exc:  # noqa: BLE001 - 读 cfg 失败不应阻塞 daemon 启动
        logger.warning(
            (
                "SnowLumaDaemon 读取 cfg.snowluma_webui_password_override 失败, "
                f"按 '无 override' 处理: {type(exc).__name__}: {exc}"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        return ""


# ==================== 主类 ====================
class SnowLumaDaemon(QObject):
    """SnowLuma 全局 daemon (进程级单例, 通过 creart 注册).

    持有唯一的 ``node.exe`` 子进程与 :class:`SnowLumaWebUIClient` 实例; 所有 SnowLuma Bot
    通过 :meth:`ensure_running` / :meth:`release` 引用计数共享同一份 daemon.

    Signals:
        crashed: ``(message: str)`` — node.exe 意外 finished 时 emit (queued connection
            派发到 :class:`BotProcessManager` 主线程槽), payload 是 ``exit_code`` 与
            ``errorString`` 的拼接, 可直接 emit 到 ``notification_signal`` error 级.
        ready: ``()`` — daemon 首次进入 ``READY`` 时 emit (供 UI 状态可视化; 本期未消费).

    Lifecycle invariants:
        - 同一时刻最多一个 ``QProcess`` 实例 (``self._node_process``)
        - ``READY`` 状态下 ``self._webui_client`` 非 ``None`` 且持有有效 Bearer token
        - ``_ref_count >= 0`` 始终成立; ``_ref_count == 0 and _state == READY`` 紧随
          :meth:`release` 触发 :meth:`_shutdown`
    """

    # 信号: 跨线程派发到主线程槽
    crashed = Signal(str)
    ready = Signal()
    # 2026-05-11: node.exe stdout 增量信号 (供 UI 日志页订阅).
    # 旧版本 daemon 没读 node.exe stdout, 用户点 SnowLuma Bot 日志按钮看不到任何输出
    # (Bot QQ.exe 用 ForwardedChannels 不进 pipe; node.exe 是 SnowLuma 业务日志真正的源头).
    # 现在 daemon 自己读 node stdout 缓存到 ``_node_log_storage`` (deque maxlen=10000),
    # emit ``node_log_output_signal(str)`` 供 :class:`SnowLumaDaemonProcessLog` 桥接到
    # ``ManagerNapCatQQLog`` 字典, BotLogPage 直接复用 NapCat 路径.
    node_log_output_signal = Signal(str)

    def __init__(self) -> None:
        super().__init__()
        self._node_process: Optional[QProcess] = None
        self._webui_client: Optional[SnowLumaWebUIClient] = None
        self._ref_count: int = 0
        self._state: DaemonState = DaemonState.STOPPED
        # node.exe stdout 缓冲 (10000 段, 与 NapCatQQProcessLog 对齐)
        self._node_log_storage: deque[str] = deque(maxlen=10000)
        # 保护 _state / _ref_count / _node_process / _webui_client 字段读写.
        # 只在状态机转移段持有, 不包住任何 QProcess 调用 (QProcess 须主线程).
        self._start_lock: threading.Lock = threading.Lock()
        # 启动完成事件 (无论成功或失败都会 set, 让 STARTING 期间的等待者退出 wait).
        self._ready_event: threading.Event = threading.Event()
        # node.exe 死亡事件: 主线程 ``_on_node_finished`` set; 工作线程的 ``wait_ready``
        # 通过 ``is_dead_check`` 读取, 实现 node 已挂时 wait_ready 立即返回 False.
        self._dead_event: threading.Event = threading.Event()
        # 最近一次启动失败原因; ``ensure_running`` 失败 / CRASHED 状态下报错时拼进去.
        self._last_error: Optional[str] = None

    # ==================== 状态查询 ====================
    @property
    def state(self) -> DaemonState:
        """当前状态机状态 (线程安全快照)."""
        with self._start_lock:
            return self._state

    @property
    def ref_count(self) -> int:
        """当前挂载的 Bot 数量快照 (线程安全)."""
        with self._start_lock:
            return self._ref_count

    def is_running(self) -> bool:
        """daemon 是否已 ``READY`` (有可用 WebUI client)."""
        return self.state == DaemonState.READY

    def webui_client(self) -> SnowLumaWebUIClient:
        """返回当前 WebUI client. **要求 daemon 处于 READY**; 否则 raise.

        Raises:
            RuntimeError: 状态非 ``READY``, 或 ``_webui_client`` 为 ``None``.
        """
        with self._start_lock:
            if self._state != DaemonState.READY or self._webui_client is None:
                raise RuntimeError(
                    f"SnowLumaDaemon 未就绪 (state={self._state.name}); "
                    "请先调用 ensure_running()"
                )
            return self._webui_client

    # ==================== 公共 API: ensure_running / release ====================
    def ensure_running(
        self,
        *,
        timeout: float = _WAIT_READY_TIMEOUT_S + 5.0,
        override: str | None = None,
    ) -> SnowLumaWebUIClient:
        """确保 daemon 处于 ``READY`` 状态, 返回 WebUI client; 引用计数 +1.

        线程模型:

        - **任意线程**可调. 内部用 ``threading.Lock`` 保护状态机.
        - 状态机转移 ``STOPPED → STARTING`` 时, 当前 caller 成为 starter:

          - 在调用线程 (可能是工作线程) 触发 spawn 流程; QProcess 创建必须**主线程**,
            因此通过 ``QTimer.singleShot(0, ...)`` 调度 + ``threading.Event`` 阻塞等待.
          - QProcess 启动成功后, 在 caller 线程跑 ``SnowLumaWebUIClient.wait_ready`` +
            ``login`` (HTTP 调用, 任意线程都安全).
        - ``STARTING`` 状态下的并发 caller 会等 starter 完成 (``_ready_event``).
        - ``READY`` 状态下任意 caller 直接 ``ref_count += 1`` 返回.
        - ``CRASHED`` 状态下抛 ``RuntimeError``: 本期不自动重启, 需用户停掉所有
          SnowLuma Bot 后再启 (W7 manager 已弹错误 notification 提示用户).

        Args:
            timeout: 等待 daemon 就绪的最长时长 (秒); 涵盖 ``wait_ready`` (30s) +
                ``login`` (5s) + 一些缓冲. 默认 35s.
            override: App 级密码 override (W5 行为).

                - ``None`` (默认): daemon 内部读 ``cfg.snowluma_webui_password_override``.
                  生产路径使用此分支, 无需显式 wiring.
                - ``""`` / ``"explicit_value"``: 调用方显式传值, 跳过 cfg 读. 主要给
                  单元测试用 (避免依赖 ``cfg`` 单例与 QApplication 初始化顺序).

        Returns:
            就绪的 :class:`SnowLumaWebUIClient` (持有 Bearer token).

        Raises:
            RuntimeError: 状态机异常 / spawn 失败 / WebUI 30s 未就绪 / login 失败 /
                CRASHED 状态.
            FileNotFoundError: ``node.exe`` 安装路径缺失 (调用方应引导用户去组件页安装).
        """
        # W5: override sentinel = None 时从 cfg 读取; 显式 "" 或 "..." 时尊重 caller.
        if override is None:
            override = _read_cfg_snowluma_override()
        # 阶段 1: 状态机决策
        i_am_starter = False
        with self._start_lock:
            if self._state == DaemonState.READY:
                # Fast path: 已 READY, 直接复用 client.
                self._ref_count += 1
                assert self._webui_client is not None
                logger.trace(
                    f"SnowLumaDaemon ensure_running 复用 READY (ref_count={self._ref_count})",
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )
                return self._webui_client

            if self._state == DaemonState.CRASHED:
                raise RuntimeError(
                    f"SnowLuma daemon 已崩溃, 请先停止所有 SnowLuma Bot 后再启动 "
                    f"(last_error={self._last_error or '<unknown>'})"
                )
            if self._state == DaemonState.STOPPING:
                raise RuntimeError(
                    "SnowLuma daemon 正在停止中, 请稍后重试"
                )

            if self._state == DaemonState.STOPPED:
                # 我是首个 caller; 切到 STARTING, 后面跑 spawn + wait_ready + login.
                self._state = DaemonState.STARTING
                self._ref_count = 1
                self._ready_event.clear()
                self._dead_event.clear()
                self._last_error = None
                i_am_starter = True
            elif self._state == DaemonState.STARTING:
                # 并发 caller: 仅 ref_count +1, 等 starter 完成.
                self._ref_count += 1

        if i_am_starter:
            try:
                client = self._do_startup(override=override, timeout=timeout)
            except BaseException as exc:
                # 启动失败: 状态回滚 + ref_count 清零 (并发 caller 也会看到 STOPPED).
                with self._start_lock:
                    self._state = DaemonState.STOPPED
                    self._ref_count = 0
                    self._webui_client = None
                    self._last_error = f"{type(exc).__name__}: {exc}"
                    self._ready_event.set()  # 解开并发 caller 的等待
                logger.warning(
                    f"SnowLumaDaemon 启动失败: {self._last_error}",
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )
                raise
            # 启动成功: 已在 _do_startup 内部把 state 改 READY + set _ready_event.
            return client

        # 并发 caller 路径: 等 starter.
        if not self._ready_event.wait(timeout=timeout):
            # 超时. 但 ref_count 已经 +1 了, 这里要 -1 还回去 (不在 lock 内调 release()
            # 避免重入).
            with self._start_lock:
                if self._ref_count > 0:
                    self._ref_count -= 1
            raise RuntimeError(
                f"SnowLuma daemon 启动超时 ({timeout}s); 另一个 caller 在 STARTING"
            )

        with self._start_lock:
            if self._state != DaemonState.READY or self._webui_client is None:
                # Starter 失败, 我们也跟着失败; ref_count 已经在 starter 失败路径
                # 被清 0, 这里只 raise.
                if self._ref_count > 0:
                    self._ref_count -= 1
                raise RuntimeError(
                    f"SnowLuma daemon 启动失败 (state={self._state.name}, "
                    f"last_error={self._last_error or '<unknown>'})"
                )
            return self._webui_client

    def release(self) -> None:
        """引用计数 -1; **不**触发 daemon shutdown (持久 daemon 模型).

        线程安全, 幂等. 主要由 :meth:`SnowLumaDriver.stop` 在 Bot 停止时调用.

        **设计变更 (2026-05-11)**: 早期版本采用 "ref=0 自动 terminate" 的 ref-counted
        lifecycle, 符合 requirement AC3 的字面意思 (停止最后一个 Bot 后 node.exe 自动
        退出). 实际验证发现该语义导致两个 UX 问题:

        1. 反复启停 Bot 时, 每次起首个 Bot 都要等 ~30s daemon spawn (wait_ready 30s);
        2. 用户停最后一个 Bot 后立即又起 Bot, ensure_running 命中 STOPPING 状态报错.

        与上游 SnowLuma "一个服务多 QQ 挂" 的设计也更贴合**持久 daemon**: daemon 一旦
        拉起一直活到 App 退出, ref_count 仅用于跟踪当前**正在用**的 Bot 数 (诊断 / 监控
        目的), 不再驱动生命周期.

        Daemon 真正 terminate 由 :meth:`shutdown` 显式触发 (一般是 App 退出钩子调).

        Note:
            内存代价: daemon spawn 后约 100MB 常驻; 但只 spawn 一次, 后续启停 Bot 即时.
        """
        with self._start_lock:
            if self._ref_count > 0:
                self._ref_count -= 1
            logger.trace(
                (
                    f"SnowLumaDaemon.release: ref_count -> {self._ref_count} "
                    f"(state={self._state.name}; 持久 daemon 模型, 不触发 terminate)"
                ),
                LogType.FILE_FUNC,
                LogSource.CORE,
            )

    def shutdown(self) -> None:
        """显式 terminate daemon (持久 daemon 模型下的唯一关停路径).

        触发时机:

        - App 退出钩子 (``QApplication.aboutToQuit`` / ``atexit``).
        - 用户在 UI 显式 "重启 daemon" (本期未做按钮).
        - 测试场景的 cleanup.

        幂等: 多次调用安全; 已 STOPPED / STOPPING / CRASHED 状态下立即返回.
        线程安全: 内部 ``_start_lock`` 保护状态切换.

        Shutdown 流程 (state ``READY → STOPPING → STOPPED``):

        1. ``webui_client.logout()`` fire-and-forget (不等结果, 失败静默忽略).
        2. ``terminate_async(node_process)``: 非阻塞 graceful kill, 5s 后兜底 SIGKILL.
        3. ``_state = STOPPED``, 清 client / node_process 引用.
        """
        should_shutdown = False
        with self._start_lock:
            if self._state == DaemonState.READY:
                self._state = DaemonState.STOPPING
                should_shutdown = True
            elif self._state == DaemonState.STARTING:
                # 正在启动中, 等启动完再次调 shutdown; 这里先标位等下一轮再清.
                logger.warning(
                    "SnowLumaDaemon.shutdown 在 STARTING 状态调用; "
                    "等 starter 完成后请重新调一次 shutdown",
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )
            # STOPPED / STOPPING / CRASHED: no-op

        if should_shutdown:
            self._shutdown()

    # ==================== 内部: startup / shutdown 实现 ====================
    def _do_startup(self, *, override: str, timeout: float) -> SnowLumaWebUIClient:
        """完整启动流程: 主线程 spawn QProcess + caller 线程 wait_ready + login.

        必须由 :meth:`ensure_running` 在确认自己是 starter 后调用, 不再二次加锁.
        失败时由 :meth:`ensure_running` 负责回滚 state.
        """
        # ---- 阶段 A: spawn QProcess (主线程亲和; 调用线程不是主线程时通过 QTimer 调度) ----
        is_main_thread = (
            QCoreApplication.instance() is not None
            and QThread.currentThread() == QCoreApplication.instance().thread()
        )

        spawn_error_box: list[BaseException] = []
        spawned_password_box: list[str] = []  # 主线程渲染配置后返回的 effective_password

        def _do_spawn_on_main() -> None:
            try:
                effective_password = self._spawn_and_start_node(override=override)
                spawned_password_box.append(effective_password)
            except BaseException as exc:  # noqa: BLE001 - 边界统一回传
                spawn_error_box.append(exc)

        if is_main_thread:
            _do_spawn_on_main()
        else:
            spawn_done = threading.Event()

            def _spawn_then_signal() -> None:
                try:
                    _do_spawn_on_main()
                finally:
                    spawn_done.set()

            # 关键: 用 3-arg 形式 ``QTimer.singleShot(msec, context, slot)``,
            # 显式指定 ``self`` (daemon QObject) 为 context. PySide6 据此把回调路由到
            # ``self.thread()`` (即 daemon 创建时所在的主线程), 而非 worker 线程.
            # 2-arg 形式从 worker 线程调用时, 由于 worker 没 event loop, 回调永远不会跑.
            QTimer.singleShot(0, self, _spawn_then_signal)
            if not spawn_done.wait(timeout=_MAIN_THREAD_SPAWN_TIMEOUT_S):
                raise RuntimeError(
                    f"SnowLuma daemon spawn 调度到主线程超时 "
                    f"({_MAIN_THREAD_SPAWN_TIMEOUT_S}s); 主线程 event loop 异常?"
                )

        if spawn_error_box:
            raise spawn_error_box[0]

        if not spawned_password_box:
            raise RuntimeError("SnowLuma daemon spawn 完成但未返回密码 (内部状态异常)")
        effective_password = spawned_password_box[0]

        # ---- 阶段 B: wait_ready + login (HTTP, 任意线程) ----
        client = SnowLumaWebUIClient(
            host=_SNOWLUMA_WEBUI_HOST,
            port=_SNOWLUMA_WEBUI_PORT,
            password=effective_password,
        )
        ready_ok = client.wait_ready(
            timeout=_WAIT_READY_TIMEOUT_S,
            is_dead_check=self._dead_event.is_set,
        )
        if not ready_ok:
            if self._dead_event.is_set():
                raise RuntimeError(
                    "SnowLuma node.exe 启动后立即崩溃 "
                    "(常见原因: 端口 5099 被占用 / 安装产物损坏 / 杀毒拦截)"
                )
            errors_summary = (
                "; ".join(
                    f"{h}={msg}" for h, msg in client.last_wait_errors.items()
                )
                if client.last_wait_errors
                else "<no probe error captured>"
            )
            raise RuntimeError(
                f"SnowLuma WebUI {_WAIT_READY_TIMEOUT_S:.0f}s 内未就绪. "
                f"各候选 host 最后错误: {errors_summary}"
            )

        try:
            client.login()
        except SnowLumaWebUIError as exc:
            raise RuntimeError(f"SnowLuma daemon login 失败: {exc.message}") from exc

        # ---- 阶段 C: 提交结果到 state 机 (state → READY) ----
        with self._start_lock:
            self._webui_client = client
            self._state = DaemonState.READY
            self._ready_event.set()

        logger.info(
            "SnowLuma daemon 已就绪 (state=READY, ref_count="
            f"{self.ref_count}, node_pid="
            f"{self._node_process.processId() if self._node_process else '<unknown>'})",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        self.ready.emit()
        return client

    def _spawn_and_start_node(self, *, override: str) -> str:
        """**主线程** 构造 + 启动 node.exe QProcess; 返回 effective_password.

        步骤:

        1. 解析 ``node.exe`` / SnowLuma 入口路径 (经由 :class:`PathFunc`).
        2. 渲染 daemon 全局配置 (runtime.json + webui.json), 拿到 effective_password.
        3. 构造 QProcess (program / arguments / cwd / channel mode).
        4. 接 ``finished`` 信号到 :meth:`_on_node_finished` (主线程槽).
        5. ``start()`` + ``waitForStarted(5s)`` 同步等; 失败抛 ``RuntimeError``.

        Raises:
            FileNotFoundError: ``node.exe`` 不存在.
            RuntimeError: ``waitForStarted`` 超时 / 内部状态异常.
        """
        path_func = it(PathFunc)
        node_exe = path_func.get_snowluma_node_executable()
        if node_exe is None:
            raise FileNotFoundError(
                "未检测到 SnowLuma node.exe; 请在组件页 (Component) 的 SnowLuma tab 下先安装 SnowLuma"
            )
        snowluma_path = path_func.snowluma_path

        # 渲染 runtime.json + webui.json (daemon 启动前一次性写入). 返回生效密码供
        # SnowLumaWebUIClient 后续 login 使用.
        effective_password = render_daemon_globals(snowluma_path, override=override)

        process = QProcess()
        process.setProgram(str(node_exe))
        process.setArguments([str(path_func.get_snowluma_entry())])
        process.setWorkingDirectory(str(snowluma_path))
        process.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)
        # 注意: ``finished`` 信号在主线程 emit (因为 QProcess 创建在主线程),
        # 槽函数 ``_on_node_finished`` 也在主线程跑.
        process.finished.connect(self._on_node_finished)
        # 2026-05-11: 读 node.exe stdout 进 _node_log_storage + emit ``node_log_output_signal``.
        # 旧版本完全不读 stdout, 既看不到日志也有 OS pipe buffer 满导致 node 写阻塞的潜在风险.
        process.readyReadStandardOutput.connect(self._on_node_stdout_ready)
        # 上一次 node spawn 残留的日志清空 (跨 daemon session 不延续, 与 NapCat 路径一致)
        self._node_log_storage.clear()

        process.start()
        if not process.waitForStarted(_QPROCESS_START_TIMEOUT_MS):
            err = process.errorString()
            # 清理: 即使 waitForStarted 失败也保险 kill 一下 (有些场景 start() 已成功
            # 但 waitForStarted 后续 OS 信号丢失).
            if process.state() != QProcess.ProcessState.NotRunning:
                process.kill()
                process.waitForFinished(_QPROCESS_FINISH_TIMEOUT_MS)
            raise RuntimeError(
                f"SnowLuma node.exe 启动失败 (waitForStarted timeout): {err}"
            )

        # 状态机持有 process 引用; ``_on_node_finished`` 在主线程会更新 state 到 CRASHED.
        self._node_process = process

        logger.info(
            (
                "SnowLuma daemon node.exe 已起 ("
                f"node_pid={process.processId()}, "
                f"snowluma_path={snowluma_path})"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        return effective_password

    def _shutdown(self) -> None:
        """``release`` 触发的回收路径: ``READY → STOPPING → STOPPED``.

        - logout fire-and-forget (失败静默, 与 :class:`_SnowLumaStopHttpWorker` 一致语义)
        - ``terminate_async(node_process)``: 非阻塞 graceful kill, 5s 兜底 SIGKILL
        - 清字段, 切回 STOPPED. ``_node_process.finished`` 仍会 emit, 但
          :meth:`_on_node_finished` 检测到 state 已是 STOPPING/STOPPED 后会静默忽略,
          不再走 CRASHED 路径.
        """
        # 复制需要操作的引用 (释放锁之外做)
        with self._start_lock:
            client = self._webui_client
            node_process = self._node_process

        # 1. logout fire-and-forget. ``client.logout`` 内部已 silent fail.
        if client is not None:
            try:
                client.logout()
            except Exception as exc:  # noqa: BLE001 - shutdown 边界吞所有异常
                logger.trace(
                    f"SnowLumaDaemon shutdown logout 静默忽略: {type(exc).__name__}: {exc}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )

        # 2. terminate node.exe. 必须主线程; 但 release() 可能从工作线程调.
        is_main_thread = (
            QCoreApplication.instance() is not None
            and QThread.currentThread() == QCoreApplication.instance().thread()
        )
        if node_process is not None:
            if is_main_thread:
                _terminate_async(node_process)
            else:
                # 同 _do_startup: 3-arg 形式确保回调跑在 daemon (主线程) context.
                QTimer.singleShot(0, self, lambda p=node_process: _terminate_async(p))

        # 3. 切状态 → STOPPED, 清字段.
        with self._start_lock:
            self._state = DaemonState.STOPPED
            self._webui_client = None
            self._node_process = None
            self._last_error = None
            # 不清 _ready_event (它在 ensure_running 下次启动时会 clear).

        logger.info(
            "SnowLuma daemon 已回收 (state=STOPPED, ref_count=0)",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )

    # ==================== node.exe stdout 日志 (2026-05-11 新增) ====================
    def get_node_log_content(self) -> str:
        """返回 node.exe stdout 的累积日志全文 (供 :class:`SnowLumaDaemonProcessLog`
        通过 ``ManagerNapCatQQLog`` 字典提供给 ``BotLogPage``).

        本方法是字符串拼接, 不阻塞. 主线程调用安全.

        Returns:
            从 node.exe spawn 起累积的 stdout 文本 (含 stderr, ``MergedChannels``).
            最多保留最近 10000 段 (与 ``NapCatQQProcessLog`` deque maxlen 对齐).
        """
        return "".join(self._node_log_storage)

    def _on_node_stdout_ready(self) -> None:
        """``QProcess.readyReadStandardOutput`` 槽 (主线程): 把 node.exe stdout 增量
        读出来, 缓存到 ``_node_log_storage`` 并通过 ``node_log_output_signal`` 推送给
        UI 订阅者.

        sanitize 沿用 ``NapCatQQProcessLog`` 同款 ``_sanitize_log_text`` (剥 ANSI 颜色
        码 / 控制字符), 让 ``BotLogPage`` 能直接显示无需额外清洗.
        """
        process = self._node_process
        if process is None:
            return
        try:
            data = bytes(process.readAllStandardOutput().data()).decode(
                "utf-8", errors="replace"
            )
        except Exception:  # noqa: BLE001 - 读 pipe 任何异常静默, 不影响 daemon
            return
        if not data:
            return
        # 复用 manager 的 sanitize 函数, 与 NapCatQQProcessLog 行为完全一致.
        from src.core.runtime.bot_process_manager import _sanitize_log_text

        cleaned = _sanitize_log_text(data)
        if not cleaned:
            return
        self._node_log_storage.append(cleaned)
        self.node_log_output_signal.emit(cleaned)

    # ==================== 内部: node.exe finished 槽 (主线程) ====================
    def _on_node_finished(
        self,
        exit_code: int,
        exit_status: QProcess.ExitStatus,
    ) -> None:
        """node.exe 退出回调 (主线程槽).

        分两种情况:

        - 我们自己 ``release()`` 触发的 ``terminate_async``: state 已是 STOPPING/STOPPED,
          静默忽略 (避免重复 CRASHED 状态).
        - 意外退出 (node.exe 自杀 / 外部 Stop-Process): state 还是 READY/STARTING,
          切到 CRASHED, set ``_dead_event``, emit ``crashed(msg)`` 让 manager 走全员清理.
        """
        sender_process = self._node_process
        exit_status_name = getattr(exit_status, "name", str(exit_status))
        message = (
            f"SnowLuma node.exe 已退出 (exit_code={exit_code}, exit_status={exit_status_name})"
        )

        # set dead_event 让仍在 wait_ready 的 caller 快速失败.
        self._dead_event.set()

        with self._start_lock:
            current_state = self._state
            # 若我们自己 release 触发, state 是 STOPPING/STOPPED, 不要进 CRASHED.
            if current_state in (DaemonState.STOPPING, DaemonState.STOPPED):
                logger.trace(
                    f"SnowLuma daemon node.exe finished (expected, state={current_state.name}): {message}",
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )
                self._node_process = None
                # ready_event 不 set, 因为 STOPPED 期间没有 STARTING caller 在等.
                emit_crash = False
            else:
                # 意外退出: 进 CRASHED 状态.
                self._state = DaemonState.CRASHED
                self._last_error = message
                self._webui_client = None
                self._node_process = None
                self._ref_count = 0  # manager 接 crashed signal 后会走 release 路径; 这里清零避免重复扣减
                self._ready_event.set()  # 解开任何在等 STARTING 的 caller (它们会 raise)
                emit_crash = True

        if emit_crash:
            logger.warning(
                f"SnowLuma daemon 意外崩溃: {message}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            # process.deleteLater 由 manager 在接 crashed 信号后做; 也兜底一下.
            if sender_process is not None:
                sender_process.deleteLater()
            self.crashed.emit(message)
        else:
            if sender_process is not None:
                sender_process.deleteLater()


# ==================== terminate_async helper (内部复制, 避免 cyclic import with driver) ====================
def _terminate_async(process: QProcess, timeout_ms: int = _QPROCESS_FINISH_TIMEOUT_MS) -> None:
    """非阻塞 graceful kill: 主线程发 terminate, ``timeout_ms`` 后兜底 SIGKILL.

    与 :func:`src.core.runtime.snowluma_driver.terminate_async` 同构. 单独定义避免
    daemon → driver 反向 import (driver 后续会 import daemon).
    """
    if process is None or process.state() == QProcess.ProcessState.NotRunning:
        return
    process.terminate()
    QTimer.singleShot(
        timeout_ms,
        lambda p=process: (
            p.kill() if p.state() != QProcess.ProcessState.NotRunning else None
        ),
    )


# ==================== creart 单例注册 ====================
class SnowLumaDaemonCreator(AbstractCreator, ABC):
    """``SnowLumaDaemon`` 单例创建器.

    与 :class:`ServerManagerCreator` 同一风格; ``it(SnowLumaDaemon)`` 返回**进程级**
    唯一实例. 创建必须在 ``QApplication`` 已构造之后 (``QObject`` 父类要求).
    """

    targets = (
        CreateTargetInfo(
            module="src.core.runtime.snowluma_daemon",
            identify="SnowLumaDaemon",
            humanized_name="SnowLuma 全局 daemon",
        ),
    )

    @staticmethod
    def available() -> bool:
        return exists_module("src.core.runtime.snowluma_daemon")

    @staticmethod
    def create(create_type: type[SnowLumaDaemon]) -> SnowLumaDaemon:
        return create_type()


add_creator(SnowLumaDaemonCreator)

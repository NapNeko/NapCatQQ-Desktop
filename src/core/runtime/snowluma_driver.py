# -*- coding: utf-8 -*-
"""SnowLuma 后端 driver (P2 W2: daemon 解耦重构, 多 Bot 支持).

本 driver 在 W2 重构后**不再**持有 ``node.exe`` / ``SnowLumaWebUIClient``; 这些 daemon
级资源由 :class:`SnowLumaDaemon` 全局共享 (见 ``snowluma_daemon.py``).
driver 仅持 per-Bot 状态:

- ``qq_pid`` (注入目标 QQ.exe PID; HOT 模式来自用户 attach, COLD 模式 Desktop spawn 后取)
- ``qq_process`` (COLD 模式: Desktop spawn 的 ``QQ.exe`` QProcess; HOT 模式: ``None``)
- ``uin`` (poller 首次 UIN 探测后回写, 一 Bot 一 UIN; W3 写入)
- ``ancillary_pids`` (poller 按 UIN 聚合时发现的同 UIN 其他 PID, W3 写入)

启动流程 (新):

- **Phase A** (主线程): 构造 model + spawn QQ.exe (COLD; HOT 跳过). **不**再 spawn node.
- **Phase C** (后台 worker): 调 ``daemon.ensure_running()`` 拿 client → ``client.load_process(qq_pid)``.
  Phase B (旧的 wait_ready + login) 整段被 daemon 接管.
- **Phase D** (主线程): 启动 :class:`SnowLumaStatusPoller`.

停止流程 (新): unload (fire-and-forget, 走 daemon 的 client) → kill QQ.exe (COLD) →
``daemon.release()``. 不再 kill node (daemon 自己引用计数 → 0 时回收).

**删除**: 一期硬限制 ``RuntimeError("一期仅支持 1 个 SnowLuma Bot")`` (W2 主目标),
让多 Bot 共享 daemon 同时跑.

参见: ``docs/requirements/2026-05-11-snowluma-daemon-refactor.md`` §2.2.
"""
from __future__ import annotations

import enum
import threading
from pathlib import Path
from time import monotonic
from typing import TYPE_CHECKING, Callable, Optional

from creart import it
from PySide6.QtCore import QObject, QProcess, QRunnable, QThreadPool, QTimer, Signal

from src.core.logging import LogSource, LogType, logger
from src.core.runtime.bot_backend_driver import BotBackendDriver, ProcessHandle
from src.core.runtime.paths import PathFunc
from src.core.runtime.snowluma_daemon import SnowLumaDaemon
from src.core.runtime.snowluma_webui_client import (
    SnowLumaWebUIClient,
    SnowLumaWebUIError,
)

if TYPE_CHECKING:
    from src.core.config.config_model import Config
    from src.core.runtime.snowluma_status_poller import SnowLumaStatusPoller


# QProcess.waitForStarted 超时 (毫秒); 与 daemon 一致.
_QPROCESS_START_TIMEOUT_MS: int = 5000
# QProcess.waitForFinished (terminate 兜底) 超时.
_QPROCESS_FINISH_TIMEOUT_MS: int = 5000
# Phase C 阻塞 worker 线程的最大时长 (含 daemon.ensure_running 35s + load_process 15s + 缓冲).
_PHASE_C_TIMEOUT_S: float = 55.0


# Q2: SnowLuma 启动模式 — 冷启动 (Desktop spawn 新 QQ.exe) vs 热启动 (附加已有 QQ.exe).
class SnowLumaStartMode(enum.Enum):
    """SnowLuma Bot 启动模式 (Q2).

    - ``COLD_START``: Desktop ``QProcess.spawn`` 一个新 ``QQ.exe``, 拿到 PID 后注入.
      历史默认行为; 用户系统没 QQ.exe 在跑时必走这个.
    - ``HOT_START``: Desktop 不 spawn QQ.exe, 而是注入到用户已经启动的某个 QQ.exe.
      ``attach_pid`` 必须有效. ``SnowLumaProcessModel.qq_process`` 为 ``None``;
      manager 不再监听 QQ.exe 的 ``QProcess.finished`` (改由 poller 兜底).
    """

    COLD_START = "cold"
    HOT_START = "hot"


# ==================== 异步 kill 辅助 (跨模块复用, 主线程亲和) ====================
def terminate_async(
    process: Optional["QProcess"],
    timeout_ms: int = _QPROCESS_FINISH_TIMEOUT_MS,
) -> None:
    """非阻塞 graceful kill: 主线程发 terminate, ``timeout_ms`` 毫秒后兜底 SIGKILL.

    所有调用必须在主线程 (QProcess 线程亲和性). 立即返回, 不等结果.

    Args:
        process: 目标 QProcess; ``None`` 或已 ``NotRunning`` 时 no-op.
        timeout_ms: graceful terminate 等待时长 (默认 5s).
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


# ==================== Process model (W2 裁剪后) ====================
class SnowLumaProcessModel:
    """SnowLuma per-Bot 进程模型 (W2 裁剪: daemon 接管 node / client / password).

    Attributes:
        qq_id: Bot 的 QQ 号 (字符串).
        qq_process: COLD 模式下 Desktop spawn 的 ``QQ.exe`` QProcess; HOT 模式 ``None``.
        qq_pid: 注入目标 QQ.exe PID. COLD: spawn 后从 ``qq_process.processId()`` 写;
            HOT: 直接来自调用方 ``attach_pid``.
        state: 当前 ``QProcess.ProcessState`` 状态机.
        started_at: ``time.monotonic()`` 启动时刻.
        dead_event: 跨线程死亡通知; 兼容 daemon 的 ``is_dead_check`` 协议 (W2 路径下
            实际由 manager / daemon 各自维护; 保留供回归兼容).
        uin: 当 :class:`SnowLumaStatusPoller` 首次探测到真实 UIN 后回写 (W3).
        ancillary_pids: 同 UIN 下 SnowLuma 自动发现的其它 PID 集合 (Electron 衍生进程);
            由 poller 按 UIN 聚合后通过 ``pid_set_changed`` 信号回写 (W3 + W7).
    """

    __slots__ = (
        "qq_id",
        "qq_process",
        "qq_pid",
        "state",
        "started_at",
        "dead_event",
        "uin",
        "ancillary_pids",
    )

    def __init__(
        self,
        qq_id: str,
        *,
        qq_process: Optional[QProcess] = None,
        qq_pid: int = 0,
        state: QProcess.ProcessState = QProcess.ProcessState.NotRunning,
        started_at: float = 0.0,
    ) -> None:
        self.qq_id = qq_id
        self.qq_process: Optional[QProcess] = qq_process
        self.qq_pid: int = qq_pid
        self.state = state
        self.started_at = started_at
        self.dead_event: threading.Event = threading.Event()
        self.uin: str = ""
        self.ancillary_pids: set[int] = set()


# ==================== Driver (W2) ====================
class SnowLumaDriver(BotBackendDriver):
    """SnowLuma per-Bot driver (W2: 不再单实例守护, 不再 spawn node).

    多个 SnowLuma Bot 通过 :class:`SnowLumaDaemon` 共享同一份 ``node.exe`` 与
    ``SnowLumaWebUIClient``; driver 仅做 per-Bot 的 QQ.exe spawn (COLD) / attach_pid
    校验 (HOT) + ``client.load_process`` 注入 + poller 启停.
    """

    def __init__(self) -> None:
        # per-Bot model 字典 (W2: 不再 1 Bot 上限)
        self._processes: dict[str, SnowLumaProcessModel] = {}
        # Poller per Bot
        self._pollers: dict[str, "SnowLumaStatusPoller"] = {}
        # daemon 实例 (lazy 通过 ``it(SnowLumaDaemon)``; 避免 import 期就触发 QObject 构造).
        self._daemon: Optional[SnowLumaDaemon] = None

    # ==================== 内部: daemon 取用 ====================
    def _get_daemon(self) -> SnowLumaDaemon:
        """惰性返回 daemon 单例 (``creart`` 注册); 多次调用幂等."""
        if self._daemon is None:
            self._daemon = it(SnowLumaDaemon)
        return self._daemon

    # ==================== BotBackendDriver: start (同步版, 供测试 / 远端) ====================
    def start(
        self,
        config: "Config",
        *,
        start_mode: SnowLumaStartMode = SnowLumaStartMode.COLD_START,
        attach_pid: int = 0,
    ) -> ProcessHandle:
        """**同步**启动 SnowLuma Bot (Phase A → C → D 全在调用线程).

        本方法主要供:
        - 测试 (Pytest 主线程, 可以阻塞)
        - 远端 SSH 路径 (本来就在 worker 线程)

        UI 主线程路径请用 :meth:`start_async`, 把 Phase C 放后台跑.

        Phase A 阻塞 ≤5s (waitForStarted QQ.exe), Phase C 阻塞 ≤55s
        (daemon.ensure_running 35s + load_process 15s + 缓冲).

        Raises:
            FileNotFoundError: ``QQ.exe`` (COLD) / ``node.exe`` 缺失.
            RuntimeError: HOT_START attach_pid 无效 / WebUI 起不来 / 注入失败.
        """
        model = self._do_phase_a(config, start_mode, attach_pid)
        try:
            self._start_phase_a_processes_blocking(model)
            self._render_onebot_config(config)
            daemon = self._get_daemon()
            client = daemon.ensure_running(timeout=_PHASE_C_TIMEOUT_S)
            self._do_phase_c_inject(model, client)
            self._do_phase_d_poller(model, client)
        except BaseException:
            self._abort_start(model)
            raise

        return ProcessHandle(
            qq_id=model.qq_id,
            primary_process=model.qq_process,  # COLD: QQ.exe; HOT: None
            secondary_process=None,  # daemon 持 node, 不再暴露给上层
        )

    def _start_phase_a_processes_blocking(self, model: SnowLumaProcessModel) -> None:
        """同步阻塞版 Phase A 启动 (仅供同步 :meth:`start` 用; 走 ``waitForStarted``).

        **禁止主线程调用**: 含 ``qq_process.waitForStarted(5000)`` 阻塞最多 5 秒.
        Manager / UI 路径请用 :meth:`_start_phase_a_processes_async` (signal-driven).

        Raises:
            RuntimeError: ``waitForStarted`` 超时 / ``processId`` 异常.
        """
        qq_process = model.qq_process
        if qq_process is None:
            # HOT 模式: nothing to start; model.qq_pid 已在 _do_phase_a 用 attach_pid 填好.
            return

        qq_process.start()
        model.state = QProcess.ProcessState.Starting
        if not qq_process.waitForStarted(_QPROCESS_START_TIMEOUT_MS):
            err = qq_process.errorString()
            self._processes.pop(model.qq_id, None)
            raise RuntimeError(
                f"QQ.exe 启动失败 (waitForStarted timeout): {err}"
            )
        qq_pid = qq_process.processId()
        if qq_pid <= 0:
            qq_process.kill()
            qq_process.waitForFinished(_QPROCESS_FINISH_TIMEOUT_MS)
            self._processes.pop(model.qq_id, None)
            raise RuntimeError("QQ.exe 启动后未返回有效 PID")
        model.qq_pid = qq_pid

    def start_async(
        self,
        config: "Config",
        *,
        start_mode: SnowLumaStartMode = SnowLumaStartMode.COLD_START,
        attach_pid: int = 0,
    ) -> tuple[ProcessHandle, "_SnowLumaPhaseCWorker", None]:
        """**异步**启动 SnowLuma Bot (Phase A 主线程同步, Phase C 后台 worker).

        Phase A 主线程: 构造 model + 渲染 onebot.json (file I/O 主线程安全).
        QQ.exe ``start()`` 由 :class:`BotProcessManager` 在接完 signal 后调
        :meth:`_start_phase_a_processes_async`; 这样 manager 可以连早期 stateChanged 信号,
        并且 ``QProcess.waitForStarted`` 不阻塞主线程 (signal-driven 异步; 2026-05-11
        修复用户实测【启动 Bot】点击瞬间 UI 卡顿).

        Phase C 后台 worker: ``daemon.ensure_running()`` → ``client.load_process()``;
        worker.succeeded 在主线程被 manager 接到后, manager 调
        :meth:`_do_phase_d_poller` 启 Poller.

        Returns:
            ``(handle, worker, None)`` —

            - ``handle.primary_process`` = QQ.exe QProcess (COLD) 或 None (HOT)
            - ``handle.secondary_process`` = None (W2 后 daemon 持 node)
            - 第三项 (历史 ``SnowLumaSession``) **现总为 None** — W2 后 daemon 持 password,
              session 不再传给 driver. 保留 3-tuple 仅为不破坏 manager 现有解构.

        Raises:
            FileNotFoundError / RuntimeError: 同 :meth:`start` 的 Phase A 阶段失败.
        """
        model = self._do_phase_a(config, start_mode, attach_pid)
        # onebot_<uin>.json 在 Phase A (主线程, 同步 file I/O) 渲染;
        # daemon 的 runtime.json / webui.json 在 daemon spawn 时由 daemon 自己渲染.
        self._render_onebot_config(config)
        handle = ProcessHandle(
            qq_id=model.qq_id,
            primary_process=model.qq_process,
            secondary_process=None,
        )
        worker = _SnowLumaPhaseCWorker(self, model, self._get_daemon())
        return handle, worker, None

    # ==================== 内部: Phase A / C / D 拆分 ====================
    def _do_phase_a(
        self,
        config: "Config",
        start_mode: SnowLumaStartMode,
        attach_pid: int,
    ) -> SnowLumaProcessModel:
        """构造 model + (COLD) 构造 QQ.exe QProcess (不 ``start()``); 注册到字典.

        **W2 删除**: 一期硬限制 ``if self._processes: raise RuntimeError(...)`` 整段去掉,
        允许多 Bot 共存于同一 daemon.

        Raises:
            RuntimeError: HOT_START attach_pid 无效 / 进程不存在.
            FileNotFoundError: COLD 模式下 ``QQ.exe`` 缺失; 或任何模式下 ``node.exe`` 缺失.
        """
        qq_id = str(config.bot.QQID)
        if qq_id in self._processes:
            # 同 QQID 重复启动: 跟旧版语义对齐 (旧版会用单例守护拒掉; W2 后只拒同 qq_id 重复).
            raise RuntimeError(
                f"SnowLuma Bot QQID={qq_id} 已在跑, 不能重复启动"
            )

        # Q2: HOT 校验先做 fail-fast.
        if start_mode == SnowLumaStartMode.HOT_START:
            if attach_pid <= 0:
                raise RuntimeError(
                    f"热启动模式必须传 attach_pid (>0), 当前为 {attach_pid}; "
                    "请确认 UI 层正确传入了用户选择的 QQ.exe PID"
                )
            try:
                import psutil

                psutil.Process(attach_pid)
            except Exception as exc:  # noqa: BLE001 - psutil / PID 任意异常视为无效
                raise RuntimeError(
                    f"热启动: 目标 QQ.exe (PID={attach_pid}) 不存在或无访问权限: "
                    f"{type(exc).__name__}: {exc}"
                ) from exc

        path_func = it(PathFunc)

        # COLD 模式才需要 QQ.exe 安装路径; HOT 不需要.
        qq_path = path_func.get_qq_path()
        if start_mode == SnowLumaStartMode.COLD_START and qq_path is None:
            raise FileNotFoundError(
                "未检测到 QQ.exe 安装路径, 请先安装 QQ (热启动模式下不需要 QQ 路径, 但需要 QQ 已在运行)"
            )

        # 任何模式下都需要 SnowLuma node.exe 存在 (daemon spawn 用); 早期 fail-fast,
        # 避免 Phase A 成功后到 Phase C 才发现 daemon 起不来.
        node_exe = path_func.get_snowluma_node_executable()
        if node_exe is None:
            raise FileNotFoundError(
                "未检测到 SnowLuma node.exe; 请在组件页 (Component) 的 SnowLuma tab 下先安装 SnowLuma"
            )

        # 构造 QQ.exe QProcess (COLD 模式; 不 start; HOT 模式跳过).
        qq_process: Optional[QProcess] = None
        if start_mode == SnowLumaStartMode.COLD_START:
            assert qq_path is not None
            qq_process = QProcess()
            qq_process.setProgram(str(qq_path / "QQ.exe"))
            # ForwardedChannels: Desktop 不读 QQ stdout, 让 OS 把 stdout 接到 Desktop
            # 自己 stdout 避免 pipe buffer 写满阻塞 QQ.
            qq_process.setProcessChannelMode(
                QProcess.ProcessChannelMode.ForwardedChannels
            )

        model = SnowLumaProcessModel(
            qq_id=qq_id,
            qq_process=qq_process,
            qq_pid=attach_pid if start_mode == SnowLumaStartMode.HOT_START else 0,
            state=QProcess.ProcessState.NotRunning,
            started_at=monotonic(),
        )
        self._processes[qq_id] = model

        mode_label = (
            "热启动 (附加现有 QQ.exe)"
            if start_mode == SnowLumaStartMode.HOT_START
            else "冷启动 (spawn 新 QQ.exe)"
        )
        logger.info(
            (
                f"SnowLuma Phase A 模型已构造 (QQID: {qq_id}, mode={mode_label}, "
                f"qq_pid={model.qq_pid or '(待 spawn)'}, snowluma_path={path_func.snowluma_path})"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        return model

    def _start_phase_a_processes_async(
        self,
        model: SnowLumaProcessModel,
        on_started: "Callable[[SnowLumaProcessModel], None]",
    ) -> None:
        """启动 QQ.exe QProcess **不阻塞主线程**, 在 ``started`` 信号触发时调 ``on_started``.

        2026-05-11 主线程卡顿修复 (用户实测点【启动 Bot】瞬间卡): 旧同步版本用
        ``qq_process.waitForStarted(5000)`` 在主线程阻塞最多 5 秒等 OS 启动 QQ.exe,
        即便 QQ 启动快 (~200-1000ms) UI 主线程也会卡顿明显. NapCat 路径
        (:meth:`_start_local_napcat`) 早就在 P3 perf 阶段改成 Qt signal-driven, 本函数
        是 SnowLuma 路径的对齐改造.

        新流程 (signal-driven):

        1. **COLD 模式** (``model.qq_process is not None``):
           - ``qq_process.start()`` — Qt 内部把 OS ``CreateProcess`` 派发到 Qt 事件循环
             下个 tick, 本调用立即返回, 不阻塞.
           - 连 ``started`` 信号 (one-shot via 内部 disconnect): 信号到时拿
             ``processId()`` 填 ``model.qq_pid`` 并调 ``on_started(model)``.
           - **失败处理由 manager 接管**: ``QProcess.errorOccurred(FailedToStart)``
             已由 :meth:`BotProcessManager._handle_local_start_error` 连过, 触发时清
             driver 字典 + 释放 daemon ref + emit ``NotRunning``. 本函数不重复处理.
        2. **HOT 模式** (``model.qq_process is None``): 直接同步调 ``on_started(model)``
           — PID 已在 :meth:`_do_phase_a` 用 ``attach_pid`` 填好, 没有需要等的 OS 启动事件.

        必须主线程调用. 调用方应已连接好 ``stateChanged`` / ``errorOccurred`` /
        ``finished`` 信号 (见 manager ``_start_local_snowluma`` line 1888+), 否则会
        丢失启动期早期事件.

        Args:
            model: ``_do_phase_a`` 已注册到 ``self._processes`` 的 process model.
            on_started: ``QProcess.started`` 信号触发后回调 (manager 在此推进 Phase C
                worker). HOT 模式同步直调. 失败路径不会调本回调.
        """
        qq_process = model.qq_process
        if qq_process is None:
            # HOT 模式: nothing to start; model.qq_pid 已在 _do_phase_a 用 attach_pid 填好.
            logger.info(
                f"SnowLuma Phase A start 跳过 (HOT 模式, QQID: {model.qq_id}, "
                f"attach_qq_pid={model.qq_pid})",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            on_started(model)
            return

        # COLD 模式: signal-driven 异步等 OS 启动 QQ.exe.
        def _on_qq_started() -> None:
            # 断开自身防止重连 (理论上 started 信号 process 周期内只 emit 一次, 防御性处理)
            try:
                qq_process.started.disconnect(_on_qq_started)
            except (RuntimeError, TypeError):
                pass

            qq_pid = qq_process.processId()
            if qq_pid <= 0:
                # started 信号已 emit 但 processId 异常 (Qt 内部 race / OS handle 异常),
                # 让 manager 通过后续 errorOccurred / finished 路径清理, 本回调仅日志.
                logger.error(
                    f"SnowLuma Phase A: started 信号已 emit 但 processId<=0 "
                    f"(QQID: {model.qq_id}, qq_pid={qq_pid}); 等待 errorOccurred/finished 清理",
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )
                return

            model.qq_pid = qq_pid
            logger.info(
                (
                    f"SnowLuma Phase A (冷启动): QQ.exe 已起 (QQID: {model.qq_id}, "
                    f"qq_pid={model.qq_pid})"
                ),
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            on_started(model)

        qq_process.started.connect(_on_qq_started)
        qq_process.start()
        model.state = QProcess.ProcessState.Starting

    def _do_phase_c_inject(
        self,
        model: SnowLumaProcessModel,
        client: SnowLumaWebUIClient,
    ) -> None:
        """Phase C: 调 ``client.load_process(qq_pid)`` 把当前 Bot 注入 daemon.

        任意线程可调 (HTTP 调用, 不碰 QProcess). 失败时 raise, 由调用方在主线程清理.

        Raises:
            RuntimeError: API 调用失败 / 注入返回 status="error".
        """
        try:
            info = client.load_process(model.qq_pid)
        except SnowLumaWebUIError as exc:
            raise RuntimeError(f"SnowLuma 注入 API 调用失败: {exc.message}") from exc

        if info.status == "error":
            raise RuntimeError(
                f"SnowLuma 注入失败: {info.error or '<no error message>'}"
            )

        logger.info(
            (
                f"SnowLuma Phase C: inject loaded for pid={model.qq_pid} "
                f"(QQID: {model.qq_id}, hook_status={info.status})"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )

    def _do_phase_d_poller(
        self,
        model: SnowLumaProcessModel,
        client: SnowLumaWebUIClient,
    ) -> "SnowLumaStatusPoller":
        """Phase D: 启动 :class:`SnowLumaStatusPoller`. **主线程**调用.

        :class:`QTimer` 创建必须在 event-loop 线程; manager 在 ``succeeded`` 回调 (主线程)
        触发本方法.
        """
        # 延迟导入避免循环 (poller 依赖 SnowLumaWebUIClient, 已在顶层导入)
        from src.core.runtime.snowluma_status_poller import SnowLumaStatusPoller

        poller = SnowLumaStatusPoller(
            qq_id=model.qq_id,
            initial_pid=model.qq_pid,  # W3: 仅首次 UIN 探测使用; 后续按 UIN 聚合
            webui_client=client,
        )
        self.attach_poller(model.qq_id, poller)
        poller.start()
        model.state = QProcess.ProcessState.Running

        logger.info(
            f"SnowLuma Phase D: poller started (QQID: {model.qq_id})",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        return poller

    def _abort_start(self, model: SnowLumaProcessModel) -> None:
        """启动失败兜底清理 (主线程亲和): kill QQ.exe (COLD) + release daemon ref + pop 字典.

        与 :meth:`stop` 的区别: ``stop`` 假设 daemon 已 ``READY`` (走 unload HTTP);
        ``_abort_start`` 处理 Phase C 失败的场景 — daemon 可能 STOPPED / STARTING /
        CRASHED, 不能依赖 ``daemon.webui_client()`` 可用, 也不发 unload.
        """
        if model.qq_process is not None:
            terminate_async(model.qq_process)
        # 释放 daemon ref (即使 ensure_running 没成功也已经 ref_count += 1 然后回滚到 0;
        # 但 release 是幂等的, 多调一次也 no-op).
        try:
            self._get_daemon().release()
        except Exception as exc:  # noqa: BLE001 - 启动失败兜底吞所有异常
            logger.warning(
                f"SnowLumaDaemon release (abort) 静默忽略: {type(exc).__name__}: {exc}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
        self._processes.pop(model.qq_id, None)

    # ==================== BotBackendDriver: stop ====================
    def stop(self, qq_id: str) -> None:
        """停止指定 Bot: unload (HTTP fire-and-forget) → kill QQ.exe (COLD) → daemon.release.

        - **daemon 的 node.exe** 由 daemon 自己引用计数, ref_count 归 0 时回收;
          本方法不再 kill node.
        - **HOT 模式** 下 ``model.qq_process is None``, ``terminate_async(None)`` no-op,
          不动用户 QQ.exe.
        - **幂等**: 对未在跑的 qq_id 静默返回.
        """
        model = self._processes.get(qq_id)
        if model is None:
            logger.warning(
                f"尝试停止不存在的 SnowLuma Bot (QQID: {qq_id})",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            return

        daemon = self._get_daemon()

        # 1. fire-and-forget unload 注入 (走 daemon 的 client; daemon 未就绪时跳过).
        try:
            client = daemon.webui_client()
        except RuntimeError:
            client = None
        if client is not None and model.qq_pid > 0:
            http_worker = _SnowLumaStopHttpWorker(
                webui_client=client,
                qq_pid=model.qq_pid,
            )
            QThreadPool.globalInstance().start(http_worker)

        # 2. stop poller (异常静默忽略)
        poller = self.detach_poller(qq_id)
        if poller is not None:
            try:
                poller.stop()
                poller.deleteLater()
            except Exception:  # noqa: BLE001
                pass

        # 3. kill QQ.exe (COLD only; HOT 模式 qq_process 为 None, no-op).
        if model.qq_process is not None:
            terminate_async(model.qq_process)

        # 4. 字典清理 (放在 daemon.release 之前避免 release 触发 shutdown 时
        #    manager 的 finished 信号回调还能看到 model)
        self._processes.pop(qq_id, None)

        # 5. daemon.release; 若本 Bot 是最后一个, daemon 自行 shutdown.
        try:
            daemon.release()
        except Exception as exc:  # noqa: BLE001 - stop 路径吞所有异常确保单 Bot 失败不影响其他
            logger.warning(
                f"SnowLumaDaemon release (stop) 静默忽略: {type(exc).__name__}: {exc}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )

        logger.info(
            (
                f"SnowLuma Bot 已停止 (QQID: {qq_id}; "
                "unload 后台 fire-and-forget → kill QQ.exe (COLD) → daemon.release)"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )

    # ==================== BotBackendDriver: 状态查询 ====================
    def is_running(self, qq_id: str) -> bool:
        """探测 SnowLuma Bot 是否在跑.

        W2 后**不再**依赖 node.exe 状态 (daemon 全局共享); 仅看 model 是否存在.
        ``state == Running`` 由 Phase D 写入, 由 ``finished`` 信号写回 ``NotRunning``.
        """
        model = self._processes.get(qq_id)
        if model is None:
            return False
        return model.state == QProcess.ProcessState.Running

    def get_status_poller(self, qq_id: str) -> "SnowLumaStatusPoller | None":
        """返回 SnowLuma 路径的状态轮询器实例; 不存在时返回 ``None``."""
        return self._pollers.get(qq_id)

    # ==================== 内部访问器 (供 BotProcessManager 使用) ====================
    def get_process_model(self, qq_id: str) -> SnowLumaProcessModel | None:
        """返回指定 QQ 号的 SnowLuma 进程模型, 不存在时为 ``None``."""
        return self._processes.get(qq_id)

    def list_processes(self) -> list[SnowLumaProcessModel]:
        """返回当前所有 SnowLuma 进程模型快照."""
        return list(self._processes.values())

    def remove_process_model(self, qq_id: str) -> SnowLumaProcessModel | None:
        """从字典移除指定 QQ 号的进程模型; 由 manager 在 finished 回调里调用."""
        return self._processes.pop(qq_id, None)

    def attach_poller(self, qq_id: str, poller: "SnowLumaStatusPoller") -> None:
        """将 :class:`SnowLumaStatusPoller` 实例挂到 driver 上."""
        existing = self._pollers.pop(qq_id, None)
        if existing is not None:
            try:
                existing.stop()
                existing.deleteLater()
            except Exception:  # noqa: BLE001
                pass
        self._pollers[qq_id] = poller

    def detach_poller(self, qq_id: str) -> "SnowLumaStatusPoller | None":
        """从 driver 取下 poller, 由调用方负责 stop / deleteLater."""
        return self._pollers.pop(qq_id, None)

    # ==================== 内部: 配置渲染 ====================
    def _render_onebot_config(self, config: "Config") -> None:
        """渲染 ``onebot_<uin>.json`` (每 Bot 启动前一次, file I/O 主线程安全).

        W2 删除: ``runtime.json`` / ``webui.json`` 渲染 (daemon 启动时由
        :func:`render_daemon_globals` 统一渲染).
        """
        from src.core.runtime.snowluma_config_renderer import render_onebot_json

        snowluma_path = it(PathFunc).snowluma_path
        render_onebot_json(
            snowluma_path,
            int(config.bot.QQID),
            connect=config.connect,
            music_sign_url=config.bot.musicSignUrl,
        )


# ==================== Phase C 后台 worker (W2: 去掉 Phase B) ====================
class _SnowLumaPhaseCWorker(QObject, QRunnable):
    """SnowLuma Phase C 后台 worker (W2: 只剩注入, daemon 接管 WebUI ready + login).

    职责:

    1. ``daemon.ensure_running(timeout=55s)`` — 同步阻塞 worker 线程, 让 daemon 首次启动
       (含 wait_ready 30s + login + 缓冲); daemon 已 READY 则立即返回 client + ref_count +1.
    2. ``client.load_process(qq_pid)`` — HTTP 调用 inject.
    3. emit ``succeeded(client)`` / ``failed(message)`` 回主线程.

    Signals:
        succeeded: ``(client,)`` — Phase C 成功; manager 在主线程接到后调
            :meth:`SnowLumaDriver._do_phase_d_poller` 启 Poller, emit ``Running``.
        failed: ``(error_message,)`` — manager 接到后 emit error 通知 + 清理.
    """

    succeeded = Signal(object)  # SnowLumaWebUIClient
    failed = Signal(str)

    def __init__(
        self,
        driver: "SnowLumaDriver",
        model: SnowLumaProcessModel,
        daemon: SnowLumaDaemon,
    ) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self._driver = driver
        self._model = model
        self._daemon = daemon
        # 不让 QThreadPool 自动 delete; 由 manager 在 succeeded/failed 槽里 deleteLater.
        self.setAutoDelete(False)

    def run(self) -> None:  # noqa: D401 - QRunnable 协议
        try:
            client = self._daemon.ensure_running(timeout=_PHASE_C_TIMEOUT_S)
            self._driver._do_phase_c_inject(self._model, client)
            self.succeeded.emit(client)
        except Exception as exc:  # noqa: BLE001 - worker 边界统一上报
            logger.warning(
                (
                    f"SnowLuma Phase C 失败 (QQID: {self._model.qq_id}): "
                    f"{type(exc).__name__}: {exc}"
                ),
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.failed.emit(str(exc))


# ==================== Stop bot HTTP 阶段后台 worker (W2 沿用) ====================
class _SnowLumaStopHttpWorker(QRunnable):
    """SnowLuma stop bot 的 HTTP unload 阶段后台执行 worker (fire-and-forget).

    W2 后 unload 走 daemon 的共享 ``SnowLumaWebUIClient`` (一个 client 注入/卸载多个 PID
    都没问题, 上游 ``hookManager.detachPid`` 是按 pid 寻找的). 把 unload HTTP (≤5s timeout)
    甩到 :class:`QThreadPool` 后台 — 主线程不等结果, stop bot UI 立即响应:

    - ``unload`` 失败 → 反正 manager 后面 ``terminate_async(QQ.exe)`` 也会让 SnowLuma 自己
      在下次 tickWatcher 时 detachPid, 无副作用.

    本 worker 不 emit signal, 跑完自动 GC (``setAutoDelete(True)``).
    """

    def __init__(self, webui_client: SnowLumaWebUIClient, qq_pid: int) -> None:
        QRunnable.__init__(self)
        self._webui_client = webui_client
        self._qq_pid = qq_pid
        self.setAutoDelete(True)

    def run(self) -> None:  # noqa: D401 - QRunnable 协议
        if self._qq_pid <= 0:
            return
        try:
            self._webui_client.unload_process(self._qq_pid)
            logger.trace(
                f"SnowLuma unload OK (qq_pid={self._qq_pid})",
                LogType.NETWORK,
                LogSource.CORE,
            )
        except SnowLumaWebUIError as exc:
            logger.trace(
                f"SnowLuma unload 静默忽略 (qq_pid={self._qq_pid}): {exc.message}",
                LogType.NETWORK,
                LogSource.CORE,
            )
        except Exception as exc:  # noqa: BLE001 - worker 不能让任何异常逃逸到 QThreadPool
            logger.trace(
                f"SnowLuma unload 未知异常 (qq_pid={self._qq_pid}): "
                f"{type(exc).__name__}: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )

# -*- coding: utf-8 -*-
"""SnowLuma 登录态轮询器 (W3 重写: 按 UIN 聚合, 支持同 UIN 多 PID).

P1/W5 旧版按 ``qq_pid`` filter ``/api/processes`` 命中单条记录, 在 Windows QQ.exe
Electron 多 main PID 场景下会漏数据 (注入完成的实际登录 PID 与 driver 启动的 spawn PID
不一定是同一个). W3 改造对齐上游
``packages/core/src/bridge/manager.ts:22-26`` 的 ``sessions_: Map<uin, QQSession>``
拓扑: poller 锁定到 **UIN**, 同 UIN 任意 PID 状态变化都被纳入合成.

状态合成 (W3 集合语义, 等价旧版 ``_STATUS_TRANSLATION_TABLE`` 单条映射 + 多源择优):

- 任一 PID ``online`` → ``logged_in``
- 否则有 ``loaded`` → ``waiting_for_qr_scan`` (扫码态)
- 否则有任一 ``available`` / ``loading`` / ``connecting`` → ``starting`` (启动期)
- 否则非空且全是 ``error`` / ``disconnected`` → ``disconnected``
- ``matched`` 为空 (即 UIN 未在 ``/api/processes`` 出现) 但 ``/api/qq-list`` 含本 UIN
  → fallback ``logged_in`` (与 W7 旧版双源 fallback 等价, 修复 Windows
  ``getAllMainProcess()`` 返回空场景)

新信号 `pid_set_changed`: ``(qq_id, list[int])`` — 当本 UIN 关联的 PID 集合变化时
emit (含 watcher 自动发现的同 UIN 其他 PID); manager (W7) 接此信号回写
``SnowLumaProcessModel.ancillary_pids``.

参见: ``docs/requirements/2026-05-11-snowluma-daemon-refactor.md`` §2.3,
``docs/plans/2026-05-11-snowluma-daemon-refactor-execution-plan.md`` §W3.
"""
from __future__ import annotations

from typing import TYPE_CHECKING, Final

import psutil
from PySide6.QtCore import QObject, QRunnable, QThreadPool, QTimer, Signal

from src.core.logging import LogSource, LogType, logger
from src.core.runtime.snowluma_webui_client import SnowLumaWebUIClient, SnowLumaWebUIError

if TYPE_CHECKING:
    from src.core.runtime.snowluma_webui_client import HookProcessInfo


# ==================== 状态名 (字符串常量, 与 BotCard 消费端协议) ====================
SNOWLUMA_STATE_STARTING: Final[str] = "starting"
SNOWLUMA_STATE_LOGGED_IN: Final[str] = "logged_in"
SNOWLUMA_STATE_WAITING_FOR_QR_SCAN: Final[str] = "waiting_for_qr_scan"
SNOWLUMA_STATE_DISCONNECTED: Final[str] = "disconnected"


# ==================== 上游 isRealUin 规则对齐 ====================
def _is_real_uin(uin: str) -> bool:
    """上游 ``packages/core/src/hook/hook-manager.ts:511-514`` ``isRealUin``:
    非空 + 非 ``"0"`` + 全数字 + 长度 ≥ 5.

    抽模块级函数复用; W3 之前散落在 ``_on_processes`` 内部.
    """
    cleaned = (uin or "").strip()
    return bool(cleaned) and cleaned != "0" and cleaned.isdigit() and len(cleaned) >= 5


def _collect_candidate_pids(initial_pid: int) -> set[int]:
    """``initial_pid`` + psutil 后代 PID 集合 (模块级辅助, 供 ``_ListProcessesRunnable``
    在工作线程调用以避免阻塞主线程).

    2026-05-11 bugfix 背景: QQ.exe 是 Electron 多子进程架构, Desktop spawn 的 PID
    是 launcher (Electron parent), 实际 SnowLuma hook 注入到的是某个 main 子进程.
    ``initial_pid`` 与 ``HookProcessInfo.pid`` 不一定相等; 直接 ``p.pid == initial_pid``
    筛选会漏匹配, 触发后续 ``qq_instances[0]`` fallback, 多 Bot 场景下拿到别 Bot 的 UIN.

    本函数用 ``psutil`` walk ``initial_pid`` 的子进程树, 把所有后代 PID 也纳入候选,
    让 ``processes`` 中我们 spawn / attach 的整棵 QQ.exe 进程树都能被识别.

    **线程安全提示**: ``children(recursive=True)`` 在 Windows 大进程树场景可能耗时数十
    毫秒到秒级 (用户实测热启动会卡顿), 必须在工作线程调用. 主线程禁止直调本函数.

    psutil 失败 (进程已退出 / 权限不足) 静默回退到只含 ``initial_pid`` 的集合.
    """
    candidates: set[int] = {initial_pid}
    try:
        proc = psutil.Process(initial_pid)
        for child in proc.children(recursive=True):
            try:
                candidates.add(child.pid)
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                continue
    except (psutil.NoSuchProcess, psutil.AccessDenied):
        pass
    return candidates


class _ListProcessesRunnable(QRunnable, QObject):
    """单次 ``list_processes`` + ``list_qq_instances`` + psutil 子进程树 walk 后台任务.

    跑在 :class:`QThreadPool` 后台线程, 避免主线程因 SnowLuma WebUI 抖动 + psutil
    进程树枚举卡 UI; 完成后通过 Qt 信号回到主线程派发到
    :meth:`SnowLumaStatusPoller._on_processes`.

    工作内容 (按顺序):

    1. ``list_processes`` (HTTP) — daemon ``/api/processes`` 拿当前 hook 的 QQ.exe 列表.
    2. ``list_qq_instances`` (HTTP, 失败静默) — ``/api/qq-list`` 拿 OneBot 实例 (UIN 备用源).
    3. **2026-05-11 新增**: ``_collect_candidate_pids(initial_pid)`` —
       ``psutil`` walk ``initial_pid`` 的子进程树.

       仅在 ``uin_locked=False`` 时执行 (UIN 已锁定后 ``_try_lock_uin`` 不会再被调,
       省下不必要的 psutil 开销). UIN 锁定后 ``processes`` 按 UIN 聚合, 不再依赖 PID 树.
    """

    # signal payload: (processes_list, qq_instances_list, candidate_pids_list).
    # 任一 HTTP 失败时对应项为空 list; candidate_pids 在 ``uin_locked=True`` 时也为空 list.
    processes_signal = Signal(object, object, object)
    error_signal = Signal(str)  # 错误描述

    def __init__(
        self,
        webui_client: SnowLumaWebUIClient,
        initial_pid: int,
        *,
        uin_locked: bool,
    ) -> None:
        QRunnable.__init__(self)
        QObject.__init__(self)
        self._webui_client = webui_client
        self._initial_pid = initial_pid
        self._uin_locked = uin_locked
        # 默认 QRunnable 跑完即销毁; 但我们继承了 QObject 用于 emit 信号,
        # 由调用方负责 deleteLater (见 SnowLumaStatusPoller._tick).
        self.setAutoDelete(False)

    def run(self) -> None:  # noqa: D401 - QRunnable 协议
        try:
            try:
                processes = self._webui_client.list_processes()
            except SnowLumaWebUIError as exc:
                logger.trace(
                    f"SnowLuma list_processes 失败 (status={exc.status_code}): {exc.message}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                self.error_signal.emit(exc.message)
                return
            except Exception as exc:  # noqa: BLE001 - SSH / 网络抖动等不应停轮询
                logger.trace(
                    f"SnowLuma list_processes 未知异常: {type(exc).__name__}: {exc}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                self.error_signal.emit(f"{type(exc).__name__}: {exc}")
                return

            # qq-list 失败不致命 (老版 SL 可能没这个端点), 静默 fallback 到空 list.
            try:
                qq_instances = self._webui_client.list_qq_instances()
            except Exception as exc:  # noqa: BLE001
                logger.trace(
                    f"SnowLuma list_qq_instances 静默忽略: {type(exc).__name__}: {exc}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                qq_instances = []

            # psutil 子进程树 walk (主线程禁止直调; 这里在工作线程跑安全).
            # UIN 已锁定后无需此数据 (slot 不会调 _try_lock_uin), 跳过省开销.
            candidate_pids: list[int] = (
                sorted(_collect_candidate_pids(self._initial_pid))
                if not self._uin_locked
                else []
            )

            self.processes_signal.emit(processes, qq_instances, candidate_pids)
        except RuntimeError as exc:
            # 动态防护: 后台线程运行中如果 QObject 的 C++ 部分已被主线程 deleteLater 销毁, 静默退出以防 sys.excepthook 触发崩溃
            if "deleted" in str(exc):
                return
            raise


class SnowLumaStatusPoller(QObject):
    """SnowLuma 登录态轮询器 (per-Bot, W3 按 UIN 聚合).

    生命周期由 :class:`BotProcessManager` 管理: SnowLuma Bot 启动时由 driver Phase D
    创建并 ``start()``; Bot 停止 / QProcess 退出时 ``stop()`` + ``deleteLater()``.

    Signals:
        state_changed: ``(qq_id, state_name)`` — ``state_name`` 取值
            ``starting`` / ``waiting_for_qr_scan`` / ``logged_in`` / ``disconnected``.
        uin_detected: ``(qq_id, detected_uin)`` — 注入完成后第一次拿到真实 UIN
            (满足 :func:`_is_real_uin` 规则) 时触发**一次**. manager (Q2) 接到后对比
            ``config.bot.QQID`` 与 ``detected_uin``, 不一致即 emit error + stop bot.
        pid_set_changed (**W3 新增**): ``(qq_id, list[int])`` — 本 UIN 关联的 PID
            集合 (升序) 变化时 emit. manager (W7) 据此回写
            ``SnowLumaProcessModel.ancillary_pids``.
    """

    state_changed = Signal(str, str)
    uin_detected = Signal(str, str)
    # W3 新增: PID 集合变化通知 (manager 写回 ancillary_pids)
    pid_set_changed = Signal(str, list)

    # 注入期需要更快感知; 从 P1 5s 调到 2s.
    _POLL_INTERVAL_MS: Final[int] = 2000
    # 连续失败阈值: 超过即 emit ``disconnected`` + 停止 emit 进一步状态.
    _MAX_CONSECUTIVE_FAILURES: Final[int] = 3

    def __init__(
        self,
        qq_id: str,
        initial_pid: int,
        webui_client: SnowLumaWebUIClient,
        parent: QObject | None = None,
    ) -> None:
        """初始化 poller.

        Args:
            qq_id: Bot 配置的 QQ 号 (字符串, 与 driver 字典 key 对齐).
            initial_pid: 首次 UIN 探测用的 PID (一般是 driver Phase A spawn 或 attach 的
                那个). UIN 锁定后此 PID 不再用作 filter, 仅用于第一次"哪条 hook_info 是
                我们"的判定.
            webui_client: daemon 的共享 :class:`SnowLumaWebUIClient` 实例.
            parent: 可选 Qt 父对象.
        """
        super().__init__(parent)
        self._qq_id = qq_id
        self._initial_pid = initial_pid
        self._webui_client = webui_client
        self._consecutive_failures = 0
        self._last_state: str | None = None
        self._disposed = False
        self._in_flight = False
        # 当前正在 QThreadPool 里跑的 runnable; ``_tick`` 提交时写入,
        # 槽函数入口或 ``stop`` 时统一通过 :meth:`_dispose_in_flight_runnable`
        # disconnect + deleteLater 释放. 这样即便后台 runnable 在 poller stop 之后
        # 才跑完 emit 信号, 槽连接已被断开, 不会再触达已 deleteLater 的 poller 槽,
        # 避免 "RuntimeError: Signal source has been deleted".
        self._in_flight_runnable: "_ListProcessesRunnable | None" = None

        # W3 新增: UIN 锁 + PID 集合
        self._uin: str = ""
        self._last_pid_set: set[int] = set()

        self._timer = QTimer(self)
        self._timer.setInterval(self._POLL_INTERVAL_MS)
        self._timer.timeout.connect(self._tick)

    @property
    def qq_id(self) -> str:
        return self._qq_id

    @property
    def uin(self) -> str:
        """本 poller 已锁定的 UIN (尚未锁定时为空字符串).

        UIN 一旦由 :meth:`_on_processes` 探测出并通过 :func:`_is_real_uin`, 则锁定
        后续 tick 不再变化; 暴露此 property 供 manager / 诊断使用.
        """
        return self._uin

    def start(self) -> None:
        """启动轮询. 立即跑一次, 之后按 ``_POLL_INTERVAL_MS`` 周期重试."""
        if self._disposed:
            return
        if self._timer.isActive():
            return
        logger.trace(
            (
                f"SnowLumaStatusPoller 启动 (QQID: {self._qq_id}, "
                f"initial_pid={self._initial_pid})"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )
        self._timer.start()
        # 立即触发一次, 不等首个 tick (注入期 2s 都嫌长).
        QTimer.singleShot(500, self._tick)

    def stop(self) -> None:
        """停止轮询并 emit 一次 ``disconnected`` 让 UI 退出登录态视图."""
        if self._disposed:
            return
        self._disposed = True
        self._timer.stop()
        self._in_flight = False
        self._dispose_in_flight_runnable()
        logger.trace(
            f"SnowLumaStatusPoller 停止 (QQID: {self._qq_id})",
            LogType.NETWORK,
            LogSource.CORE,
        )
        self.state_changed.emit(self._qq_id, SNOWLUMA_STATE_DISCONNECTED)

    # ==================== 内部 ====================
    def _tick(self) -> None:
        if self._disposed:
            return
        # 单飞保护: 上次请求未完成时不再排队 (避免子线程池堆积)
        if self._in_flight:
            logger.trace(
                f"SnowLumaStatusPoller _tick 跳过 (QQID: {self._qq_id}, 上次请求未完成)",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return
        self._in_flight = True

        logger.trace(
            (
                f"SnowLumaStatusPoller _tick 发起 list_processes "
                f"(QQID: {self._qq_id}, initial_pid={self._initial_pid}, uin={self._uin or '<unknown>'})"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )
        runnable = _ListProcessesRunnable(
            self._webui_client,
            initial_pid=self._initial_pid,
            uin_locked=bool(self._uin),
        )
        runnable.processes_signal.connect(self._on_processes)
        runnable.error_signal.connect(self._on_error)
        self._in_flight_runnable = runnable
        QThreadPool.globalInstance().start(runnable)

    def _on_processes(
        self,
        processes: object,
        qq_instances: object = None,
        candidate_pids: object = None,
    ) -> None:
        """收到 list_processes + list_qq_instances 的双源数据 (W3 重写: 按 UIN 聚合).

        步骤:

        1. **UIN 探测** (仅 ``self._uin == ""`` 时跑): 用 ``initial_pid`` 找匹配的
           ``HookProcessInfo``; 若拿不到, fallback ``qq_instances[0].uin``. 命中
           :func:`_is_real_uin` 后锁定 ``self._uin`` 并 emit ``uin_detected``.
        2. **状态合成** (按 ``self._uin`` 聚合 ``matched`` PID): 集合语义见类
           docstring. ``matched`` 为空且 qq_instances 含本 UIN → fallback ``logged_in``.
        3. **PID 集合变化通知** (新): 若 ``{p.pid for p in matched}`` 与上次不同, emit
           ``pid_set_changed`` (升序 list payload).
        """
        # 注意: 即使本对象已 dispose, 也允许这次回调正常处理 in_flight 标记; 之后 emit 检查.
        self._in_flight = False
        self._dispose_in_flight_runnable()
        if self._disposed:
            return

        if not isinstance(processes, list):
            logger.trace(
                (
                    f"SnowLumaStatusPoller _on_processes 收到非 list 类型 "
                    f"(QQID: {self._qq_id}): {type(processes).__name__}"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            return
        if not isinstance(qq_instances, list):
            qq_instances = []

        # 重置失败计数 (任何一次成功响应)
        self._consecutive_failures = 0

        # 诊断日志: 记录两源关键数据
        found_pids = [getattr(p, "pid", "?") for p in processes]
        qq_uins = [getattr(q, "uin", "?") for q in qq_instances]
        logger.trace(
            (
                f"SnowLumaStatusPoller 双源轮询 (QQID: {self._qq_id}, "
                f"initial_pid={self._initial_pid}, uin={self._uin or '<unknown>'}, "
                f"processes_pids={found_pids}, qq_list_uins={qq_uins})"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

        # ========== 1. UIN 探测 (仅首次) ==========
        # candidate_pids 由 ``_ListProcessesRunnable`` 在工作线程预算 (避免 psutil
        # 在主线程卡 UI). 测试 / 直接调本方法的路径若没传, fallback 到 ``{initial_pid}``,
        # 不阻塞但牺牲 Electron 子进程匹配能力 (psutil 已禁止主线程直调).
        if not self._uin:
            if isinstance(candidate_pids, (list, tuple, set)):
                candidate_set = {int(p) for p in candidate_pids}
            else:
                candidate_set = {self._initial_pid}
            self._try_lock_uin(processes, qq_instances, candidate_set)

        # ========== 2. 按 UIN 聚合状态 ==========
        matched: list["HookProcessInfo"] = (
            [p for p in processes if getattr(p, "uin", "") == self._uin]
            if self._uin
            else []
        )
        new_state = self._synthesize_state(matched, qq_instances)

        if new_state is not None and new_state != self._last_state:
            logger.trace(
                (
                    f"SnowLuma 状态更新 (QQID: {self._qq_id}, uin={self._uin}, "
                    f"matched_pids={[p.pid for p in matched]}, desktop_state={new_state})"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            self.state_changed.emit(self._qq_id, new_state)
            self._last_state = new_state

        # ========== 3. PID 集合变化 (W3 新增) ==========
        new_pid_set = {p.pid for p in matched}
        if new_pid_set != self._last_pid_set:
            self.pid_set_changed.emit(self._qq_id, sorted(new_pid_set))
            self._last_pid_set = new_pid_set
            logger.trace(
                (
                    f"SnowLuma PID set 变化 (QQID: {self._qq_id}, uin={self._uin}, "
                    f"new_pid_set={sorted(new_pid_set)})"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )

    def _try_lock_uin(
        self,
        processes: list,
        qq_instances: list,
        candidate_pids: set[int],
    ) -> None:
        """首次 UIN 探测 (2026-05-11 多 Bot 误匹配修复后的严格策略).

        策略:

        1. 用 ``candidate_pids`` (由 ``_ListProcessesRunnable`` 工作线程预算的
           ``initial_pid`` + psutil 子进程树) 匹配 ``processes`` 条目;
           命中且 ``uin`` 通过 :func:`_is_real_uin` → 锁定该 UIN.
        2. 仅当 ``processes`` **完全为空** (Windows ``getAllMainProcess()`` 返空场景)
           **且** ``qq_instances`` 恰好 1 条时, 才 fallback 到 ``qq_instances[0].uin``
           (单 Bot 场景安全; 多实例下不可知是哪一个属于本 Bot).
        3. 其他情况 (initial_pid 树不在非空 processes / qq_instances 多条) →
           **不锁定**, 保持 ``self._uin = ""``, 等下次 tick 重试.

        历史 bug (W3 旧版): fallback 直接用 ``qq_instances[0].uin``, 多 Bot 场景下
        会拿到另一个已登录 Bot 的 UIN, 触发 manager 的 UIN 不匹配 → stop bot.
        典型复现: Bot A (QQID=A) 冷启动注入完成前, qq-list 里只有正在跑的 Bot B (QQID=B),
        poller A 错锁 B 的 UIN, 显示 "实际登录账号是 B" 报错.

        Args:
            processes: ``/api/processes`` 返回的 ``HookProcessInfo`` 列表.
            qq_instances: ``/api/qq-list`` 返回的 ``OneBotInstanceInfo`` 列表 (备用源).
            candidate_pids: ``initial_pid`` + 其 psutil 后代 PID 的集合, 由调用方
                (``_ListProcessesRunnable.run`` 工作线程) 预先计算并传入, 主线程不再做
                psutil 调用以避免 UI 卡顿.
        """
        # 优先: processes 里匹 initial_pid 树 (兼容 QQ.exe Electron 多子进程)
        info = next(
            (
                p
                for p in processes
                if hasattr(p, "pid") and p.pid in candidate_pids
            ),
            None,
        )
        uin_from_processes = getattr(info, "uin", "") if info is not None else ""

        chosen_uin = ""
        source = "(none)"
        uin_from_qqlist = ""

        if _is_real_uin(uin_from_processes):
            chosen_uin = uin_from_processes.strip()
            source = "processes (pid_tree)"
        elif not processes and len(qq_instances) == 1:
            # 严格 fallback: 仅 processes 完全空 (Windows enum bug) + 单 instance 时才信任.
            # 多 instance 时不可知哪个是我们 (qq_instances 没有 pid 字段对照),
            # 直接放弃锁定避免 cross-Bot 误匹配.
            uin_from_qqlist = qq_instances[0].uin or ""
            if _is_real_uin(uin_from_qqlist):
                chosen_uin = uin_from_qqlist.strip()
                source = "qq-list (single instance, windows enum fallback)"

        logger.trace(
            (
                f"SnowLumaStatusPoller UIN 探测 (QQID: {self._qq_id}, "
                f"initial_pid={self._initial_pid}, candidate_pids={sorted(candidate_pids)}, "
                f"uin_processes={uin_from_processes!r}, uin_qqlist={uin_from_qqlist!r}, "
                f"qq_instances_count={len(qq_instances)}, processes_count={len(processes)}, "
                f"chosen={chosen_uin!r}, source={source})"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

        if chosen_uin:
            self._uin = chosen_uin
            logger.info(
                (
                    f"SnowLuma UIN 锁定 (QQID: {self._qq_id}, "
                    f"detected_uin={chosen_uin}, source={source})"
                ),
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.uin_detected.emit(self._qq_id, chosen_uin)

    def _synthesize_state(
        self,
        matched: list,
        qq_instances: list,
    ) -> str | None:
        """按 ``matched`` PID 状态集合合成 Desktop 4 档登录态.

        合成规则 (W3 集合语义):

        - 任一 ``online`` → ``logged_in``
        - 否则有 ``loaded`` → ``waiting_for_qr_scan``
        - 否则有 ``available`` / ``loading`` / ``connecting`` → ``starting``
        - 否则非空且全是 ``error`` / ``disconnected`` → ``disconnected``
        - ``matched`` 空但 ``qq_instances`` 含本 UIN → ``logged_in`` (W7 fallback)
        - 其他情况 → ``None`` (不发状态更新, 等下次 tick 或失败阈值触发)

        Returns:
            字符串状态码 (4 档之一) 或 ``None`` (本次无更新).
        """
        statuses = {getattr(p, "status", "") for p in matched}

        if "online" in statuses:
            return SNOWLUMA_STATE_LOGGED_IN
        if "loaded" in statuses:
            return SNOWLUMA_STATE_WAITING_FOR_QR_SCAN
        if statuses & {"available", "loading", "connecting"}:
            return SNOWLUMA_STATE_STARTING
        if statuses and statuses <= {"error", "disconnected"}:
            return SNOWLUMA_STATE_DISCONNECTED

        # matched 为空 / 状态都不属于上面 case:
        # 若 qq_instances 含本 UIN, fallback logged_in (W7 双源 fallback).
        if self._uin and any(
            getattr(q, "uin", "") == self._uin for q in qq_instances
        ):
            return SNOWLUMA_STATE_LOGGED_IN

        # 真的没数据, 不发状态; 由失败阈值或下次 tick 接管.
        return None

    def _on_error(self, _message: str) -> None:
        """list_processes 调用失败的回调."""
        self._in_flight = False
        self._dispose_in_flight_runnable()
        if self._disposed:
            return

        self._consecutive_failures += 1
        if self._consecutive_failures >= self._MAX_CONSECUTIVE_FAILURES:
            logger.warning(
                (
                    "SnowLuma WebUI 连续 "
                    f"{self._consecutive_failures} 次调用失败 (QQID: {self._qq_id}); "
                    "emit disconnected"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            if self._last_state != SNOWLUMA_STATE_DISCONNECTED:
                self.state_changed.emit(self._qq_id, SNOWLUMA_STATE_DISCONNECTED)
                self._last_state = SNOWLUMA_STATE_DISCONNECTED

    def _dispose_in_flight_runnable(self) -> None:
        """断开 ``_in_flight_runnable`` 的两个槽连接并 deleteLater 释放.

        在两个时机调用:

        - **runnable 跑完** (``_on_processes`` / ``_on_error`` 入口): 这是正常路径,
          槽连接到此完成使命; 不主动 disconnect 也不会再触发, 但显式 disconnect +
          deleteLater 让 Qt 立即收回 C++ 端, 避免 ``setAutoDelete(False)`` 导致的
          泄漏堆积.
        - **poller 自身 stop**: 此时 runnable 可能仍在 ``QThreadPool`` 后台跑
          (HTTP 未回). 必须先 disconnect — 否则后台跑完 emit 时, queued connection
          会把 signal 投递到主线程, 而 poller 已被上层 ``deleteLater``, PySide6
          抛 ``RuntimeError: Signal source has been deleted``.

        多次调用幂等. 异常静默吞掉 (``deleteLater`` 在已释放对象上调一次也安全).
        """
        runnable = self._in_flight_runnable
        if runnable is None:
            return
        self._in_flight_runnable = None
        try:
            runnable.processes_signal.disconnect(self._on_processes)
        except (RuntimeError, TypeError):
            pass
        try:
            runnable.error_signal.disconnect(self._on_error)
        except (RuntimeError, TypeError):
            pass
        try:
            runnable.deleteLater()
        except RuntimeError:
            pass

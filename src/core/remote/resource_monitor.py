# -*- coding: utf-8 -*-
"""远端资源监控服务 (P4 W2·F3).

核心职责
========

1. 提供 [`ResourceSample`](src/core/remote/resource_monitor.py) 数据载体, 描述远端
   单次采样结果 (CPU / 内存 / 磁盘水位 + load_avg_1).
2. 提供 [`parse_sample_output`](src/core/remote/resource_monitor.py) 解析辅助, 把
   `RemoteBackend.sample_resources()` 推送的 4 行 shell 输出还原为 ``ResourceSample``.
3. 提供 [`ResourceMonitorService`](src/core/remote/resource_monitor.py) 单例,
   按 ``INTERVAL_OK = 10s`` 周期采样; 失败时按 ``INTERVAL_BACKOFF`` 指数退避;
   阈值越界 (CPU > 90% 连续 3 个采样点 / mem > 90% / disk > 90%) 触发
   ``threshold_breached`` 信号, 同 (server_id, metric) 冷却 5 分钟避免轰炸 InfoBar.

设计原则
========

- ``QObject`` + ``Signal`` 信号驱动, 不做任何持久化.
- 真正的 SSH I/O 由 ``RemoteBackend.sample_resources()`` 完成,
  ``ResourceMonitorService`` 仅做调度 / 阈值滑窗 / 冷却簿记.
- 采样 worker 使用 ``threading.Thread`` (daemon=True), 不接 ``QThreadPool``;
  这样单元测试无需 mock Qt 线程层即可断言 attach / detach 状态,
  也避免 ``QRunnable.setAutoDelete`` 在 mock 环境下的析构边界问题.
- worker 内部用 ``threading.Event`` 实现"可中断 sleep", 这样
  ``detach()`` 调用后最多 ``INTERVAL_OK`` 内退出, 不阻塞 UI 关闭.
"""
from __future__ import annotations

# 标准库导入
import re
import threading
import time
from abc import ABC
from dataclasses import dataclass, field
from typing import Any, Callable

# 第三方库导入
from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtCore import QObject, Signal


# ==================== 数据模型 ====================
@dataclass(slots=True, frozen=True)
class ResourceSample:
    """单次远端资源采样.

    Attributes:
        timestamp: 采样时间 (Unix 秒).
        cpu_percent: CPU 总使用率 (0-100).
        mem_percent: 内存使用率 (0-100), used / total.
        disk_percent: ``$HOME`` 所在分区已用率 (0-100).
        load_avg_1: 1 分钟 load average; 解析失败为 None.
        raw: 原始 SSH 输出 4 行 (``CPU`` / ``MEM`` / ``DISK`` / ``LOAD``), 用于调试.
    """

    timestamp: float
    cpu_percent: float
    mem_percent: float
    disk_percent: float
    load_avg_1: float | None
    raw: dict[str, str] = field(default_factory=dict)


# 线上推荐的 sampling oneliner; 实际 RemoteBackend.sample_resources() 走它.
SAMPLE_COMMAND = (
    'echo "CPU $(top -bn1 2>/dev/null | awk \'NR==3{print $2+$4}\')"; '
    'echo "MEM $(free 2>/dev/null | awk \'/Mem/{printf \\"%.1f\\", $3/$2*100}\')"; '
    'echo "DISK $(df -P \\"$HOME\\" 2>/dev/null | awk \'NR==2{print $5}\' | tr -d \\"%\\")"; '
    'echo "LOAD $(awk \'{print $1}\' /proc/loadavg 2>/dev/null)"'
)


_LINE_RE = re.compile(r"^(CPU|MEM|DISK|LOAD)\s+(\S+)\s*$")


def parse_sample_output(stdout: str) -> ResourceSample | None:
    """把 4 行 shell 输出解析为 ``ResourceSample``.

    解析失败 (任一关键字段缺失或非法浮点) 返回 ``None``, 由调用方决定保留上次值.
    """
    if not stdout:
        return None
    raw: dict[str, str] = {}
    for line in stdout.splitlines():
        m = _LINE_RE.match(line.strip())
        if m is None:
            continue
        raw[m.group(1)] = m.group(2)

    def _as_float(key: str) -> float | None:
        value = raw.get(key)
        if value is None or value == "":
            return None
        try:
            return float(value)
        except (TypeError, ValueError):
            return None

    cpu = _as_float("CPU")
    mem = _as_float("MEM")
    disk = _as_float("DISK")
    load = _as_float("LOAD")

    if cpu is None or mem is None or disk is None:
        return None  # 关键字段缺失即整次采样作废

    return ResourceSample(
        timestamp=time.time(),
        cpu_percent=cpu,
        mem_percent=mem,
        disk_percent=disk,
        load_avg_1=load,
        raw=raw,
    )


# ==================== 监控服务 ====================
# Backend 适配回调签名: 接收 server_id, 返回 ResourceSample 或抛异常.
SamplerCallable = Callable[[str], ResourceSample | None]


class _SamplerWorker:
    """单服务器轮询 worker; 由 ``ResourceMonitorService.attach`` 启动.

    使用 ``threading.Thread (daemon=True)`` 实现, 避免 ``QRunnable`` 在测试 mock
    场景下的析构边界问题; 与 Qt 的交互全部走 ``Signal`` (本身线程安全).
    """

    def __init__(
        self,
        server_id: str,
        sampler: SamplerCallable,
        on_sample: Callable[[str, ResourceSample], None],
        on_failure: Callable[[str, Exception], None],
        *,
        interval_ok: float,
        interval_backoff: tuple[float, ...],
    ) -> None:
        self._server_id = server_id
        self._sampler = sampler
        self._on_sample = on_sample
        self._on_failure = on_failure
        self._interval_ok = interval_ok
        self._interval_backoff = interval_backoff
        self._stop_event = threading.Event()
        self._consecutive_failures = 0
        self._thread = threading.Thread(
            target=self._loop,
            name=f"ResourceMonitor[{server_id}]",
            daemon=True,
        )

    @property
    def is_running(self) -> bool:
        return self._thread.is_alive()

    def start(self) -> None:
        if not self._thread.is_alive():
            self._thread.start()

    def stop(self) -> None:
        self._stop_event.set()

    def _loop(self) -> None:
        while not self._stop_event.is_set():
            try:
                sample = self._sampler(self._server_id)
            except Exception as exc:  # noqa: BLE001 - 任意异常都走退避
                self._consecutive_failures += 1
                try:
                    self._on_failure(self._server_id, exc)
                except Exception:  # noqa: BLE001
                    pass
                wait_for = self._next_backoff()
            else:
                if sample is None:
                    self._consecutive_failures += 1
                    wait_for = self._next_backoff()
                else:
                    self._consecutive_failures = 0
                    try:
                        self._on_sample(self._server_id, sample)
                    except Exception:  # noqa: BLE001
                        pass
                    wait_for = self._interval_ok
            # 可中断 sleep, 让 detach() 最多 wait_for 后退出
            self._stop_event.wait(timeout=wait_for)

    def _next_backoff(self) -> float:
        if not self._interval_backoff:
            return self._interval_ok
        idx = min(self._consecutive_failures, len(self._interval_backoff)) - 1
        idx = max(idx, 0)
        return self._interval_backoff[idx]


class ResourceMonitorService(QObject):
    """周期性采样所有附着的服务器 + 阈值越界判定 + 5min 冷却.

    Signals:
        sample_arrived (str, ResourceSample): 采样成功后逐次发射.
        threshold_breached (str, str, float): ``(server_id, metric, value)``;
            metric 取 ``"cpu"`` / ``"mem"`` / ``"disk"``.
        sample_failed (str, str): ``(server_id, error_message)``; UI 可选择性显示.
    """

    sample_arrived = Signal(str, ResourceSample)
    threshold_breached = Signal(str, str, float)
    sample_failed = Signal(str, str)

    INTERVAL_OK: float = 10.0
    # 连续失败次数 → 退避秒数; 超过 len 后取最后一档
    INTERVAL_BACKOFF: tuple[float, ...] = (10.0, 30.0, 60.0)
    BREACH_COOLDOWN: float = 300.0  # 5 分钟
    # CPU 需要连续 3 个采样点 (≈30s) 都超阈值才触发, 减少抖动
    CPU_BREACH_WINDOW: int = 3
    THRESHOLDS: dict[str, float] = {"cpu": 90.0, "mem": 90.0, "disk": 90.0}

    def __init__(self, *, sampler: SamplerCallable | None = None) -> None:
        super().__init__()
        # sampler 默认通过 ServerManager 拉取 RemoteBackend; 测试可注入 mock
        self._sampler: SamplerCallable = sampler or self._default_sampler
        self._workers: dict[str, _SamplerWorker] = {}
        self._latest: dict[str, ResourceSample] = {}
        # 滑窗: server_id -> {metric -> 连续超阈值次数}
        self._cpu_streak: dict[str, int] = {}
        # (server_id, metric) -> 最近一次告警时间戳; 用于冷却
        self._last_breach: dict[tuple[str, str], float] = {}
        # 是否已绑定 ServerManager 信号 (避免重复绑定)
        self._bound_to_server_manager: bool = False

    # ---------- 与 ServerManager 的可选绑定 ----------
    def bind_to_server_manager(self, manager: Any | None = None) -> None:
        """显式订阅 ``ServerManager.server_added`` / ``server_removed`` 信号.

        预期由首个使用方 (`RemoteSummaryCard` / `StatusOverviewDialog`) 在显示前
        调用一次, 之后增删服务器会自动 attach / detach 采样 worker;
        新方法对应已存在的服务器一并 attach.

        不在 ``__init__`` 自动调用是为了避免单元测试 / W1 阶段下意外启动 SSH
        worker (`creart` 单例环引也是同理).
        """
        if self._bound_to_server_manager:
            return
        # 项目内模块导入: 延迟 import 避免 ServerManager 尚未就绪时形成环引
        if manager is None:
            from creart import it

            from src.core.remote.server_manager import ServerManager

            manager = it(ServerManager)
        manager.server_added.connect(self.attach)
        manager.server_removed.connect(self.detach)
        # 已有服务器一并 attach
        for profile in manager.list_servers():
            self.attach(profile.id)
        self._bound_to_server_manager = True

    # ---------- 公共 API ----------
    def attach(self, server_id: str) -> None:
        """启动指定服务器的轮询 worker; 重复 attach 幂等."""
        if server_id in self._workers:
            return
        worker = _SamplerWorker(
            server_id,
            self._sampler,
            on_sample=self._handle_sample,
            on_failure=self._handle_failure,
            interval_ok=self.INTERVAL_OK,
            interval_backoff=self.INTERVAL_BACKOFF,
        )
        self._workers[server_id] = worker
        worker.start()

    def detach(self, server_id: str) -> None:
        """停止指定服务器的轮询; 不存在时静默."""
        worker = self._workers.pop(server_id, None)
        if worker is not None:
            worker.stop()
        # 清理簿记, 让下次 attach 重新计数
        self._cpu_streak.pop(server_id, None)
        self._latest.pop(server_id, None)
        # last_breach 跨 attach 周期保留: 用户连续删除/添加同一服务器不应 5min 内重复告警
        # (产品决策, 非死板规则; 可按需改为同步清理)

    def detach_all(self) -> None:
        """退出 / 清理时调用; 关闭所有 worker."""
        for worker in list(self._workers.values()):
            worker.stop()
        self._workers.clear()
        self._cpu_streak.clear()
        self._latest.clear()

    def latest(self, server_id: str) -> ResourceSample | None:
        """返回该服务器最新一次成功采样, 没有则 None."""
        return self._latest.get(server_id)

    def is_attached(self, server_id: str) -> bool:
        return server_id in self._workers

    # ---------- 内部 ----------
    def _handle_sample(self, server_id: str, sample: ResourceSample) -> None:
        self._latest[server_id] = sample
        self.sample_arrived.emit(server_id, sample)
        self._evaluate_thresholds(server_id, sample)

    def _handle_failure(self, server_id: str, exc: Exception) -> None:
        # 失败不更新 latest, 让 UI 仍展示上一次有效值
        self.sample_failed.emit(server_id, str(exc))

    def _evaluate_thresholds(self, server_id: str, sample: ResourceSample) -> None:
        # CPU 走 N 个连续采样点窗口
        if sample.cpu_percent > self.THRESHOLDS["cpu"]:
            streak = self._cpu_streak.get(server_id, 0) + 1
            self._cpu_streak[server_id] = streak
            if streak >= self.CPU_BREACH_WINDOW:
                self._maybe_emit_breach(server_id, "cpu", sample.cpu_percent)
        else:
            self._cpu_streak[server_id] = 0
        # 内存 / 磁盘单点判定
        if sample.mem_percent > self.THRESHOLDS["mem"]:
            self._maybe_emit_breach(server_id, "mem", sample.mem_percent)
        if sample.disk_percent > self.THRESHOLDS["disk"]:
            self._maybe_emit_breach(server_id, "disk", sample.disk_percent)

    def _maybe_emit_breach(self, server_id: str, metric: str, value: float) -> None:
        key = (server_id, metric)
        now = time.time()
        last = self._last_breach.get(key)
        if last is not None and (now - last) < self.BREACH_COOLDOWN:
            return
        self._last_breach[key] = now
        self.threshold_breached.emit(server_id, metric, value)

    # ---------- 默认 sampler ----------
    def _default_sampler(self, server_id: str) -> ResourceSample | None:
        """默认 sampler: 通过 ``ServerManager`` 拉取 ``RemoteBackend`` 并采样.

        放在实例方法而非模块顶层 import, 避免 ``creart`` 循环依赖
        (ServerManager 创建期间会创建 ResourceMonitorService).
        """
        from creart import it

        from src.core.remote.server_manager import ServerManager

        manager = it(ServerManager)
        backend: Any = manager.get_backend(server_id) if hasattr(manager, "get_backend") else None
        if backend is None or not hasattr(backend, "sample_resources"):
            return None
        try:
            return backend.sample_resources()
        except Exception:  # noqa: BLE001 - sampler 内部静默, worker 走退避路径
            raise


# ==================== creart 单例 ====================
class ResourceMonitorServiceCreator(AbstractCreator, ABC):
    """``ResourceMonitorService`` 单例创建器."""

    targets = (
        CreateTargetInfo(
            module="src.core.remote.resource_monitor",
            identify="ResourceMonitorService",
            humanized_name="远端资源监控",
            description="周期采样远端 CPU / Mem / Disk 并触发阈值告警 (P4 F3)",
        ),
    )

    @staticmethod
    def available() -> bool:
        return exists_module("src.core.remote.resource_monitor")

    @staticmethod
    def create(create_type):
        return create_type()


add_creator(ResourceMonitorServiceCreator)

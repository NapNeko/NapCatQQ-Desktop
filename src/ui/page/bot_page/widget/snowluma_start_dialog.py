# -*- coding: utf-8 -*-
"""SnowLuma Bot 启动模式选择对话框 (Q2).

用户点 "启动 Bot" 按钮 (SnowLuma 后端) → 弹两个对话框 (按需):

1. :class:`SnowLumaStartModeDialog` - 让用户在 **冷启动** 与 **热启动** 之间二选一.
2. :class:`SnowLumaPidPickerDialog` - 热启动 且 系统有多个 QQ.exe 时, 让用户选具体哪个.

冷启动 = Desktop 自己 spawn 新 QQ.exe (历史默认行为).
热启动 = 注入到用户系统里已经运行的某个 QQ.exe (不动 QQ 进程生命周期).

参见: ``src/core/runtime/snowluma_driver.py`` ``SnowLumaStartMode`` 枚举.
"""
from __future__ import annotations

import os
from dataclasses import dataclass, replace
from datetime import datetime
from typing import TYPE_CHECKING

import psutil
from PySide6.QtCore import QObject, QRunnable, Qt, Signal
from PySide6.QtWidgets import QButtonGroup, QGridLayout, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    MessageBoxBase,
    RadioButton,
    SimpleCardWidget,
    TitleLabel,
)

from src.core.runtime.q_port_probe import probe_qq_login
from src.core.runtime.snowluma_driver import SnowLumaStartMode

if TYPE_CHECKING:
    pass


# ==================== 进程枚举 ====================
@dataclass(frozen=True)
class QQProcessInfo:
    """已运行的 ``QQ.exe`` 进程信息 (用于 :class:`SnowLumaPidPickerDialog` 展示).

    Attributes:
        pid: 进程 PID.
        create_time_iso: 进程启动时间 (ISO 8601, 本地时区, 精度到秒).
        memory_mb: 物理内存占用 (MB, 整数, 仅作参考让用户辨识哪个是"主"实例).
        login_uin: 通过 :func:`probe_qq_login` 探测到的当前登录 uin; 空字符串表示
            未登录或探测失败. **仅供 UI 展示**, 不参与注入决策 (uin 与配置 QQID
            的最终匹配仍由 SnowLumaStatusPoller 在 inject 后确认).
        login_probed: 是否完成过登录探测 (区分 "未探测" 与 "探测了但未登录").
    """

    pid: int
    create_time_iso: str
    memory_mb: int
    login_uin: str = ""
    login_probed: bool = False



def _enumerate_qq_processes_via_toolhelp32() -> list[QQProcessInfo] | None:
    """Windows ToolHelp32 ``CreateToolhelp32Snapshot`` + ``Process32First/Next`` 快速路径.

    2026-05-11 用户实测 ``psutil.process_iter`` 在工作线程跑 1-3s, 持续占 GIL 让主线程
    饥饿, UI 完全锁死. ctypes 直调 Windows API 拿 PID/ParentPID/Name 全在 C 层, **不持
    GIL**, 总耗时 ~50ms, 主线程不被卡.

    覆盖 Rule 1 (name=qq.exe) + Rule 2 (parent != qq.exe). cmdline / 内存 / 创建时间
    走廉价的 psutil 二次 lookup (主候选 1-3 个, 不影响整体性能).

    非 Windows 环境返回 ``None``, 调用方 fallback 到纯 psutil.
    """
    if os.name != "nt":
        return None

    try:
        import ctypes
        from ctypes import wintypes

        # ToolHelp32 结构体 (winapi.h)
        TH32CS_SNAPPROCESS = 0x00000002
        MAX_PATH = 260
        INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value

        class PROCESSENTRY32W(ctypes.Structure):
            _fields_ = [
                ("dwSize", wintypes.DWORD),
                ("cntUsage", wintypes.DWORD),
                ("th32ProcessID", wintypes.DWORD),
                ("th32DefaultHeapID", ctypes.c_void_p),
                ("th32ModuleID", wintypes.DWORD),
                ("cntThreads", wintypes.DWORD),
                ("th32ParentProcessID", wintypes.DWORD),
                ("pcPriClassBase", wintypes.LONG),
                ("dwFlags", wintypes.DWORD),
                ("szExeFile", wintypes.WCHAR * MAX_PATH),
            ]

        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
        kernel32.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
        kernel32.Process32FirstW.restype = wintypes.BOOL
        kernel32.Process32FirstW.argtypes = [wintypes.HANDLE, ctypes.POINTER(PROCESSENTRY32W)]
        kernel32.Process32NextW.restype = wintypes.BOOL
        kernel32.Process32NextW.argtypes = [wintypes.HANDLE, ctypes.POINTER(PROCESSENTRY32W)]
        kernel32.CloseHandle.restype = wintypes.BOOL
        kernel32.CloseHandle.argtypes = [wintypes.HANDLE]

        snapshot = kernel32.CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
        if snapshot == 0 or snapshot == INVALID_HANDLE_VALUE:
            return None

        try:
            entry = PROCESSENTRY32W()
            entry.dwSize = ctypes.sizeof(PROCESSENTRY32W)
            if not kernel32.Process32FirstW(snapshot, ctypes.byref(entry)):
                return []

            qq_entries: list[tuple[int, int]] = []  # (pid, ppid)
            while True:
                name = entry.szExeFile.lower()
                if name == "qq.exe":
                    qq_entries.append(
                        (int(entry.th32ProcessID), int(entry.th32ParentProcessID))
                    )
                if not kernel32.Process32NextW(snapshot, ctypes.byref(entry)):
                    break
        finally:
            kernel32.CloseHandle(snapshot)

        # Rule 2: 排除 parent 也是 QQ.exe 的 (Chromium 子进程)
        qq_pids = {pid for pid, _ in qq_entries}
        main_candidates = [
            pid for pid, ppid in qq_entries if ppid not in qq_pids
        ]

        # 对主候选 (通常 1-3 个) 用 psutil 拿 cmdline / mem / create_time
        results: list[QQProcessInfo] = []
        for pid in main_candidates:
            try:
                proc = psutil.Process(pid)
                cmdline = proc.cmdline()
                if any("--type=" in arg for arg in cmdline):
                    continue  # Rule 3: Chromium 子进程 (有些不在 qq_pids 但 cmdline 含 --type)
                create_time_ts = proc.create_time()
                mem_bytes = proc.memory_info().rss
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                continue
            except Exception:  # noqa: BLE001
                continue

            create_time = datetime.fromtimestamp(float(create_time_ts))
            mem_mb = int(int(mem_bytes) / 1024 / 1024)
            results.append(
                QQProcessInfo(
                    pid=pid,
                    create_time_iso=create_time.strftime("%Y-%m-%d %H:%M:%S"),
                    memory_mb=mem_mb,
                )
            )

        results.sort(key=lambda x: x.pid)
        return results
    except Exception:  # noqa: BLE001 - ctypes 失败 → fallback 到 psutil 路径
        return None


def enumerate_qq_processes() -> list[QQProcessInfo]:
    """枚举当前系统中**主** ``QQ.exe`` 进程 (排除 Electron 子进程), 返回 PID 升序列表.

    Q2 (热启动支持): 用 :mod:`psutil` 替代上游 SnowLuma 的 native C++ addon
    ``getAllMainProcess()`` (见 ``@example/SnowLuma-main/packages/core/src/hook/injector.ts:99``).
    上游用 native addon 因为它要做注入; 我们这里只是 UI 层枚举, psutil 就够.

    QQ NT 客户端基于 Electron / Chromium, 一个**主进程**会 spawn 一堆子进程
    (renderer / GPU / utility / crash-handler 等), **全部都叫 ``QQ.exe``**. 直接按 name
    枚举会看到 5-15 个进程, 但实际可注入的"主"进程通常只有 1-2 个 (每登录账号 1 个).

    过滤规则 (与 Electron / Chromium 行为对齐):

    1. ``name.lower() == "qq.exe"``  (大小写不敏感)
    2. **排除**: parent process 也叫 ``QQ.exe`` → 这是 Chromium 子进程
    3. **排除**: 命令行含 ``--type=`` 参数 → Chromium 用此标记 renderer / GPU / utility
       (e.g. ``--type=renderer``, ``--type=gpu-process``)
    4. 跳过 ZOMBIE / DEAD 进程
    5. 静默忽略 :class:`psutil.NoSuchProcess` / :class:`psutil.AccessDenied`

    Returns:
        QQProcessInfo 列表, 按 PID 升序; 没主进程时返回空列表.

    2026-05-11 性能优化 v2 (用户实测热启动 worker 提交后 UI 完全锁死):
    根本原因是 ``psutil.process_iter`` 在工作线程里跑 1-3s **持续占 GIL** (psutil
    Python 层 wrapper 频繁 next() yield, 主线程几乎拿不到 GIL 时间, 即便事件循环
    在跑也响应不了 UI 事件). 修复策略:

    - **Windows 快速路径**: 用 ctypes 直调 ``CreateToolhelp32Snapshot`` + ``Process32Next``,
      整轮枚举在 C 层做, **完全不持 GIL**, 总耗时 ~50ms; 主候选 (1-3 个) 才用 psutil
      lookup cmdline / mem / create_time.
    - **fallback (非 Windows / ctypes 失败)**: 沿用旧 psutil 路径但只取最小字段
      ``["pid", "name", "ppid"]``, 二次 lookup 详细字段.
    """
    import time

    from src.core.logging import LogSource, LogType, logger

    t0 = time.monotonic()

    # 优先 Windows ToolHelp32 快速路径
    fast_results = _enumerate_qq_processes_via_toolhelp32()
    if fast_results is not None:
        t_total = time.monotonic() - t0
        logger.trace(
            f"enumerate_qq_processes (toolhelp32 快速路径) 总耗时 {t_total*1000:.1f}ms, "
            f"主 QQ 进程 {len(fast_results)} 个",
            LogType.NONE_TYPE,
            LogSource.UI,
        )
        return fast_results

    # Fallback: 纯 psutil (非 Windows / ctypes 不可用)
    raw_qq: dict[int, dict[str, object]] = {}
    for proc in psutil.process_iter(["pid", "name", "ppid"]):
        try:
            info = proc.info
            name = (info.get("name") or "").lower()
            if name != "qq.exe":
                continue
            pid = int(info["pid"])
            ppid = int(info.get("ppid") or 0)
            raw_qq[pid] = {"ppid": ppid, "_proc": proc}
        except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
            continue
        except Exception:  # noqa: BLE001 - 单个 process 出错不阻断整体枚举
            continue

    qq_pids = set(raw_qq.keys())
    results: list[QQProcessInfo] = []
    for pid, payload in raw_qq.items():
        ppid = int(payload["ppid"])  # type: ignore[arg-type]
        if ppid in qq_pids:
            continue
        proc_obj = payload["_proc"]
        try:
            if proc_obj.status() in (psutil.STATUS_ZOMBIE, psutil.STATUS_DEAD):  # type: ignore[union-attr]
                continue
            cmdline = proc_obj.cmdline()  # type: ignore[union-attr]
            if any("--type=" in arg for arg in cmdline):
                continue
            create_time_ts = proc_obj.create_time()  # type: ignore[union-attr]
            mem_bytes = proc_obj.memory_info().rss  # type: ignore[union-attr]
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            continue
        except Exception:  # noqa: BLE001
            continue

        create_time = datetime.fromtimestamp(float(create_time_ts))
        mem_mb = int(int(mem_bytes) / 1024 / 1024)
        results.append(
            QQProcessInfo(
                pid=pid,
                create_time_iso=create_time.strftime("%Y-%m-%d %H:%M:%S"),
                memory_mb=mem_mb,
            )
        )
    results.sort(key=lambda x: x.pid)

    t_total = time.monotonic() - t0
    logger.trace(
        f"enumerate_qq_processes (psutil fallback) 总耗时 {t_total*1000:.1f}ms, "
        f"主 QQ 进程 {len(results)} 个",
        LogType.NONE_TYPE,
        LogSource.UI,
    )
    return results


# ==================== 启动模式选择对话框 ====================
class _StartModeCard(SimpleCardWidget):
    """启动模式单选卡片 (与 ChooseConfigCard 同类的轻量卡片)."""

    def __init__(
        self,
        button: RadioButton,
        title: str,
        description: str,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self.button = button
        self.title_label = BodyLabel(title, self)
        self.description_label = CaptionLabel(description, self)

        self.title_label.setStyleSheet("font-weight: bold;")
        self.description_label.setWordWrap(True)

        self.setMinimumSize(280, 140)

        v_layout = QVBoxLayout(self)
        v_layout.setContentsMargins(16, 16, 16, 16)
        v_layout.setSpacing(6)
        v_layout.addWidget(self.button, alignment=Qt.AlignmentFlag.AlignTop)
        v_layout.addWidget(self.title_label, alignment=Qt.AlignmentFlag.AlignTop)
        v_layout.addWidget(self.description_label, alignment=Qt.AlignmentFlag.AlignTop)
        v_layout.addStretch(1)

        # 整张卡片可点选 (不只是 RadioButton 本身)
        self.clicked.connect(lambda: self.button.setChecked(True))


class SnowLumaStartModeDialog(MessageBoxBase):
    """Q2: 让用户在 **冷启动** 与 **热启动** 之间二选一.

    冷启动卡默认选中 (与历史行为一致). 用户点 OK 后用 :meth:`get_value` 取选择.

    Note:
        本对话框**只**做模式选择, 不做 PID 选择 - PID 选择由调用方根据 mode 决定:

        - 冷启动: 不需要 PID (Phase A 自己 spawn).
        - 热启动: 用 :class:`SnowLumaPidPickerDialog` 选 PID (或 0/1 个 QQ.exe 时直接决定).
    """

    def __init__(self, parent: QObject) -> None:
        super().__init__(parent=parent)

        self.title_label = TitleLabel(self.tr("选择 SnowLuma 启动方式"), self)

        # RadioButton + ButtonGroup
        self.button_group = QButtonGroup(self)
        self.cold_button = RadioButton(self.tr("冷启动 (推荐)"))
        self.hot_button = RadioButton(self.tr("热启动"))
        self.button_group.setExclusive(True)
        self.button_group.addButton(self.cold_button, 1)
        self.button_group.addButton(self.hot_button, 2)

        # 卡片
        self.cold_card = _StartModeCard(
            self.cold_button,
            self.tr("冷启动: 启动新的 QQ + 注入"),
            self.tr(
                "Desktop 自动 spawn 一个新的 QQ.exe 进程, 等其启动完成后再注入 SnowLuma. "
                "适用于: 系统里没有 QQ 正在运行 / 想要全新的 QQ 实例. "
                "停止 Bot 时会同时关闭这个 QQ.exe."
            ),
            self,
        )
        self.hot_card = _StartModeCard(
            self.hot_button,
            self.tr("热启动: 注入到现有 QQ"),
            self.tr(
                "不启动新 QQ, 直接把 SnowLuma 注入到用户已经登录的 QQ.exe 中 (节省启动时间, "
                "避免重复扫码). 适用于: QQ 已经在运行且已登录. "
                "停止 Bot 时只卸载注入, 不关闭用户的 QQ."
            ),
            self,
        )

        # 默认选中冷启动 (与历史行为一致)
        self.cold_button.setChecked(True)

        self.widget.setMinimumSize(640, 320)

        # 布局
        grid = QGridLayout()
        grid.setSpacing(12)
        grid.setContentsMargins(0, 0, 0, 0)
        grid.addWidget(self.cold_card, 0, 0)
        grid.addWidget(self.hot_card, 0, 1)

        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addLayout(grid, stretch=1)

    def get_value(self) -> SnowLumaStartMode:
        """返回用户选择的启动模式 (默认冷启动)."""
        return (
            SnowLumaStartMode.HOT_START
            if self.button_group.checkedId() == 2
            else SnowLumaStartMode.COLD_START
        )


# ==================== PID picker dialog ====================
class _PidPickerCard(SimpleCardWidget):
    """PID 单选卡片 (每个 QQ.exe 进程一张)."""

    def __init__(
        self,
        button: RadioButton,
        info: QQProcessInfo,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self.button = button
        self.info = info

        self.pid_label = BodyLabel(f"PID = {info.pid}", self)
        self.pid_label.setStyleSheet("font-weight: bold;")

        # 登录账号显示 (上游 SnowLuma#56 思路): 探测命中且已登录时显示 uin,
        # 探测过但未登录显示 "未登录", 没探测过 (旧测试 / 主线程构造) 不显示这行.
        if info.login_probed:
            if info.login_uin:
                self.login_label = BodyLabel(
                    self.tr("已登录: {uin}").format(uin=info.login_uin), self
                )
            else:
                self.login_label = BodyLabel(self.tr("未登录"), self)
                self.login_label.setStyleSheet("color: #6c757d;")
        else:
            self.login_label = None

        self.detail_label = CaptionLabel(
            self.tr("启动于 {started_at} · 内存 {mem_mb} MB").format(
                started_at=info.create_time_iso, mem_mb=info.memory_mb
            ),
            self,
        )
        self.detail_label.setWordWrap(True)

        self.setMinimumHeight(100 if info.login_probed else 80)
        self.setMinimumWidth(320)

        v_layout = QVBoxLayout(self)
        v_layout.setContentsMargins(16, 12, 16, 12)
        v_layout.setSpacing(4)
        v_layout.addWidget(self.button, alignment=Qt.AlignmentFlag.AlignTop)
        v_layout.addWidget(self.pid_label)
        if self.login_label is not None:
            v_layout.addWidget(self.login_label)
        v_layout.addWidget(self.detail_label)

        self.clicked.connect(lambda: self.button.setChecked(True))



class SnowLumaPidPickerDialog(MessageBoxBase):
    """Q2: 当系统里有多个 QQ.exe 进程时, 让用户选要注入哪一个.

    调用方先用 :func:`enumerate_qq_processes` 拿到候选列表; 若数量 ``>= 2`` 才有必要弹本对话框
    (1 个时直接用, 0 个时显示错误并 abort 热启动).

    Note:
        卡片展示 PID / 启动时间 / 内存, 并通过 :func:`probe_qq_login` 探测每个 QQ.exe
        当前登录的 ``uin`` (走 QQ 自带的 ``tencent://`` 深链接 9210-9219 端口服务,
        无需注入). 用户能直接看到哪个 QQ 登录的是哪个号, 大幅降低多开场景下选错的风险.
        若探测失败 (端口未开 / 权限不足) 卡片显示"未登录", 此时仍可通过启动时间 + 内存辨识;
        最终 uin 与配置 QQID 是否匹配仍由 SnowLumaStatusPoller 在 inject 后兜底确认.
    """

    def __init__(self, parent: QObject, candidates: list[QQProcessInfo]) -> None:
        super().__init__(parent=parent)
        if not candidates:
            raise ValueError("SnowLumaPidPickerDialog 需要至少 1 个候选 QQProcessInfo")

        self.title_label = TitleLabel(self.tr("选择要注入的 QQ.exe"), self)
        # 探测命中时优先按"已登录"提示, 否则退化到旧的启动时间提示
        any_logged_in = any(info.login_uin for info in candidates)
        if any_logged_in:
            hint_text = self.tr(
                "检测到 {count} 个 QQ.exe 进程, 请选择 SnowLuma 应该注入到哪一个. "
                "提示: 已显示当前登录的 QQ 号, 选择与 Bot 配置 QQID 一致的进程."
            ).format(count=len(candidates))
        else:
            hint_text = self.tr(
                "检测到 {count} 个 QQ.exe 进程, 请选择 SnowLuma 应该注入到哪一个. "
                "提示: 启动时间最久的通常是你已经登录的那个."
            ).format(count=len(candidates))
        self.hint_label = BodyLabel(hint_text, self)

        self.hint_label.setWordWrap(True)

        self.button_group = QButtonGroup(self)
        self.button_group.setExclusive(True)

        self.cards: list[_PidPickerCard] = []
        for index, info in enumerate(candidates):
            radio = RadioButton(self.tr("注入到此进程"))
            self.button_group.addButton(radio, info.pid)
            card = _PidPickerCard(radio, info, self)
            self.cards.append(card)

        # 默认选中第一个 (PID 最小的, 一般是用户最早起的 QQ.exe)
        if self.cards:
            self.cards[0].button.setChecked(True)

        self.widget.setMinimumSize(560, min(560, 200 + 100 * len(candidates)))

        # 布局: vertical list of cards
        cards_layout = QVBoxLayout()
        cards_layout.setSpacing(8)
        cards_layout.setContentsMargins(0, 0, 0, 0)
        for card in self.cards:
            cards_layout.addWidget(card)
        cards_layout.addStretch(1)

        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.hint_label)
        self.viewLayout.addLayout(cards_layout, stretch=1)

    def get_value(self) -> int:
        """返回用户选择的 PID; 用户没选任何 button 时返回 0 (调用方应判断 > 0)."""
        checked_id = self.button_group.checkedId()
        return int(checked_id) if checked_id > 0 else 0


# ==================== QQ.exe 枚举后台 worker (Q2: 避免 UI 卡顿) ====================
class EnumerateQQProcessesWorker(QObject, QRunnable):
    """后台运行 :func:`enumerate_qq_processes`, 避免 psutil 在主线程卡 UI.

    实测 cold call ~2.6s, warm call ~500ms. 即使 warm call 500ms 在主线程也是可见的
    卡顿, 必须甩到线程池. 调用方应:

    1. 创建 worker 实例
    2. 连接 ``finished`` 信号到主线程 slot
    3. 提交到 :class:`QThreadPool.globalInstance().start(worker)`
    4. 主线程 slot 收到 ``list[QQProcessInfo]`` 后继续流程

    Note:
        ``setAutoDelete(False)`` - 调用方须在 ``finished`` slot 里 ``deleteLater``, 或
        用临时强引用防止 Python GC (Qt 不会自动清理 QObject, PySide6 也不会主动调
        ``deleteLater`` 哪怕 Python refs 归零).
    """

    # 参数: list[QQProcessInfo] (只 emit 主 QQ 的过滤结果)
    finished = Signal(object)

    def __init__(self) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self.setAutoDelete(False)

    def run(self) -> None:  # noqa: D401 - QRunnable 协议
        # 2026-05-11 诊断: 在 worker 实际开跑时打 trace, 帮排查 "提交后 UI 卡顿" 是
        # worker 本身慢, 还是 QThreadPool 排队等待 (e.g. PhaseC worker 长期占位).
        import threading

        from src.core.logging import LogSource, LogType, logger

        thread_name = threading.current_thread().name
        logger.trace(
            f"EnumerateQQProcessesWorker.run 开始 (thread={thread_name})",
            LogType.NONE_TYPE,
            LogSource.UI,
        )
        try:
            results = enumerate_qq_processes()
        except Exception as exc:  # noqa: BLE001 - worker 不能让异常逃逸到 QThreadPool
            logger.warning(
                f"EnumerateQQProcessesWorker 异常: {type(exc).__name__}: {exc}",
                LogType.NONE_TYPE,
                LogSource.UI,
            )
            results = []

        # 对每个候选 PID 同步探测登录账号 (上游 SnowLuma#56 思路).
        # 每个端口探测 ≤1s, 实测命中后 <100ms; 主候选 1-3 个, 总耗时可控.
        # 探测失败不阻塞流程, login_probed=True + login_uin="" 表示 "已尝试但未登录/无响应".
        probed: list[QQProcessInfo] = []
        for info in results:
            try:
                login_info = probe_qq_login(info.pid)
            except Exception as exc:  # noqa: BLE001
                logger.trace(
                    f"probe_qq_login 异常 (pid={info.pid}): {type(exc).__name__}: {exc}",
                    LogType.NETWORK,
                    LogSource.UI,
                )
                login_info = None
            uin = login_info.uin if (login_info and login_info.logged_in) else ""
            probed.append(replace(info, login_uin=uin, login_probed=True))

        logger.trace(
            f"EnumerateQQProcessesWorker.run 完成 (results={len(probed)}), 即将 emit finished",
            LogType.NONE_TYPE,
            LogSource.UI,
        )
        self.finished.emit(probed)

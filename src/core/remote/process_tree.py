# -*- coding: utf-8 -*-
"""远端进程树 RSS 探测 (NC / SL flavor 公用).

两条 flavor 的远端 Bot 都需要 "给定主进程 PID, 统计该 PID 整棵子进程树的 RSS 总和"
(Electron 主进程 + GPU/renderer/utility helper 一堆子进程, 单进程 RSS 极大低估).

历史: NC ``RemoteBackend._fetch_rss_bytes`` 内联实现, SL 一直 backlog 返 None,
导致 SL Bot 卡片内存恒为 0. 抽出共享函数让两侧对齐.
"""

from __future__ import annotations

import re

from .execution_backend import ExecutionBackend


# ``ps -e -o pid=,ppid=,rss=`` 单行格式: 任意空白分隔的三列整数 (pid, ppid, rss_kib).
# 列宽随发行版浮动, 故按任意空白分隔.
_PS_TREE_LINE_PATTERN = re.compile(r"^\s*(\d+)\s+(\d+)\s+(\d+)\s*$")


def fetch_process_tree_rss_bytes(backend: ExecutionBackend, pid: int) -> int | None:
    """读取远端 ``pid`` 及其所有后代进程的 RSS 之和, 返回字节数.

    与本地路径 :class:`src.core.runtime.napcat.BotProcessManager.get_memory_usage`
    通过 ``psutil`` 累加进程树 RSS 的行为对齐.

    远端 LinuxQQ 由 launcher (xvfb-run / SnowLuma daemon) 拉起 ``qq`` (Electron),
    进程结构示意 (NC 路径):

    - ``/bin/sh xvfb-run -a /usr/local/bin/qq --no-sandbox -q <qq_id>``  (~1 MB)
      - ``/usr/local/bin/qq --no-sandbox -q <qq_id>``                    (Electron main)
        - 多个 GPU / renderer / utility 子进程                            (各占数十-数百 MB)

    若仅取 ``ps -o rss= -p <pid>`` 的单进程 RSS:

    - 命中 shell wrapper 时显示 1 MB (用户报告的现象)
    - 即便命中 Electron main, 也漏掉所有 helper 进程

    实现: 单次 SSH 拉全量 ``ps -e -o pid=,ppid=,rss=``, 客户端 BFS 走 ``pid`` 的子树
    并累加 RSS. 输出单位 KiB → 转字节.

    Args:
        backend: 远端执行后端 (SSH); 使用 ``run("ps -e -o pid=,ppid=,rss= ...")``
        pid: 主进程 PID (NC: launcher 进程 / SL: ``qq.exe`` main)

    Returns:
        - 进程树 RSS 总和 (字节); ``ps`` 输出单位为 KiB, 此处 ×1024
        - ``ps`` 命令失败 → ``None``
        - ``pid`` 已不在 ``ps`` 输出中 (进程退出) → ``None``;
          上层应据此显示 "未知" 而非 "0", 让用户知道是探测不到, 不是真没占内存
    """
    result = backend.run("ps -e -o pid=,ppid=,rss= 2>/dev/null || true")
    if not result.ok:
        return None

    # pid -> rss_kib; ppid -> [child_pid, ...]
    rss_by_pid: dict[int, int] = {}
    children: dict[int, list[int]] = {}
    for raw in result.stdout.splitlines():
        match = _PS_TREE_LINE_PATTERN.match(raw)
        if match is None:
            continue
        cpid = int(match.group(1))
        cppid = int(match.group(2))
        crss = int(match.group(3))
        rss_by_pid[cpid] = crss
        children.setdefault(cppid, []).append(cpid)

    if pid not in rss_by_pid:
        # 进程已退出, 或 ps 输出无法解析 -> 报告 None 而不是 0,
        # 让上层走 "未知" 而不是 "已停"
        return None

    total_kib = 0
    visited: set[int] = set()
    stack: list[int] = [pid]
    while stack:
        current = stack.pop()
        if current in visited:
            continue
        visited.add(current)
        total_kib += rss_by_pid.get(current, 0)
        stack.extend(children.get(current, ()))

    # ``ps`` 输出单位为 KiB
    return total_kib * 1024


_MEMTOTAL_LINE_PATTERN = re.compile(r"^MemTotal:\s*(\d+)\s*kB\s*$", re.MULTILINE)


def fetch_remote_total_memory_bytes(backend: ExecutionBackend) -> int | None:
    """读取远端服务器物理总内存 (字节); 用于 Bot 卡片 ``X MB / Y MB`` 中的 Y.

    实现: ``cat /proc/meminfo`` 拿 ``MemTotal:`` 行 (Linux 标准, KiB 单位).
    单 App session 内服务器 RAM 不变, 建议调用方按 backend 缓存一次, 不需要每次轮询.

    Args:
        backend: 远端执行后端 (SSH); 仅调 ``run("cat /proc/meminfo")``

    Returns:
        - 总内存字节数 (``MemTotal`` 行的 KiB × 1024)
        - ``cat`` 失败 / ``MemTotal`` 行解析失败 → ``None`` (上层应 fallback 本地 RAM 或显示 0)
    """
    result = backend.run("cat /proc/meminfo 2>/dev/null || true")
    if not result.ok or not result.stdout:
        return None
    match = _MEMTOTAL_LINE_PATTERN.search(result.stdout)
    if match is None:
        return None
    try:
        kib = int(match.group(1))
    except ValueError:
        return None
    return kib * 1024


__all__ = [
    "fetch_process_tree_rss_bytes",
    "fetch_remote_total_memory_bytes",
]

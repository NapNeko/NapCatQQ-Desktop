# -*- coding: utf-8 -*-
"""NapCat 后端 driver (Tier I, P2 SnowLuma WebUI 编程客户端化重构).

把原 ``napcat.py:BotProcessManager`` 中**只属于 NapCat 注入式**的 QProcess
创建/停止逻辑搬到这里, 让 :class:`BotProcessManager` (在 ``bot_process_manager.py``)
仅负责 dispatch 与生命周期管理.

搬移来源 (一字不改函数体):

- ``napcat.py:1374-1392`` ``_write_load_script``
- ``napcat.py:1394-1420`` ``_create_napcat_process``
- ``napcat.py:1375-1392`` ``_get_env_variable``
- ``napcat.py:1762-1810`` ``stop_process`` 内 NapCat psutil 进程树 kill 部分

本 driver 不持久任何状态; ``ProcessHandle`` 由 manager 拿走后注册到
``BotProcessManager._napcat_process_dict`` (内部字典名沿用 P1 名 ``napcat_process_dict``,
不改避免连带影响).
"""
from __future__ import annotations

from collections import deque
from pathlib import Path
from time import monotonic
from typing import TYPE_CHECKING

import psutil
from creart import it
from PySide6.QtCore import QProcess

from src.core.logging import LogSource, LogType, logger
from src.core.runtime.bot_backend_driver import BotBackendDriver, ProcessHandle
from src.core.runtime.paths import PathFunc

if TYPE_CHECKING:
    from src.core.config.config_model import Config


class NapCatDriver(BotBackendDriver):
    """NapCat (NTQQ 注入式) 后端 driver.

    NapCat 的进程模型是单 QProcess (``NapCatWinBootMain.exe``); 它会派生
    ``QQ.exe`` 子进程并把 ``NapCatWinBootHook.dll`` 注入. Desktop 仅持有 launcher
    QProcess, 子进程通过 ``psutil`` 进程树定位与 kill.

    本 driver 不持有任何状态, 所有 QProcess 实例由 :class:`BotProcessManager` 注册
    到自己的 ``napcat_process_dict``.
    """

    # ==================== BotBackendDriver 接口实现 ====================
    def start(self, config: "Config") -> ProcessHandle:
        """启动 NapCat Bot 进程, 返回 ``ProcessHandle``.

        Raises:
            FileNotFoundError: 未检测到 ``QQ.exe`` 注册表路径.
        """
        path_func = it(PathFunc)
        qq_path = path_func.get_qq_path()
        if qq_path is None:
            raise FileNotFoundError(
                "未检测到 QQ 安装路径，无法启动 NapCatQQ 进程!"
            )

        logger.trace(
            (
                "NapCatQQ 进程启动参数已解析: "
                f"QQID={config.bot.QQID}, qq_path={qq_path}, "
                f"launcher={getattr(path_func, 'napcat_path', '<unknown>')}"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )

        process = self._create_napcat_process(config, qq_path)
        return ProcessHandle(
            qq_id=str(config.bot.QQID),
            primary_process=process,
            secondary_process=None,
        )

    def stop(self, qq_id: str, *, process: QProcess) -> None:
        """停止指定 QQ 号的 NapCat 进程 + psutil 进程树清理.

        与原 ``napcat.py:1762-1810`` ``stop_process`` 的 NapCat 分支一字不改.
        ``process`` 由 manager 从 ``napcat_process_dict`` 取出后传入, 这样 driver
        无需持有状态.
        """
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

    def is_running(self, qq_id: str) -> bool:
        """NapCat driver 自己不持状态; 调用方应直接查 ``BotProcessManager.napcat_process_dict``.

        保留接口签名以满足 :class:`BotBackendDriver` 抽象, 但实际返回 ``False``.
        """
        return False

    def get_status_poller(self, qq_id: str):
        """NapCat 路径不走 driver-level poller (登录态由 ``ManagerNapCatQQLoginState`` 管),
        总是返回 ``None``.
        """
        return None

    # ==================== 内部 (机械搬移自 napcat.py) ====================
    def _get_env_variable(self) -> list[str]:
        """获取环境变量 (从 napcat.py 一字不改搬移)."""
        env = QProcess.systemEnvironment()
        env.append(f"NAPCAT_PATCH_PACKAGE={it(PathFunc).napcat_path / 'qqnt.json'}")
        env.append(f"NAPCAT_LOAD_PATH={it(PathFunc).napcat_path / 'loadNapCat.js'}")
        env.append(f"NAPCAT_INJECT_PATH={it(PathFunc).napcat_path / 'NapCatWinBootHook.dll'}")
        env.append(f"NAPCAT_LAUNCHER_PATH={it(PathFunc).napcat_path / 'NapCatWinBootMain.exe'}")
        env.append(f"NAPCAT_MAIN_PATH={it(PathFunc).napcat_path / 'napcat.mjs'}")

        return env

    def _write_load_script(self) -> None:
        """写入 loadNapCat.js 脚本文件 (从 napcat.py 一字不改搬移)."""
        with open(str(it(PathFunc).napcat_path / "loadNapCat.js"), "w") as file:
            file.write(
                "(async () => {await import("
                f"'{(it(PathFunc).napcat_path / 'napcat.mjs').as_uri()}'"
                ")})()"
            )
        logger.info("NapCatQQ 进程加载脚本已写入")

    def _create_napcat_process(self, config: "Config", qq_path: Path) -> QProcess:
        """创建并配置 QProcess (从 napcat.py 一字不改搬移).

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

    # ==================== 进程树内存查询 (供 manager.get_memory_usage 复用) ====================
    @staticmethod
    def get_memory_usage_for_pid(main_pid: int) -> int:
        """累加进程树 RSS 内存 (从 napcat.py:get_memory_usage 内核搬移).

        Args:
            main_pid: 启动器 ``NapCatWinBootMain.exe`` 的 PID

        Returns:
            进程树总内存 (MB); 进程不存在时返回 0.
        """
        if main_pid <= 0:
            return 0

        try:
            total_memory = 0
            processed_pids: set[int] = set()
            queue: deque[int] = deque([main_pid])

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

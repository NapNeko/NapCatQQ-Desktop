# -*- coding: utf-8 -*-
"""
## 运行 NapCat 流程
"""
# 标准库导入
from collections import deque
import re

# 第三方库导入
import psutil
from PySide6.QtCore import QObject, QProcess, Signal

# 项目内模块导入
from src.core.config.config_model import Config
from src.core.utils.logger import logger
from src.core.utils.path_func import PathFunc
from src.ui.components.info_bar import error_bar


class NapCatQQProcessLogger(QObject):
    """进程的日志缓冲区"""

    handle_output_log_signal = Signal(str)

    def __init__(self, qq_id: str) -> None:
        super().__init__()
        # 设置属性
        self._qq_id = qq_id
        self._log_storage = []

    # ==================== 公共函数===================
    def set_process(self, process: QProcess):
        """设置 process 对象"""
        self._process = process

    def get_log(self) -> str:
        """返回所有 log"""
        print(self._log_storage)
        return "".join(self._log_storage)

    def clear(self) -> None:
        """清理所有 log"""
        self._log_storage.clear()

    # ==================== 响应函数===================
    def handle_output(self):
        """处理日志数据"""
        # 拿到解码后的数据
        data = manager_process.get_process(self._qq_id).readAllStandardOutput().data().decode()
        # 正则处理转义吗
        data = re.compile(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])").sub("", data)
        data = re.compile(r"\r+\n$").sub("\n", data)
        self._log_storage.append(data)
        self.handle_output_log_signal.emit(data)


class ManagerNapCatQQProcess(QObject):
    """NapCatQQ 进程管理类

    负责创建和管理 NapCatQQ 的 QProcess 实例
    """

    # 进程状态改变信号
    process_changed_signal = Signal(str, QProcess.ProcessState)

    def __init__(self) -> None:
        """初始化 NapCatQQ 进程管理器

        Args:
            config (Config): 配置对象
        """
        super().__init__()
        self.napcat_process_dict: dict[str, QProcess] = {}
        logger.info("NapCatQQ 进程管理器已初始化")

    def create_napcat_process(self, config: Config, log: NapCatQQProcessLogger) -> None:
        """创建并配置 QProcess

        Args:
            config (Config): 配置对象
            log (NapCatQQProcessLogger): 日志缓冲对象(需要实例)

        Returns:
            QProcess: 配置好的 QProcess 对象
        """

        # 配置环境变量
        env = QProcess.systemEnvironment()
        env.append(f"NAPCAT_PATCH_PACKAGE={PathFunc().napcat_path / 'qqnt.json'}")
        env.append(f"NAPCAT_LOAD_PATH={PathFunc().napcat_path / 'loadNapCat.js'}")
        env.append(f"NAPCAT_INJECT_PATH={PathFunc().napcat_path / 'NapCatWinBootHook.dll'}")
        env.append(f"NAPCAT_LAUNCHER_PATH={PathFunc().napcat_path / 'NapCatWinBootMain.exe'}")
        env.append(f"NAPCAT_MAIN_PATH={PathFunc().napcat_path / 'napcat.mjs'}")
        logger.info(f"NapCatQQ 进程环境变量已配置(QQID: {config.bot.QQID})")

        # 写入 loadNapCat.js 文件
        with open(str(PathFunc().napcat_path / "loadNapCat.js"), "w") as file:
            file.write("(async () => {await import(" f"'{ (PathFunc().napcat_path / 'napcat.mjs').as_uri() }'" ")})()")
        logger.info(f"NapCatQQ 进程加载脚本已写入(QQID: {config.bot.QQID})")

        # 创建 QProcess 并配置
        process = QProcess()
        process.setEnvironment(env)
        process.setProgram(str(PathFunc().napcat_path / "NapCatWinBootMain.exe"))
        process.setArguments(
            [
                str(PathFunc().get_qq_path() / "QQ.exe"),
                str(PathFunc().napcat_path / "NapCatWinBootHook.dll"),
                str(config.bot.QQID),
            ]
        )

        # 链接信号
        process.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)
        process.readyReadStandardOutput.connect(log.handle_output)

        # 启动进程
        process.start()
        logger.info(f"NapCatQQ 进程已创建并启动(QQID: {config.bot.QQID})")

        # 确保进程已启动
        if not process.waitForStarted(5000):
            error_bar(f"NapCatQQ 进程启动失败!")
            return

        # 添加到进程字典
        self.napcat_process_dict[str(config.bot.QQID)] = process

        # 发出新进程创建信号
        self.process_changed_signal.emit(str(config.bot.QQID), process.state())

    def get_process(self, qq_id: str) -> QProcess | None:
        """获取指定 QQ 号的 QProcess

        Args:
            qq_id (str): QQ 号

        Returns:
            QProcess | None: 对应的 QProcess 对象, 如果不存在则返回 None
        """
        return self.napcat_process_dict.get(qq_id, None)

    def stop_process(self, qq_id: str) -> None:
        """停止指定 QQ 号的 QProcess

        Args:
            qq_id (str): QQ 号
        """
        process = self.get_process(qq_id)

        if (parent := psutil.Process(process.processId())).pid != 0:
            [child.kill() for child in parent.children(recursive=True)]
            parent.kill()
            process.kill()
            process.waitForFinished()
            process.deleteLater()
            self.napcat_process_dict.pop(qq_id)

        logger.info(f"NapCatQQ 进程已停止(QQID: {qq_id})")
        self.process_changed_signal.emit(qq_id, QProcess.ProcessState.NotRunning)

    def get_memory_usage(self, qq_id: str) -> int:
        """获取指定 QQ 号的 NapCatQQ 进程树内存使用情况"""
        if not (process := self.get_process(qq_id)) or process.state() != QProcess.ProcessState.Running:
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

                processed_pids.add(pid)

                ps_process = psutil.Process(pid)
                memory_info = psutil.Process(pid).memory_info()
                total_memory += memory_info.rss

                # 添加子进程到队列
                for child in ps_process.children():
                    if child.pid not in processed_pids:
                        queue.append(child.pid)

            return int(total_memory / (1024 * 1024))

        except (psutil.NoSuchProcess, psutil.AccessDenied):
            return 0


manager_process = ManagerNapCatQQProcess()

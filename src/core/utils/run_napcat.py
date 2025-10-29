# -*- coding: utf-8 -*-
"""
## 运行 NapCat 流程
"""
# 标准库导入
import re
from abc import ABC
from calendar import c
from collections import deque
from dataclasses import dataclass
from time import monotonic

# 第三方库导入
import psutil
from creart import add_creator, exists_module, it
from creart.creator import AbstractCreator, CreateTargetInfo
from PySide6.QtCore import QObject, QProcess, Signal

# 项目内模块导入
from src.core.config.config_model import Config
from src.core.utils.logger import logger
from src.core.utils.path_func import PathFunc
from src.ui.components.info_bar import error_bar


# ==================== 数据模型 ====================
@dataclass
class NapCatProcessModel:
    """NapCat 进程数据模型"""

    qq_id: str
    process: QProcess
    state: QProcess.ProcessState = QProcess.ProcessState.NotRunning
    started_at: float = 0.0


# ==================== 工具类 ====================
class NapCatQQProcessLog(QObject):
    """进程的日志功能"""

    handle_output_log_signal = Signal(str)

    def __init__(self, config: Config, process: QProcess) -> None:
        super().__init__()
        # 设置属性
        self._config = config
        self._process = process

        # 日志存储
        self._log_storage = deque(maxlen=10000)

        # 连接信号
        self._process.readyReadStandardOutput.connect(self.handle_output)

    # ==================== 公共函数===================
    def get_log_content(self) -> str:
        """返回所有 log"""
        return "".join(self._log_storage)

    def clear(self) -> None:
        """清理所有 log"""
        self._log_storage.clear()

    # ==================== 响应函数===================
    def handle_output(self):
        """处理日志数据"""
        # 拿到解码后的数据
        data = self._process.readAllStandardOutput().data().decode()
        # 正则处理转义吗
        data = re.compile(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])").sub("", data)
        data = re.compile(r"\r+\n$").sub("\n", data)
        self._log_storage.append(data)
        self.handle_output_log_signal.emit(data)


class NapCatQQLogManager(QObject):
    """NapCatQQ 日志管理器"""

    def __init__(self) -> None:
        super().__init__()
        self.napcat_log_dict: dict[str, NapCatQQProcessLog] = {}

    # ==================== 公共函数===================
    def create_log(self, config: Config, process: QProcess) -> None:
        """创建指定 QQ 号的日志缓冲区

        Args:
            config (Config): 配置对象
            process (QProcess): NapCatQQ 进程对象
        """
        self.napcat_log_dict[str(config.bot.QQID)] = NapCatQQProcessLog(config, process)

    def get_log(self, qq_id: str) -> NapCatQQProcessLog | None:
        """获取指定 QQ 号的日志缓冲区

        Args:
            qq_id (str): QQ 号

        Returns:
            NapCatQQProcessLogger | None: 对应的日志缓冲区对象, 如果不存在则返回 None
        """
        return self.napcat_log_dict.get(qq_id, None)


class ManagerNapCatQQLoginState(QObject):
    """管理NapCatQQ登录状态"""

    def __init__(self) -> None:
        super().__init__()


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
        self.napcat_process_dict: dict[str, NapCatProcessModel] = {}
        logger.info("NapCatQQ 进程管理器已初始化")

    # ==================== 私有函数===================
    def _get_env_variable(self) -> list[str]:
        """获取环境变量"""
        env = QProcess.systemEnvironment()
        env.append(f"NAPCAT_PATCH_PACKAGE={PathFunc().napcat_path / 'qqnt.json'}")
        env.append(f"NAPCAT_LOAD_PATH={PathFunc().napcat_path / 'loadNapCat.js'}")
        env.append(f"NAPCAT_INJECT_PATH={PathFunc().napcat_path / 'NapCatWinBootHook.dll'}")
        env.append(f"NAPCAT_LAUNCHER_PATH={PathFunc().napcat_path / 'NapCatWinBootMain.exe'}")
        env.append(f"NAPCAT_MAIN_PATH={PathFunc().napcat_path / 'napcat.mjs'}")

        return env

    def _write_load_script(self) -> None:
        """写入 loadNapCat.js 脚本文件"""
        with open(str(PathFunc().napcat_path / "loadNapCat.js"), "w") as file:
            file.write("(async () => {await import(" f"'{ (PathFunc().napcat_path / 'napcat.mjs').as_uri() }'" ")})()")
        logger.info("NapCatQQ 进程加载脚本已写入")

    def _create_napcat_process(self, config: Config) -> QProcess:
        """创建并配置 QProcess

        Args:
            config (Config): 配置对象

        Returns:
            QProcess: 配置好的 QProcess 对象
        """
        # 写入 loadNapCat.js 文件
        self._write_load_script()

        # 创建 QProcess 并配置
        process = QProcess()
        process.setEnvironment(self._get_env_variable())
        process.setProgram(str(PathFunc().napcat_path / "NapCatWinBootMain.exe"))
        process.setArguments(
            [
                str(PathFunc().get_qq_path() / "QQ.exe"),
                str(PathFunc().napcat_path / "NapCatWinBootHook.dll"),
                str(config.bot.QQID),
            ]
        )
        process.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)

        return process

    # ==================== 公共函数===================
    def create_napcat_process(self, config: Config) -> None:
        """创建并配置 QProcess

        Args:
            config (Config): 配置对象
            log (NapCatQQProcessLogger): 日志缓冲对象(需要实例)

        Returns:
            QProcess: 配置好的 QProcess 对象
        """
        # 创建 QProcess
        process = self._create_napcat_process(config)

        # 进行一些操作
        it(NapCatQQLogManager).create_log(config, process)

        # 启动进程
        process.start()
        logger.info(f"NapCatQQ 进程已创建并启动(QQID: {config.bot.QQID})")

        # 确保进程已启动
        if not process.waitForStarted(5000):
            error_bar(f"NapCatQQ 进程启动失败!")
            return

        # 添加到进程字典
        self.napcat_process_dict[str(config.bot.QQID)] = NapCatProcessModel(
            qq_id=str(config.bot.QQID), process=process, state=QProcess.ProcessState.Running, started_at=monotonic()
        )

        # 发出新进程创建信号
        self.process_changed_signal.emit(str(config.bot.QQID), process.state())

    def get_process(self, qq_id: str) -> NapCatProcessModel | None:
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
        process = self.get_process(qq_id).process

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
        if not (process := self.get_process(qq_id).process) or process.state() != QProcess.ProcessState.Running:
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

                for child in psutil.Process(pid).children():
                    if child.pid not in processed_pids:
                        queue.append(child.pid)
                processed_pids.add(pid)

            return int((total_memory := total_memory + psutil.Process(pid).memory_info().rss) / (1024 * 1024))

        except (psutil.NoSuchProcess, psutil.AccessDenied):
            return 0


# ==================== 创建器 ====================
class ManagerNapCatQQLogManagerCreator(AbstractCreator, ABC):
    """NapCatQQ 日志管理器创建器"""

    targets = (CreateTargetInfo("src.core.utils.run_napcat", "NapCatQQLogManager"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.core.utils.run_napcat")

    @staticmethod
    def create(create_type: type[NapCatQQLogManager]) -> NapCatQQLogManager:
        """创建 NapCatQQLogManager 实例"""
        return create_type()


add_creator(ManagerNapCatQQLogManagerCreator)


class ManagerNapCatQQLoginStateCreator(AbstractCreator, ABC):
    """NapCatQQ 登录状态管理器创建器"""

    targets = (CreateTargetInfo("src.core.utils.run_napcat", "ManagerNapCatQQLoginState"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.core.utils.run_napcat")

    @staticmethod
    def create(create_type: type[ManagerNapCatQQLoginState]) -> ManagerNapCatQQLoginState:
        """创建 ManagerNapCatQQLoginState 实例"""
        return create_type()


add_creator(ManagerNapCatQQLoginStateCreator)


class ManagerNapCatQQProcessCreator(AbstractCreator, ABC):
    """NapCatQQ 进程管理器创建器"""

    targets = (CreateTargetInfo("src.core.utils.run_napcat", "ManagerNapCatQQProcess"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.core.utils.run_napcat")

    @staticmethod
    def create(create_type: type[ManagerNapCatQQProcess]) -> ManagerNapCatQQProcess:
        """创建 ManagerNapCatQQProcess 实例"""
        return create_type()


add_creator(ManagerNapCatQQProcessCreator)

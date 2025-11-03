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
from time import monotonic

# 第三方库导入
import psutil
from creart import add_creator, exists_module, it
from creart.creator import AbstractCreator, CreateTargetInfo
from httpx import Client, post
from PySide6.QtCore import QObject, QProcess, QRunnable, QThreadPool, QTimer, Signal

# 项目内模块导入
from src.core.config.config_model import Config
from src.core.utils.logger import logger
from src.core.utils.path_func import PathFunc
from src.core.network.email import offline_email
from src.core.network.webhook import offline_webhook
from src.core.config import cfg


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
        return "".join(self._log_storage)

    def clear(self) -> None:
        """清理所有 log"""
        self._log_storage.clear()

    # ==================== 响应函数===================
    def handle_output(self):
        """处理日志数据"""
        # 拿到解码后的数据
        data = self._process.readAllStandardOutput().data().decode()
        # 正则处理转义字符
        data = re.compile(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])").sub("", data)
        data = re.compile(r"\r+\n$").sub("\n", data)
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


class ManagerNapCatQQLog(QObject):
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

    def run(self):
        response = post(
            f"http://localhost:{self.port}/api/auth/login",
            json={"hash": hashlib.sha256((self.token + ".napcat").encode("utf-8")).hexdigest()},
            headers={"Content-Type": "application/json"},
        )
        if response.status_code == 200:
            self.login_auth_signal.emit(response.json().get("data", {}).get("Credential", ""))


class GetLoginStatusRunnable(QObject, QRunnable):
    """获取 NapCatQQ 登录状态的任务类"""

    # 信号
    login_status_signal = Signal(bool)
    login_qrcode_signal = Signal(str)
    online_status_signal = Signal(bool)

    def __init__(self, port: int, token: str, auth: str) -> None:
        """获取 NapCatQQ Auth 信息的任务类

        Args:
            port (int): WebUI 端口
            token (str): WebUI Token
            auth (str): 认证信息
        """
        QObject.__init__(self)
        QRunnable.__init__(self)
        # 设置属性
        self.port = port
        self.token = token
        self.auth = auth

    def run(self) -> None:
        """执行获取认证信息的任务"""
        # 创建 HTTP 客户端
        self.client = Client(base_url=f"http://localhost:{self.port}", timeout=5)
        self.client.headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {self.auth}",
        }

        # # 获取登录状态
        self.get_login_status()
        # 获取在线状态
        self.get_online_status()

    def get_login_status(self) -> None:
        """获取 NapCatQQ 登录状态"""
        if (response := self.client.post("/api/QQLogin/CheckLoginStatus")).status_code != 200:
            return

        # 解析结果
        result = response.json().get("data", {})
        is_login = result.get("isLogin", False)
        qr_code = result.get("qrcodeurl", "")

        # 发出信号
        self.login_status_signal.emit(is_login)

        if not is_login and qr_code:
            self.login_qrcode_signal.emit(qr_code)

    def get_online_status(self) -> None:
        """获取 NapCatQQ 在线状态"""
        if (response := self.client.post("/api/QQLogin/GetQQLoginInfo")).status_code == 200:
            result = response.json().get("data", {})
            self.online_status_signal.emit(result.get("online", False))


class NapCatQQLoginState(QObject):
    """NapCatQQ 登录状态类

    负责管理 NapCatQQ 的登录状态
    """

    def __init__(self, config: Config, port: int, token: str) -> None:
        """初始化 NapCatQQ 登录状态"""
        super().__init__()
        # 设置属性
        self.config = config
        self.port = port
        self.token = token
        self.auth = None

        # 登录状态属性
        self._is_logged_in = False
        self._online_status = False
        self._offline_notice = False

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

    # ==================== 槽函数 ====================
    def slot_get_login_state(self) -> None:
        """获取登录状态"""
        runner = GetLoginStatusRunnable(port=self.port, token=self.token, auth=self.auth)
        runner.login_status_signal.connect(self.slot_update_login_state)
        runner.online_status_signal.connect(self.slot_update_online_status)
        runner.login_qrcode_signal.connect(self.slot_update_login_qrcode)
        QThreadPool.globalInstance().start(runner)

    def slot_get_auth_status(self) -> None:
        """获取认证状态"""
        runner = GetAuthStatusRunnable(port=self.port, token=self.token)
        runner.login_auth_signal.connect(self.slot_update_auth)
        QThreadPool.globalInstance().start(runner)

    def slot_update_auth(self, auth: str) -> None:
        """更新认证信息

        Args:
            auth (str): 认证信息
        """
        self.auth = auth

    def slot_update_login_state(self, is_login: bool) -> None:
        """更新登录状态

        Args:
            is_login (bool): 是否已登录
        """
        self._is_logged_in = is_login

        if is_login:
            from src.ui.page.bot_page.widget.msg_box import QRCodeDialogFactory

            it(QRCodeDialogFactory).remove_qr_code(str(self.config.bot.QQID))

    def slot_update_online_status(self, online_status: bool) -> None:
        """更新在线状态

        Args:
            online_status (bool): 是否在线
        """
        self._online_status = online_status

        if online_status or not self._is_logged_in:
            return

        if self._offline_notice:
            return

        # 离线通知
        if self.config.advanced.offlineNotice:
            return

        if cfg.get(cfg.bot_offline_web_hook_notice):
            offline_webhook(self.config)
        if cfg.get(cfg.bot_offline_email_notice):
            offline_email(self.config)

        # 更改状态
        self._offline_notice = True

    def slot_update_login_qrcode(self, qr_code: str) -> None:
        """更新登录二维码

        Args:
            qr_code (str): 登录二维码
        """
        from src.ui.page.bot_page.widget.msg_box import QRCodeDialogFactory

        it(QRCodeDialogFactory).add_qr_code(str(self.config.bot.QQID), qr_code)


class ManagerNapCatQQLoginState(QObject):
    """NapCatQQ 登录状态管理类

    负责管理 NapCatQQ 的登录状态
    """

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
        self.napcat_login_state_dict[str(config.bot.QQID)] = NapCatQQLoginState(config=config, port=port, token=token)

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
        # 如果超过 4 个进程，则取消创建
        if len(self.napcat_process_dict) >= 4:
            from src.ui.components.info_bar import error_bar

            error_bar("NapCatQQ 进程数量已达上限，无法创建新进程!")
            return

        # 创建 QProcess
        process = self._create_napcat_process(config)

        # 进行一些操作
        it(ManagerNapCatQQLog).create_log(config, process)

        # 启动进程
        process.start()
        logger.info(f"NapCatQQ 进程已创建并启动(QQID: {config.bot.QQID})")

        # 确保进程已启动
        if not process.waitForStarted(5000):
            from src.ui.components.info_bar import error_bar

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

    def has_running_bot(self) -> bool:
        """检查是否有正在运行的 Bot

        Returns:
            bool: 如果有正在运行的 Bot 则返回 True, 否则返回 False
        """
        return any(
            process_model.state == QProcess.ProcessState.Running for process_model in self.napcat_process_dict.values()
        )

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

        it(ManagerNapCatQQLoginState).remove_login_state(qq_id)

        logger.info(f"NapCatQQ 进程已停止(QQID: {qq_id})")
        self.process_changed_signal.emit(qq_id, QProcess.ProcessState.NotRunning)

    def stop_all_processes(self) -> None:
        """停止所有 NapCatQQ 进程"""
        for qq_id in list(self.napcat_process_dict.keys()):
            self.stop_process(qq_id)

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

    targets = (CreateTargetInfo("src.core.utils.run_napcat", "ManagerNapCatQQLog"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.core.utils.run_napcat")

    @staticmethod
    def create(create_type: type[ManagerNapCatQQLog]) -> ManagerNapCatQQLog:
        """创建 ManagerNapCatQQLog 实例"""
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

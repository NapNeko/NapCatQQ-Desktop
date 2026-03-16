# -*- coding: utf-8 -*-
# 标准库导入
import json
from dataclasses import dataclass
from datetime import datetime
from string import Template

# 第三方库导入
import httpx
from PySide6.QtCore import QObject, QRunnable, Signal

# 项目内模块导入
from src.core.config import cfg
from src.core.config.config_model import Config
from src.core.utils.logger import logger


@dataclass
class WebHookData:
    url: str
    secret: str
    json: str

    def __init__(self, url: str | None = None, secret: str | None = None, json: str | None = None) -> None:
        """初始化 WebHookData, 动态从配置中获取值

        Args:
            url (str | None, optional): WebHook 地址. Defaults to None.
            secret (str | None, optional): WebHook 密钥. Defaults to None.
            json (str | None, optional): WebHook JSON 内容. Defaults to None.
        """
        self.url = url or cfg.get(cfg.web_hook_url)
        self.secret = secret or cfg.get(cfg.web_hook_secret)
        self.json = json or cfg.get(cfg.web_hook_json)


class WebHook(QObject, QRunnable):
    """WebHook 发送类

    继承自 QObject 和 QRunnable, 用于在子线程中发送 WebHook 请求
    通过信号与槽机制反馈结果, 避免阻塞主线程
    """

    error_signal = Signal(str)
    success_signal = Signal(str)

    def __init__(self, data: WebHookData) -> None:
        """初始化 WebHook 发送任务

        Args:
            data (WebHookData): WebHook 数据类
        """
        super().__init__()
        QRunnable.__init__(self)
        self.data = data

        # 自动销毁
        self.setAutoDelete(True)

    def run(self) -> None:
        """发送 WebHook 请求"""
        try:
            if not self.validate_and_format_json():
                self.error_signal.emit("无效的 JSON 内容")
                return

            logger.info("发送 WebHook 请求")

            headers = (
                {
                    "Authorization": f"Bearer {self.data.secret}",
                    "Content-Type": "application/json",
                }
                if self.data.secret
                else {
                    "Content-Type": "application/json",
                }
            )

            logger.debug(f"WebHook URL: {self.data.url}")
            logger.debug(f"WebHook Headers: {headers}")
            logger.debug(f"WebHook JSON: {self.data.json}")

            response = httpx.post(self.data.url, json=self.data.json, headers=headers, timeout=10.0)
            response.raise_for_status()
            self.success_signal.emit("WebHook 发送成功")

            logger.info(f"WebHook 请求成功, 状态码: {response.status_code}")

        except (httpx.RequestError, httpx.HTTPStatusError, Exception) as e:
            self.error_signal.emit(f"{e.__class__.__name__}: {str(e)}")
            logger.error(f"WebHook 请求错误: {e.__class__.__name__}: {str(e)}")

    def validate_and_format_json(self) -> bool:
        """验证并格式化 JSON 内容"""
        try:
            self.data.json = json.dumps(json.loads(self.data.json), indent=4)
            return True
        except json.JSONDecodeError:
            return False


def create_test_webhook_task() -> WebHook:
    """构建测试 WebHook 任务, 由调用方决定如何处理信号和启动任务"""

    return WebHook(WebHookData(json='{"text": "Hello, World!"}'))


def create_offline_webhook_task(config: Config) -> WebHook:
    """构建离线通知 WebHook 任务, 由调用方决定如何处理信号和启动任务"""

    return WebHook(
        WebHookData(
            json=Template(cfg.get(cfg.web_hook_json)).safe_substitute(
                {
                    "bot_name": config.bot.name,
                    "bot_qq_id": config.bot.QQID,
                    "disconnect_time": datetime.now().strftime("%Y/%m/%d %H:%M:%S"),
                }
            ),
        )
    )

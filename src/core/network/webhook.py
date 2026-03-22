# -*- coding: utf-8 -*-
# 标准库导入
import json
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from datetime import datetime
from string import Template
from typing import Any

# 第三方库导入
import httpx
from PySide6.QtCore import QObject, QRunnable, Signal

# 项目内模块导入
from src.core.config import cfg
from src.core.config.config_model import Config
from src.core.logging import LogSource, LogType, logger


@dataclass
class WebHookData:
    url: str
    secret: str
    json: str

    def __init__(self, url: str | None = None, secret: str | None = None, json: str | None = None) -> None:
        """初始化 WebHookData, 动态从配置中获取值

        Args:
            url (str | None): WebHook 地址，可为 None。
            secret (str | None): WebHook 密钥，可为 None。
            json (str | None): WebHook JSON 内容，可为 None。
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

    @staticmethod
    def _summarize_payload(payload: Mapping[str, Any] | Sequence[Any] | str | int | float | bool | None) -> str:
        """生成便于调试的请求体摘要，避免直接输出完整内容。"""
        if isinstance(payload, Mapping):
            keys = list(payload.keys())
            preview = ", ".join(str(key) for key in keys[:6])
            suffix = "" if len(keys) <= 6 else ", ..."
            return f"object(len={len(keys)}, keys=[{preview}{suffix}])"

        if isinstance(payload, Sequence) and not isinstance(payload, (str, bytes, bytearray)):
            return f"array(len={len(payload)})"

        return f"{type(payload).__name__}"

    def run(self) -> None:
        """发送 WebHook 请求"""
        try:
            is_valid, payload = self.validate_and_format_json()
            if not is_valid:
                self.error_signal.emit("无效的 JSON 内容")
                logger.warning("WebHook JSON 校验失败", LogType.NETWORK, LogSource.CORE)
                logger.trace(
                    f"WebHook JSON 原始内容摘要: chars={len(self.data.json)}, url={self.data.url}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                return

            payload_size = len(self.data.json.encode("utf-8"))
            logger.info(
                (
                    "发送 WebHook 请求: "
                    f"configured={bool(self.data.url)}, "
                    f"has_auth={bool(self.data.secret)}, "
                    f"payload_bytes={payload_size}"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            logger.trace(
                (
                    "WebHook 请求细节: "
                    f"url={self.data.url}, has_auth={bool(self.data.secret)}, "
                    f"payload_summary={self._summarize_payload(payload)}"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )

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

            response = httpx.post(self.data.url, json=payload, headers=headers, timeout=10.0)
            response.raise_for_status()
            self.success_signal.emit("WebHook 发送成功")

            logger.info(f"WebHook 请求成功, status_code={response.status_code}", LogType.NETWORK, LogSource.CORE)

        except (httpx.RequestError, httpx.HTTPStatusError, Exception) as e:
            self.error_signal.emit(f"{e.__class__.__name__}: {str(e)}")
            logger.error(f"WebHook 请求错误: {e.__class__.__name__}", LogType.NETWORK, LogSource.CORE)

    def validate_and_format_json(
        self,
    ) -> tuple[bool, Mapping[str, Any] | Sequence[Any] | str | int | float | bool | None]:
        """验证并规范化 JSON 内容，返回可直接发送的 Python 对象。"""
        try:
            payload = json.loads(self.data.json)
            self.data.json = json.dumps(payload, indent=4, ensure_ascii=False)
            return True, payload
        except json.JSONDecodeError:
            return False, None


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


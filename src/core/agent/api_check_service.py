# -*- coding: utf-8 -*-
"""API 连通性检测服务.

使用 QThread + httpx 同步客户端实现非阻塞的 API 连通性检测，
向供应商的 /models 端点发送 GET 请求以验证 API 密钥和地址是否可用。
"""

from __future__ import annotations

import httpx
from PySide6.QtCore import QThread, Signal


class ApiCheckService(QThread):
    """异步 API 连通性检测线程.

    通过 QThread 在后台执行 HTTP 请求，避免阻塞 UI 线程。
    使用 httpx.Client（同步）发送请求，因为运行在独立的 QThread 中。

    Signals:
        check_started: 检测开始时发射.
        check_finished: 检测完成时发射，参数为 (success: bool, message: str).
    """

    check_started = Signal()
    check_finished = Signal(bool, str)

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self._api_base_url: str = ""
        self._api_key: str = ""

    def start_check(self, api_base_url: str, api_key: str) -> None:
        """启动 API 连通性检测.

        存储参数并启动线程执行检测。

        Args:
            api_base_url: API 基础地址（如 https://api.openai.com/v1）.
            api_key: API 密钥.
        """
        self._api_base_url = api_base_url
        self._api_key = api_key
        self.start()

    def run(self) -> None:
        """线程执行体：发送 HTTP GET 请求检测 API 连通性."""
        self.check_started.emit()

        url = f"{self._api_base_url.rstrip('/')}/models"
        headers = {"Authorization": f"Bearer {self._api_key}"}

        try:
            with httpx.Client(timeout=10.0) as client:
                response = client.get(url, headers=headers)

            if response.is_success:
                self.check_finished.emit(True, "连接成功")
            elif response.status_code in (401, 403):
                self.check_finished.emit(False, "API 密钥无效")
            else:
                self.check_finished.emit(False, f"HTTP {response.status_code} 错误")

        except httpx.TimeoutException:
            self.check_finished.emit(False, "连接超时")
        except httpx.ConnectError:
            self.check_finished.emit(False, "无法连接到服务器")
        except httpx.HTTPError:
            self.check_finished.emit(False, "无法连接到服务器")

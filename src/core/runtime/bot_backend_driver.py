# -*- coding: utf-8 -*-
"""Backend driver 抽象基类 (Tier I, P2 SnowLuma WebUI 编程客户端化).

为两个具体 driver 实现提供统一接口:

- :class:`NapCatDriver` (``napcat_driver.py``): NapCat 注入式 (NTQQ 加载 ``napcat.cjs``)
- :class:`SnowLumaDriver` (``snowluma_driver.py``): SnowLuma WebUI 客户端 + spawn 独立
  ``QQ.exe`` + 注入

每个 driver 持自己的 process model 字典, 由 :class:`BotProcessManager` (在
``bot_process_manager.py``) 按 ``config.bot.backend_type`` dispatch.

P1 妥协决策反转记录: P1 §3 / §8 当初决定 "不重写 Backend 抽象", 因为 SnowLuma 当时只占
~115 行; P2 在加 Tier D-G (~1000 行) 后选择把 NapCat / SnowLuma 各自抽成独立 driver,
让 :class:`BotProcessManager` 只负责 dispatch, 不再混合两套 QProcess 创建逻辑. 详见:
``docs/requirements/2026-05-10-snowluma-bot-form-backend-aware.md`` §2.15 / D-I-1.
"""
from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional

if TYPE_CHECKING:
    from PySide6.QtCore import QProcess

    from src.core.config.config_model import Config


@dataclass
class ProcessHandle:
    """统一进程句柄 (NapCat 单 QProcess 或 SnowLuma per-Bot QQ.exe).

    Attributes:
        qq_id: Bot 的 QQ 号 (字符串, 与现有 dict key 对齐)
        primary_process: 当前 Bot **由 Desktop 自己 spawn** 的 ``QProcess``.

            - NapCat: ``NapCatWinBootMain.exe`` (单 QProcess 注入式)
            - SnowLuma **COLD 模式**: Desktop spawn 的 ``QQ.exe``
            - SnowLuma **HOT 模式**: ``None`` (Desktop 不拥有 QQ.exe; SnowLuma daemon
              通过 ``attach_pid`` 注入到用户已有 QQ.exe, manager 不再能通过
              ``QProcess.finished`` 信号感知 QQ.exe 退出, 改由 :class:`SnowLumaStatusPoller`
              的 ``state_changed("disconnected")`` 通道兜底)
        secondary_process: 历史用于 SnowLuma 的 ``node.exe`` 子进程; **P2 W2 后**
            ``node.exe`` 由 :class:`SnowLumaDaemon` 全局共享, 不再属于单 Bot;
            因此 SnowLuma 路径下本字段总为 ``None``. NapCat 路径下也为 ``None``.
            字段保留是为了不破坏字段元数据 (旧调用方读到 None 即可).
    """

    qq_id: str
    primary_process: Optional["QProcess"]
    secondary_process: Optional["QProcess"] = None


class BotBackendDriver(ABC):
    """Bot 后端启动 / 停止 / 状态抽象基类 (Tier I).

    具体实现见 :class:`NapCatDriver` / :class:`SnowLumaDriver`.
    """

    @abstractmethod
    def start(self, config: "Config") -> ProcessHandle:
        """启动 Bot 进程, 返回统一进程句柄.

        Args:
            config: 完整 :class:`Config` 对象, driver 仅消费 ``config.bot`` /
                ``config.connect`` 等与启动相关的子结构.

        Returns:
            :class:`ProcessHandle` 描述刚启动的进程句柄. 调用方 (
            :class:`BotProcessManager`) 据此完成 signal 连接 / 进程注册.

        Raises:
            FileNotFoundError: 启动器路径不可用 (例如未检测到 ``QQ.exe`` /
                ``node.exe``).
            RuntimeError: 启动失败的其他原因 (注入失败 / 单实例守护命中等).
        """

    @abstractmethod
    def stop(self, qq_id: str) -> None:
        """停止指定 QQ 号的进程, 反向清理资源.

        实现需要保证调用幂等 (对未在跑的 ``qq_id`` 静默返回).
        """

    @abstractmethod
    def is_running(self, qq_id: str) -> bool:
        """探测指定 QQ 号当前是否在跑."""

    @abstractmethod
    def get_status_poller(self, qq_id: str):
        """返回该 Bot 的状态轮询器实例.

        - NapCat 路径: 返回 ``None`` (其登录态由 :class:`ManagerNapCatQQLoginState`
          独立管理, 不走 driver 层 poller).
        - SnowLuma 路径: 返回 :class:`SnowLumaStatusPoller` 实例 (W5 重写后);
          调用方据此连 ``state_changed`` 信号转发到 BotCard.
        """

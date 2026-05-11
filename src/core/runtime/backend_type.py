# -*- coding: utf-8 -*-
"""SnowLuma 适配 P1: Bot 后端类型枚举.

提供给 [`BotConfig.backend_type`](../config/config_model.py) 与
[`BotProcessManager._create_*_process`](./napcat.py) 在
NapCat (NTQQ 注入式) 与 SnowLuma (独立 Node 进程) 两种后端之间分流.

参见: ``docs/requirements/2026-05-10-snowluma-backend-adapter.md`` §2.1.
"""
from __future__ import annotations

from enum import Enum


class BackendType(str, Enum):
    """Bot 后端类型枚举.

    - ``NAPCAT``: NapCatQQ, 基于 NTQQ 进程注入 (默认; 历史行为).
    - ``SNOWLUMA``: SnowLuma, 独立 Node 进程, 自带 ``node.exe`` 的发布包;
      启动后用户在 SnowLuma WebUI 内扫码登录.
    """

    NAPCAT = "napcat"
    SNOWLUMA = "snowluma"

    @classmethod
    def from_str(cls, value: str | None) -> "BackendType":
        """从字符串构造, ``None`` / 未知字符串均降级为 ``NAPCAT``.

        用于反序列化 bot.json 时的 backwards-compatibility:
        旧配置文件没有 backend_type 字段, 应当视为 NapCat (历史行为).
        """
        if not value:
            return cls.NAPCAT
        try:
            return cls(value)
        except ValueError:
            return cls.NAPCAT

    @property
    def display_name(self) -> str:
        """UI 上展示的人类可读名称."""
        return {
            BackendType.NAPCAT: "NapCat",
            BackendType.SNOWLUMA: "SnowLuma",
        }[self]

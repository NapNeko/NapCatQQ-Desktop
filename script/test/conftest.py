# -*- coding: utf-8 -*-

# 标准库导入
import os
import sys
from pathlib import Path

# 第三方库导入
import pytest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

PROJECT_ROOT = Path(__file__).resolve().parents[2]

if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

# 项目内模块导入
from src.desktop.core.config.config_model import (
    AdvancedConfig,
    AutoRestartScheduleConfig,
    BotConfig,
    Config,
    ConnectConfig,
)


def make_config(qqid: int = 114514, name: str = "TestBot") -> Config:
    """构造测试使用的完整配置对象。"""
    return Config(
        bot=BotConfig(
            name=name,
            QQID=qqid,
            musicSignUrl=f"https://example.com/music/{qqid}",
            autoRestartSchedule=AutoRestartScheduleConfig(enable=True, time_unit="h", duration=2),
            offlineAutoRestart=False,
        ),
        connect=ConnectConfig(
            httpServers=[],
            httpSseServers=[],
            httpClients=[],
            websocketServers=[],
            websocketClients=[],
            plugins=[],
        ),
        advanced=AdvancedConfig(
            autoStart=False,
            offlineNotice=True,
            parseMultMsg=False,
            packetServer="ws://127.0.0.1:3001",
            enableLocalFile2Url=False,
            fileLog=True,
            consoleLog=False,
            fileLogLevel="debug",
            consoleLogLevel="info",
            o3HookMode=1,
        ),
    )


@pytest.fixture
def config_factory():
    """返回一个可按需构造 Config 的工厂函数。"""
    return make_config

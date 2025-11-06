# -*- coding: utf-8 -*-
# 标准库导入
import random
import string
from typing import Literal

# 第三方库导入
from pydantic import BaseModel, Field, HttpUrl, WebsocketUrl, field_validator

from .config_enum import TimeUnitEnum

"""
这个模块包含的是机器人配置模型

每个配置模型都继承自 pydantic 的 BaseModel, 用于数据验证和序列化
每个配置模型都可以包含多个字段, 每个字段都对应一个配置项
每个字段都可以指定类型, 默认值, 验证器等

注意:
- 配置模型主要用于定义与机器人相关的配置项, 如机器人名称, QQ号, 连接方式等
- 配置模型可以嵌套, 如 ConnectConfig 包含多个网络连接配置模型
- 配置模型可以与 JSON 等格式互相转换, 方便存储和读取配置文件
- 具体使用方法可以参考 pydantic 的文档
- 此文件可以不遵守 naming_convention 中的命名规范, 因为 NapCatQQ 配置项采用驼峰体命名, 以保持与 OneBot 标准的一致
- 如果使用 snake_case 命名, 则需要在读取配置文件时进行转换, 增加不必要的复杂度
"""


def _coerce_interval_default(v, default: int = 30000) -> int:
    """将可能来自配置的间隔值规范化为整数，无法解析时返回默认值。

    行为与之前重复实现保持一致：
    - None -> default
    - 空字符串或仅空白 -> default
    - 纯数字字符串 -> 转为 int
    - 其他无法转换的值 -> default
    """
    if v is None:
        return default
    if isinstance(v, str):
        val = v.strip()
        if val == "":
            return default
        try:
            return int(val)
        except ValueError:
            return default
    try:
        return int(v)
    except (TypeError, ValueError):
        return default


class AutoRestartScheduleConfig(BaseModel):
    """自动重启计划配置"""

    # 是否启用自动重启计划任务
    enable: bool = Field(
        default=False,
        description="是否启用自动重启计划任务",
    )
    # 时间单位
    time_unit: TimeUnitEnum = Field(
        default=TimeUnitEnum.HOUR,
        description="时间单位, m: 分钟, h: 小时, d: 天, mon: 月, year: 年",
    )
    # 时间长度, 不包含单位
    duration: int = Field(
        default=6,
        description="时间长度, 仅包含数字, 不包含单位",
    )


class BotConfig(BaseModel):
    name: str
    QQID: str | int
    musicSignUrl: str = ""
    autoRestartSchedule: AutoRestartScheduleConfig
    offlineAutoRestart: bool = False

    @field_validator("name")
    @staticmethod
    def validate_name(value: str) -> str:
        """验证机器人名称

        如果名称为空, 则生成一个随机的8位字母字符串作为名称

        Args:
            value (str): 机器人名称

        Returns:
            str: 验证后的机器人名称
        """
        if not value:
            return "".join(random.choices(string.ascii_letters, k=8))
        return value

    @field_validator("QQID")
    @staticmethod
    def validate_qqid(value: str | int) -> int:
        """验证QQID

        v1.6.x版本及以前使用的是 str 类型, 现在要处理成 int 类型储存

        Args:
            value (str | int): QQID, QQ号
        Returns:
            int: 验证后的QQID
        """
        # 如果已经是整数，直接通过
        if isinstance(value, int):
            return value

        # 如果是字符串，尝试转换为整数
        if isinstance(value, str):
            try:
                return int(value)
            except ValueError:
                raise ValueError(f"QQ号 '{value}' 无法转换为整数")

        raise TypeError("QQ号必须是字符串或整数")


class NetworkBaseConfig(BaseModel):
    enable: bool = True
    name: str
    messagePostFormat: Literal["array", "string"] = "array"
    token: str = ""
    debug: bool = False


class HttpServersConfig(NetworkBaseConfig):
    host: str
    port: int
    enableCors: bool = False
    enableWebsocket: bool = False


class HttpSseServersConfig(NetworkBaseConfig):
    host: str
    port: int
    enableCors: bool = False
    enableWebsocket: bool = False
    reportSelfMessage: bool = False


class HttpClientsConfig(NetworkBaseConfig):
    url: HttpUrl
    reportSelfMessage: bool = False


class WebsocketServersConfig(NetworkBaseConfig):
    host: str
    port: int
    reportSelfMessage: bool = False
    enableForcePushEvent: bool = False
    heartInterval: int = 30000

    @field_validator("heartInterval", mode="before")
    @classmethod
    def _coerce_heart_interval(cls, v):
        """Coerce heartInterval using the shared helper."""
        return _coerce_interval_default(v, 30000)


class WebsocketClientsConfig(NetworkBaseConfig):
    url: WebsocketUrl
    reportSelfMessage: bool = False
    heartInterval: int = 30000
    reconnectInterval: int = 30000

    @field_validator("heartInterval", "reconnectInterval", mode="before")
    @classmethod
    def _coerce_client_intervals(cls, v):
        """Coerce client intervals using the shared helper."""
        return _coerce_interval_default(v, 30000)


class ConnectConfig(BaseModel):
    httpServers: list[HttpServersConfig]
    httpSseServers: list[HttpSseServersConfig]
    httpClients: list[HttpClientsConfig]
    websocketServers: list[WebsocketServersConfig]
    websocketClients: list[WebsocketClientsConfig]
    plugins: list


class AdvancedConfig(BaseModel):
    autoStart: bool = False
    offlineNotice: bool = False
    parseMultMsg: bool = False
    packetServer: str = ""
    packetBackend: str = Field(default="auto", exclude=True)
    enableLocalFile2Url: bool
    fileLog: bool
    consoleLog: bool
    fileLogLevel: Literal["debug", "info", "error"] = "debug"
    consoleLogLevel: Literal["debug", "info", "error"] = "info"
    o3HookMode: Literal[0, 1] = 1


class Config(BaseModel):
    bot: BotConfig
    connect: ConnectConfig
    advanced: AdvancedConfig


class OneBotConfig(BaseModel):
    network: ConnectConfig
    musicSignUrl: str = ""
    enableLocalFile2Url: bool = False
    parseMultMsg: bool = False


class NapCatConfig(BaseModel):
    fileLog: bool
    consoleLog: bool
    fileLogLevel: str
    consoleLogLevel: str
    packetBackend: str = Field(default="auto", exclude=True)
    packetServer: str
    o3HookMode: Literal[0, 1] = 1


DEFAULT_CONFIG = {
    "bot": {
        "name": "",
        "QQID": "",
        "musicSignUrl": "",
        "autoRestartSchedule": {
            "taskType": "none",
            "interval": "6h",
            "crontab": "0 4 * * *",
            "jitter": 0,
        },
    },
    "connect": {
        "httpServers": [],
        "httpSseServers": [],
        "httpClients": [],
        "websocketServers": [],
        "websocketClients": [],
        "plugins": [],
    },
    "advanced": {
        "autoStart": False,
        "offlineNotice": False,
        "packetServer": "",
        "packetBackend": "auto",
        "enableLocalFile2Url": False,
        "fileLog": False,
        "consoleLog": True,
        "fileLogLevel": "debug",
        "consoleLogLevel": "info",
        "o3HookMode": 1,
    },
}

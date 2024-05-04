import random
import string
from enum import Enum
from typing import List

from pydantic import BaseModel, HttpUrl, WebsocketUrl, field_validator


class ScriptType(Enum):
    """
    ## 脚本类型枚举
    """

    BAT = "bat"
    PS1 = "ps1"
    SH = "sh"


class BotConfig(BaseModel):
    # 需要验证的值
    name: str
    QQID: str
    msgFormat: str
    reportSelfMsg: bool
    heartInterval: str
    accessToken: str

    @field_validator("name")
    @staticmethod
    def validate_name(value):
        # 验证 name 如果为空则生成一个
        if not value:
            return random.choices(string.ascii_letters, k=8)
        return value

    @field_validator("QQID")
    @staticmethod
    def validate_qqid(value):
        # 验证 value 如果为空则抛出 ValueError
        if not value:
            raise ValueError("QQID cannot be empty")
        return value

    @field_validator("heartInterval")
    @staticmethod
    def validate_heartInterval(value):
        # 验证 heartInterval 如果为非数字则抛出 ValueError
        if not value:
            # 如果为空值则不进行验证
            return value
        if not str(value).isdigit():
            raise ValueError("Port must be a number")
        return value


class HttpConfig(BaseModel):
    enable: bool
    addresses: str
    port: str

    @field_validator("port")
    @staticmethod
    def validate_port(value):
        if not value:
            # 如果为空值则不进行验证
            return value
        if not str(value).isdigit():
            # 验证是否为数字
            raise ValueError("Port must be a number")
        return value


class HttpReportConfig(BaseModel):
    enable: bool
    enableHeart: bool
    token: str


class ConnectConfig(BaseModel):
    http: HttpConfig
    httpReport: HttpReportConfig
    httpReportUrls: List[HttpUrl]
    ws: HttpConfig
    wsReverse: bool
    wsReverseUrls: List[WebsocketUrl]


class AdvancedConfig(BaseModel):
    QQPath: str
    startScriptPath: str
    ffmpegPath: str
    debug: bool
    localFile2url: bool
    fileLog: bool
    consoleLog: bool
    fileLogLevel: str
    consoleLogLevel: str


class Config(BaseModel):
    bot: BotConfig
    connect: ConnectConfig
    advanced: AdvancedConfig

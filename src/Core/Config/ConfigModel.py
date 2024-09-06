import random
import string
from enum import Enum
from typing import List, Optional

from pydantic import HttpUrl, BaseModel, WebsocketUrl, field_validator


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
    messagePostFormat: str
    reportSelfMsg: bool
    musicSignUrl: str
    heartInterval: str
    accessToken: Optional[str]

    @field_validator("name")
    @staticmethod
    def validate_name(value):
        # 验证 name 如果为空则生成一个
        if not value:
            return "".join(random.choices(string.ascii_letters, k=8))
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
    host: str
    port: str
    secret: str
    enableHeart: bool
    enablePost: bool
    postUrls: List[Optional[HttpUrl]]

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


class WsConfig(BaseModel):
    enable: bool
    host: str
    port: str


class ReverseWsConfig(BaseModel):
    enable: bool
    urls: List[Optional[WebsocketUrl]]


class ConnectConfig(BaseModel):
    http: HttpConfig
    ws: WsConfig
    reverseWs: ReverseWsConfig


class AdvancedConfig(BaseModel):
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


DEFAULT_CONFIG = {
    "bot": {
        "name": "",
        "QQID": "",
        "messagePostFormat": "array",
        "reportSelfMsg": False,
        "musicSignUrl": "",
        "heartInterval": "30000",
        "accessToken": "",
    },
    "connect": {
        "http": {
            "enable": False,
            "host": "",
            "port": "",
            "secret": "",
            "enableHeart": False,
            "enablePost": False,
            "postUrls": [],
        },
        "ws": {"enable": False, "host": "", "port": 3001},
        "reverseWs": {"enable": False, "urls": []},
    },
    "advanced": {
        "debug": False,
        "localFile2url": False,
        "fileLog": False,
        "consoleLog": False,
        "fileLogLevel": "debug",
        "consoleLogLevel": "info",
    },
}

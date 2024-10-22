# 标准库导入
import random
import string
from typing import List, Optional

# 第三方库导入
from pydantic import HttpUrl, BaseModel, WebsocketUrl, field_validator


class BotConfig(BaseModel):
    # 需要验证的值
    name: str
    QQID: str
    messagePostFormat: str
    reportSelfMessage: bool
    musicSignUrl: str
    heartInterval: int = 30000
    token: str

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
    port: int = 3000
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
    port: int = 3001


class ReverseWsConfig(BaseModel):
    enable: bool
    urls: List[Optional[WebsocketUrl]]


class ConnectConfig(BaseModel):
    http: HttpConfig
    ws: WsConfig
    reverseWs: ReverseWsConfig


class AdvancedConfig(BaseModel):
    autoStart: bool = False
    offlineNotice: bool = False
    packetServer: str
    debug: bool
    enableLocalFile2Url: bool
    fileLog: bool
    consoleLog: bool
    fileLogLevel: str
    consoleLogLevel: str

    @field_validator("packetServer")
    def set_default_packet_server(cls, value):
        """
        ## 设置默认值
        """
        # 项目内模块导入
        from src.Core.Config.OperateConfig import read_config

        if not value:
            # 从项目配置读取现有端口号
            ports = [
                int(config.advanced.packetServer.split(":")[1])
                for config in read_config()
                if config.advanced.packetServer
            ]

            # 如果有端口，排序并取最大端口号
            if ports:
                max_port = max(ports)
                new_port = max_port + 1
            else:
                # 如果没有任何已占用端口，设置初始端口为8000
                new_port = 8000

            # 设置默认值为 127.0.0.1:最大端口号+1
            return f"127.0.0.1:{new_port}"

        return value


class Config(BaseModel):
    bot: BotConfig
    connect: ConnectConfig
    advanced: AdvancedConfig


class OneBotConfig(BaseModel):
    http: HttpConfig
    ws: WsConfig
    reverseWs: ReverseWsConfig
    debug: bool
    heartInterval: int = 30000
    messagePostFormat: str
    enableLocalFile2Url: bool
    musicSignUrl: str
    reportSelfMessage: bool
    token: str


class NapCatConfig(BaseModel):
    fileLog: bool
    consoleLog: bool
    fileLogLevel: str
    consoleLogLevel: str
    packetServer: str


class WebUiConfig(BaseModel):
    host: str
    port: int
    prefix: str
    token: str
    loginRate: int


DEFAULT_CONFIG = {
    "bot": {
        "name": "",
        "QQID": "",
        "messagePostFormat": "array",
        "reportSelfMessage": False,
        "musicSignUrl": "",
        "heartInterval": 30000,
        "token": "",
    },
    "connect": {
        "http": {
            "enable": False,
            "host": "",
            "port": 3000,
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
        "enableLocalFile2Url": "debug",
        "consoleLogLevel": "info",
        "autoStart": False,
        "offline_notice": False,
        "packetServer": "",
    },
}

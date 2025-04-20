# 标准库导入
import random
import string
from typing import Literal

# 第三方库导入
from pydantic import HttpUrl, BaseModel, WebsocketUrl, field_validator


class BotConfig(BaseModel):
    name: str
    QQID: str
    musicSignUrl: str
    parseMultMsg: bool = False

    @field_validator("name")
    @staticmethod
    def validate_name(value):
        if not value:
            return "".join(random.choices(string.ascii_letters, k=8))
        return value

    @field_validator("QQID")
    @staticmethod
    def validate_qqid(value):
        if not value:
            raise ValueError("QQID cannot be empty")
        return value


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


class WebsocketClientsConfig(NetworkBaseConfig):
    url: WebsocketUrl
    reportSelfMessage: bool = False
    heartInterval: int = 30000
    reconnectInterval: int = 30000


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
    packetServer: str = ""
    packetBackend: str = "auto"
    enableLocalFile2Url: bool
    fileLog: bool
    consoleLog: bool
    fileLogLevel: str
    consoleLogLevel: str
    o3HookMode: int


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

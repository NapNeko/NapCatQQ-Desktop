# 标准库导入
import random
import string
from typing import Literal

# 第三方库导入
from pydantic import Field, HttpUrl, BaseModel, WebsocketUrl, field_validator


class BotConfig(BaseModel):
    name: str
    QQID: str
    musicSignUrl: str

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
        "parseMultMsg": False,
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

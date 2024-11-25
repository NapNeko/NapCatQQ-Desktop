# -*- coding: utf-8 -*-
# 标准库导入
from typing import Dict, List, Optional

# 项目内模块导入
from src.Core.Config.bot_config.base import BaseModel


class BotConfig(BaseModel):
    """机器人配置"""

    network: Network
    musicSignUrl: str = ""
    enableLocalFile2Url: bool = False


class Network(BaseModel):
    """机器人网络配置"""

    httpServers: List[Optional[HttpServers]]
    httpClients: List[Optional[HttpClients]]
    websocketServers: List[Optional[WebsocketServers]]
    websocketClients: List[Optional[WebsocketClients]]


class BaseConnection(BaseModel):
    """通用连接配置"""

    name: str = ""
    enable: bool = False
    token: str = ""
    messagePostFormat: str = "array"
    debug: bool = False


class HttpServers(BaseNetworkConfig):
    """HTTP 服务器配置"""

    port: int = 3000
    host: str = "0.0.0.0"
    enableCors: bool = True
    enableWebsocket: bool = True


class HttpClients(BaseNetworkConfig):
    """HTTP 客户端配置"""

    url: str = "http://localhost:8080"
    reportSelfMessage: bool = False


class WebsocketServers(BaseNetworkConfig):
    """Websocket 服务器配置"""

    port: int = 3000
    host: str = "0.0.0.0"
    reportSelfMessage: bool = False
    enableForcePushEvent: bool = True
    heartInterval: int = 30000


class WebsocketClients(BaseNetworkConfig):
    """Websocket 客户端配置"""

    url: str = "ws://localhost:8082"
    reportSelfMessage: bool = False
    reconnectInterval: int = 5000
    heartInterval: int = 30000

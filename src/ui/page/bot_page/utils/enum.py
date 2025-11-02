# -*- coding: utf-8 -*-
from enum import Enum


class ConnectType(Enum):
    """连接类型枚举"""

    NO_TYPE = 0
    HTTP_SERVER = 1
    HTTP_SSE_SERVER = 2
    HTTP_CLIENT = 3
    WEBSOCKET_SERVER = 4
    WEBSOCKET_CLIENT = 5

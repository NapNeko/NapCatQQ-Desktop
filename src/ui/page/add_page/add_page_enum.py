# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum, auto


class ConnectType(Enum):
    """连接类型枚举"""

    NO_TYPE = auto()
    HTTP_SERVER = auto()
    HTTP_SSE_SERVER = auto()
    HTTP_CLIENT = auto()
    WEBSOCKET_SERVER = auto()
    WEBSOCKET_CLIENT = auto()

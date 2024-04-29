# -*- coding: utf-8 -*-
import re
from enum import Enum
from typing import List

from pydantic import BaseModel, field_validator
from PySide6.QtCore import QObject


class ScriptType(Enum):
    """
    ## 脚本类型枚举
    """

    BAT = "bat"
    PS1 = "ps1"
    SH = "sh"


class ServerConfig(BaseModel):
    enable: bool
    port: str

    @classmethod
    @field_validator("port")
    def validate_port(cls, value):
        """校验 port 输入是否为阿拉伯数字

        ### 参数
            - cls: 类对象
            - value: 输入值

        """
        try:
            int(value)
        except ValueError:
            raise ValueError(QObject.tr("The port must be an Arabic numeral"))
        return value


class HttpReportConfig(BaseModel):
    enable: bool
    enableHeart: bool
    token: str

    @classmethod
    @field_validator("token")
    def validate_token(cls, value):
        """校验 token 输入是否为字符串

        ### 参数
            - cls: 类对象
            - value: 输入值

        """
        if not re.match(r"^[a-zA-Z0-9]+$", value):
            # 限制范围为 ASCII 内，防止发生意外
            raise ValueError(
                QObject.tr(
                    "The token must be a string containing only ASCII characters"
                )
            )
        return value


class BotConfig(BaseModel):
    name: str
    QQID: str
    http: ServerConfig
    httpReport: HttpReportConfig
    httpReportUrls: List[str]
    ws: ServerConfig
    wsReverse: bool
    wsReverseUrls: List[str]
    msgFormat: str
    reportSelfMsg: bool
    heartInterval: str
    accessToken: str


class AdvancedConfig(BaseModel):
    QQPath: str
    ffmpegPath: str
    debug: bool
    localFile2url: bool


class Config(BaseModel):
    bot: BotConfig
    advanced: AdvancedConfig

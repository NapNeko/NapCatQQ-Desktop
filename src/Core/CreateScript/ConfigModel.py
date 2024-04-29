# -*- coding: utf-8 -*-
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


class httpConfig(BaseModel):
    enable: bool
    addresses: str
    port: str


class wsConfig(BaseModel):
    enable: bool
    addresses: str
    port: str


class HttpReportConfig(BaseModel):
    enable: bool
    enableHeart: bool
    token: str


class BotConfig(BaseModel):
    name: str
    QQID: str
    http: httpConfig
    httpReport: HttpReportConfig
    httpReportUrls: List[HttpUrl]
    ws: wsConfig
    wsReverse: bool
    wsReverseUrls: List[WebsocketUrl]
    msgFormat: str
    reportSelfMsg: bool
    heartInterval: str
    accessToken: str

    @field_validator("name")
    @staticmethod
    def name_validator(name: str) -> str:
        if not name:
            # 如果 name 没有输入则自动生成一个name
            name = random.choices(string.ascii_letters + string.digits, k=16)
        return name

    @field_validator("QQID")
    @staticmethod
    def QQID_validator(QQID: str) -> str:
        if not QQID:
            raise ValueError("QQID It can't be empty")
        return QQID


class AdvancedConfig(BaseModel):
    QQPath: str
    ffmpegPath: str
    debug: bool
    localFile2url: bool


class Config(BaseModel):
    bot: BotConfig
    advanced: AdvancedConfig

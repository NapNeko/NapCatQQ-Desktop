# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum

from PySide6.QtCore import QUrl


class Urls(Enum):
    """存放所有需要用到的 URL 地址"""

    # NCD相关地址
    NCD_REPO = QUrl("https://github.com/NapNeko/NapCatQQ-Desktop")
    NCD_ISSUES = QUrl("https://github.com/NapNeko/NapCatQQ-Desktop/issues")
    NCD_REPO_API = QUrl("https://nclatest.znin.net/get_ncd_ver")
    NCD_DOWNLOAD = QUrl("https://github.com/NapNeko/NapCatQQ-Desktop/releases/latest/download/NapCatQQ-Desktop.exe")

    # NapCat 相关地址
    NAPCATQQ_REPO = QUrl("https://github.com/NapNeko/NapCatQQ")
    NAPCATQQ_ISSUES = QUrl("https://github.com/NapNeko/NapCatQQ/issues")
    NAPCATQQ_REPO_API = QUrl("https://nclatest.znin.net")
    NAPCATQQ_DOCUMENT = QUrl("https://napneko.github.io/")

    # 直接写入下载地址, 不请求 API 获取, 期望达到节省时间的目的
    NAPCATQQ_DOWNLOAD = QUrl("https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.Shell.zip")

    # QQ 相关
    QQ_OFFICIAL_WEBSITE = QUrl("https://im.qq.com/index/")
    QQ_AVATAR = QUrl("https://q.qlogo.cn/headimg_dl")
    QQ_Version = QUrl("https://nclatest.znin.net/get_qq_ver")

    # 镜像站地址
    MIRROR_SITE = [
        QUrl("https://gh.ddlc.top"),
        QUrl("https://slink.ltd"),
        QUrl("https://cors.isteed.cc"),
        QUrl("https://hub.gitmirror.com"),
        QUrl("https://ghproxy.cc"),
        QUrl("https://github.moeyy.xyz"),
    ]

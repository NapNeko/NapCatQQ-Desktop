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
    NCD_REPO_API_FALLBACK = QUrl("https://api.github.com/repos/NapNeko/NapCatQQ-Desktop/releases/latest")
    NCD_DOWNLOAD = QUrl("https://github.com/NapNeko/NapCatQQ-Desktop/releases/latest/download/NapCatQQ-Desktop-x64.msi")

    @staticmethod
    def get_ncd_download_url(version: str, install_type: str = "msi") -> QUrl:
        """获取桌面端下载链接。

        Desktop 应用内更新统一使用 MSI 安装包。

        Args:
            version: 版本号（如 "1.7.28"，不含 v 前缀）
            install_type: 保留参数，仅兼容旧调用；当前始终返回 MSI 包地址

        Returns:
            QUrl: 下载链接
        """
        base_url = "https://github.com/NapNeko/NapCatQQ-Desktop/releases/download"
        tag = f"v{version}"

        filename = f"NapCatQQ-Desktop-{version}-x64.msi"

        return QUrl(f"{base_url}/{tag}/{filename}")

    @staticmethod
    def get_ncd_latest_url(install_type: str = "msi") -> QUrl:
        """获取最新版下载链接。

        Note:
            由于文件名包含版本号，无法使用固定的 latest 下载链接。
            需要通过 API 获取实际版本号后再构造完整 URL。

        Args:
            install_type: 保留参数，仅兼容旧调用

        Returns:
            QUrl: 指向 releases 页面的链接（需要配合 API 使用）
        """
        return QUrl("https://github.com/NapNeko/NapCatQQ-Desktop/releases/latest")

    # NapCat 相关地址
    NAPCATQQ_REPO = QUrl("https://github.com/NapNeko/NapCatQQ")
    NAPCATQQ_ISSUES = QUrl("https://github.com/NapNeko/NapCatQQ/issues")
    NAPCATQQ_REPO_API = QUrl("https://nclatest.znin.net")
    NAPCATQQ_REPO_API_FALLBACK = QUrl("https://api.github.com/repos/NapNeko/NapCatQQ/releases/latest")
    NAPCATQQ_DOCUMENT = QUrl("https://napneko.github.io/")

    # 直接写入下载地址, 不请求 API 获取, 期望达到节省时间的目的
    NAPCATQQ_DOWNLOAD = QUrl("https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.Shell.zip")

    # QQ 相关
    QQ_OFFICIAL_WEBSITE = QUrl("https://im.qq.com/index/")
    QQ_AVATAR = QUrl("https://q.qlogo.cn/headimg_dl")
    QQ_Version = QUrl("https://cdn-go.cn/qq-web/im.qq.com_new/latest/rainbow/pcConfig.json")

    # 镜像站地址
    MIRROR_SITE = [
        QUrl("https://gh.ddlc.top"),
        QUrl("https://slink.ltd"),
        QUrl("https://cors.isteed.cc"),
        QUrl("https://hub.gitmirror.com"),
        QUrl("https://ghproxy.cc"),
        QUrl("https://github.moeyy.xyz"),
    ]

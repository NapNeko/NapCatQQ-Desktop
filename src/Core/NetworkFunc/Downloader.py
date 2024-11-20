# -*- coding: utf-8 -*-
# 标准库导入
from pathlib import Path

# 第三方库导入
import httpx
from PySide6.QtCore import QUrl, Signal, QThread

# 项目内模块导入
from src.Core.Utils.logger import logger
from src.Ui.UnitPage.status import ButtonStatus, ProgressRingStatus
from src.Core.Utils.PathFunc import PathFunc
from src.Core.NetworkFunc.Urls import Urls


class DownloaderBase(QThread):
    """
    ## 下载器基类
    """

    # 下载进度
    downloadProgress = Signal(int)
    # 下载完成
    downloadFinish = Signal()
    # 引发错误导致结束
    errorFinsh = Signal()

    def __init__(self) -> None:
        """
        ## 初始化下载器
        """
        super().__init__()


class GithubDownloader(DownloaderBase):
    """
    ## 执行下载 Github文件 的任务
    """

    # 按钮模式切换
    buttonToggle = Signal(ButtonStatus)
    # 进度条模式切换
    progressRingToggle = Signal(ProgressRingStatus)
    # 状态标签
    statusLabel = Signal(str)

    def __init__(self, url: QUrl) -> None:
        """
        ## 初始化下载器
            - url 下载连接
            - path 下载路径
        """
        super().__init__()
        self.url: QUrl = url
        self.path: Path = PathFunc().tmp_path
        self.filename = self.url.fileName()
        self.mirror_urls = [QUrl(f"{mirror.toString()}/{self.url.toString()}") for mirror in Urls.MIRROR_SITE.value]

    def run(self) -> None:
        """
        ## 运行下载 NapCat 的任务
        """
        # 显示进度环为不确定进度环
        self.progressRingToggle.emit(ProgressRingStatus.INDETERMINATE)

        # 检查网络环境
        if self.checkNetwork():
            # 如果网络环境好, 则直接下载
            if self.download():
                return  # 下载成功, 直接返回

        for mirror_url in self.mirror_urls:
            self.url = QUrl(mirror_url)
            if self.download():
                self.downloadFinish.emit()  # 发送下载完成信号
                return

        self.statusLabel.emit(self.tr("下载失败"))
        self.errorFinsh.emit()

    def download(self) -> bool:
        """
        ## 下载文件
        """
        try:
            self.statusLabel.emit(self.tr(f" 开始下载 {self.filename} ~ "))
            with httpx.stream("GET", self.url.url(), follow_redirects=True) as response:
                if (total_size := int(response.headers.get("content-length", 0))) == 0:
                    # 尝试获取文件大小
                    self.statusLabel.emit(self.tr("无法获取文件大小"))
                    return False

                # 设置进度条为 进度模式
                self.progressRingToggle.emit(ProgressRingStatus.DETERMINATE)

                with open(f"{self.path / self.url.fileName()}", "wb") as file:
                    self.statusLabel.emit(self.tr(f"正在下载 {self.filename} ~ "))
                    for chunk in response.iter_bytes():
                        file.write(chunk)  # 写入字节
                        self.downloadProgress.emit(int((file.tell() / total_size) * 100))  # 设置进度条

            # 下载完成
            self.statusLabel.emit(self.tr("下载完成"))
            return True

        except (httpx.RequestError, httpx.HTTPStatusError, PermissionError, Exception) as e:
            logger.error(f"下载失败: {e}")
        finally:
            self.progressRingToggle.emit(ProgressRingStatus.INDETERMINATE)
        return False

    def checkNetwork(self) -> bool:
        """
        ## 检查网络能否正常访问 Github
        """
        try:
            self.statusLabel.emit(self.tr("检查网络环境"))
            # 如果 3 秒内能访问到 Github 表示网络环境非常奈斯
            response = httpx.head("https://objects.githubusercontent.com", timeout=3)
            if response.status_code == 200:
                self.statusLabel.emit(self.tr("网络环境良好"))
                return True
            else:
                self.statusLabel.emit(self.tr("网络环境较差"))
                return False
        except httpx.RequestError as e:
            self.statusLabel.emit(self.tr(f"网络检查失败: {e}"))
            return False


class QQDownloader(DownloaderBase):
    """
    ## 执行下载 QQ 的任务
    """

    # 按钮模式切换
    buttonToggle = Signal(ButtonStatus)
    # 进度条模式切换
    progressRingToggle = Signal(ProgressRingStatus)
    # 状态标签
    statusLabel = Signal(str)

    def __init__(self, url: QUrl) -> None:
        """
        ## 初始化下载器
        """
        super().__init__()
        self.url: QUrl = url
        self.path: Path = PathFunc().tmp_path

    def run(self) -> None:
        """
        ## 运行下载 QQ 的任务
            - 自动下载 QQ
        """

        # 开始下载 QQ
        try:
            with httpx.stream("GET", self.url.url(), follow_redirects=True) as response:

                if (total_size := int(response.headers.get("content-length", 0))) == 0:
                    # 尝试获取文件大小
                    self.statusLabel.emit(self.tr("无法获取文件大小"))
                    self.errorFinsh.emit()
                    return

                # 设置进度条为 进度模式
                self.progressRingToggle.emit(ProgressRingStatus.DETERMINATE)

                with open(f"{self.path / self.url.fileName()}", "wb") as file:
                    self.statusLabel.emit(self.tr("正在下载 QQ ~ "))
                    for chunk in response.iter_bytes():
                        file.write(chunk)  # 写入字节
                        self.downloadProgress.emit(int((file.tell() / total_size) * 100))  # 设置进度条

            # 下载完成
            self.downloadFinish.emit()  # 发送下载完成信号
            self.statusLabel.emit(self.tr("下载完成"))

        except (httpx.RequestError, httpx.HTTPStatusError, PermissionError, Exception) as e:
            self.statusLabel.emit(self.tr(f"下载失败: {e}"))
            self.errorFinsh.emit()

        finally:
            # 无论是否出错,都会重置
            self.progressRingToggle.emit(ProgressRingStatus.INDETERMINATE)

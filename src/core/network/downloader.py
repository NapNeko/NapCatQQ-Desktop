# -*- coding: utf-8 -*-
# 标准库导入
from pathlib import Path

# 第三方库导入
import httpx
from PySide6.QtCore import QObject, QRunnable, QUrl, Signal

# 项目内模块导入
from src.core.network.urls import Urls
from src.core.utils.logger import logger
from src.core.utils.path_func import PathFunc
from src.ui.page.unit_page.status import ButtonStatus, ProgressRingStatus


class DownloaderBase(QObject, QRunnable):
    """下载器基类

    定义了3个信号:
    - download_progress_signal(int): 下载进度, 范围 0-100
    - download_finish_signal(): 下载完成
    - error_finsh_signal(): 引发错误导致结束
    """

    # 下载进度
    download_progress_signal = Signal(int)
    # 下载完成
    download_finish_signal = Signal()
    # 引发错误导致结束
    error_finsh_signal = Signal()

    # 按钮模式切换
    button_toggle_signal = Signal(ButtonStatus)
    # 进度条模式切换
    progress_ring_toggle_signal = Signal(ProgressRingStatus)
    # 状态标签
    status_label_signal = Signal(str)

    def __init__(self) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)


class GithubDownloader(DownloaderBase):
    """执行下载 Github文件 的任务

    可以根据网络环境自动切换下载源, 支持多镜像源下载
    """

    def __init__(self, url: QUrl) -> None:
        """初始化下载器

        Args:
            url (QUrl): 下载链接
        """
        super().__init__()
        self.url: QUrl = url
        self.path: Path = PathFunc().tmp_path
        self.file_name = self.url.fileName()
        self.mirror_urls = [QUrl(f"{mirror.toString()}/{self.url.toString()}") for mirror in Urls.MIRROR_SITE.value]

    def run(self) -> None:
        """运行下载 NapCatQQ 的任务"""
        # 显示进度环为不确定进度环
        self.progress_ring_toggle_signal.emit(ProgressRingStatus.INDETERMINATE)

        # 检查网络环境
        if self.check_network():
            # 如果网络环境好, 则直接下载
            if self.download():
                return  # 下载成功, 直接返回

        for mirror_url in self.mirror_urls:
            self.url = QUrl(mirror_url)
            if self.download():
                self.download_finish_signal.emit()  # 发送下载完成信号
                return

        self.status_label_signal.emit(self.tr("下载失败"))
        self.error_finsh_signal.emit()

    def download(self) -> bool:
        """下载文件"""
        try:
            self.status_label_signal.emit(self.tr(f" 开始下载 {self.file_name} ~ "))
            with httpx.stream("GET", self.url.url(), follow_redirects=True) as response:
                if (total_size := int(response.headers.get("content-length", 0))) == 0:
                    # 尝试获取文件大小
                    self.status_label_signal.emit(self.tr("无法获取文件大小"))
                    return False

                # 设置进度条为 进度模式
                self.progress_ring_toggle_signal.emit(ProgressRingStatus.DETERMINATE)

                with open(f"{self.path / self.url.fileName()}", "wb") as file:
                    self.status_label_signal.emit(self.tr(f"正在下载 {self.file_name} ~ "))
                    for chunk in response.iter_bytes():
                        file.write(chunk)  # 写入字节
                        self.download_progress_signal.emit(int((file.tell() / total_size) * 100))  # 设置进度条

            # 下载完成
            self.status_label_signal.emit(self.tr("下载完成"))
            return True

        except (httpx.RequestError, httpx.HTTPStatusError, PermissionError, Exception) as e:
            logger.error(f"下载失败: {e}")
        finally:
            self.progress_ring_toggle_signal.emit(ProgressRingStatus.INDETERMINATE)
        return False

    def check_network(self) -> bool:
        """检查网络能否正常访问 Github"""
        try:
            self.status_label_signal.emit(self.tr("检查网络环境"))
            # 如果 3 秒内能访问到 Github 表示网络环境非常奈斯
            response = httpx.head("https://objects.githubusercontent.com", timeout=3)
            if response.status_code == 200:
                self.status_label_signal.emit(self.tr("网络环境良好"))
                return True
            else:
                self.status_label_signal.emit(self.tr("网络环境较差"))
                return False
        except httpx.RequestError as e:
            self.status_label_signal.emit(self.tr(f"网络检查失败: {e}"))
            return False


class QQDownloader(DownloaderBase):
    """执行下载 QQ 的任务"""

    def __init__(self, url: QUrl) -> None:
        super().__init__()
        self.url: QUrl = url
        self.path: Path = PathFunc().tmp_path

    def run(self) -> None:
        """运行下载 QQ 的任务"""

        # 开始下载 QQ
        try:
            with httpx.stream("GET", self.url.url(), follow_redirects=True) as response:

                if (total_size := int(response.headers.get("content-length", 0))) == 0:
                    # 尝试获取文件大小
                    self.status_label_signal.emit(self.tr("无法获取文件大小"))
                    self.error_finsh_signal.emit()
                    return

                # 设置进度条为 进度模式
                self.progress_ring_toggle_signal.emit(ProgressRingStatus.DETERMINATE)

                with open(f"{self.path / self.url.fileName()}", "wb") as file:
                    self.status_label_signal.emit(self.tr("正在下载 QQ ~ "))
                    for chunk in response.iter_bytes():
                        file.write(chunk)  # 写入字节
                        self.download_progress_signal.emit(int((file.tell() / total_size) * 100))  # 设置进度条

            # 下载完成
            self.download_finish_signal.emit()  # 发送下载完成信号
            self.status_label_signal.emit(self.tr("下载完成"))

        except (httpx.RequestError, httpx.HTTPStatusError, PermissionError, Exception) as e:
            self.status_label_signal.emit(self.tr(f"下载失败: {e}"))
            self.error_finsh_signal.emit()

        finally:
            # 无论是否出错,都会重置
            self.progress_ring_toggle_signal.emit(ProgressRingStatus.INDETERMINATE)

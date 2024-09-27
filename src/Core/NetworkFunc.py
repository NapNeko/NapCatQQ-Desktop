# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC
from enum import Enum
from typing import Any, Callable
from pathlib import Path

# 第三方库导入
import httpx
from creart import AbstractCreator, CreateTargetInfo, it, add_creator, exists_module
from loguru import logger
from PySide6.QtCore import QUrl, Signal, QObject, QThread
from PySide6.QtNetwork import QNetworkReply, QNetworkRequest, QNetworkAccessManager


class Urls(Enum):
    """
    ## 软件内部可能用的到的 Url
    """

    # NCD相关地址
    NCD_REPO = QUrl("https://github.com/HeartfeltJoy/NapCatQQ-Desktop")
    NCD_ISSUES = QUrl("https://github.com/HeartfeltJoy/NapCatQQ-Desktop/issues")

    # NapCat 相关地址
    NAPCATQQ_REPO = QUrl("https://github.com/NapNeko/NapCatQQ")
    NAPCATQQ_ISSUES = QUrl("https://github.com/NapNeko/NapCatQQ/issues")
    NAPCATQQ_REPO_API = QUrl("https://nclatest.znin.net")
    NAPCATQQ_DOCUMENT = QUrl("https://napneko.github.io/")

    # 直接写入下载地址, 不请求 API 获取, 期望达到节省时间的目的
    NAPCAT_DOWNLOAD = QUrl("https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.Shell.zip")

    # QQ 相关
    QQ_OFFICIAL_WEBSITE = QUrl("https://im.qq.com/index/")
    QQ_WIN_DOWNLOAD = QUrl("https://dldir1.qq.com/qqfile/qq/QQNT/0f14ef6e/QQ9.9.15.28131_x64.exe")
    QQ_AVATAR = QUrl("https://q.qlogo.cn/headimg_dl")


class NetworkFunc(QObject):
    """
    ## 软件内部的所有网络请求均通过此类实现
    """

    def __init__(self):
        """
        ## 创建 QNetworkAccessManager
        """
        super().__init__()
        self.manager = QNetworkAccessManager()


class NetworkFuncClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Core.NetworkFunc", "NetworkFunc"),)

    # 静态方法available()，用于检查模块"PathFunc"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Core.NetworkFunc")

    # 静态方法create()，用于创建PathFunc类的实例，返回值为PathFunc对象。
    @staticmethod
    def create(create_type: list[NetworkFunc]) -> NetworkFunc:
        return NetworkFunc()


add_creator(NetworkFuncClassCreator)


def async_request(url: QUrl, _bytes: bool = False) -> Callable[[Callable[..., None]], Callable[..., None]]:
    """
    装饰器函数，用于装饰其他函数，使其在QUrl请求完成后执行
        - url (QUrl): 用于进行网络请求的QUrl对象。
    """

    def decorator(func: Callable[..., None]) -> Callable[..., None]:
        """
        装饰器内部函数，用于接收被装饰的函数
            - func (Callable): 被装饰的函数
        """

        def wrapper(*args: Any, **kwargs: Any) -> None:
            """
            包装函数，用于执行网络请求并在请求完成后调用被装饰的函数。
                - *args: 传递给被装饰函数的位置参数
                - **kwargs: 传递给被装饰函数的关键字参数
            """

            def on_finished(_reply: QNetworkReply) -> None:
                """
                请求完成后的回调函数，读取响应并调用被装饰的函数。
                    - _reply (QNetworkReply): 网络响应对象
                    - _bytes (bool): 是否直接返回字节
                """
                if _reply.error() == QNetworkReply.NetworkError.NoError:
                    # 调用被装饰的函数并传递响应数据
                    if _bytes:
                        func(*args, reply=_reply.readAll().data(), *kwargs)
                    else:
                        func(*args, reply=_reply.readAll().data().decode().strip(), *kwargs)
                else:
                    func(*args, reply=None, *kwargs)
                    logger.error(f"Error: {_reply.errorString()}")
                # 清理回复对象
                _reply.deleteLater()

            # 创建并发送网络请求
            request = QNetworkRequest(url)
            reply = it(NetworkFunc).manager.get(request)
            # 连接请求完成信号到回调函数
            reply.finished.connect(lambda: on_finished(reply))

        return wrapper

    return decorator


class NapCatDownloader(QThread):
    """
    ## 执行下载 NapCat 的任务
    """

    # 进度条模式切换 (进度模式: 0 \ 未知进度模式: 1 \ 文字模式: 2)
    progressBarToggle = Signal(int)
    # 下载进度
    downloadProgress = Signal(int)
    # 下载完成
    downloadFinish = Signal()
    # 引发错误导致结束
    errorFinsh = Signal()

    def __init__(self, url: QUrl, path: Path) -> None:
        """
        ## 初始化下载器
            - url 下载连接
            - path 下载路径
        """
        super().__init__()
        self.url: QUrl = url if url else None
        self.path: Path = path if path else None

    def run(self) -> None:
        """
        ## 运行下载 NapCat 的任务
            - 自动检查是否需要使用代理
            - 尽可能下载成功
        """
        # 调整按钮样式为禁用
        self.progressBarToggle.emit(3)

        # 检查网络环境
        if not self.checkNetwork():
            # 如果网络环境不好, 则调整下载链接
            self.url = QUrl(f"https://gh.ddlc.top/{self.url.url()}")
            logger.info(f"访问 GITHUB 速度偏慢,切换下载链接为: https://gh.ddlc.top/{self.url.url()}")

        # 开始下载
        try:
            logger.info(f"{'-' * 10} 开始下载 NapCat ~ {'-' * 10}")
            with httpx.stream("GET", self.url.url(), follow_redirects=True) as response:

                if (total_size := int(response.headers.get("content-length", 0))) == 0:
                    # 尝试获取文件大小
                    logger.error("无法获取文件大小, Content-Length为空或无法连接到下载链接")
                    self.progressBarToggle.emit(2)
                    self.errorFinsh.emit()
                    return

                self.progressBarToggle.emit(0)  # 设置进度条为 进度模式

                with open(f"{self.path / self.url.fileName()}", "wb") as file:
                    for chunk in response.iter_bytes():
                        file.write(chunk)  # 写入字节
                        self.downloadProgress.emit(int((file.tell() / total_size) * 100))  # 设置进度条

            # 下载完成
            self.downloadFinish.emit()  # 发送下载完成信号
            logger.info(f"{'-' * 10} 下载 NapCat 结束 ~ {'-' * 10}")

        except httpx.HTTPStatusError as e:
            logger.error(
                f"发送下载 NapCat 请求时引发 HTTPStatusError, "
                f"响应码: {e.response.status_code}, 响应内容: {e.response.content}"
            )
            self.errorFinsh.emit()
        except (httpx.RequestError, FileNotFoundError, PermissionError, Exception) as e:
            logger.error(f"下载 NapCat 时引发 {type(e).__name__}: {e}")
            self.errorFinsh.emit()

        finally:
            # 无论是否出错,都会重置
            self.downloadProgress.emit(0)  # 重置进度条进度
            self.progressBarToggle.emit(2)  # 设置进度条为 文字模式
            self.progressBarToggle.emit(4)  # 解除禁用

    def checkNetwork(self) -> bool:
        """
        ## 检查网络能否正常访问 Github
        """
        try:
            logger.info(f"{'-' * 10} 检查网络环境 {'-' * 10}")
            self.progressBarToggle.emit(1)  # 设置进度条为 未知进度模式
            # 如果 5 秒内能访问到 Github 表示网络环境非常奈斯
            response = httpx.head(r"https://github.com", timeout=5)
            logger.info("网络环境非常奈斯")
            return response.status_code == 200
        except httpx.RequestError:
            # 引发错误返回 False
            return False

    def setUrl(self, url: QUrl) -> None:
        self.url = url

    def setPath(self, path: Path) -> None:
        self.path = path


class QQDownloader(QThread):
    """
    ## 执行下载 QQ 的任务
    """

    # 进度条模式切换 (进度模式: 0 \ 未知进度模式: 1 \ 文字模式: 2)
    progressBarToggle = Signal(int)
    # 下载进度
    downloadProgress = Signal(int)
    # 下载完成
    downloadFinish = Signal()
    # 引发错误导致结束
    errorFinsh = Signal()

    def __init__(self, url: QUrl, path: Path) -> None:
        """
        ## 初始化下载器
        """
        super().__init__()
        self.url: QUrl = url if url else None
        self.path: Path = Path(path) if path else None

    def run(self) -> None:
        """
        ## 运行下载 QQ 的任务
            - 自动下载 QQ
        """
        # 调整按钮样式为禁用
        self.progressBarToggle.emit(3)

        # 开始下载 QQ
        try:
            logger.info(f"{'-' * 10} 开始下载 QQ ~ {'-' * 10}")
            with httpx.stream("GET", self.url.url(), follow_redirects=True) as response:

                if (total_size := int(response.headers.get("content-length", 0))) == 0:
                    # 尝试获取文件大小
                    logger.error("无法获取文件大小, Content-Length为空或无法连接到下载链接")
                    self.progressBarToggle.emit(2)
                    self.errorFinsh.emit()
                    return

                self.progressBarToggle.emit(0)  # 设置进度条为 进度模式
                with open(f"{self.path / self.url.fileName()}", "wb") as file:
                    for chunk in response.iter_bytes():
                        file.write(chunk)  # 写入字节
                        self.downloadProgress.emit(int((file.tell() / total_size) * 100))  # 设置进度条

            # 下载完成
            self.downloadFinish.emit()  # 发生下载完成信号
            logger.info(f"{'-' * 10} 下载 QQ 结束 ~ {'-' * 10}")

        except httpx.HTTPStatusError as e:
            logger.error(
                f"发送下载 QQ 请求时引发 HTTPStatusError, "
                f"响应码: {e.response.status_code}, 响应内容: {e.response.content}"
            )
            self.errorFinsh.emit()
        except (httpx.RequestError, FileNotFoundError, PermissionError, Exception) as e:
            logger.error(f"下载 QQ 时引发 {type(e).__name__}: {e}")
            self.errorFinsh.emit()

        finally:
            # 无论是否出错,都会重置
            self.downloadProgress.emit(0)  # 重置进度条进度
            self.progressBarToggle.emit(2)  # 设置进度条为 文字模式
            self.progressBarToggle.emit(4)  # 解除禁用

    def setUrl(self, url: QUrl) -> None:
        self.url = url

    def setPath(self, path: Path) -> None:
        self.path = path

# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC
from typing import Any, Callable

# 第三方库导入
from creart import AbstractCreator, CreateTargetInfo, it, add_creator, exists_module
from loguru import logger
from PySide6.QtCore import QUrl, QObject
from PySide6.QtNetwork import QNetworkReply, QNetworkRequest, QNetworkAccessManager


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



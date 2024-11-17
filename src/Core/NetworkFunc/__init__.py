# -*- coding: utf-8 -*-
# 标准库导入
from typing import Any, Callable

from PySide6.QtCore import QUrl
from PySide6.QtNetwork import QNetworkReply, QNetworkRequest, QNetworkAccessManager


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
                # 清理回复对象
                _reply.deleteLater()

            # 创建并发送网络请求
            request = QNetworkRequest(url)
            manager = QNetworkAccessManager()
            reply = manager.get(request)
            # 连接请求完成信号到回调函数
            reply.finished.connect(lambda: on_finished(reply))

        return wrapper

    return decorator



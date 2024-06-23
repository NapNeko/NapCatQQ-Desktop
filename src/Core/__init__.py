# -*- coding: utf-8 -*-
from functools import wraps
from typing import Callable, Any

from PySide6.QtCore import QTimer, QObject


def timer(interval: int, single_shot: bool = False) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    """
    ## 将 QTimer 附加到它所修饰的类方法的实例的修饰器
    ## 计时器以 “interval” 指定的定期间隔调用修饰函数

        - interval （int）: 计时器应触发的间隔（以毫秒为单位）
        - single_shot （bool）: 如果为 True，则计时器只超时一次, 默认值为 False

    """

    def decorator(func: Callable[..., Any]) -> Callable[..., Any]:
        @wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            instance = args[0]
            if not hasattr(instance, '_timers'):
                instance._timers = []

            timer_instance = QTimer(instance)
            timer_instance.setInterval(interval)
            timer_instance.setSingleShot(single_shot)
            timer_instance.timeout.connect(lambda: func(*args, **kwargs))
            timer_instance.start()

            instance._timers.append(timer_instance)  # 将计时器保存到实例变量
            return func(*args, **kwargs)

        return wrapper

    return decorator

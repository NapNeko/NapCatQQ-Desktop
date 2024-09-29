# -*- coding: utf-8 -*-
# 标准库导入
from typing import Any, Callable
from functools import wraps

from PySide6.QtCore import QTimer


def timer(interval: int, single_shot: bool = False) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    """
    ## 将 QTimer 附加到它所修饰的类方法的实例的修饰器
    ## 计时器以 "interval" 指定的定期间隔调用修饰函数

        - interval （int）: 计时器应触发的间隔（以毫秒为单位）
        - single_shot （bool）: 如果为 True，则计时器只超时一次, 默认值为 False

    ## 以上你看不懂我就说下人话
        - 传个函数进来套个 QTimer, 超时调用传入的函数
        - 至于为什么里面那么多 Any, 这个问题也不需要思考太多

    """

    def decorator(func: Callable[..., Any]) -> Callable[..., Any]:
        @wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            instance = args[0]
            if not hasattr(instance, "_timers"):
                instance._timers = {}

            if func not in instance._timers:
                timer_instance = QTimer(instance)
                timer_instance.setInterval(interval)
                timer_instance.setSingleShot(single_shot)
                timer_instance.timeout.connect(lambda: func(*args, **kwargs))
                timer_instance.start()

                instance._timers[func] = timer_instance  # 将计时器保存到字典中
            return func(*args, **kwargs)

        return wrapper

    return decorator

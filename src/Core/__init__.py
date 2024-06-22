# -*- coding: utf-8 -*-
import sys
from functools import wraps
from typing import Callable, Any

from PySide6.QtCore import QTimer, QObject
from PySide6.QtWidgets import QApplication


def timer(interval: int) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    """
    将 QTimer 附加到它所修饰的类方法的实例的修饰器。
    计时器以“interval”指定的定期间隔调用修饰函数。

    Args:
        interval （int）：计时器应触发的间隔（以毫秒为单位）。

    Returns:
        Callable[[Callable[...， Any]]， Callable[...， Any]]：修饰函数。
    """

    def decorator(func: Callable[..., Any]) -> Callable[..., Any]:
        @wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            instance = args[0]
            if not hasattr(instance, '_timers'):
                instance.timers = []

            timer_instance = QTimer(instance)
            timer_instance.timeout.connect(lambda: func(*args, **kwargs))
            timer_instance.start(interval)

            instance.timers.append(timer_instance)  # 将计时器保存到实例变量
            return func(*args, **kwargs)

        return wrapper

    return decorator


# Example usage
if __name__ == "__main__":
    class ExampleClass(QObject):
        @timer(1000)
        def print_message(self):
            print("Hello, world!")
    app = QApplication(sys.argv)

    example = ExampleClass()
    example.print_message()

    sys.exit(app.exec_())

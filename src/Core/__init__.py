# -*- coding: utf-8 -*-
from typing import Any, Callable
from pathlib import Path
from functools import wraps

from loguru import logger
from PySide6.QtCore import QTimer


def timer(interval: int, single_shot: bool = False) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    """
    ## 将 QTimer 附加到它所修饰的类方法的实例的修饰器
    ## 计时器以 “interval” 指定的定期间隔调用修饰函数

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
                instance.timers = []

            timer_instance = QTimer(instance)
            timer_instance.setInterval(interval)
            timer_instance.setSingleShot(single_shot)
            timer_instance.timeout.connect(lambda: func(*args, **kwargs))
            timer_instance.start()

            instance.timers.append(timer_instance)  # 将计时器保存到实例变量
            return func(*args, **kwargs)

        return wrapper

    return decorator


def stdout() -> None:
    """
    ## 调整程序输出
    """
    # 获取路径
    log_path = Path.cwd() / "log"
    all_log_path = log_path / "ALL.log"

    # 检查路径, 如果不存在 log 文件夹则创建一个
    if not log_path.exists():
        log_path.mkdir(parents=True, exist_ok=True)

    # 自定义格式化器
    custom_format = "{time:YYYY-MM-DD HH:mm:ss.SSS} | {level} | {message}"
    # 添加自定义的 level
    logger.level("FIX", no=30, color="<yellow>")
    # 添加自定义的 logger
    logger.add(all_log_path, level="DEBUG", format=custom_format)

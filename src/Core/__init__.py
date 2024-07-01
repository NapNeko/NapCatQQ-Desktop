# -*- coding: utf-8 -*-
import sys
from functools import wraps
from pathlib import Path
from typing import Callable, Any

from PySide6.QtCore import QTimer
from loguru import logger


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


def stdout():
    """
    ## 调整程序输出
    """
    # 获取路径
    logPath = Path.cwd() / "log"
    allLogPath = logPath / "ALL.log"

    # 检查路径
    if not logPath.exists():
        logPath.mkdir(parents=True, exist_ok=True)

    # 打开文件
    all_file = open(str(allLogPath), "w", encoding="utf-8")

    # 调整 log
    # 自定义格式化器
    custom_format = "{time:YYYY-MM-DD HH:mm:ss.SSS} | {level} | {message}"
    # 移除默认的 logger
    logger.remove()
    # 添加自定义的 logger
    logger.add(all_file, format=custom_format)

    # 重定向输出
    sys.stdout = all_file
    sys.stderr = all_file

# 标准库导入
import inspect
from typing import Any, Callable
from pathlib import Path
from functools import wraps

# 项目内模块导入
from src.Core.Utils.logger.log_data import LogPosition


def capture_call_location(func: Callable[..., Any]) -> Callable[..., Any]:
    """
    ## 日志位置装饰器
        - 用于捕获调用者的位置信息，并将其添加到关键字参数中

    ## 参数
        - func: Callable[..., Any] - 被装饰的函数

    ## 返回
        - Callable[..., Any] - 装饰后的函数
    """

    @wraps(func)
    def wrapper(*args: Any, **kwargs: Any) -> None:
        """
        ## 装饰器函数
        """
        # 获取调用者的位置信息
        pos = inspect.stack()[1]
        position = LogPosition(
            module=pos.frame.f_globals.get('__name__'),
            file=Path(pos.filename).name,
            line=pos.lineno,
        )
        # 检查被装饰函数是否存在 log_position 形参
        if 'log_position' in inspect.signature(func).parameters:
            # 如果存在则将 position 添加到关键字参数中
            kwargs['log_position'] = position
            return func(*args, **kwargs)
        else:
            # 否则直接调用函数
            return func(*args, **kwargs)

    return wrapper

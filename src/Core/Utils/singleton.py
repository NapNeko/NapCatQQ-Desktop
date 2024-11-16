# -*- coding: utf-8 -*-
# 标准库导入
from typing import Any, Dict, TypeVar, Callable

T = TypeVar("T", bound=type)


class Singleton(type):
    """
    ## 通过自定义元类实现单例模式
    """

    _instances: Dict[T, Any] = {}

    def __call__(cls: T, *args: Any, **kwargs: Any) -> T:
        if cls not in cls._instances:
            cls._instances[cls] = super(Singleton, cls).__call__(*args, **kwargs)
        return cls._instances[cls]


def singleton(cls: T) -> Callable[..., T]:
    """
    ## 通过装饰器实现单例模式
    """
    _instance: T = None

    def wrapper(*args: Any, **kwargs: Any) -> T:
        nonlocal _instance
        if _instance is None:
            _instance = cls(*args, **kwargs)
        return _instance

    # 将 wrapper 函数绑定到 cls 类上,这样就可以直接通过类名访问实例属性和方法
    for attr in dir(cls):
        if not attr.startswith("__"):
            setattr(wrapper, attr, getattr(cls, attr))

    return wrapper

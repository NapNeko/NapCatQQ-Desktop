# -*- coding: utf-8 -*-
# 标准库导入
from typing import Any, Callable, Dict, TypeVar

T = TypeVar("T", bound=type)


class Singleton(type):
    """通过自定义元类实现单例模式

    Usage:
        class MyClass(metaclass=Singleton):
            pass

        instance1 = MyClass()
        instance2 = MyClass()
        assert instance1 is instance2  # True

    Attributes:
        _instances (Dict[T, Any]): 存储类的唯一实例的字典
    """

    _instances: Dict[T, Any] = {}

    def __call__(cls: T, *args: Any, **kwargs: Any) -> T:
        """构造函数, 用于创建类的实例

        Args:
            cls (T): 类本身

        Returns:
            T: 类的唯一实例
        """
        if cls not in cls._instances:
            cls._instances[cls] = super(Singleton, cls).__call__(*args, **kwargs)
        return cls._instances[cls]


def singleton(cls: T) -> Callable[..., T]:
    """通过装饰器实现单例模式

    Args:
        cls (T): 需要实现单例模式的类

    Returns:
        Callable[..., T]: 返回类的唯一实例
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

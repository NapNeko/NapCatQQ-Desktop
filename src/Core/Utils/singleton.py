# -*- coding: utf-8 -*-
# 自定义元类实现单例模式


class Singleton(type):
    """
    ## 单例元类
    """

    _instances = {}

    def __call__(cls, *args, **kwargs):
        if cls not in cls._instances:
            cls._instances[cls] = super(Singleton, cls).__call__(*args, **kwargs)
        return cls._instances[cls]


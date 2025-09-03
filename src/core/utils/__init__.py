# -*- coding: utf-8 -*-


def my_int(value: str, defualt: int) -> int:
    """将字符串转换为整数, 如果转换失败则返回默认值"""
    try:
        return int(value)
    except ValueError:
        return defualt

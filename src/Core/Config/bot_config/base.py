# -*- coding: utf-8 -*-
# 项目内模块导入
from src.Core.Utils.logger import LogType, LogSource, logger


class BaseModel:

    def __init__(self, **kwargs):
        annotations = self.__class__.__annotations__
        self.__dict__ = {}

        # 填充字段的默认值
        for field, field_type in annotations.items():
            # 用传入值或默认值填充
            default_value = getattr(self.__class__, field, None)
            value = kwargs.pop(field, default_value)

            # 如果字段类型是 BaseModel 的子类且值是字典，递归实例化嵌套模型
            if isinstance(value, dict) and issubclass(field_type, BaseModel):
                value = field_type(**value)

            # 填充字段
            self.__dict__[field] = value

        # 检查是否有缺失字段
        if [field for field in annotations if self.__dict__[field] is None]:
            raise ValueError(f"配置项缺少属性: {', '.join(missing)}")

        # 忽略未定义的多余字段
        if kwargs:
            logger.warning(f"配置项传入额外属性: {', '.join(kwargs.keys())}", LogType.NONE_TYPE, LogSource.CORE)

    def __repr__(self):
        attrs = ", ".join(f"{key}={value!r}" for key, value in self.__dict__.items())
        return f"{self.__class__.__name__}({attrs})"

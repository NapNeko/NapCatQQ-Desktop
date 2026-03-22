# -*- coding: utf-8 -*-
"""运行时子域。

避免在包初始化阶段级联导入 `napcat` 等高耦合模块，防止产生循环依赖。
"""

__all__ = ["napcat", "paths"]

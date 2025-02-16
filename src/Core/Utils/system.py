# -*- coding: utf-8 -*-
"""
通用系统工具
"""

# 标准库导入
import sys
import platform

isWin11 = platform.system() == "Windows" and sys.getwindowsversion().build >= 22000

# -*- coding: utf-8 -*-
# 第三方库导入
from creart import it
from PySide6.QtCore import QLockFile

# 项目内模块导入
from src.core.utils.path_func import PathFunc

"""
确保应用程序的单实例运行, 使用共享内存和系统信号量, 防止多个实例同时运行
"""


class SingleInstanceApplication:
    """确保应用程序的单实例运行

    使用 QLockFile 基于文件的进程互斥, 更适合 Windows 环境, 支持陈旧锁自动清理。
    """

    # 使用类级别引用, 确保锁对象在进程生命周期内不被 GC 回收
    _lock_file: QLockFile | None = None

    def __init__(self) -> None:
        """初始化(懒加载锁文件)"""
        if SingleInstanceApplication._lock_file is None:
            # 确保临时目录存在
            tmp_dir = it(PathFunc).tmp_path
            tmp_dir.mkdir(parents=True, exist_ok=True)

            # 在运行时临时目录下创建锁文件
            lock_path = tmp_dir / "napcatqq-desktop.lock"
            lock = QLockFile(str(lock_path))
            # 默认 30s 认为锁陈旧, 可在异常退出后自动恢复
            lock.setStaleLockTime(30_000)
            SingleInstanceApplication._lock_file = lock

    def is_running(self) -> bool:
        """检查是否已经有实例正在运行

        返回 True 表示已有实例在运行; 返回 False 表示成功获得锁, 当前为首个实例。
        """

        # 保障锁已创建
        if SingleInstanceApplication._lock_file is None:
            self.__init__()

        lock = SingleInstanceApplication._lock_file

        # 非阻塞尝试获取锁(1ms 超时), 获取失败视为已有实例运行
        if lock.tryLock(1):
            # 成功获得锁 -> 当前进程持有, 持续到进程退出
            return False
        else:
            return True

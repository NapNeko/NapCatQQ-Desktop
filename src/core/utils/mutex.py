# -*- coding: utf-8 -*-
from PySide6.QtCore import QSharedMemory, QSystemSemaphore

"""
确保应用程序的单实例运行, 使用共享内存和系统信号量, 防止多个实例同时运行
"""


class SingleInstanceApplication:
    """确保应用程序的单实例运行

    通过使用共享内存和系统信号量来检查是否已经有实例在运行
    以防止多个实例同时运行, 从而避免资源冲突和数据损坏
    """

    def __init__(self) -> None:
        """初始化共享内存和信号量"""
        self.key = "NapCatQQ_Desktop"
        self.shared_memory = QSharedMemory(self.key)
        self.semaphore = QSystemSemaphore(self.key + "_sem", 1)

    def is_running(self) -> bool:
        """检查是否已经有实例正在运行"""

        # 锁定信号量以访问共享内存
        self.semaphore.acquire()

        if not self.shared_memory.attach():
            # 如果附加失败，共享内存不存在，所以我们创建它
            self.shared_memory.create(1)
            running = False
        else:
            # 如果附加成功，则表示另一个实例正在运行
            running = True
            self.shared_memory.detach()

        # 释放信号量锁
        self.semaphore.release()

        return running

# -*- coding: utf-8 -*-
# 标准库导入

from PySide6.QtCore import QSharedMemory, QSystemSemaphore


class SingleInstanceApplication:
    def __init__(self):
        self.key = "NapCat_Desktop"
        self.shared_memory = QSharedMemory(self.key)
        self.semaphore = QSystemSemaphore(self.key + "_sem", 1)

    def is_running(self):
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
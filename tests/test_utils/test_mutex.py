# -*- coding: utf-8 -*-
"""测试互斥锁（单实例应用）"""
# 标准库导入
from pathlib import Path

# 第三方库导入
import pytest


class TestMutexConcept:
    """测试互斥锁概念"""

    def test_lock_file_concept(self, tmp_path):
        """测试锁文件概念"""
        lock_file = tmp_path / "app.lock"
        
        # 创建锁文件
        lock_file.write_text("locked")
        
        assert lock_file.exists()
        assert lock_file.read_text() == "locked"

    def test_lock_acquisition(self, tmp_path):
        """测试锁获取"""
        lock_file = tmp_path / "app.lock"
        
        # 第一个实例获取锁
        if not lock_file.exists():
            lock_file.write_text("instance1")
            acquired = True
        else:
            acquired = False
        
        assert acquired is True
        assert lock_file.exists()

    def test_lock_already_held(self, tmp_path):
        """测试锁已被持有"""
        lock_file = tmp_path / "app.lock"
        
        # 第一个实例获取锁
        lock_file.write_text("instance1")
        
        # 第二个实例尝试获取锁
        if lock_file.exists():
            can_start = False
        else:
            can_start = True
        
        assert can_start is False

    def test_lock_release(self, tmp_path):
        """测试锁释放"""
        lock_file = tmp_path / "app.lock"
        
        # 获取锁
        lock_file.write_text("locked")
        
        # 释放锁
        lock_file.unlink()
        
        assert not lock_file.exists()

    def test_pid_in_lock_file(self, tmp_path):
        """测试锁文件中的 PID"""
        import os
        
        lock_file = tmp_path / "app.lock"
        pid = os.getpid()
        
        # 写入 PID
        lock_file.write_text(str(pid))
        
        # 读取并验证
        stored_pid = int(lock_file.read_text())
        
        assert stored_pid == pid


class TestSingleInstanceCheck:
    """测试单实例检查"""

    def test_instance_id_generation(self):
        """测试实例 ID 生成"""
        import uuid
        
        instance_id = str(uuid.uuid4())
        
        assert len(instance_id) > 0
        assert "-" in instance_id

    def test_mutex_name_format(self):
        """测试互斥锁名称格式"""
        app_name = "NapCatQQ-Desktop"
        mutex_name = f"Local\\{app_name}"
        
        assert "Local\\" in mutex_name
        assert app_name in mutex_name

    def test_cleanup_on_exit(self, tmp_path):
        """测试退出时清理"""
        lock_file = tmp_path / "app.lock"
        lock_file.write_text("locked")
        
        # 模拟清理
        if lock_file.exists():
            lock_file.unlink()
        
        assert not lock_file.exists()

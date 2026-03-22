# -*- coding: utf-8 -*-

# 标准库导入
from types import SimpleNamespace

# 项目内模块导入
import src.core.platform.single_instance as mutex_module


class FakeLockFile:
    """最小可控的 QLockFile 替身。"""

    next_try_lock_result = True

    def __init__(self, path: str) -> None:
        self.path = path
        self.stale_lock_time = None

    def setStaleLockTime(self, value: int) -> None:
        self.stale_lock_time = value

    def tryLock(self, timeout: int) -> bool:
        self.timeout = timeout
        return self.next_try_lock_result


def setup_mutex(monkeypatch, tmp_path) -> None:
    """重置单实例状态并注入假锁。"""
    mutex_module.SingleInstanceApplication._lock_file = None
    monkeypatch.setattr(mutex_module, "QLockFile", FakeLockFile)
    monkeypatch.setattr(mutex_module, "it", lambda cls: SimpleNamespace(tmp_path=tmp_path))
    monkeypatch.setattr(mutex_module.logger, "info", lambda *args, **kwargs: None)
    monkeypatch.setattr(mutex_module.logger, "warning", lambda *args, **kwargs: None)


def test_single_instance_initializes_lock_file(monkeypatch, tmp_path) -> None:
    """初始化时应创建锁文件并设置陈旧锁超时。"""
    setup_mutex(monkeypatch, tmp_path)

    app = mutex_module.SingleInstanceApplication()

    assert isinstance(app._lock_file, FakeLockFile)
    assert app._lock_file.path.endswith("napcatqq-desktop.lock")
    assert app._lock_file.stale_lock_time == 30000


def test_is_running_returns_false_when_lock_is_acquired(monkeypatch, tmp_path) -> None:
    """获取锁成功表示当前实例是首个实例。"""
    setup_mutex(monkeypatch, tmp_path)
    FakeLockFile.next_try_lock_result = True

    assert mutex_module.SingleInstanceApplication().is_running() is False


def test_is_running_returns_true_when_lock_is_busy(monkeypatch, tmp_path) -> None:
    """获取锁失败表示已有实例在运行。"""
    setup_mutex(monkeypatch, tmp_path)
    FakeLockFile.next_try_lock_result = False

    assert mutex_module.SingleInstanceApplication().is_running() is True


# -*- coding: utf-8 -*-
"""远端 SSH 专用线程池 (P3 perf W4).

动机
====

远端 NapCat 的日志 ``tail`` / 状态轮询 / 部署 / 连接测试 / 配置同步都是阻塞式 SSH
I/O, 过去全部派发到 [`QThreadPool.globalInstance()`](https://doc.qt.io/qt-6/qthreadpool.html),
与 UI 本身的轻量后台任务 (头像下载, 本地文件保存, 版本探测 ...) 共用一个有限容量池
(默认 = ``QThread.idealThreadCount()`` ≈ 4-16). 多个远端 Bot × 多条 5s 轮询在 SSH
抖动时会把池占满, 导致 UI 感知到的 "头像半天不出" / "保存卡顿" 这些**间接**表现.

本模块提供一个**专用线程池**, 把所有会触发 SSH I/O 的 runnable 都派发到这里, 与全局池
彻底隔离:

- 全局池继续服务 HTTP 版本探测 / 头像下载 / 本地文件操作 / ``NapCatQQLoginState``
  HTTP 轮询等短任务;
- 远端 SSH 相关的 runnable (含 tail / poll / start / stop / deploy / sync config ...)
  都走 [`remote_ssh_pool`](src/core/remote/thread_pool.py);
- 线程池容量默认按 "每服务器预留 2 个槽位" 估算, 下限 4, 上限 12, 避免:
  * 单条 SSH transport 级别的串行化 + 轻微并发冲突;
  * 上限太高带来的 paramiko transport 重入 / sshd MaxSessions 触发.

测试友好
========

测试里广泛使用 ``monkeypatch.setattr(module, "QThreadPool", FakePool)`` 来捕获派发.
为了让迁移对测试的破坏面最小, 本模块给每个入口额外暴露 [`remote_ssh_pool`](src/core/remote/thread_pool.py)
这个**独立 callable**, 调用站点通过 ``from src.core.remote.thread_pool import remote_ssh_pool``
在本地 namespace 拿到它, 测试可直接
``monkeypatch.setattr(<module>, "remote_ssh_pool", lambda: FakePool())`` 即可. 
"""
from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from PySide6.QtCore import QRunnable, QThreadPool


# ==================== 内部状态 ====================
# 懒加载的全局单例. 首次调用 ``remote_ssh_pool()`` 时根据 CPU 数 / 服务器数构造.
# 在无 Qt 上下文的纯 logic test 里, 保持为 ``None``, 调用方会回退到
# ``QThreadPool.globalInstance()`` (见 [`remote_ssh_pool`](src/core/remote/thread_pool.py)).
_POOL: "QThreadPool | None" = None


def _default_max_thread_count() -> int:
    """按"每服务器 2 个槽位 + 基础值"估算合适的池容量, 下限 4 上限 12.

    粗略折中: 每台服务器典型负载 ~= 1 条 ``tail`` + 1 条 poll + 少量 deploy/test;
    2 个槽位足以让两条轮询并发跑且不会全部相互排队. 超过 12 后边际收益极低,
    反而会让 paramiko transport 触发 ``MaxSessions`` / 触发 ``ChannelException``.
    """
    try:
        from PySide6.QtCore import QThread

        cpu_hint = max(2, QThread.idealThreadCount())
    except Exception:  # noqa: BLE001 - 非 Qt 环境 (pure logic test) fallback
        cpu_hint = 4

    try:
        from creart import it

        from src.core.remote.server_manager import ServerManager

        server_count = max(1, len(it(ServerManager).list_servers()))
    except Exception:  # noqa: BLE001 - ServerManager 未就绪时给保守默认
        server_count = 1

    estimate = max(cpu_hint // 2, server_count * 2)
    return max(4, min(estimate, 12))


# ==================== 公共入口 ====================
def remote_ssh_pool() -> "QThreadPool":
    """返回远端 SSH 专用 [`QThreadPool`](https://doc.qt.io/qt-6/qthreadpool.html).

    首次调用时构造一个独立池并设定 ``setMaxThreadCount``; 无 Qt 上下文时退化为
    全局池, 让纯 logic test (无 QApplication) 仍能跑通同步监控钩子.

    Note:
        调用方应当**始终通过本函数拿池**, 不要把返回值缓存到模块全局里,
        否则测试通过 ``monkeypatch.setattr(module, "remote_ssh_pool", ...)``
        替换时无法拦截.
    """
    global _POOL

    if _POOL is not None:
        return _POOL

    try:
        from PySide6.QtCore import QThreadPool
    except Exception:  # noqa: BLE001 - 极少数打包异常场景
        raise

    try:
        pool = QThreadPool()
        pool.setMaxThreadCount(_default_max_thread_count())
        # 释放策略: 空闲 30s 回收线程, 避免常驻内存
        try:
            pool.setExpiryTimeout(30_000)
        except Exception:  # noqa: BLE001 - 老版本 PySide6 可能签名不同
            pass
        _POOL = pool
        return pool
    except Exception:
        # 构造失败 (例如无 QApplication); 退到全局池保证可用性
        return QThreadPool.globalInstance()


def dispatch_remote_ssh(runnable: "QRunnable") -> None:
    """把 runnable 派发到远端 SSH 专用池的便捷函数.

    等价于 ``remote_ssh_pool().start(runnable)``; 提供独立 callable 便于测试监控. 
    """
    remote_ssh_pool().start(runnable)


def reset_remote_ssh_pool() -> None:
    """清空模块级单例缓存. 仅供测试 / 应用关闭前收尾用, 生产代码不应调用."""
    global _POOL
    _POOL = None


def shutdown_remote_ssh_pool(*, wait_ms: int = 3000) -> None:
    """等待并关闭远端 SSH 池, 在应用退出流程中使用.

    Args:
        wait_ms: ``QThreadPool.waitForDone`` 的超时; 到点仍有未完成任务时直接丢弃.
    """
    global _POOL
    pool = _POOL
    _POOL = None
    if pool is None:
        return
    try:
        pool.waitForDone(wait_ms)
    except Exception:  # noqa: BLE001 - 退出路径不应再抛
        pass


__all__ = [
    "remote_ssh_pool",
    "dispatch_remote_ssh",
    "reset_remote_ssh_pool",
    "shutdown_remote_ssh_pool",
]

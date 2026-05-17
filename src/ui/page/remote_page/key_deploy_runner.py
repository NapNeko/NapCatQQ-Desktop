# -*- coding: utf-8 -*-
"""SSH 密钥自动下发后台运行器, 避免在 UI 线程内同步阻塞.

对应 [`ServerManager.auto_setup_ssh_key`](src/core/remote/server_manager.py):
用密码登录一次远端, 把本地公钥幂等写入 ``~/.ssh/authorized_keys``,
成功后档案自动切到密钥认证. 行为对齐 ``ssh-copy-id``.

调度方式与 [`ConnectionTester`](src/ui/page/remote_page/connection_tester.py)
完全一致, 走 [`remote_ssh_pool`](src/core/remote/thread_pool.py).
"""

from __future__ import annotations

from PySide6.QtCore import QObject, QRunnable, Signal


class KeyDeployRunnerSignals(QObject):
    """[`KeyDeployRunner`] 信号载体.

    Qt 不允许 [`QRunnable`] 直接定义信号, 用独立 [`QObject`] 中转.
    """

    finished = Signal(str, bool, str)  # (server_id, ok, message)


class KeyDeployRunner(QRunnable):
    """后台执行 ``ssh-copy-id`` 等价流程.

    执行步骤(委托给 [`ServerManager.auto_setup_ssh_key`]):
    1. 复用或生成 ``~/.ssh/id_ed25519``;
    2. 用密码 SSH 登录远端;
    3. 幂等写入 ``~/.ssh/authorized_keys``;
    4. 把档案切到密钥认证, 清掉密码缓存与 keyring 中的密码.

    任何阶段失败都不修改档案, UI 可以安全地让用户重试.
    """

    def __init__(self, server_id: str, *, password: str) -> None:
        super().__init__()
        self.signals = KeyDeployRunnerSignals()
        self._server_id = server_id
        self._password = password
        self.setAutoDelete(True)

    def run(self) -> None:  # noqa: D401 - 实现 QRunnable.run
        from creart import it

        from src.core.remote import ServerManager
        from src.ui.page.remote_page.deployment_runner import _server_label_suffix, _tracked

        manager = it(ServerManager)
        profile = manager.get_server(self._server_id)
        suffix = _server_label_suffix(profile)
        with _tracked(
            f"ssh-key-deploy-{self._server_id}",
            f"配置 SSH 密钥{suffix}",
            content="密码登录并下发公钥…",
        ) as tracker:
            ok, message = manager.auto_setup_ssh_key(
                self._server_id, password=self._password
            )
            self.signals.finished.emit(self._server_id, ok, message)
            if ok:
                tracker.success(message or "已配置免密登录")
            else:
                tracker.fail(message or "SSH 密钥配置失败")

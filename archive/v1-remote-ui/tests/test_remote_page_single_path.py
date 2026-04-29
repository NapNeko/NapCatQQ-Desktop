# -*- coding: utf-8 -*-

import os
import sys
from types import ModuleType
from typing import Any, cast

from PySide6.QtWidgets import QApplication, QWidget

sys.modules.setdefault("qrcode", ModuleType("qrcode"))
markdown_stub = ModuleType("markdown")
setattr(markdown_stub, "markdown", lambda text, *args, **kwargs: text)
sys.modules.setdefault("markdown", markdown_stub)

from src.desktop.ui.page.remote_page import RemotePage
from src.desktop.ui.page.remote_page.agent_panel import AgentConfigPanel


def ensure_qapp() -> QApplication:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return cast(QApplication, app)


def test_remote_page_uses_agent_panel_as_single_primary_path() -> None:
    ensure_qapp()
    host = QWidget()
    page = RemotePage().initialize(cast(Any, host))

    assert isinstance(page.agent_panel, AgentConfigPanel)
    assert page.title_label.text() == "远程 Daemon 管理"
    assert not page.right_panel.deploy_btn.isVisible()
    assert page._current_handler is not None

    page.close()
    host.close()


def test_agent_panel_does_not_expose_persistent_ssh_form_fields() -> None:
    ensure_qapp()
    panel = AgentConfigPanel()

    assert panel.deploy_btn.text() == "打开部署向导"
    assert not hasattr(panel, "ssh_user_edit")
    assert not hasattr(panel, "ssh_pwd_edit")

    panel.close()

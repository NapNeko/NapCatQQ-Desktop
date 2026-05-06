# -*- coding: utf-8 -*-
"""[`BotCard`](src/ui/page/bot_page/widget/card.py) 启动中状态可视化测试 (P3 perf W2 + ProgressInfoBar 重构).

验证 ``slot_process_changed_button`` 在 ``QProcess.ProcessState`` 三态间的渲染:

- ``Starting``: ``run_button`` 仍可见但 disabled, 文案改为 ``启动中…``;
  其他按钮隐藏. 启动进度条 / 完成反馈交由
  [`ProgressInfoBarBridge`](src/ui/components/progress_info_bar_bridge.py) 在主窗口右上展示.
- ``Running``: 隐藏 run, 显示 stop/log/web_ui.
- ``NotRunning``: run_button 重新可用 + 文案恢复, 其他按钮隐藏.
"""
from __future__ import annotations

# 标准库导入
import importlib.util
import os
import sys
from pathlib import Path
from types import ModuleType, SimpleNamespace

# 第三方库导入
import pytest
from PySide6.QtCore import QProcess
from PySide6.QtWidgets import QApplication, QWidget

# 项目内模块导入
sys.modules.setdefault("qrcode", ModuleType("qrcode"))


def load_card_module():
    """按文件路径加载 card 模块, 避免触发页面包的全量导入。

    与 [`test_bot_card.py`](script/test/test_bot_card.py) 同款的旁路加载, 这样
    本测试不会被 `BotPage.__init__` 的 creart 链路影响.
    """
    project_root = Path(__file__).resolve().parents[2]
    module_name = "src.ui.page.bot_page.widget.card"

    page_package = ModuleType("src.ui.page")
    page_package.__path__ = [str(project_root / "src" / "ui" / "page")]
    sys.modules["src.ui.page"] = page_package

    bot_page_package = ModuleType("src.ui.page.bot_page")
    bot_page_package.__path__ = [str(project_root / "src" / "ui" / "page" / "bot_page")]
    sys.modules["src.ui.page.bot_page"] = bot_page_package

    widget_package = ModuleType("src.ui.page.bot_page.widget")
    widget_package.__path__ = [str(project_root / "src" / "ui" / "page" / "bot_page" / "widget")]
    sys.modules["src.ui.page.bot_page.widget"] = widget_package

    sys.modules.pop(module_name, None)
    spec = importlib.util.spec_from_file_location(
        module_name,
        project_root / "src" / "ui" / "page" / "bot_page" / "widget" / "card.py",
    )
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


card_module = load_card_module()


def ensure_qapp() -> QApplication:
    """创建或复用测试用 QApplication。"""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


class DummySignal:
    def connect(self, *_args, **_kwargs) -> None:
        return None


class DummyAvatarWidget(QWidget):
    def __init__(self, _qq_id: str, parent: QWidget | None = None) -> None:
        super().__init__(parent)


class DummyInfoWidget(QWidget):
    def __init__(self, _config, parent: QWidget | None = None) -> None:  # noqa: ANN001
        super().__init__(parent)


def _make_card(monkeypatch: pytest.MonkeyPatch, config) -> object:
    """构造一个 BotCard 实例, 旁路掉 manager / 头像下载副作用。"""
    ensure_qapp()
    fake_process_manager = SimpleNamespace(process_changed_signal=DummySignal())
    fake_login_state_manager = SimpleNamespace(
        qr_code_available_signal=DummySignal(),
        qr_code_removed_signal=DummySignal(),
    )
    fake_qr_code_factory = SimpleNamespace(has_qr_code=lambda qq_id: False)

    monkeypatch.setattr(card_module, "BotAvatarWidget", DummyAvatarWidget)
    monkeypatch.setattr(card_module, "BotInfoWidget", DummyInfoWidget)
    monkeypatch.setattr(
        card_module,
        "it",
        lambda cls: {
            "ManagerNapCatQQProcess": fake_process_manager,
            "ManagerNapCatQQLoginState": fake_login_state_manager,
            "QRCodeDialogFactory": fake_qr_code_factory,
        }[cls.__name__],
    )

    return card_module.BotCard(config)


def test_starting_state_disables_run_button_and_changes_text(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """``Starting``: run_button 仍可见但 disabled, 文案改 ``启动中…``;
    其他按钮隐藏, 不再嵌入进度条 (转给 ProgressInfoBar 桥)."""
    card = _make_card(monkeypatch, config_factory(11112222))

    card.slot_process_changed_button("11112222", QProcess.ProcessState.Starting)

    assert card.run_button.isHidden() is False
    assert card.run_button.isEnabled() is False
    assert card.run_button.text() == "启动中…"
    assert card.stop_button.isHidden() is True
    assert card.log_button.isHidden() is True
    assert card.web_ui_button.isHidden() is True


def test_running_state_restores_run_button_and_shows_stop(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """``Running``: 退出 Starting, run_button 还原 enabled+"启动"文案后被隐藏,
    显示 stop/log/web_ui."""
    card = _make_card(monkeypatch, config_factory(11112222))
    # 先进入 Starting
    card.slot_process_changed_button("11112222", QProcess.ProcessState.Starting)

    card.slot_process_changed_button("11112222", QProcess.ProcessState.Running)

    assert card.run_button.isEnabled() is True
    assert card.run_button.text() == "启动"
    assert card.run_button.isHidden() is True
    assert card.stop_button.isHidden() is False
    assert card.log_button.isHidden() is False
    assert card.web_ui_button.isHidden() is False


def test_not_running_state_restores_run_button(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """``NotRunning``: run_button 重新可用 + 默认文案, 其他按钮隐藏."""
    card = _make_card(monkeypatch, config_factory(11112222))
    card.slot_process_changed_button("11112222", QProcess.ProcessState.Starting)

    card.slot_process_changed_button("11112222", QProcess.ProcessState.NotRunning)

    assert card.run_button.isEnabled() is True
    assert card.run_button.text() == "启动"
    assert card.run_button.isHidden() is False
    assert card.stop_button.isHidden() is True
    assert card.log_button.isHidden() is True
    assert card.web_ui_button.isHidden() is True


def test_other_qq_id_does_not_trigger_render(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """信号 qq_id 与本卡片不一致时应忽略, 避免误显示其他 Bot 的 Starting."""
    card = _make_card(monkeypatch, config_factory(11112222))

    # 默认状态下 run_button 可见且 enabled
    assert card.run_button.isHidden() is False
    assert card.run_button.isEnabled() is True
    assert card.run_button.text() == "启动"

    card.slot_process_changed_button("99998888", QProcess.ProcessState.Starting)

    # 仍未变化
    assert card.run_button.isEnabled() is True
    assert card.run_button.text() == "启动"


def test_update_info_card_reflects_starting_state(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """update_info_card: record.state==Starting 时也应渲染指示, 不只 Running。"""
    ensure_qapp()
    fake_record = SimpleNamespace(state=QProcess.ProcessState.Starting)
    fake_process_manager = SimpleNamespace(
        process_changed_signal=DummySignal(),
        get_process=lambda qq_id: fake_record,
    )
    fake_login_state_manager = SimpleNamespace(
        qr_code_available_signal=DummySignal(),
        qr_code_removed_signal=DummySignal(),
    )
    fake_qr_code_factory = SimpleNamespace(has_qr_code=lambda qq_id: False)

    monkeypatch.setattr(card_module, "BotAvatarWidget", DummyAvatarWidget)
    monkeypatch.setattr(card_module, "BotInfoWidget", DummyInfoWidget)
    monkeypatch.setattr(
        card_module,
        "it",
        lambda cls: {
            "ManagerNapCatQQProcess": fake_process_manager,
            "ManagerNapCatQQLoginState": fake_login_state_manager,
            "QRCodeDialogFactory": fake_qr_code_factory,
        }[cls.__name__],
    )

    card = card_module.BotCard(config_factory(11112222))
    card.update_info_card()

    # update_info_card 走 slot_process_changed_button(Starting) 路径
    assert card.run_button.isEnabled() is False
    assert card.run_button.text() == "启动中…"
    assert card.run_button.isHidden() is False

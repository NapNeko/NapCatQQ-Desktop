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

    P4 W1 修复: 如果 ``src.ui.page`` 已被其他测试/代码以真实模块身份加载
    (``__file__`` 存在), 不要覆盖为空命名空间 — 否则后续跳转到
    [`MainWindow`](src/ui/window/main_window/window.py) 的 ``from src.ui.page import
    ApiDebugPage`` 将报 ``cannot import name 'ApiDebugPage'``.
    """
    project_root = Path(__file__).resolve().parents[2]
    module_name = "src.ui.page.bot_page.widget.card"

    def _ensure_namespace(name: str, path: Path) -> None:
        existing = sys.modules.get(name)
        # 已是真实加载的 package (有 ``__file__``) -> 保留, 不要被空命名空间覆盖
        if existing is not None and getattr(existing, "__file__", None):
            return
        package = ModuleType(name)
        package.__path__ = [str(path)]
        sys.modules[name] = package

    _ensure_namespace("src.ui.page", project_root / "src" / "ui" / "page")
    _ensure_namespace("src.ui.page.bot_page", project_root / "src" / "ui" / "page" / "bot_page")
    _ensure_namespace(
        "src.ui.page.bot_page.widget",
        project_root / "src" / "ui" / "page" / "bot_page" / "widget",
    )

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


# P4 W1 修复: 不在模块加载期 (collection 阶段) 就调用 load_card_module(),
# 避免混入 ``src.ui.page.bot_page.widget`` 空命名空间污染其他测试文件的 collection.
_card_module_cache = None


def _get_card_module():
    """懒加载 card 模块; 首次调用才手动拼装 namespace package。"""
    global _card_module_cache
    if _card_module_cache is None:
        _card_module_cache = load_card_module()
    return _card_module_cache


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
    card_module = _get_card_module()
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
            "BotProcessManager": fake_process_manager,
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
    card_module = _get_card_module()
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
            "BotProcessManager": fake_process_manager,
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


# ==================== "关闭中" UX 防御 (2026-05-20 SnowLuma 析构竞态) ====================
def test_begin_stopping_locks_both_buttons(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """``_begin_stopping``: run/stop 按钮均 disabled, 文案均改 "关闭中…", 阻止重启."""
    card = _make_card(monkeypatch, config_factory(11112222))
    # 先到 Running 态以便 stop_button 可见
    card.slot_process_changed_button("11112222", QProcess.ProcessState.Running)

    card._begin_stopping()

    assert card._stopping is True
    assert card.run_button.isEnabled() is False
    assert card.run_button.text() == "关闭中…"
    assert card.stop_button.isEnabled() is False
    assert card.stop_button.text() == "关闭中…"
    assert card._stopping_timer.isActive() is True


def test_finish_stopping_restores_ui_to_not_running(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """``_finish_stopping``: 解除关闭中态, run_button 恢复 + 其他按钮隐藏 (NotRunning UI)."""
    card = _make_card(monkeypatch, config_factory(11112222))
    card.slot_process_changed_button("11112222", QProcess.ProcessState.Running)
    card._begin_stopping()

    card._finish_stopping()

    assert card._stopping is False
    assert card._stopping_timer.isActive() is False
    assert card.run_button.isEnabled() is True
    assert card.run_button.text() == "启动"
    assert card.stop_button.text() == "停止"
    assert card.run_button.isHidden() is False
    assert card.stop_button.isHidden() is True


def test_not_running_during_stopping_is_ignored(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """关闭中收到 NotRunning 信号 → **忽略**, 不解锁 UI; 让定时器超时再解锁.

    背景: stop_bot 同步路径立即 emit NotRunning, 但底层 SnowLuma zombie QProcess
    实际还在回收中 (terminate 优雅退出 ~1-5s, 期间用户点启动会触发析构竞态).
    UX 防御必须忽略 NotRunning, 强制等定时器超时, 确保 zombie 真正完成.
    """
    card = _make_card(monkeypatch, config_factory(11112222))
    card.slot_process_changed_button("11112222", QProcess.ProcessState.Running)
    card._begin_stopping()

    card.slot_process_changed_button("11112222", QProcess.ProcessState.NotRunning)

    # 关闭中态不应被 NotRunning 解锁
    assert card._stopping is True
    assert card.run_button.isEnabled() is False
    assert card.run_button.text() == "关闭中…"
    assert card._stopping_timer.isActive() is True


def test_starting_during_stopping_breaks_guard(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """关闭中收到 Starting (异常路径) → 解除关闭中态, 走 Starting UI."""
    card = _make_card(monkeypatch, config_factory(11112222))
    card.slot_process_changed_button("11112222", QProcess.ProcessState.Running)
    card._begin_stopping()

    card.slot_process_changed_button("11112222", QProcess.ProcessState.Starting)

    # 关闭中被打破, 走 Starting UI
    assert card._stopping is False
    assert card._stopping_timer.isActive() is False
    assert card.run_button.text() == "启动中…"
    assert card.run_button.isEnabled() is False


def test_stop_button_napcat_backend_skips_stopping_guard(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """NapCat backend 同步退出, 不需要关闭中态防御 — ``slot_stop_button`` 直接走 stop_bot."""
    card_module = _get_card_module()
    config = config_factory(11112222)
    # 默认 backend 是 NAPCAT
    card = _make_card(monkeypatch, config)
    assert card._needs_stopping_guard() is False

    stop_called: list[str] = []
    remove_called: list[str] = []
    monkeypatch.setattr(
        card_module,
        "it",
        lambda cls: {
            "BotProcessManager": SimpleNamespace(stop_bot=lambda qq_id: stop_called.append(qq_id)),
            "ManagerAutoRestartProcess": SimpleNamespace(
                remove_auto_restart_timer=lambda qq_id: remove_called.append(qq_id)
            ),
        }[cls.__name__],
    )

    card.slot_stop_button()

    assert card._stopping is False
    assert stop_called == ["11112222"]
    assert remove_called == ["11112222"]


def test_stop_button_local_snowluma_enables_stopping_guard(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """本地 SnowLuma backend → ``slot_stop_button`` 启用关闭中态防御."""
    from src.core.runtime.backend_type import BackendType

    card_module = _get_card_module()
    config = config_factory(11112222)
    config.bot.backend_type = BackendType.SNOWLUMA
    card = _make_card(monkeypatch, config)
    # 先到 Running 状态
    card.slot_process_changed_button("11112222", QProcess.ProcessState.Running)

    stop_called: list[str] = []
    monkeypatch.setattr(
        card_module,
        "it",
        lambda cls: {
            "BotProcessManager": SimpleNamespace(stop_bot=lambda qq_id: stop_called.append(qq_id)),
            "ManagerAutoRestartProcess": SimpleNamespace(remove_auto_restart_timer=lambda qq_id: None),
        }[cls.__name__],
    )

    card.slot_stop_button()

    assert card._stopping is True
    assert card.run_button.text() == "关闭中…"
    assert card.stop_button.text() == "关闭中…"
    assert card._stopping_timer.isActive() is True
    assert stop_called == ["11112222"]

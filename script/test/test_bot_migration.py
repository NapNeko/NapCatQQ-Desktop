# -*- coding: utf-8 -*-
"""[`BotMigrationService`](src/core/operation/migration.py) 单元测试 (P3.W3.B).

回归保护点 (对应 [`docs/general/remote_ssh_p3_plan.md`](../../docs/general/remote_ssh_p3_plan.md) §3.3):

- ``MigrationPlan.validate`` 校验语义
- 仅迁移 ``onebot11_<qq>.json`` / ``napcat_<qq>.json`` (其他文件保留)
- 复制 + 清理流程: 目标端拿到正确内容, 源端被清空
- backend connect 失败时 ``finished_signal(False, ...)`` 被发出, 抛 ``BotMigrationError``
- 跨 backend 类型 (本地↔远端) 的 dispatch
- ``BotMigrationRunnable`` end-to-end 通过 [`QThreadPool`](https://doc.qt.io/qt-6/qthreadpool.html)

不依赖真实 SSH; 通过自定义 ``_InMemoryBackend`` + ``monkeypatch`` 替换 resolver.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from pathlib import Path

import pytest

from src.core.operation import migration as migration_mod
from src.core.operation.backend import (
    FileEntry,
    InstallationInfo,
    OperationBackend,
    ProcessStatus,
    WebUIEndpoint,
)
from src.core.operation.migration import (
    BotMigrationError,
    BotMigrationRunnable,
    BotMigrationService,
    MigrationPlan,
    derive_plan_from_bot_config,
)


# ==================== In-Memory Backend ====================
@dataclass
class _InMemoryBackend(OperationBackend):
    """以字典模拟 backend; 仅实现迁移服务实际调用的方法."""

    label: str = "fake"
    config_dir: str = "/fake/config"
    files: dict[str, str] = field(default_factory=dict)
    connect_calls: int = 0
    connect_should_fail: bool = False
    _open: bool = field(default=True, init=False)

    # ---- 生命周期 ----
    def connect(self) -> None:  # type: ignore[override]
        self.connect_calls += 1
        if self.connect_should_fail:
            raise RuntimeError(f"{self.label} connect failed")
        self._open = True

    def close(self) -> None:  # type: ignore[override]
        self._open = False

    @property
    def is_connected(self) -> bool:  # type: ignore[override]
        return self._open

    # ---- 文件操作 ----
    def read_file(self, path: str) -> str:  # type: ignore[override]
        if path not in self.files:
            raise FileNotFoundError(path)
        return self.files[path]

    def write_file(self, path: str, content: str) -> None:  # type: ignore[override]
        self.files[path] = content

    def file_exists(self, path: str) -> bool:  # type: ignore[override]
        if path == self.config_dir:
            return True
        return path in self.files

    def list_dir(self, path: str) -> list[FileEntry]:  # type: ignore[override]
        if path != self.config_dir:
            raise FileNotFoundError(path)
        prefix = self.config_dir.rstrip("/") + "/"
        return [
            FileEntry(name=p.removeprefix(prefix), is_dir=False, size=len(self.files[p]))
            for p in self.files
            if p.startswith(prefix) and "/" not in p.removeprefix(prefix)
        ]

    def mkdir(self, path: str, *, parents: bool = True, exist_ok: bool = True) -> None:  # type: ignore[override]
        return None

    def remove(self, path: str, *, recursive: bool = False) -> None:  # type: ignore[override]
        self.files.pop(path, None)

    def upload(self, local_path: str | Path, remote_path: str) -> None:  # type: ignore[override]
        self.files[remote_path] = Path(local_path).read_text(encoding="utf-8")

    def download(self, remote_path: str, local_path: str | Path) -> None:  # type: ignore[override]
        Path(local_path).write_text(self.files[remote_path], encoding="utf-8")

    # ---- 以下方法迁移服务用不到; 实现仅为满足抽象接口 ----
    def start_napcat(self, qq_id: str, config: object) -> ProcessStatus:  # type: ignore[override]
        raise NotImplementedError

    def stop_napcat(self, qq_id: str) -> None:  # type: ignore[override]
        raise NotImplementedError

    def get_process_status(self, qq_id: str) -> ProcessStatus:  # type: ignore[override]
        raise NotImplementedError

    def get_memory_usage(self, qq_id: str) -> int | None:  # type: ignore[override]
        return None

    def install_napcat(self, archive_path=None, *, progress=None) -> None:  # type: ignore[override]
        raise NotImplementedError

    def install_qq(self, *, progress=None) -> None:  # type: ignore[override]
        raise NotImplementedError

    def detect_napcat_version(self) -> str | None:  # type: ignore[override]
        return None

    def detect_qq_path(self) -> str | None:  # type: ignore[override]
        return None

    def detect_installation(self) -> InstallationInfo:  # type: ignore[override]
        return InstallationInfo()

    def read_log(self, qq_id: str) -> str:  # type: ignore[override]
        return ""

    def tail_log(self, qq_id: str, *, lines: int = 200) -> str:  # type: ignore[override]
        return ""

    def get_webui_endpoint(self, qq_id: str) -> WebUIEndpoint | None:  # type: ignore[override]
        return None


def _seed(backend: _InMemoryBackend, *, qq_id: str, content_prefix: str = "src") -> None:
    """往 ``backend`` 的 config_dir 里塞一对 onebot11/napcat JSON + 一个不相关文件."""
    base = backend.config_dir.rstrip("/")
    backend.files[f"{base}/onebot11_{qq_id}.json"] = f"{content_prefix}-onebot"
    backend.files[f"{base}/napcat_{qq_id}.json"] = f"{content_prefix}-napcat"
    backend.files[f"{base}/onebot11_99999999.json"] = f"{content_prefix}-other-bot"
    backend.files[f"{base}/global.json"] = f"{content_prefix}-global"


@pytest.fixture
def patched_resolver(monkeypatch: pytest.MonkeyPatch):
    """把 ``migration_mod.resolve_backend_for_bot`` 替换成按 target 派发到字典."""
    backends: dict[str, _InMemoryBackend] = {}

    def _fake_resolve(bot_shim, *, server_manager=None):  # noqa: ARG001
        target = getattr(bot_shim, "runtime_target", None)
        if target not in backends:
            raise migration_mod.BackendResolutionError(
                f"unknown target: {target}", stage="server_not_found", target=target
            )
        return backends[target]

    monkeypatch.setattr(migration_mod, "resolve_backend_for_bot", _fake_resolve)
    return backends


def _patch_config_dir_for(monkeypatch: pytest.MonkeyPatch) -> None:
    """让 ``BotMigrationService._config_dir_for`` 直接读 ``backend.config_dir`` 属性,
    避免触发真实的 LocalBackend / RemoteBackend 类型分支 + creart PathFunc."""

    def _fake_config_dir(backend: object) -> str:
        return getattr(backend, "config_dir", "/fake/config")

    def _fake_join(backend: object, filename: str) -> str:
        config_dir = _fake_config_dir(backend)
        if config_dir.endswith("/"):
            return f"{config_dir}{filename}"
        return f"{config_dir}/{filename}"

    monkeypatch.setattr(BotMigrationService, "_config_dir_for", staticmethod(_fake_config_dir))
    monkeypatch.setattr(BotMigrationService, "_join_config_path", staticmethod(_fake_join))


# ==================== MigrationPlan.validate ====================
class TestMigrationPlanValidate:
    def test_valid_plan(self) -> None:
        plan = MigrationPlan(qq_id="123456", source_target="local", dest_target="srv-1")
        plan.validate()  # 不抛即通过

    @pytest.mark.parametrize("bad_qq", ["", "   "])
    def test_empty_qq_id_raises(self, bad_qq: str) -> None:
        with pytest.raises(ValueError, match="qq_id"):
            MigrationPlan(qq_id=bad_qq, source_target="local", dest_target="srv-1").validate()

    def test_same_source_dest_raises(self) -> None:
        with pytest.raises(ValueError, match="源/目标相同"):
            MigrationPlan(qq_id="1", source_target="local", dest_target="local").validate()

    def test_empty_target_raises(self) -> None:
        with pytest.raises(ValueError, match="不能为空"):
            MigrationPlan(qq_id="1", source_target="", dest_target="srv-1").validate()


# ==================== derive_plan_from_bot_config ====================
class TestDerivePlan:
    def test_local_to_remote(self) -> None:
        plan = derive_plan_from_bot_config(qq_id=2477817352, old_target="local", new_target="srv-1")
        assert plan.qq_id == "2477817352"
        assert plan.source_target == "local"
        assert plan.dest_target == "srv-1"
        assert plan.move_persistent_data is False

    def test_empty_targets_default_to_local(self) -> None:
        plan = derive_plan_from_bot_config(qq_id="1", old_target="", new_target="")
        assert plan.source_target == "local"
        assert plan.dest_target == "local"


# ==================== BotMigrationService.execute - happy path ====================
class TestServiceHappyPath:
    def test_copies_qq_specific_files_and_cleans_source(
        self,
        patched_resolver: dict[str, _InMemoryBackend],
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        _patch_config_dir_for(monkeypatch)
        qq_id = "2477817352"
        source = _InMemoryBackend(label="src", config_dir="/src/config")
        dest = _InMemoryBackend(label="dst", config_dir="/dst/config")
        _seed(source, qq_id=qq_id, content_prefix="src")
        # 目标端预先存放与本 qq 无关的文件, 验证不会被破坏
        dest.files["/dst/config/global.json"] = "dst-global"

        patched_resolver["local"] = source
        patched_resolver["srv-1"] = dest

        service = BotMigrationService()
        progress: list[tuple[str, int]] = []
        finished: list[tuple[bool, str]] = []
        service.progress_signal.connect(lambda msg, pct: progress.append((msg, pct)))
        service.finished_signal.connect(lambda ok, msg: finished.append((ok, msg)))

        plan = MigrationPlan(qq_id=qq_id, source_target="local", dest_target="srv-1")
        service.execute(plan)

        # 目标端拿到 qq-specific 配置
        assert dest.files[f"/dst/config/onebot11_{qq_id}.json"] == "src-onebot"
        assert dest.files[f"/dst/config/napcat_{qq_id}.json"] == "src-napcat"
        # 目标端原有不相关文件未被破坏
        assert dest.files["/dst/config/global.json"] == "dst-global"
        # 源端 qq-specific 配置被清空
        assert f"/src/config/onebot11_{qq_id}.json" not in source.files
        assert f"/src/config/napcat_{qq_id}.json" not in source.files
        # 源端其他文件保留
        assert "/src/config/onebot11_99999999.json" in source.files
        assert "/src/config/global.json" in source.files

        # 信号语义
        assert any(pct == 100 for _, pct in progress), "应至少发出 100% 进度"
        assert finished == [(True, finished[0][1])]
        assert "已迁移" in finished[0][1] or "ok" in finished[0][1].lower()

        # 两端 connect 都被调用
        assert source.connect_calls == 1
        assert dest.connect_calls == 1

    def test_no_qq_files_succeeds_silently(
        self,
        patched_resolver: dict[str, _InMemoryBackend],
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """源端没有任何匹配文件时, 仍应 emit finished(True, ...)."""
        _patch_config_dir_for(monkeypatch)
        source = _InMemoryBackend(label="src", config_dir="/src/config")
        dest = _InMemoryBackend(label="dst", config_dir="/dst/config")
        # 仅放无关文件
        source.files["/src/config/global.json"] = "x"

        patched_resolver["local"] = source
        patched_resolver["srv-1"] = dest

        service = BotMigrationService()
        finished: list[tuple[bool, str]] = []
        service.finished_signal.connect(lambda ok, msg: finished.append((ok, msg)))

        plan = MigrationPlan(qq_id="2477817352", source_target="local", dest_target="srv-1")
        service.execute(plan)

        assert finished[0][0] is True
        assert dest.files == {}, "目标端不应被写入"
        # 源端无关文件也不动
        assert "/src/config/global.json" in source.files


# ==================== 失败路径 ====================
class TestServiceFailures:
    def test_invalid_plan_raises_and_emits_finished_false(
        self,
        patched_resolver: dict[str, _InMemoryBackend],
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        _patch_config_dir_for(monkeypatch)
        service = BotMigrationService()
        finished: list[tuple[bool, str]] = []
        service.finished_signal.connect(lambda ok, msg: finished.append((ok, msg)))

        plan = MigrationPlan(qq_id="1", source_target="local", dest_target="local")
        with pytest.raises(BotMigrationError) as exc_info:
            service.execute(plan)
        assert exc_info.value.stage == "validate"
        assert finished[0][0] is False

    def test_unresolvable_target_raises(
        self,
        patched_resolver: dict[str, _InMemoryBackend],
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """resolver 找不到 target 时应抛 BotMigrationError(stage=resolve)."""
        _patch_config_dir_for(monkeypatch)
        # 故意只注册 source, 不注册 dest
        patched_resolver["local"] = _InMemoryBackend(label="src", config_dir="/src/config")

        service = BotMigrationService()
        finished: list[tuple[bool, str]] = []
        service.finished_signal.connect(lambda ok, msg: finished.append((ok, msg)))

        plan = MigrationPlan(qq_id="2477817352", source_target="local", dest_target="srv-1")
        with pytest.raises(BotMigrationError) as exc_info:
            service.execute(plan)
        assert exc_info.value.stage == "resolve"
        assert finished[0][0] is False

    def test_source_connect_failure_propagates(
        self,
        patched_resolver: dict[str, _InMemoryBackend],
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        _patch_config_dir_for(monkeypatch)
        source = _InMemoryBackend(label="src", config_dir="/src/config", connect_should_fail=True)
        dest = _InMemoryBackend(label="dst", config_dir="/dst/config")
        patched_resolver["local"] = source
        patched_resolver["srv-1"] = dest

        service = BotMigrationService()
        finished: list[tuple[bool, str]] = []
        service.finished_signal.connect(lambda ok, msg: finished.append((ok, msg)))

        plan = MigrationPlan(qq_id="2477817352", source_target="local", dest_target="srv-1")
        with pytest.raises(BotMigrationError) as exc_info:
            service.execute(plan)
        assert exc_info.value.stage == "connect"
        assert finished[0][0] is False


# ==================== Runnable end-to-end ====================
class TestRunnable:
    def test_runnable_executes_via_thread_pool(
        self,
        patched_resolver: dict[str, _InMemoryBackend],
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """``BotMigrationRunnable.run()`` 直接调用 (无需真起 QThreadPool) 应成功并 emit signals."""
        _patch_config_dir_for(monkeypatch)
        # 需要 QApplication 支持 QObject 信号
        os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
        from PySide6.QtWidgets import QApplication

        app = QApplication.instance() or QApplication([])
        _ = app  # keep reference

        qq_id = "2477817352"
        source = _InMemoryBackend(label="src", config_dir="/src/config")
        dest = _InMemoryBackend(label="dst", config_dir="/dst/config")
        _seed(source, qq_id=qq_id, content_prefix="src")
        patched_resolver["local"] = source
        patched_resolver["srv-1"] = dest

        plan = MigrationPlan(qq_id=qq_id, source_target="local", dest_target="srv-1")
        runnable = BotMigrationRunnable(plan)

        progress: list[tuple[str, int]] = []
        finished: list[tuple[bool, str]] = []
        runnable.signals.progress.connect(lambda msg, pct: progress.append((msg, pct)))
        runnable.signals.finished.connect(lambda ok, msg: finished.append((ok, msg)))

        # 直接同步调用 run() 而非走 QThreadPool, 简化测试可观察性
        runnable.run()

        assert finished[0][0] is True
        assert dest.files[f"/dst/config/onebot11_{qq_id}.json"] == "src-onebot"
        assert any(pct == 100 for _, pct in progress)

    def test_runnable_swallows_migration_error(
        self,
        patched_resolver: dict[str, _InMemoryBackend],
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """run() 内部 BotMigrationError 不应再向外抛, 只 emit finished(False)."""
        _patch_config_dir_for(monkeypatch)
        os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
        from PySide6.QtWidgets import QApplication

        app = QApplication.instance() or QApplication([])
        _ = app

        # 缺 dest 触发 resolve 失败
        patched_resolver["local"] = _InMemoryBackend(label="src", config_dir="/src/config")

        plan = MigrationPlan(qq_id="2477817352", source_target="local", dest_target="srv-1")
        runnable = BotMigrationRunnable(plan)
        finished: list[tuple[bool, str]] = []
        runnable.signals.finished.connect(lambda ok, msg: finished.append((ok, msg)))

        # 不应抛
        runnable.run()

        assert finished[0][0] is False

# -*- coding: utf-8 -*-

# 标准库导入
import json
from pathlib import Path
from types import SimpleNamespace

# 第三方库导入
import httpx

# 项目内模块导入
import src.desktop.core.versioning.service as versioning


def mute_version_logger(monkeypatch) -> None:
    """屏蔽版本模块的日志副作用。"""
    monkeypatch.setattr(versioning.logger, "error", lambda *args, **kwargs: None)


def test_remote_version_execute_assembles_three_sources(monkeypatch) -> None:
    """远程版本任务应汇总 NapCat、QQ 和 NCD 三路数据。"""
    runner = versioning.RemoteVersionTask()
    sequence = iter(
        [
            {"version": "v1.0.0", "update_log": "napcat log"},
            {"version": "9.9.9", "download_url": "https://qq.example.com"},
            {"version": "v2.0.0", "update_log": "ncd log"},
        ]
    )
    monkeypatch.setattr(runner, "_get_version", lambda *args, **kwargs: next(sequence))

    result = runner.execute()

    assert result.napcat_version == "v1.0.0"
    assert result.qq_version == "9.9.9"
    assert result.ncd_version == "v2.0.0"
    assert result.qq_download_url == "https://qq.example.com"
    assert result.napcat_update_log == "napcat log"
    assert result.ncd_update_log == "ncd log"


def test_remote_version_request_handles_network_error(monkeypatch) -> None:
    """网络异常应转为 None 并发出错误信号。"""
    mute_version_logger(monkeypatch)
    runner = versioning.RemoteVersionTask()
    errors: list[str] = []
    runner.error_signal.connect(errors.append)

    class FakeClient:
        def __init__(self, timeout: int, follow_redirects: bool = False) -> None:
            self.timeout = timeout
            self.follow_redirects = follow_redirects

        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc_val, exc_tb) -> None:
            return None

        def get(self, url: str):
            raise httpx.RequestError("boom", request=httpx.Request("GET", url))

    monkeypatch.setattr(versioning.httpx, "Client", FakeClient)

    assert runner.request(versioning.QUrl("https://example.com"), "NapCat") is None
    assert len(errors) == 1
    assert "获取 NapCat 版本信息失败" in errors[0]


def test_get_version_returns_error_value_when_parser_raises_key_error(monkeypatch) -> None:
    """解析响应缺少关键字段时应回退为错误值。"""
    mute_version_logger(monkeypatch)
    runner = versioning.RemoteVersionTask()
    errors: list[str] = []
    runner.error_signal.connect(errors.append)
    monkeypatch.setattr(runner, "request", lambda url, name: {"unexpected": "value"})

    result = runner._get_version("https://example.com", "NapCat", runner._parse_github_response)

    assert result == {"version": None, "update_log": None}
    assert errors == ["解析 NapCat 版本信息失败: 'tag_name'"]


def test_parse_qq_response_returns_none_when_windows_section_missing() -> None:
    """QQ 版本接口缺少 Windows 键时应返回空结果。"""
    runner = versioning.RemoteVersionTask()

    assert runner._parse_qq_response({}) == {"version": None, "download_url": None}


def test_parse_qq_response_emits_error_when_windows_payload_is_invalid(monkeypatch) -> None:
    """QQ 版本响应结构异常时应发出错误信号。"""
    mute_version_logger(monkeypatch)
    runner = versioning.RemoteVersionTask()
    errors: list[str] = []
    runner.error_signal.connect(errors.append)

    result = runner._parse_qq_response({"Windows": object()})

    assert result == {"version": None, "download_url": None}
    assert len(errors) == 1
    assert "解析 QQ 版本信息失败" in errors[0]


def test_local_version_reads_package_and_qq_files(monkeypatch, tmp_path: Path) -> None:
    """本地版本任务应读取 NapCat package.json 和 QQ config.json。"""
    mute_version_logger(monkeypatch)
    napcat_path = tmp_path / "NapCatQQ"
    qq_path = tmp_path / "QQ"
    (napcat_path / "package.json").parent.mkdir(parents=True, exist_ok=True)
    (qq_path / "versions").mkdir(parents=True, exist_ok=True)
    (napcat_path / "package.json").write_text(json.dumps({"version": "9.8.7"}), encoding="utf-8")
    (qq_path / "versions" / "config.json").write_text(json.dumps({"curVersion": "9.9.23-41857"}), encoding="utf-8")

    fake_path_func = SimpleNamespace(napcat_path=napcat_path, get_qq_path=lambda: qq_path)
    fake_cfg = SimpleNamespace(napcat_desktop_version="ncd_version", get=lambda item: "v1.7.28")
    monkeypatch.setattr(versioning, "it", lambda cls: fake_path_func)
    monkeypatch.setattr(versioning, "cfg", fake_cfg)

    result = versioning.LocalVersionTask().execute()

    assert result.napcat_version == "v9.8.7"
    assert result.qq_version == "9.9.23"
    assert result.ncd_version == "v1.7.28"


def test_local_version_prefers_napcat_mjs_embedded_version(monkeypatch, tmp_path: Path) -> None:
    """本地 NapCat 版本应优先读取 napcat.mjs 中的真实核心版本。"""
    mute_version_logger(monkeypatch)
    napcat_path = tmp_path / "NapCatQQ"
    napcat_path.mkdir(parents=True, exist_ok=True)
    (napcat_path / "napcat.mjs").write_text(
        'const napCatVersion = typeof (__vite_import_meta_env__) !== "undefined" && "4.17.52" || "1.0.0-dev";',
        encoding="utf-8",
    )
    (napcat_path / "package.json").write_text(json.dumps({"version": "0.0.1"}), encoding="utf-8")

    fake_path_func = SimpleNamespace(napcat_path=napcat_path, get_qq_path=lambda: None)
    fake_cfg = SimpleNamespace(napcat_desktop_version="ncd_version", get=lambda item: "v1.7.28")
    monkeypatch.setattr(versioning, "it", lambda cls: fake_path_func)
    monkeypatch.setattr(versioning, "cfg", fake_cfg)

    runner = versioning.LocalVersionTask()

    assert runner.get_napcat_version() == "v4.17.52"


def test_local_version_handles_missing_files(monkeypatch, tmp_path: Path) -> None:
    """本地版本文件缺失时应返回 None 并发出错误。"""
    mute_version_logger(monkeypatch)
    errors: list[str] = []
    fake_path_func = SimpleNamespace(napcat_path=tmp_path / "NapCatQQ", get_qq_path=lambda: None)
    fake_cfg = SimpleNamespace(napcat_desktop_version="ncd_version", get=lambda item: "v1.7.28")
    monkeypatch.setattr(versioning, "it", lambda cls: fake_path_func)
    monkeypatch.setattr(versioning, "cfg", fake_cfg)

    runner = versioning.LocalVersionTask()
    runner.error_signal.connect(errors.append)

    assert runner.get_napcat_version() is None
    assert runner.get_qq_version() is None
    assert errors == ["获取 NapCat 版本信息失败: 文件不存在"]


def test_version_service_refresh_submits_local_and_remote_tasks(monkeypatch) -> None:
    """VersionService.refresh 应向线程池同时提交本地和远程任务。"""
    started: list[str] = []

    class FakeThreadPool:
        def start(self, runnable) -> None:
            started.append(type(runnable).__name__)

    monkeypatch.setattr(versioning.QThreadPool, "globalInstance", staticmethod(lambda: FakeThreadPool()))

    versioning.VersionService().refresh()

    assert started == ["LocalVersionTask", "RemoteVersionTask"]


def test_version_runnable_base_run_emits_execute_result() -> None:
    """基类 run 应发出 execute 的返回值。"""

    class DummyRunnable(versioning.VersionTaskBase):
        def execute(self) -> versioning.VersionSnapshot:
            return versioning.VersionSnapshot(napcat_version="v1", qq_version="v2", ncd_version="v3")

    emitted: list[versioning.VersionSnapshot] = []
    runnable = DummyRunnable()
    runnable.version_signal.connect(emitted.append)

    runnable.run()

    assert [(item.napcat_version, item.qq_version, item.ncd_version) for item in emitted] == [("v1", "v2", "v3")]

# -*- coding: utf-8 -*-

# 标准库导入
import json
from pathlib import Path
from types import SimpleNamespace

# 第三方库导入
import httpx

# 项目内模块导入
import src.core.utils.get_version as get_version


def mute_version_logger(monkeypatch) -> None:
    """屏蔽版本模块的日志副作用。"""
    monkeypatch.setattr(get_version.logger, "error", lambda *args, **kwargs: None)


def test_remote_version_execute_assembles_three_sources(monkeypatch) -> None:
    """远程版本任务应汇总 NapCat、QQ 和 NCD 三路数据。"""
    runner = get_version.GetRemoteVersionRunnable()
    sequence = iter(
        [
            {"version": "v1.0.0", "update_log": "napcat log"},
            {"version": "9.9.9", "download_url": "https://qq.example.com"},
            {"version": "v2.0.0", "update_log": "ncd log"},
        ]
    )
    monkeypatch.setattr(runner, "_get_version", lambda *args, **kwargs: next(sequence))
    monkeypatch.setattr(runner, "_get_desktop_update_manifest", lambda: None)

    result = runner.execute()

    assert result.napcat_version == "v1.0.0"
    assert result.qq_version == "9.9.9"
    assert result.ncd_version == "v2.0.0"
    assert result.qq_download_url == "https://qq.example.com"
    assert result.napcat_update_log == "napcat log"
    assert result.ncd_update_log == "ncd log"
    assert result.ncd_update_manifest is None


def test_remote_version_request_handles_network_error(monkeypatch) -> None:
    """网络异常应转为 None 并发出错误信号。"""
    mute_version_logger(monkeypatch)
    runner = get_version.GetRemoteVersionRunnable()
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

    monkeypatch.setattr(get_version.httpx, "Client", FakeClient)

    assert runner.request(get_version.QUrl("https://example.com"), "NapCat") is None
    assert len(errors) == 1
    assert "获取 NapCat 版本信息失败" in errors[0]


def test_get_version_returns_error_value_when_parser_raises_key_error(monkeypatch) -> None:
    """解析响应缺少关键字段时应回退为错误值。"""
    mute_version_logger(monkeypatch)
    runner = get_version.GetRemoteVersionRunnable()
    errors: list[str] = []
    runner.error_signal.connect(errors.append)
    monkeypatch.setattr(runner, "request", lambda url, name: {"unexpected": "value"})

    result = runner._get_version("https://example.com", "NapCat", runner._parse_github_response)

    assert result == {"version": None, "update_log": None}
    assert errors == ["解析 NapCat 版本信息失败: 'tag_name'"]


def test_parse_qq_response_returns_none_when_windows_section_missing() -> None:
    """QQ 版本接口缺少 Windows 键时应返回空结果。"""
    runner = get_version.GetRemoteVersionRunnable()

    assert runner._parse_qq_response({}) == {"version": None, "download_url": None}


def test_parse_qq_response_emits_error_when_windows_payload_is_invalid(monkeypatch) -> None:
    """QQ 版本响应结构异常时应发出错误信号。"""
    mute_version_logger(monkeypatch)
    runner = get_version.GetRemoteVersionRunnable()
    errors: list[str] = []
    runner.error_signal.connect(errors.append)

    result = runner._parse_qq_response({"Windows": object()})

    assert result == {"version": None, "download_url": None}
    assert len(errors) == 1
    assert "解析 QQ 版本信息失败" in errors[0]


def test_get_desktop_update_manifest_returns_manifest_entry(monkeypatch) -> None:
    """远端 manifest 应能解析出迁移规则。"""
    runner = get_version.GetRemoteVersionRunnable()
    monkeypatch.setattr(
        runner,
        "request",
        lambda *args, **kwargs: {
            "schema_version": 2,
            "min_auto_update_version": "v1.8.0",
            "migrations": [
                {
                    "id": "cfg-layout-v2",
                    "from_min": "v1.8.0",
                    "from_max": "v1.9.99",
                    "to_version": "v2.0.0",
                    "script_url": "https://example.com/update.bat",
                    "summary": "需要迁移配置目录",
                }
            ],
        },
    )

    result = runner._get_desktop_update_manifest()

    assert result is not None
    assert result.min_auto_update_version == "v1.8.0"
    assert len(result.migrations) == 1
    assert result.migrations[0].id == "cfg-layout-v2"
    assert result.migrations[0].to_version == "v2.0.0"
    assert result.migrations[0].script_url == "https://example.com/update.bat"


def test_desktop_update_migration_matches_only_target_window() -> None:
    """迁移规则应只在指定的本地和目标版本窗口内生效。"""
    migration = get_version.DesktopUpdateMigration(
        id="cfg-layout-v2",
        from_min="v1.7.0",
        from_max="v1.7.99",
        to_version="v2.0.0",
        script_url="https://example.com/update.bat",
    )

    assert migration.matches("v1.7.9", "v2.0.0") is True
    assert migration.matches("v1.8.0", "v2.0.0") is False
    assert migration.matches("v1.7.9", "v2.1.0") is False


def test_resolve_desktop_update_plan_returns_unsupported_for_too_old_local_version() -> None:
    """低于最小自动升级版本时应直接阻止自动更新。"""
    manifest = get_version.DesktopUpdateManifest(schema_version=2, min_auto_update_version="v1.8.0", migrations=[])

    result = get_version.resolve_desktop_update_plan("v1.7.9", "v2.0.0", manifest)

    assert result is not None
    assert result.blocks_update() is True
    assert result.min_auto_update_version == "v1.8.0"


def test_resolve_desktop_update_plan_returns_migration_when_rule_matches() -> None:
    """命中区间规则时应返回迁移计划。"""
    manifest = get_version.DesktopUpdateManifest(
        schema_version=2,
        min_auto_update_version="v1.8.0",
        migrations=[
            get_version.DesktopUpdateMigration(
                id="cfg-layout-v2",
                from_min="v1.8.0",
                from_max="v1.9.99",
                to_version="v2.0.0",
                script_url="https://example.com/update.bat",
            )
        ],
    )

    result = get_version.resolve_desktop_update_plan("v1.8.5", "v2.0.0", manifest)

    assert result is not None
    assert result.requires_remote_script() is True
    assert result.migration is not None
    assert result.migration.id == "cfg-layout-v2"


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
    monkeypatch.setattr(get_version, "it", lambda cls: fake_path_func)
    monkeypatch.setattr(get_version, "cfg", fake_cfg)

    result = get_version.GetLocalVersionRunnable().execute()

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
    monkeypatch.setattr(get_version, "it", lambda cls: fake_path_func)
    monkeypatch.setattr(get_version, "cfg", fake_cfg)

    runner = get_version.GetLocalVersionRunnable()

    assert runner.get_napcat_version() == "v4.17.52"


def test_local_version_handles_missing_files(monkeypatch, tmp_path: Path) -> None:
    """本地版本文件缺失时应返回 None 并发出错误。"""
    mute_version_logger(monkeypatch)
    errors: list[str] = []
    fake_path_func = SimpleNamespace(napcat_path=tmp_path / "NapCatQQ", get_qq_path=lambda: None)
    fake_cfg = SimpleNamespace(napcat_desktop_version="ncd_version", get=lambda item: "v1.7.28")
    monkeypatch.setattr(get_version, "it", lambda cls: fake_path_func)
    monkeypatch.setattr(get_version, "cfg", fake_cfg)

    runner = get_version.GetLocalVersionRunnable()
    runner.error_signal.connect(errors.append)

    assert runner.get_napcat_version() is None
    assert runner.get_qq_version() is None
    assert errors == ["获取 NapCat 版本信息失败: 文件不存在"]


def test_get_version_update_submits_local_and_remote_runnables(monkeypatch) -> None:
    """GetVersion.update 应向线程池同时提交本地和远程任务。"""
    started: list[str] = []

    class FakeThreadPool:
        def start(self, runnable) -> None:
            started.append(type(runnable).__name__)

    monkeypatch.setattr(get_version.QThreadPool, "globalInstance", staticmethod(lambda: FakeThreadPool()))

    get_version.GetVersion().update()

    assert started == ["GetLocalVersionRunnable", "GetRemoteVersionRunnable"]


def test_version_runnable_base_run_emits_execute_result() -> None:
    """基类 run 应发出 execute 的返回值。"""

    class DummyRunnable(get_version.VersionRunnableBase):
        def execute(self) -> get_version.VersionData:
            return get_version.VersionData(napcat_version="v1", qq_version="v2", ncd_version="v3")

    emitted: list[get_version.VersionData] = []
    runnable = DummyRunnable()
    runnable.version_signal.connect(emitted.append)

    runnable.run()

    assert [(item.napcat_version, item.qq_version, item.ncd_version) for item in emitted] == [("v1", "v2", "v3")]

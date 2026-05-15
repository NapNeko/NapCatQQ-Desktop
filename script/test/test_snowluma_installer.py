# -*- coding: utf-8 -*-
"""SnowLuma 适配 P7.4: SnowLumaInstall 单测.

校验解压, 覆盖, 保留 ``config/data/`` 子目录, verify_install 失败语义, 临时 zip 清理.
参见: ``docs/requirements/2026-05-10-snowluma-backend-adapter.md`` §4.2
"""

from __future__ import annotations

import io
import zipfile
from pathlib import Path

import pytest
from creart import it

from src.core.installation.installers import SnowLumaInstall
from src.core.runtime.paths import PathFunc


@pytest.fixture
def snowluma_paths(tmp_path: Path, monkeypatch):
    """monkeypatch PathFunc 的 tmp_path 与 snowluma_path 到测试隔离目录."""
    fake_tmp = tmp_path / "tmp"
    fake_install = tmp_path / "SnowLuma"
    fake_tmp.mkdir()
    fake_install.mkdir()

    path_func = it(PathFunc)
    monkeypatch.setattr(path_func, "tmp_path", fake_tmp)
    monkeypatch.setattr(path_func, "snowluma_path", fake_install)

    return {"tmp": fake_tmp, "install": fake_install}


def _make_zip(tmp_path: Path, tag: str = "v1.7.5", *, include_node: bool = True) -> Path:
    """构造一份合法的 SnowLuma 发布 zip; include_node=False 用于测 verify_install 失败路径."""
    zip_path = tmp_path / f"SnowLuma-{tag}-win-x64.zip"
    top = f"SnowLuma-{tag}-win-x64"
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
        if include_node:
            zf.writestr(f"{top}/node.exe", b"NODE-BIN")
        zf.writestr(f"{top}/index.mjs", b"// snowluma entry")
        zf.writestr(f"{top}/package.json", b'{"version":"0.1.0"}')
        zf.writestr(f"{top}/config/runtime.json", b'{"webuiPort":5099}')
        zf.writestr(f"{top}/native/snowluma-win32-x64.dll", b"DLL-BIN")
    return zip_path


def _make_flat_zip(tmp_path: Path, tag: str = "v1.7.5", *, include_node: bool = True) -> Path:
    """构造与上游 GitHub release 一致的扁平 zip (无 ``SnowLuma-<tag>-win-x64/`` 包装目录).

    这是 https://github.com/SnowLuma/SnowLuma/releases 实际下发的结构, 顶层直接是
    ``client/``, ``native/``, ``index.mjs`` 等. P1 验收阶段发现安装器误以为存在
    包装目录会把 ``client/index.html`` 错误剥离为 ``index.html``, 导致
    SnowLuma 启动后立即退出 (exit_code=1).
    """
    zip_path = tmp_path / f"SnowLuma-{tag}-win-x64.zip"
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
        if include_node:
            zf.writestr("node.exe", b"NODE-BIN")
        zf.writestr("index.mjs", b"// snowluma entry")
        zf.writestr("package.json", b'{"version":"0.1.0"}')
        zf.writestr("launcher.bat", b"@echo off\r\nnode index.mjs\r\n")
        zf.writestr("config-DyfbYA36.js", b"// runtime config helper")
        zf.writestr("client/index.html", b"<!DOCTYPE html>")
        zf.writestr("client/assets/app.css", b"body{}")
        zf.writestr("native/snowluma-win32-x64.dll", b"DLL-BIN")
        zf.writestr("native/ffmpeg/ffmpeg.exe", b"FFMPEG-BIN")
    return zip_path


# ==================== \u521d\u59cb\u5316 ====================
class TestSnowLumaInstallInit:
    def test_empty_tag_raises(self) -> None:
        with pytest.raises(ValueError):
            SnowLumaInstall(tag="")

    def test_whitespace_tag_raises(self) -> None:
        with pytest.raises(ValueError):
            SnowLumaInstall(tag="   ")

    def test_zip_filename_reflects_tag(self, snowluma_paths) -> None:
        installer = SnowLumaInstall(tag="v1.7.5")
        assert installer.zip_file_path.name == "SnowLuma-v1.7.5-win-x64.zip"
        assert installer.zip_file_path.parent == snowluma_paths["tmp"]


# ==================== \u6210\u529f\u8def\u5f84 ====================
class TestSnowLumaInstallSuccess:
    def test_extracts_into_install_path_root(self, snowluma_paths) -> None:
        zip_src = _make_zip(snowluma_paths["tmp"])
        assert zip_src.exists()
        installer = SnowLumaInstall(tag="v1.7.5")
        installer.run()

        install = snowluma_paths["install"]
        # 顶层目录已被剥离, 文件直接落在 install_path
        assert (install / "node.exe").exists()
        assert (install / "index.mjs").exists()
        assert (install / "package.json").exists()
        assert (install / "config" / "runtime.json").exists()
        assert (install / "native" / "snowluma-win32-x64.dll").exists()
        # zip 已被清理
        assert not zip_src.exists()

    def test_writes_installed_tag(self, snowluma_paths) -> None:
        _make_zip(snowluma_paths["tmp"])
        SnowLumaInstall(tag="v1.7.5").run()

        installed_tag = snowluma_paths["install"] / ".installed_tag"
        assert installed_tag.exists()
        assert installed_tag.read_text(encoding="utf-8").strip() == "v1.7.5"


# ==================== 上游发布 zip (扁平结构) ====================
class TestSnowLumaInstallFlatZip:
    """上游发布包为扁平结构时 (无 ``SnowLuma-<tag>-win-x64/`` 包装目录) 的回归保护.

    发现于 P1 SnowLuma 适配人工验收阶段: 安装后 SnowLuma 启动立即退出
    (exit_code=1), 原因是原安装器不区分情况一律剥离首段, 误将 ``client/index.html``
    剥为 ``index.html``, 导致 SnowLuma 运行时找不到 ``client/`` ,  ``native/``
    子目录.
    """

    def test_flat_zip_preserves_client_and_native_dirs(self, snowluma_paths) -> None:
        _make_flat_zip(snowluma_paths["tmp"])
        SnowLumaInstall(tag="v1.7.5").run()

        install = snowluma_paths["install"]
        # 顶层文件 (原本就在 zip 顶层)
        assert (install / "node.exe").exists()
        assert (install / "index.mjs").exists()
        assert (install / "launcher.bat").exists()
        assert (install / "config-DyfbYA36.js").exists()
        # 关键回归: client/ 与 native/ 子目录以及其内部文件必须完整保留
        assert (install / "client" / "index.html").exists()
        assert (install / "client" / "assets" / "app.css").exists()
        assert (install / "native" / "snowluma-win32-x64.dll").exists()
        assert (install / "native" / "ffmpeg" / "ffmpeg.exe").exists()
        # 保护: 不应出现被误剥离后的扁平产物
        assert not (install / "index.html").exists(), "client/ 不得被错误剥离"
        assert not (install / "snowluma-win32-x64.dll").exists(), "native/ 不得被错误剥离"
        assert not (install / "app.css").exists(), "client/assets/ 不得被错误剥离"
        assert not (install / "ffmpeg.exe").exists(), "native/ffmpeg/ 不得被错误剥离"

    def test_flat_zip_passes_verify_install(self, snowluma_paths) -> None:
        _make_flat_zip(snowluma_paths["tmp"])
        installer = SnowLumaInstall(tag="v1.7.5")

        finished = []
        errored = []
        installer.install_finish_signal.connect(lambda: finished.append(True))
        installer.error_finish_signal.connect(lambda: errored.append(True))
        installer.run()

        assert finished == [True]
        assert errored == []


# ==================== 探测包装目录 ====================
class TestDetectWrapperPrefix:
    """``SnowLumaInstall._detect_wrapper_prefix`` 的单元说明."""

    def test_returns_none_for_flat_zip(self) -> None:
        members = [
            "client/",
            "client/index.html",
            "native/",
            "native/dll",
            "index.mjs",
            "package.json",
        ]
        assert SnowLumaInstall._detect_wrapper_prefix(members) is None

    def test_returns_prefix_for_wrapped_zip(self) -> None:
        members = [
            "SnowLuma-v1.7.5-win-x64/",
            "SnowLuma-v1.7.5-win-x64/index.mjs",
            "SnowLuma-v1.7.5-win-x64/native/dll",
        ]
        assert (
            SnowLumaInstall._detect_wrapper_prefix(members) == "SnowLuma-v1.7.5-win-x64/"
        )

    def test_returns_none_for_single_top_file_only(self) -> None:
        # 只有顶层文件无子目录: 不是包装目录, 不剥离
        members = ["only_file.txt"]
        assert SnowLumaInstall._detect_wrapper_prefix(members) is None

    def test_returns_none_for_empty_members(self) -> None:
        assert SnowLumaInstall._detect_wrapper_prefix([]) is None


# ==================== 保留 config/data 子目录 ====================
class TestSnowLumaInstallPreservesUserState:
    def test_existing_config_runtime_json_not_overwritten(self, snowluma_paths) -> None:
        # 先创建一份用户已有的 config/runtime.json (与 zip 内的不同)
        existing_dir = snowluma_paths["install"] / "config"
        existing_dir.mkdir(parents=True)
        existing_target = existing_dir / "runtime.json"
        user_payload = '{"webuiPort": 7777, "_user_marker": true}'
        existing_target.write_text(user_payload, encoding="utf-8")

        _make_zip(snowluma_paths["tmp"])
        SnowLumaInstall(tag="v1.7.5").run()

        # 用户的 runtime.json 必须保留
        assert existing_target.read_text(encoding="utf-8") == user_payload

    def test_existing_data_db_not_overwritten(self, snowluma_paths) -> None:
        # 模拟用户已登录, data/<uin>/messages.db 已存在
        data_dir = snowluma_paths["install"] / "data" / "12345"
        data_dir.mkdir(parents=True)
        db_target = data_dir / "messages.db"
        user_db = b"USER-MESSAGES-DB-BYTES"
        db_target.write_bytes(user_db)

        # 构造 zip 时 data/ 不在 zip 里 (上游 release 也不带 data 子目录),
        # 但即便 zip 含 data/, 已存在的也不应被覆盖
        zip_path = snowluma_paths["tmp"] / "SnowLuma-v1.7.5-win-x64.zip"
        top = "SnowLuma-v1.7.5-win-x64"
        with zipfile.ZipFile(zip_path, "w") as zf:
            zf.writestr(f"{top}/node.exe", b"NODE")
            zf.writestr(f"{top}/index.mjs", b"// entry")
            zf.writestr(f"{top}/package.json", b"{}")
            # 故意写入与已有同路径的 data/.../messages.db
            zf.writestr(f"{top}/data/12345/messages.db", b"FRESH-DB-FROM-RELEASE")

        SnowLumaInstall(tag="v1.7.5").run()
        assert db_target.read_bytes() == user_db


# ==================== \u5931\u8d25\u8def\u5f84 ====================
class TestSnowLumaInstallFailure:
    def test_missing_node_exe_emits_error_and_cleans_zip(
        self, snowluma_paths, qtbot=None
    ) -> None:
        """zip 内缺 node.exe → verify 抛 RuntimeError → emit error_finish_signal."""
        _make_zip(snowluma_paths["tmp"], include_node=False)

        installer = SnowLumaInstall(tag="v1.7.5")

        error_emitted = []
        finish_emitted = []
        installer.error_finish_signal.connect(lambda: error_emitted.append(True))
        installer.install_finish_signal.connect(lambda: finish_emitted.append(True))

        installer.run()

        assert len(error_emitted) == 1, "error_finish_signal 应被 emit 一次"
        assert len(finish_emitted) == 0, "失败路径不应触发 install_finish_signal"
        # 临时 zip 已被清理 (异常路径上 finally 兜底)
        assert not installer.zip_file_path.exists()

    def test_corrupt_zip_emits_error(self, snowluma_paths) -> None:
        zip_path = snowluma_paths["tmp"] / "SnowLuma-v1.7.5-win-x64.zip"
        zip_path.write_bytes(b"not a zip file")

        installer = SnowLumaInstall(tag="v1.7.5")
        error_emitted = []
        installer.error_finish_signal.connect(lambda: error_emitted.append(True))
        installer.run()

        assert len(error_emitted) == 1


# ==================== Tier G: 密码钩子 (P2 W4b) ====================
@pytest.fixture
def isolated_session_for_install(tmp_path: Path, monkeypatch):
    """把 ``snowluma_session.session_path`` 重定向到 tmp_path, 让 ``_init_or_update_password``
    钩子在隔离环境下跑."""
    fake_session_path = tmp_path / "snowluma-session.json"
    monkeypatch.setattr(
        "src.core.runtime.snowluma_session.session_path",
        lambda: fake_session_path,
    )
    return fake_session_path


class TestSnowLumaInstallPasswordHook:
    """P2 (Tier G): 验证 :meth:`SnowLumaInstall._init_or_update_password` 的 install 钩子.

    覆盖:
    - 安装成功后 ``snowluma-session.json`` 含 password / created_at / last_rendered_at;
    - SnowLuma 侧 ``webui.json`` 包含 password hash + salt + ``mustChangePassword=False``;
    - 重复 install 不改 ``password`` (sticky), 仅刷 ``last_rendered_at``.
    """

    def test_snowluma_session_json_created_after_install(
        self, snowluma_paths, isolated_session_for_install: Path
    ) -> None:
        import json

        from src.core.runtime.snowluma_session import load_session

        _make_zip(snowluma_paths["tmp"])
        SnowLumaInstall(tag="v1.7.5").run()

        # snowluma-session.json 已被 _init_or_update_password 创建
        assert isolated_session_for_install.exists()
        session = load_session()
        assert session is not None
        assert len(session.password) >= 10
        assert session.created_at != ""
        assert session.last_rendered_at != ""
        # session.json 内容也匹配
        payload = json.loads(isolated_session_for_install.read_text(encoding="utf-8"))
        assert payload["password"] == session.password

    def test_webui_json_contains_password(
        self, snowluma_paths, isolated_session_for_install: Path
    ) -> None:
        import json

        _make_zip(snowluma_paths["tmp"])
        SnowLumaInstall(tag="v1.7.5").run()

        # SnowLuma webui.json 含 hash + salt + mustChangePassword=False
        webui_json = snowluma_paths["install"] / "config" / "webui.json"
        assert webui_json.exists()
        payload = json.loads(webui_json.read_text(encoding="utf-8"))
        assert "passwordHash" in payload
        assert "passwordSalt" in payload
        assert payload.get("mustChangePassword") is False
        assert len(payload["passwordHash"]) == 128  # scrypt keylen=64 → 128 hex chars
        assert len(payload["passwordSalt"]) == 32  # 16 bytes → 32 hex chars

    def test_repeated_install_does_not_change_password(
        self, snowluma_paths, isolated_session_for_install: Path
    ) -> None:
        from src.core.runtime.snowluma_session import load_session

        _make_zip(snowluma_paths["tmp"])
        SnowLumaInstall(tag="v1.7.5").run()
        first_session = load_session()
        assert first_session is not None

        # 再 run 一次安装 (重新构造 zip 因为上次安装清掉了)
        _make_zip(snowluma_paths["tmp"])
        SnowLumaInstall(tag="v1.7.5").run()
        second_session = load_session()
        assert second_session is not None

        # password 和 created_at sticky
        assert first_session.password == second_session.password
        assert first_session.created_at == second_session.created_at
        # last_rendered_at 可能变化, 不强断言

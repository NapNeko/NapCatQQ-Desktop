# -*- coding: utf-8 -*-
""":mod:`src.core.remote.snowluma.paths` 单元测试.

覆盖 plan §W1 单测列表:

- ``from_base`` 各派生字段
- ``pid_bot`` / ``status_bot`` / ``log_bot`` POSIX 路径拼接
- 显式覆盖单一字段时不被默认派生覆盖
- 非法路径 (含 shell 元字符 / 空格) 必 raise
- ``qq_id`` / ``uin`` 非数字必 raise
"""

from __future__ import annotations

import pytest

from src.core.remote.snowluma.paths import SnowLumaRemotePaths


class TestFromBase:
    """``SnowLumaRemotePaths.from_base`` 派生路径测试."""

    def test_default(self) -> None:
        paths = SnowLumaRemotePaths.from_base()
        assert paths.base_dir == "$HOME/snowluma-remote"
        assert paths.workspace_dir == "$HOME/snowluma-remote/workspace"
        assert paths.snowluma_framework_dir == "$HOME/snowluma-remote/workspace/snowluma"
        assert paths.config_dir == "$HOME/snowluma-remote/workspace/snowluma/config"
        assert paths.runtime_dir == "$HOME/snowluma-remote/workspace/runtime"
        assert paths.log_dir == "$HOME/snowluma-remote/workspace/log"
        assert paths.vnc_secret == "$HOME/snowluma-remote/workspace/vnc.secret"
        assert paths.webui_secret == "$HOME/snowluma-remote/workspace/webui.secret"
        assert (
            paths.daemon_launcher_script
            == "$HOME/snowluma-remote/workspace/snowluma_daemon_launcher.sh"
        )
        assert (
            paths.bot_launcher_script
            == "$HOME/snowluma-remote/workspace/snowluma_bot_launcher.sh"
        )

    def test_absolute_base(self) -> None:
        paths = SnowLumaRemotePaths.from_base("/opt/snowluma")
        assert paths.workspace_dir == "/opt/snowluma/workspace"
        assert paths.snowluma_framework_dir == "/opt/snowluma/workspace/snowluma"
        assert paths.runtime_dir == "/opt/snowluma/workspace/runtime"

    def test_trailing_slash_normalized(self) -> None:
        # base_dir 末尾 ``/`` 应被规范化, 不出现 ``//``
        paths = SnowLumaRemotePaths.from_base("/opt/snowluma/")
        assert "//" not in paths.workspace_dir
        assert paths.workspace_dir == "/opt/snowluma/workspace"


class TestDerivedProperties:
    """运行时派生路径 (pid / status / log / config) 测试."""

    @pytest.fixture
    def paths(self) -> SnowLumaRemotePaths:
        return SnowLumaRemotePaths.from_base("/opt/snowluma")

    def test_pid_daemon(self, paths: SnowLumaRemotePaths) -> None:
        assert paths.pid_daemon == "/opt/snowluma/workspace/runtime/pid_daemon"

    def test_status_daemon(self, paths: SnowLumaRemotePaths) -> None:
        assert paths.status_daemon == "/opt/snowluma/workspace/runtime/status_daemon.json"

    def test_log_daemon(self, paths: SnowLumaRemotePaths) -> None:
        assert paths.log_daemon == "/opt/snowluma/workspace/log/daemon.log"

    def test_dbus_env_file(self, paths: SnowLumaRemotePaths) -> None:
        assert paths.dbus_env_file == "/opt/snowluma/workspace/runtime/dbus.env"

    def test_pid_bot(self, paths: SnowLumaRemotePaths) -> None:
        assert paths.pid_bot("114514") == "/opt/snowluma/workspace/runtime/pid_bot_114514"

    def test_status_bot(self, paths: SnowLumaRemotePaths) -> None:
        assert (
            paths.status_bot("114514")
            == "/opt/snowluma/workspace/runtime/status_bot_114514.json"
        )

    def test_log_bot(self, paths: SnowLumaRemotePaths) -> None:
        assert paths.log_bot("114514") == "/opt/snowluma/workspace/log/bot_114514.log"

    def test_runtime_json(self, paths: SnowLumaRemotePaths) -> None:
        assert (
            paths.runtime_json()
            == "/opt/snowluma/workspace/snowluma/config/runtime.json"
        )

    def test_webui_json(self, paths: SnowLumaRemotePaths) -> None:
        assert (
            paths.webui_json()
            == "/opt/snowluma/workspace/snowluma/config/webui.json"
        )

    def test_onebot_json(self, paths: SnowLumaRemotePaths) -> None:
        assert (
            paths.onebot_json("2707600964")
            == "/opt/snowluma/workspace/snowluma/config/onebot_2707600964.json"
        )


class TestExplicitOverride:
    """显式传字段时, 不被 ``__post_init__`` 默认派生覆盖."""

    def test_workspace_override(self) -> None:
        paths = SnowLumaRemotePaths(
            base_dir="/opt/sl",
            workspace_dir="/var/sl-workspace",
        )
        # workspace 显式给的, 不被 base_dir 派生覆盖
        assert paths.workspace_dir == "/var/sl-workspace"
        # 但 snowluma_framework_dir 没显式给, 应基于已给的 workspace 派生
        assert paths.snowluma_framework_dir == "/var/sl-workspace/snowluma"
        assert paths.runtime_dir == "/var/sl-workspace/runtime"

    def test_full_explicit(self) -> None:
        # 全字段显式, 不走任何派生
        paths = SnowLumaRemotePaths(
            base_dir="/opt/sl",
            workspace_dir="/a",
            snowluma_framework_dir="/b",
            config_dir="/c",
            runtime_dir="/d",
            log_dir="/e",
            vnc_secret="/f.secret",
            webui_secret="/g.secret",
            daemon_launcher_script="/h.sh",
            bot_launcher_script="/i.sh",
        )
        assert paths.workspace_dir == "/a"
        assert paths.snowluma_framework_dir == "/b"
        assert paths.runtime_dir == "/d"


class TestSecurityValidation:
    """路径合法性 (P5 F2.3 同源校验) 拒绝 shell 元字符."""

    @pytest.mark.parametrize(
        "evil_base",
        [
            "$HOME/sl$(rm -rf /)",        # command substitution
            "/opt/sl;rm -rf /",            # 命令分隔符
            "/opt/sl && evil",             # AND 操作
            "/opt/sl|evil",                # 管道
            "/opt/sl`evil`",               # 反引号
            "/opt/sl$VAR",                 # 非 $HOME 变量展开
            "/opt/sl with space",          # 空格 (regex 不允许)
            "/opt/sl\nnext",               # 换行
            "relative/path",               # 不以 $HOME 或 / 起头
            "",                            # 空串
        ],
    )
    def test_evil_base_dir_raises(self, evil_base: str) -> None:
        with pytest.raises(ValueError):
            SnowLumaRemotePaths.from_base(evil_base)

    def test_evil_workspace_override_raises(self) -> None:
        with pytest.raises(ValueError):
            SnowLumaRemotePaths(
                base_dir="/opt/sl",
                workspace_dir="/var/$(evil)",
            )


class TestQQIdValidation:
    """``qq_id`` / ``uin`` 校验."""

    @pytest.fixture
    def paths(self) -> SnowLumaRemotePaths:
        return SnowLumaRemotePaths.from_base()

    @pytest.mark.parametrize("bad_qq_id", ["", "abc", "12 34", "1;2", "$(rm)"])
    def test_pid_bot_rejects_invalid(
        self, paths: SnowLumaRemotePaths, bad_qq_id: str
    ) -> None:
        with pytest.raises(ValueError):
            paths.pid_bot(bad_qq_id)

    @pytest.mark.parametrize("bad_qq_id", ["", "abc", "12 34"])
    def test_status_bot_rejects_invalid(
        self, paths: SnowLumaRemotePaths, bad_qq_id: str
    ) -> None:
        with pytest.raises(ValueError):
            paths.status_bot(bad_qq_id)

    @pytest.mark.parametrize("bad_qq_id", ["", "abc"])
    def test_log_bot_rejects_invalid(
        self, paths: SnowLumaRemotePaths, bad_qq_id: str
    ) -> None:
        with pytest.raises(ValueError):
            paths.log_bot(bad_qq_id)

    @pytest.mark.parametrize("bad_uin", ["", "abc", "12 34"])
    def test_onebot_json_rejects_invalid_uin(
        self, paths: SnowLumaRemotePaths, bad_uin: str
    ) -> None:
        with pytest.raises(ValueError):
            paths.onebot_json(bad_uin)
